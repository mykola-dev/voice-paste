//! Pinned GGML provisioning. Only an explicit download command enables network.
use crate::adapters::model_preparation::{
    Job, Progress, check_cancel as check, prepare_artifact, publish, verify,
};
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
#[cfg(test)]
use voicepaste_core::models::Artifact;
use voicepaste_core::{models::Model, settings::SettingsPlatform};

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
pub(crate) type Preparation = Job;
impl Job {
    pub fn start(root: PathBuf, name: &str) -> Result<Self, String> {
        Self::start_with_download(root, name, false)
    }
    pub fn start_with_download(root: PathBuf, name: &str, download: bool) -> Result<Self, String> {
        let model = selected(name)?;
        let progress = Progress::staging_file(format!("Checking {}", model.name));
        Self::spawn(progress, move |stop, status| {
            prepare(&root, &model, download, stop, status)
        })
    }
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
        status.message(format!("Verifying {} / {}", model.name, artifact.path));
        if verify(&path, artifact, stop)? {
            continue;
        }
        let partial = directory.join(format!("{}.partial", artifact.path));
        prepare_artifact(
            (&partial, &path), artifact, stop, download,
            |path| OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string()),
            |_file, _offset| {
            status.track_file(partial.clone(), artifact.bytes);
            status.set_downloading(true);
            status.message(format!(
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
                .arg("--max-filesize")
                .arg(artifact.bytes.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
            let result = loop {
                if fs::metadata(&partial).is_ok_and(|m| m.len() > artifact.bytes) {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(
                        "Download exceeded the pinned artifact size; partial was not published"
                            .into(),
                    );
                }
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
            status.set_downloading(false);
            result?;
            status.message(format!("Verifying {} / {}", model.name, artifact.path));
            Ok(())
            },
        ).map_err(|error| format!("{error} Use Download selected model in the menu to retry."))?;
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
            if !publish(&temp, &destination, &vad.files[0], stop)? {
                return Err("VAD staging copy failed verification; not published".into());
            }
        }
    }
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_preparation_verifies_complete_partials_rejects_corruption_and_respects_lock() {
        let root =
            std::env::temp_dir().join(format!("voicepaste-ggml-offline-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _owner = Owned(root.clone());
        let bytes = b"fixture weights, not an actual model";
        let model = Model {
            name: "silero-v6.2.0".into(),
            repository: "never/requested".into(),
            revision: "fixture".into(),
            files: vec![Artifact {
                path: "test.bin".into(),
                bytes: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(bytes)),
            }],
        };
        let directory = directory(&root, &model);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("test.bin");
        let partial = directory.join("test.bin.partial");
        fs::write(&partial, bytes).unwrap();
        let status = Progress::staging_file(String::new());
        let stop = AtomicBool::new(false);
        assert_eq!(
            prepare(&root, &model, false, &stop, &status).unwrap(),
            directory
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(!partial.exists());
        assert!(!status.downloading());
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.join(".download.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        assert!(prepare(&root, &model, false, &stop, &status).is_err());
        drop(lock);
        let corrupt = vec![0; bytes.len()];
        fs::write(&path, &corrupt).unwrap();
        assert!(prepare(&root, &model, false, &stop, &status).is_err());
        assert_eq!(fs::read(&path).unwrap(), corrupt);
        assert!(
            !partial.exists(),
            "offline misses must not create a network staging file"
        );
        fs::write(&partial, bytes).unwrap();
        stop.store(true, Ordering::Release);
        assert!(prepare(&root, &model, false, &stop, &status).is_err());
        assert_eq!(fs::read(&path).unwrap(), corrupt);
        assert_eq!(fs::read(&partial).unwrap(), bytes);
        stop.store(false, Ordering::Release);
        prepare(&root, &model, false, &stop, &status).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let backups: Vec<_> = fs::read_dir(&directory)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|e| e.to_string_lossy().starts_with("corrupt-"))
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(&backups[0]).unwrap(), corrupt);
    }
}
