//! Pinned GGML provisioning. Only an explicit download command enables network.
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use voicepaste_core::{
    models::{Artifact, Model},
    settings::SettingsPlatform,
};

fn selected(name: &str) -> Result<Model, String> {
    voicepaste_core::models::find_for(name, SettingsPlatform::Macos)
}
fn directory(root: &Path, model: &Model) -> PathBuf {
    root.join("data/models-ggml")
        .join(&model.name)
        .join(&model.revision)
}
pub fn local_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    let model = selected(name)?;
    let directory = directory(root, &model);
    let vad = voicepaste_core::models::macos_vad()?;
    if fs::symlink_metadata(directory.join("vad.bin"))
        .is_ok_and(|m| m.is_file() && m.len() == vad.files[0].bytes)
        && model.files.iter().all(|file| {
            fs::metadata(directory.join(&file.path))
                .is_ok_and(|m| m.is_file() && m.len() == file.bytes)
        })
    {
        Ok(directory)
    } else {
        Err(
            "Model is not ready. Use Download selected model in the menu (approval required)."
                .into(),
        )
    }
}
pub fn filter(
    settings: &voicepaste_core::settings::Settings,
) -> Result<voicepaste_core::statistics::Filter, String> {
    voicepaste_core::statistics::Filter::for_platform(settings, SettingsPlatform::Macos)
}
pub struct Progress {
    message: Mutex<String>,
    downloading: AtomicBool,
}
impl Progress {
    pub fn snapshot(&self) -> String {
        self.message
            .lock()
            .map(|m| m.clone())
            .unwrap_or_else(|_| "Preparing model".into())
    }
    pub fn downloading(&self) -> bool {
        self.downloading.load(Ordering::Acquire)
    }
    fn set(&self, text: String) {
        if let Ok(mut value) = self.message.lock() {
            *value = text;
        }
    }
}
pub struct Preparation {
    pub progress: Arc<Progress>,
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<PathBuf, String>>,
    thread: Option<JoinHandle<()>>,
}
impl Preparation {
    pub fn start(root: PathBuf, name: &str) -> Result<Self, String> {
        Self::start_with_download(root, name, false)
    }
    pub fn start_with_download(root: PathBuf, name: &str, download: bool) -> Result<Self, String> {
        let model = selected(name)?;
        let progress = Arc::new(Progress {
            message: Mutex::new(format!("Checking {}", model.name)),
            downloading: AtomicBool::new(false),
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, result) = mpsc::sync_channel(1);
        let status = progress.clone();
        let stop = cancel.clone();
        let thread = thread::Builder::new()
            .name("ggml-provisioning".into())
            .spawn(move || {
                let result = prepare(&root, &model, download, &stop, &status);
                status.downloading.store(false, Ordering::Release);
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
            Err(_) => Some(Err("Model provisioning worker exited".into())),
        }
    }
}
impl Drop for Preparation {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn check(stop: &AtomicBool) -> Result<(), String> {
    if stop.load(Ordering::Acquire) {
        Err("Model preparation cancelled; partial retained".into())
    } else {
        Ok(())
    }
}
fn verify(path: &Path, artifact: &Artifact, stop: &AtomicBool) -> Result<bool, String> {
    if !fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && m.len() == artifact.bytes) {
        return Ok(false);
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 256 * 1024];
    loop {
        check(stop)?;
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&artifact.sha256))
}
fn prepare(
    root: &Path,
    model: &Model,
    download: bool,
    stop: &AtomicBool,
    status: &Progress,
) -> Result<PathBuf, String> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let directory = directory(root, model);
    for ancestor in directory.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Refusing symbolic link in model cache".into());
        }
    }
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .map_err(|e| e.to_string())?;
    if fs::symlink_metadata(directory.join(".download.lock"))
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Refusing symbolic-link model lock".into());
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(directory.join(".download.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock()
        .map_err(|e| format!("Model is locked: {e}"))?;
    for artifact in &model.files {
        check(stop)?;
        let path = directory.join(&artifact.path);
        status.set(format!("Verifying {} / {}", model.name, artifact.path));
        if verify(&path, artifact, stop)? {
            continue;
        }
        if !download {
            return Err(format!(
                "{} is missing or unverified. Use Download selected model in the menu.",
                model.name
            ));
        }
        let partial = directory.join(format!("{}.partial", artifact.path));
        if fs::symlink_metadata(&partial).is_ok_and(|m| !m.is_file()) {
            return Err("Refusing non-file model staging path".into());
        }
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&partial)
            .map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > artifact.bytes {
            file.set_len(0).map_err(|e| e.to_string())?;
        }
        drop(file);
        if !verify(&partial, artifact, stop)? {
            if fs::metadata(&partial).is_ok_and(|m| m.len() == artifact.bytes) {
                OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(&partial)
                    .map_err(|e| e.to_string())?;
            }
            status.downloading.store(true, Ordering::Release);
            status.set(format!(
                "Downloading {} ({:.0} MiB); resumable",
                model.name,
                artifact.bytes as f64 / 1048576.
            ));
            let url = format!(
                "https://huggingface.co/{}/resolve/{}/{}",
                model.repository, model.revision, artifact.path
            );
            let mut child = Command::new("/usr/bin/curl")
                .args([
                    "--fail",
                    "--location",
                    "--silent",
                    "--show-error",
                    "--proto",
                    "=https",
                    "--proto-redir",
                    "=https",
                    "--connect-timeout",
                    "15",
                    "--speed-time",
                    "30",
                    "--speed-limit",
                    "1024",
                    "--continue-at",
                    "-",
                    "--output",
                ])
                .arg(&partial)
                .arg(&url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
            let result = loop {
                if stop.load(Ordering::Acquire) {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err("Download cancelled; partial retained".into());
                }
                match child.try_wait() {
                    Ok(Some(exit)) => {
                        break if exit.success() {
                            Ok(())
                        } else {
                            Err(format!(
                                "Model download failed ({exit}); partial retained. Corporate network may block Hugging Face weight endpoints."
                            ))
                        };
                    }
                    Ok(None) => thread::sleep(Duration::from_millis(50)),
                    Err(e) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(e.to_string());
                    }
                }
            };
            status.downloading.store(false, Ordering::Release);
            result?;
        }
        if !verify(&partial, artifact, stop)? {
            return Err(
                "Downloaded model failed exact size/SHA-256 verification; not published".into(),
            );
        }
        fs::rename(&partial, &path).map_err(|e| e.to_string())?;
    }
    // The worker expects the pinned VAD beside each model. Hard-linking is not
    // required; a subdirectory preserves the independent VAD manifest/revision.
    if model.name != "silero-v6.2.0" {
        let vad = voicepaste_core::models::macos_vad()?;
        let vad_directory = prepare(root, &vad, download, stop, status)?;
        let source = vad_directory.join(&vad.files[0].path);
        let destination = directory.join("vad.bin");
        if !verify(&destination, &vad.files[0], stop)? {
            let temp = directory.join("vad.bin.partial");
            if fs::symlink_metadata(&temp).is_ok_and(|m| !m.is_file()) {
                return Err("Refusing non-file VAD staging path".into());
            }
            fs::copy(source, &temp).map_err(|e| e.to_string())?;
            fs::rename(temp, destination).map_err(|e| e.to_string())?;
        }
    }
    Ok(directory)
}
