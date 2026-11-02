//! Shared provisioning ownership, progress, integrity and publication policy.
//! Catalog, transport and cache locking remain adapter responsibilities.
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};
use voicepaste_core::models::Artifact;

/// Caller owns the cache lock and has rejected the completed artifact. Native
/// staging permissions/sharing and transport stay outside this shared scenario.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn prepare_artifact(
    paths: (&Path, &Path),
    artifact: &Artifact,
    cancel: &AtomicBool,
    allow_network: bool,
    open: impl FnOnce(&Path) -> Result<File, String>,
    download: impl FnOnce(&mut File, u64) -> Result<(), String>,
) -> Result<(), String> {
    let (partial, destination) = paths;
    // Recover a complete verified partial even when network access is forbidden.
    if publish(partial, destination, artifact, cancel)? {
        return Ok(());
    }
    if !allow_network {
        return Err(
            "The selected model needs to be downloaded. Start recording to approve or retry."
                .into(),
        );
    }
    check_cancel(cancel)?;
    match fs::symlink_metadata(partial) {
        Ok(metadata) if !metadata.is_file() => {
            return Err("Refusing non-file model staging path".into());
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(error.to_string());
        }
        _ => {}
    }
    let mut file = open(partial)?;
    let mut offset = file.metadata().map_err(|e| e.to_string())?.len();
    // Full but corrupt, or oversized: never append to bytes that cannot resume.
    if offset >= artifact.bytes {
        file.set_len(0).map_err(|e| e.to_string())?;
        offset = 0;
    }
    download(&mut file, offset)?;
    drop(file); // Release native exclusive sharing before verification/publication.
    if !publish(partial, destination, artifact, cancel)? {
        return Err(format!(
            "Downloaded {} failed exact size/SHA-256 verification; not published. Retry resets a complete corrupt partial.",
            artifact.path
        ));
    }
    Ok(())
}

pub(crate) fn verify(
    path: &Path,
    artifact: &Artifact,
    cancel: &AtomicBool,
) -> Result<bool, String> {
    check_cancel(cancel)?;
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
        Ok(metadata) if !metadata.is_file() => {
            return Err("Refusing non-file model artifact".into());
        }
        Ok(metadata) if metadata.len() != artifact.bytes => return Ok(false),
        Ok(_) => {}
    }
    Ok(hash(path, cancel)?.eq_ignore_ascii_case(&artifact.sha256))
}

/// Caller holds the cache writer lock. A false result retains an invalid partial
/// for the caller's existing retry policy; no completed artifact is touched.
pub(crate) fn publish(
    partial: &Path,
    destination: &Path,
    artifact: &Artifact,
    cancel: &AtomicBool,
) -> Result<bool, String> {
    check_cancel(cancel)?;
    if !verify(partial, artifact, cancel)? {
        return Ok(false);
    }
    fs::OpenOptions::new()
        .write(true)
        .open(partial)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let backup = match fs::symlink_metadata(destination) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
        Ok(metadata) if !metadata.is_file() => {
            return Err("Refusing non-file model destination".into());
        }
        Ok(_) => Some(loop {
            let path = destination.with_extension(format!(
                "corrupt-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break path,
                Err(error) => return Err(error.to_string()),
                Ok(_) => {}
            }
        }),
    };
    // Verification and sync can be lengthy; cancellation must win before commit.
    check_cancel(cancel)?;
    super::files::replace_preserving(partial, destination, backup.as_deref())?;
    Ok(true)
}

/// Streaming transports report cumulative bytes; external transports expose the
/// current staging file. This is a transport policy, not a platform decision.
enum Measurement {
    Cumulative { bytes: AtomicU64, total: u64 },
    StagingFile(Mutex<Option<(PathBuf, u64, u64)>>),
}

pub(crate) struct Progress {
    message: Mutex<String>,
    downloading: AtomicBool,
    network_attempted: AtomicBool,
    indeterminate: AtomicBool,
    measurement: Measurement,
}

impl Progress {
    #[allow(
        dead_code,
        reason = "Used by the streaming transport; both policies are tested on every host"
    )]
    pub fn cumulative(message: String, total: u64) -> Self {
        Self {
            message: Mutex::new(message),
            downloading: AtomicBool::new(false),
            network_attempted: AtomicBool::new(false),
            indeterminate: AtomicBool::new(false),
            measurement: Measurement::Cumulative {
                bytes: AtomicU64::new(0),
                total,
            },
        }
    }

    #[allow(
        dead_code,
        reason = "Used by the external-process transport; both policies are tested on every host"
    )]
    pub fn staging_file(message: String) -> Self {
        Self {
            message: Mutex::new(message),
            downloading: AtomicBool::new(false),
            network_attempted: AtomicBool::new(false),
            indeterminate: AtomicBool::new(false),
            measurement: Measurement::StagingFile(Mutex::new(None)),
        }
    }

    pub fn message(&self, message: String) {
        if let Ok(mut value) = self.message.lock() {
            *value = message;
        }
    }

    #[allow(dead_code, reason = "Used by the streaming transport")]
    pub fn bytes(&self, value: u64) {
        self.indeterminate.store(false, Ordering::Relaxed);
        if let Measurement::Cumulative { bytes, .. } = &self.measurement {
            bytes.store(value, Ordering::Relaxed);
        }
    }

    #[allow(dead_code, reason = "Used by the streaming transport")]
    pub fn complete(&self) {
        if let Measurement::Cumulative { total, .. } = &self.measurement {
            self.bytes(*total);
        }
    }

    #[allow(dead_code, reason = "Used by the external-process transport")]
    pub fn track_file(&self, path: PathBuf, total: u64) {
        if let Measurement::StagingFile(file) = &self.measurement
            && let Ok(mut file) = file.lock()
        {
            *file = Some((path, 0, total));
            self.indeterminate.store(false, Ordering::Relaxed);
        }
    }

    #[allow(dead_code, reason = "Multipart staging is used on macOS")]
    pub fn track_file_range(&self, path: PathBuf, completed: u64, total: u64) {
        if let Measurement::StagingFile(file) = &self.measurement
            && let Ok(mut file) = file.lock()
        {
            *file = Some((path, completed, total));
            self.indeterminate.store(false, Ordering::Relaxed);
        }
    }

    pub fn set_downloading(&self, value: bool) {
        if value {
            self.network_attempted.store(true, Ordering::Release);
        }
        self.downloading.store(value, Ordering::Release);
    }

    pub fn network_attempted(&self) -> bool {
        self.network_attempted.load(Ordering::Acquire)
    }

    pub fn downloading(&self) -> bool {
        self.downloading.load(Ordering::Acquire)
    }

    pub fn snapshot(&self) -> String {
        let message = self
            .message
            .lock()
            .map(|m| m.clone())
            .unwrap_or_else(|_| "Preparing model".into());
        let counts = match &self.measurement {
            _ if self.indeterminate.load(Ordering::Relaxed) => None,
            Measurement::Cumulative { bytes, total } => {
                Some((bytes.load(Ordering::Relaxed), *total))
            }
            Measurement::StagingFile(file) if self.downloading() => {
                // Drop the lock before filesystem IO.
                let file = file.lock().ok().and_then(|file| file.clone());
                file.map(|(path, completed, total)| {
                    (completed + fs::metadata(path).map_or(0, |m| m.len()), total)
                })
            }
            Measurement::StagingFile(_) => None,
        };
        match counts {
            Some((bytes, total)) => format!(
                "{message} — {:.1} / {:.1} MiB",
                bytes as f64 / 1048576.,
                total as f64 / 1048576.
            ),
            None => message,
        }
    }
}

pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("Model preparation cancelled; partial download retained".into())
    } else {
        Ok(())
    }
}

pub(crate) fn hash(path: &Path, cancel: &AtomicBool) -> Result<String, String> {
    check_cancel(cancel)?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    hash_reader(&mut file, cancel)
}

fn hash_reader(file: &mut impl Read, cancel: &AtomicBool) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0; 256 * 1024];
    loop {
        check_cancel(cancel)?;
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:X}", hasher.finalize()))
}

/// One bounded terminal reply, one cancellation owner, one joined thread.
/// Progress representation stays transport-specific (stream bytes vs curl file).
pub(crate) struct Job {
    pub progress: Arc<Progress>,
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<PathBuf, String>>,
    thread: Option<JoinHandle<()>>,
    completed: bool,
}

impl Job {
    pub fn spawn(
        progress: Progress,
        task: impl FnOnce(&AtomicBool, &Progress) -> Result<PathBuf, String> + Send + 'static,
    ) -> Result<Self, String> {
        let progress = Arc::new(progress);
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, result) = mpsc::sync_channel(1);
        let status = progress.clone();
        let stop = cancel.clone();
        let thread = thread::Builder::new()
            .name("model-provisioning".into())
            .spawn(move || {
                struct Finish<'a>(&'a Progress);
                impl Drop for Finish<'_> {
                    fn drop(&mut self) {
                        self.0.set_downloading(false);
                    }
                }
                let finish = Finish(&status);
                let result = task(&stop, &status);
                drop(finish);
                let _ = sender.send(result);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            progress,
            cancel,
            result,
            thread: Some(thread),
            completed: false,
        })
    }
}

impl Job {
    pub fn poll(&mut self) -> Option<Result<PathBuf, String>> {
        if self.completed {
            return None;
        }
        let result = match self.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err("Model preparation thread exited".into()),
        };
        self.completed = true;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        Some(result)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PublicationFixture {
        root: PathBuf,
        partial: PathBuf,
        destination: PathBuf,
        artifact: Artifact,
    }
    impl PublicationFixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "voicepaste-publication-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let partial = root.join("model.bin.partial");
            fs::write(&partial, b"verified weights").unwrap();
            Self {
                destination: root.join("model.bin"),
                root,
                partial,
                artifact: Artifact {
                    path: "model.bin".into(),
                    bytes: 16,
                    sha256: format!("{:X}", Sha256::digest(b"verified weights")),
                },
            }
        }
        fn publish(&self, cancel: bool) -> Result<bool, String> {
            publish(
                &self.partial,
                &self.destination,
                &self.artifact,
                &AtomicBool::new(cancel),
            )
        }
    }
    impl Drop for PublicationFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn pointers_html_and_same_size_corruption_are_never_published() {
        for bytes in [
            b"version https://git-lfs.github.com/spec/v1\noid sha256:fixture\nsize 16\n".as_slice(),
            b"<html>blocked</html>",
            b"corrupt weights!",
        ] {
            let fixture = PublicationFixture::new();
            fs::write(&fixture.partial, bytes).unwrap();
            fs::write(&fixture.destination, b"previous model").unwrap();
            assert!(!fixture.publish(false).unwrap());
            assert_eq!(fs::read(&fixture.destination).unwrap(), b"previous model");
        }
    }

    #[test]
    fn downloaded_bytes_are_published_and_short_partials_resume_at_their_offset() {
        use std::io::{Seek, SeekFrom, Write};
        let fixture = PublicationFixture::new();
        fs::write(&fixture.partial, b"verified").unwrap();
        prepare_artifact(
            (&fixture.partial, &fixture.destination),
            &fixture.artifact,
            &AtomicBool::new(false),
            true,
            |path| {
                fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(path)
                    .map_err(|e| e.to_string())
            },
            |file, offset| {
                assert_eq!(offset, 8);
                file.seek(SeekFrom::Start(offset)).unwrap();
                file.write_all(b" weights").unwrap();
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(fs::read(&fixture.destination).unwrap(), b"verified weights");
        assert!(!fixture.partial.exists());
    }

    #[test]
    fn complete_partial_recovery_needs_neither_staging_creation_nor_network() {
        let fixture = PublicationFixture::new();
        prepare_artifact(
            (&fixture.partial, &fixture.destination),
            &fixture.artifact,
            &AtomicBool::new(false),
            false,
            |_| panic!("offline recovery must not open staging for transport"),
            |_, _| panic!("offline recovery must not download"),
        )
        .unwrap();
        assert_eq!(fs::read(&fixture.destination).unwrap(), b"verified weights");
    }

    #[test]
    fn offline_miss_preserves_corrupt_or_short_partial_without_transport() {
        for bytes in [
            b"short".as_slice(),
            b"corrupt weights!",
            b"oversized corrupt weights",
        ] {
            let fixture = PublicationFixture::new();
            fs::write(&fixture.partial, bytes).unwrap();
            assert!(
                prepare_artifact(
                    (&fixture.partial, &fixture.destination),
                    &fixture.artifact,
                    &AtomicBool::new(false),
                    false,
                    |_| panic!("offline miss must not create staging"),
                    |_, _| panic!("offline miss must not download"),
                )
                .is_err()
            );
            assert_eq!(fs::read(&fixture.partial).unwrap(), bytes);
            assert!(!fixture.destination.exists());
        }
    }

    #[test]
    fn corrupt_complete_and_oversized_partials_restart_before_download() {
        use std::io::Write;
        for bytes in [b"corrupt weights!".as_slice(), b"oversized corrupt weights"] {
            let fixture = PublicationFixture::new();
            fs::write(&fixture.partial, bytes).unwrap();
            prepare_artifact(
                (&fixture.partial, &fixture.destination),
                &fixture.artifact,
                &AtomicBool::new(false),
                true,
                |path| {
                    fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(path)
                        .map_err(|e| e.to_string())
                },
                |file, offset| {
                    assert_eq!(offset, 0);
                    assert_eq!(file.metadata().unwrap().len(), 0);
                    file.write_all(b"verified weights").unwrap();
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(fs::read(&fixture.destination).unwrap(), b"verified weights");
        }
    }

    #[test]
    fn failed_or_cancelled_transfer_keeps_partial_and_previous_destination() {
        use std::io::Write;
        for cancelled in [false, true] {
            let fixture = PublicationFixture::new();
            fs::write(&fixture.partial, b"short").unwrap();
            fs::write(&fixture.destination, b"previous").unwrap();
            let cancel = AtomicBool::new(false);
            let result = prepare_artifact(
                (&fixture.partial, &fixture.destination),
                &fixture.artifact,
                &cancel,
                true,
                |path| {
                    fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(path)
                        .map_err(|e| e.to_string())
                },
                |file, _| {
                    file.set_len(0).unwrap();
                    file.write_all(b"verified weights").unwrap();
                    if cancelled {
                        cancel.store(true, Ordering::Release);
                        Ok(())
                    } else {
                        Err("transport interrupted".into())
                    }
                },
            );
            assert!(result.is_err());
            assert_eq!(fs::read(&fixture.destination).unwrap(), b"previous");
            assert_eq!(fs::read(&fixture.partial).unwrap(), b"verified weights");
        }
    }

    #[test]
    fn publication_preserves_old_artifact_and_commits_verified_bytes() {
        let fixture = PublicationFixture::new();
        fs::write(&fixture.destination, b"old corrupt weights").unwrap();
        assert!(fixture.publish(false).unwrap());
        assert_eq!(fs::read(&fixture.destination).unwrap(), b"verified weights");
        assert!(!fixture.partial.exists());
        let backups: Vec<_> = fs::read_dir(&fixture.root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p != &fixture.destination)
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(&backups[0]).unwrap(), b"old corrupt weights");
    }

    #[test]
    fn publication_creates_missing_destination_without_backup() {
        let fixture = PublicationFixture::new();
        assert!(fixture.publish(false).unwrap());
        assert!(
            verify(
                &fixture.destination,
                &fixture.artifact,
                &AtomicBool::new(false)
            )
            .unwrap()
        );
        assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 1);
    }

    #[test]
    fn cancellation_or_invalid_size_or_hash_never_replaces_completed_artifact() {
        for contents in [b"short".as_slice(), b"corrupt weights!".as_slice()] {
            let fixture = PublicationFixture::new();
            fs::write(&fixture.destination, b"existing").unwrap();
            assert!(fixture.publish(true).unwrap_err().contains("cancelled"));
            fs::write(&fixture.partial, contents).unwrap();
            assert!(!fixture.publish(false).unwrap());
            assert_eq!(fs::read(&fixture.destination).unwrap(), b"existing");
            assert_eq!(fs::read(&fixture.partial).unwrap(), contents);
            assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 2);
        }
    }

    #[test]
    fn publication_refuses_non_file_destination_and_retains_partial() {
        let fixture = PublicationFixture::new();
        fs::create_dir(&fixture.destination).unwrap();
        assert!(fixture.publish(false).is_err());
        assert!(fixture.partial.exists());
        assert!(fixture.destination.is_dir());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_publication_failure_keeps_completed_and_staging_bytes() {
        let fixture = PublicationFixture::new();
        fs::write(&fixture.destination, b"old weights").unwrap();
        let backup = fixture.root.join("occupied-backup");
        fs::write(&backup, b"do not overwrite").unwrap();
        assert!(
            super::super::files::replace_preserving(
                &fixture.partial,
                &fixture.destination,
                Some(&backup)
            )
            .is_err()
        );
        assert_eq!(fs::read(&fixture.destination).unwrap(), b"old weights");
        assert_eq!(fs::read(&fixture.partial).unwrap(), b"verified weights");
        assert_eq!(fs::read(&backup).unwrap(), b"do not overwrite");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn publication_refuses_symbolic_links_without_touching_their_targets() {
        let fixture = PublicationFixture::new();
        let target = fixture.root.join("unrelated");
        fs::write(&target, b"verified weights").unwrap();
        fs::remove_file(&fixture.partial).unwrap();
        std::os::unix::fs::symlink(&target, &fixture.partial).unwrap();
        assert!(fixture.publish(false).is_err());
        assert!(!fixture.destination.exists());
        assert_eq!(fs::read(&target).unwrap(), b"verified weights");
    }

    fn progress() -> Progress {
        Progress::cumulative("Checking fixture".into(), 1)
    }

    #[test]
    fn cumulative_progress_preserves_byte_totals_and_messages() {
        let progress = Progress::cumulative("Checking".into(), 2 * 1048576);
        assert_eq!(progress.snapshot(), "Checking — 0.0 / 2.0 MiB");
        progress.bytes(1048576);
        progress.message("Downloading".into());
        progress.set_downloading(true);
        assert!(progress.downloading());
        assert_eq!(progress.snapshot(), "Downloading — 1.0 / 2.0 MiB");
        progress.complete();
        assert_eq!(progress.snapshot(), "Downloading — 2.0 / 2.0 MiB");
    }

    #[test]
    fn staging_progress_counts_only_active_downloads_and_hashes_real_bytes() {
        let root =
            std::env::temp_dir().join(format!("voicepaste-shared-progress-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _owned = Owned(root.clone());
        let path = root.join("model.partial");
        let data = vec![42; 1048576];
        fs::write(&path, &data).unwrap();
        let progress = Progress::staging_file("Checking".into());
        progress.track_file(path.clone(), 2 * 1048576);
        assert_eq!(progress.snapshot(), "Checking");
        progress.message("Downloading".into());
        progress.set_downloading(true);
        assert_eq!(progress.snapshot(), "Downloading — 1.0 / 2.0 MiB");
        progress.track_file_range(path.clone(), 2 * 1048576, 4 * 1048576);
        assert_eq!(progress.snapshot(), "Downloading — 3.0 / 4.0 MiB");
        progress.track_file(path.clone(), 2 * 1048576);
        assert_eq!(
            hash(&path, &AtomicBool::new(false)).unwrap(),
            format!("{:X}", Sha256::digest(&data))
        );
        progress.set_downloading(false);
        progress.message("Verifying".into());
        assert_eq!(progress.snapshot(), "Verifying");
        fs::remove_file(&path).unwrap();
        progress.set_downloading(true);
        assert_eq!(progress.snapshot(), "Verifying — 0.0 / 2.0 MiB");
    }

    #[test]
    fn completed_jobs_clear_network_activity_on_success_and_failure() {
        for result in [Ok(PathBuf::from("verified")), Err("HTTP failure".into())] {
            let mut job = Job::spawn(progress(), move |_, progress| {
                progress.set_downloading(true);
                result
            })
            .unwrap();
            job.thread.take().unwrap().join().unwrap();
            assert!(!job.progress.downloading());
            assert!(job.poll().is_some());
        }
    }

    #[test]
    fn success_and_failure_are_delivered_exactly_once() {
        for result in [
            Ok(PathBuf::from("verified")),
            Err("integrity failure".into()),
        ] {
            let expected = result.clone();
            let mut job = Job::spawn(progress(), move |_, _| result).unwrap();
            // Deterministically wait for the producer, not a timing-based poll loop.
            job.thread.take().unwrap().join().unwrap();
            assert_eq!(job.poll(), Some(expected));
            assert_eq!(job.poll(), None);
        }
    }

    #[test]
    fn worker_panic_reports_one_failure_instead_of_success() {
        let mut job = Job::spawn(progress(), |_, progress| {
            progress.set_downloading(true);
            panic!("fixture panic")
        })
        .unwrap();
        assert!(job.thread.take().unwrap().join().is_err());
        assert!(!job.progress.downloading());
        assert!(job.poll().unwrap().is_err());
        assert!(job.poll().is_none());
    }

    #[test]
    fn drop_cancels_and_joins_before_releasing_progress() {
        let (started, ready) = mpsc::sync_channel(1);
        let (finished, done) = mpsc::sync_channel(1);
        let job = Job::spawn(progress(), move |cancel, _| {
            started.send(()).unwrap();
            while !cancel.load(Ordering::Acquire) {
                thread::yield_now();
            }
            finished.send(()).unwrap();
            check_cancel(cancel)?;
            unreachable!()
        })
        .unwrap();
        ready
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        drop(job);
        done.try_recv().unwrap();
    }

    #[test]
    fn pending_job_is_nonblocking_and_explicit_cancel_is_observed() {
        let (release, wait) = mpsc::sync_channel(1);
        let mut job = Job::spawn(progress(), move |cancel, _| {
            wait.recv().unwrap();
            check_cancel(cancel)?;
            Ok(PathBuf::new())
        })
        .unwrap();
        assert!(job.poll().is_none());
        job.cancel();
        release.send(()).unwrap();
        job.thread.take().unwrap().join().unwrap();
        assert!(job.poll().unwrap().is_err());
        assert!(job.poll().is_none());
    }

    #[test]
    fn cancelled_hash_does_not_open_a_file() {
        let cancel = AtomicBool::new(true);
        assert!(
            hash(Path::new("nonexistent"), &cancel)
                .unwrap_err()
                .contains("cancelled")
        );
    }
}
