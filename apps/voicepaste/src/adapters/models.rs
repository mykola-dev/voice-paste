mod http;
mod verification;
#[cfg(target_os = "windows")]
mod windows;
use super::files;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};
use voicepaste_core::models::{Artifact, Model};
#[cfg(target_os = "windows")]
use windows::times as receipt_times;

pub fn local_path(root: &Path, selected: &str) -> Result<PathBuf, String> {
    let model = voicepaste_core::models::find(selected)?;
    let directory = directory(root, &model);
    if model
        .files
        .iter()
        .all(|f| fs::metadata(directory.join(&f.path)).is_ok_and(|m| m.len() == f.bytes))
    {
        Ok(directory)
    } else {
        Err(format!(
            "Model {selected} is not ready. Wait for the model download or use Retry model download."
        ))
    }
}
fn directory(root: &Path, model: &Model) -> PathBuf {
    root.join("data/models")
        .join(&model.name)
        .join(&model.revision)
}
fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("Model preparation cancelled; partial download retained".into())
    } else {
        Ok(())
    }
}

pub struct Progress {
    bytes: AtomicU64,
    total: u64,
    message: Mutex<String>,
    downloading: AtomicBool,
}
impl Progress {
    pub fn downloading(&self) -> bool {
        self.downloading.load(Ordering::Acquire)
    }
    fn message(&self, message: String) {
        if let Ok(mut value) = self.message.lock() {
            *value = message;
        }
    }
    pub fn snapshot(&self) -> String {
        let message = self
            .message
            .lock()
            .map(|v| v.clone())
            .unwrap_or_else(|_| "Preparing model".into());
        format!(
            "{message} — {:.1} / {:.1} MiB",
            self.bytes.load(Ordering::Relaxed) as f64 / 1048576.0,
            self.total as f64 / 1048576.0
        )
    }
}

pub struct Preparation {
    pub progress: Arc<Progress>,
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<PathBuf, String>>,
    thread: Option<JoinHandle<()>>,
}
impl Preparation {
    pub fn start(root: PathBuf, selected: &str) -> Result<Self, String> {
        let model = voicepaste_core::models::find(selected)?;
        let progress = Arc::new(Progress {
            bytes: AtomicU64::new(0),
            total: model.files.iter().map(|f| f.bytes).sum(),
            message: Mutex::new(format!("Checking {}", model.name)),
            downloading: AtomicBool::new(false),
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, result) = mpsc::sync_channel(1);
        let task_progress = progress.clone();
        let task_cancel = cancel.clone();
        let thread = thread::Builder::new()
            .name("model-provisioning".into())
            .spawn(move || {
                let result = ensure(&root, &model, &task_cancel, &task_progress, true);
                let _ = sender.send(result);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            progress,
            cancel,
            result,
            thread: Some(thread),
        })
    }
    pub fn poll(&mut self) -> Option<Result<PathBuf, String>> {
        match self.result.try_recv() {
            Ok(result) => {
                if let Some(thread) = self.thread.take() {
                    let _ = thread.join();
                }
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Model preparation thread exited".into()))
            }
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Drop for Preparation {
    fn drop(&mut self) {
        self.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn hash(path: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
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

fn ensure(
    root: &Path,
    model: &Model,
    cancel: &AtomicBool,
    progress: &Progress,
    allow_network: bool,
) -> Result<PathBuf, String> {
    check_cancel(cancel)?;
    let directory = directory(root, model);
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let _lock = files::exclusive_options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(".download.lock"))
        .map_err(|e| format!("Model preparation is locked or unwritable: {e}"))?;
    if verification::Receipt::current(&directory, model) {
        progress.message(format!(
            "Using verified local {} (unchanged files)",
            model.name
        ));
        progress.bytes.store(progress.total, Ordering::Relaxed);
        return Ok(directory);
    }
    let mut completed = 0;
    // Hold successful artifacts read-only through receipt publication so an
    // external writer cannot change bytes between hashing and fingerprinting.
    let mut verified_files = Vec::with_capacity(model.files.len());
    for artifact in &model.files {
        check_cancel(cancel)?;
        let final_path = directory.join(&artifact.path);
        progress.message(format!("Checking {} / {}", model.name, artifact.path));
        let candidate = match files::stable_read(&final_path) {
            Ok(file) => Some(file),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("Cannot read model artifact: {error}")),
        };
        let valid = candidate
            .as_ref()
            .is_some_and(|file| file.metadata().is_ok_and(|m| m.len() == artifact.bytes))
            && hash(&final_path, cancel)?.eq_ignore_ascii_case(&artifact.sha256);
        if valid {
            verified_files.push(candidate.ok_or("Missing verified model artifact")?);
        } else {
            drop(candidate);
            let partial_path = directory.join(format!("{}.partial", artifact.path));
            let mut partial = files::exclusive_options()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&partial_path)
                .map_err(|e| e.to_string())?;
            let mut offset = partial.metadata().map_err(|e| e.to_string())?.len();
            if offset > artifact.bytes {
                partial.set_len(0).map_err(|e| e.to_string())?;
                offset = 0;
            }
            // A complete partial can have survived a crash immediately before
            // publication. Verify it without making a network request.
            let partial_complete = if offset == artifact.bytes {
                drop(partial);
                let valid = hash(&partial_path, cancel)?.eq_ignore_ascii_case(&artifact.sha256);
                partial = files::exclusive_options()
                    .read(true)
                    .write(true)
                    .open(&partial_path)
                    .map_err(|e| e.to_string())?;
                if !valid {
                    partial.set_len(0).map_err(|e| e.to_string())?;
                    offset = 0;
                }
                valid
            } else {
                false
            };
            if !partial_complete {
                progress.message(format!("Downloading {} / {}", model.name, artifact.path));
                progress.bytes.store(completed + offset, Ordering::Relaxed);
                let object = format!(
                    "/{}/resolve/{}/{}",
                    model.repository, model.revision, artifact.path
                );
                check_cancel(cancel)?;
                if !allow_network {
                    return Err("Model is incomplete and networking is disabled".into());
                }
                progress.downloading.store(true, Ordering::Release);
                let mut response =
                    http::Response::get("huggingface.co", 443, &object, true, offset)?;
                if response.status == 200 {
                    offset = 0;
                    partial.set_len(0).map_err(|e| e.to_string())?;
                } else if response.status != 206 {
                    return Err(format!(
                        "Download HTTP {} for {}",
                        response.status, artifact.path
                    ));
                }
                if response.status == 206 {
                    response.validate_range(offset, artifact.bytes)?;
                }
                partial
                    .seek(SeekFrom::Start(offset))
                    .map_err(|e| e.to_string())?;
                transfer(
                    &mut response,
                    &mut partial,
                    offset,
                    artifact,
                    cancel,
                    |bytes| {
                        progress.bytes.store(completed + bytes, Ordering::Relaxed);
                    },
                )?;
            }
            partial.sync_all().map_err(|e| e.to_string())?;
            drop(partial);
            progress.message(format!("Verifying {} / {}", model.name, artifact.path));
            if !hash(&partial_path, cancel)?.eq_ignore_ascii_case(&artifact.sha256) {
                // This is our own failed partial, not a user's completed model.
                fs::remove_file(&partial_path).map_err(|e| e.to_string())?;
                return Err(format!(
                    "SHA-256 mismatch for {}; retry downloads a fresh copy",
                    artifact.path
                ));
            }
            check_cancel(cancel)?;
            publish(&partial_path, &final_path)?;
            verified_files.push(files::stable_read(&final_path).map_err(|e| e.to_string())?);
        }
        completed += artifact.bytes;
        progress.bytes.store(completed, Ordering::Relaxed);
    }
    verification::Receipt::inspect(&directory, model)
        .ok_or("Model files changed during verification")?
        .save(&directory)?;
    progress.message(format!("Verified local {}", model.name));
    Ok(directory)
}

fn transfer(
    reader: &mut impl Read,
    writer: &mut impl Write,
    mut offset: u64,
    artifact: &Artifact,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<(), String> {
    let mut buffer = [0; 64 * 1024];
    loop {
        check_cancel(cancel)?;
        let count = reader
            .read(&mut buffer)
            .map_err(|e| format!("Download interrupted (partial retained): {e}"))?;
        if count == 0 {
            break;
        }
        if offset + count as u64 > artifact.bytes {
            return Err("Download exceeds manifest size".into());
        }
        writer
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        offset += count as u64;
        progress(offset);
    }
    if offset != artifact.bytes {
        return Err("Incomplete download; partial retained for resume".into());
    }
    Ok(())
}

fn publish(partial: &Path, final_path: &Path) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    // Model policy retains a corrupt completed artifact rather than overwriting
    // it blindly. Native replacement mechanics belong to the files adapter.
    let backup = if final_path.exists() {
        Some(loop {
            let candidate = final_path.with_extension(format!(
                "corrupt-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            if !candidate.exists() {
                break candidate;
            }
        })
    } else {
        None
    };
    files::replace_preserving(partial, final_path, backup.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_partial_is_verified_and_published_without_network() {
        let root = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join(format!("model-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        struct OwnedTestRoot(PathBuf);
        impl Drop for OwnedTestRoot {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _owned = OwnedTestRoot(root.clone());
        let bytes = b"offline local fixture";
        let model = Model {
            name: "fixture".into(),
            repository: "no-network-is-permitted-in-this-test".into(),
            revision: "pinned".into(),
            files: vec![Artifact {
                path: "model.bin".into(),
                bytes: bytes.len() as u64,
                sha256: format!("{:X}", Sha256::digest(bytes)),
            }],
        };
        let target = directory(&root, &model);
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("model.bin.partial"), bytes).unwrap();
        let progress = Progress {
            bytes: AtomicU64::new(0),
            total: bytes.len() as u64,
            message: Mutex::new(String::new()),
            downloading: AtomicBool::new(false),
        };
        let cancelled = AtomicBool::new(false);
        assert_eq!(
            ensure(&root, &model, &cancelled, &progress, false).unwrap(),
            target
        );
        assert_eq!(fs::read(target.join("model.bin")).unwrap(), bytes);
        assert!(!target.join("model.bin.partial").exists());
        assert!(verification::Receipt::current(&target, &model));
        // An already-complete local model also succeeds offline.
        assert!(ensure(&root, &model, &cancelled, &progress, false).is_ok());
        assert!(!progress.downloading());
        // A same-size replacement and a changed pinned hash both invalidate.
        fs::write(target.join("model.bin"), vec![b'x'; bytes.len()]).unwrap();
        assert!(!verification::Receipt::current(&target, &model));
        assert!(ensure(&root, &model, &cancelled, &progress, false).is_err());
        fs::write(target.join("model.bin.partial"), bytes).unwrap();
        ensure(&root, &model, &cancelled, &progress, false).unwrap();
        let mut changed_manifest = model.clone();
        changed_manifest.files[0].sha256 = "different".into();
        assert!(!verification::Receipt::current(&target, &changed_manifest));
        fs::write(
            target.join("model.bin"),
            b"broken model preserved as backup",
        )
        .unwrap();
        fs::write(target.join("model.bin.partial"), bytes).unwrap();
        assert!(ensure(&root, &model, &cancelled, &progress, false).is_ok());
        let backup_preserved = fs::read_dir(&target)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .any(|path| {
                path.to_string_lossy().contains(".corrupt-")
                    && fs::read(path).unwrap() == b"broken model preserved as backup"
            });
        assert!(backup_preserved);
        fs::write(target.join(".verified.json"), b"interrupted cache").unwrap();
        assert!(!verification::Receipt::current(&target, &model));
        ensure(&root, &model, &cancelled, &progress, false).unwrap();
        assert!(verification::Receipt::current(&target, &model));
    }
    #[test]
    fn transfer_resumes_and_rejects_wrong_lengths_and_cancellation() {
        let artifact = Artifact {
            path: "fixture".into(),
            bytes: 6,
            sha256: String::new(),
        };
        let cancel = AtomicBool::new(false);
        let mut result = b"abc".to_vec();
        transfer(
            &mut b"def".as_slice(),
            &mut result,
            3,
            &artifact,
            &cancel,
            |_| {},
        )
        .unwrap();
        assert_eq!(result, b"abcdef");
        assert!(
            transfer(
                &mut b"de".as_slice(),
                &mut Vec::new(),
                3,
                &artifact,
                &cancel,
                |_| {}
            )
            .is_err()
        );
        assert!(
            transfer(
                &mut b"defg".as_slice(),
                &mut Vec::new(),
                3,
                &artifact,
                &cancel,
                |_| {}
            )
            .is_err()
        );
        cancel.store(true, Ordering::Relaxed);
        assert!(
            transfer(
                &mut b"def".as_slice(),
                &mut Vec::new(),
                3,
                &artifact,
                &cancel,
                |_| {}
            )
            .is_err()
        );
    }
}
