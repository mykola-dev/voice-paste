mod http;
mod verification;
#[cfg(target_os = "windows")]
mod windows;
use super::files;
use super::model_preparation::{Job, Progress, check_cancel, prepare_artifact, verify};
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
use voicepaste_core::models::{Artifact, Model};
pub fn filter(
    settings: &voicepaste_core::settings::Settings,
) -> Result<voicepaste_core::statistics::Filter, String> {
    voicepaste_core::statistics::Filter::from_settings(settings)
}
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
pub(crate) type Preparation = Job;
impl Job {
    pub fn start(root: PathBuf, selected: &str) -> Result<Self, String> {
        Self::start_with_download(root, selected, true)
    }
    pub fn start_with_download(
        root: PathBuf,
        selected: &str,
        allow_network: bool,
    ) -> Result<Self, String> {
        let model = voicepaste_core::models::find(selected)?;
        let progress = Progress::cumulative(
            format!("Checking {}", model.name),
            model.files.iter().map(|f| f.bytes).sum(),
        );
        Self::spawn(progress, move |cancel, progress| {
            ensure(&root, &model, cancel, progress, allow_network)
        })
    }
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
        progress.complete();
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
        let valid = candidate.is_some() && verify(&final_path, artifact, cancel)?;
        if valid {
            verified_files.push(candidate.ok_or("Missing verified model artifact")?);
        } else {
            drop(candidate);
            let partial_path = directory.join(format!("{}.partial", artifact.path));
            prepare_artifact(
                (&partial_path, &final_path),
                artifact,
                cancel,
                allow_network,
                |path| {
                    files::exclusive_options()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(false)
                        .open(path)
                        .map_err(|e| e.to_string())
                },
                |partial, mut offset| {
                    progress.message(format!("Downloading {} / {}", model.name, artifact.path));
                    progress.bytes(completed + offset);
                    let object = format!(
                        "/{}/resolve/{}/{}",
                        model.repository, model.revision, artifact.path
                    );
                    check_cancel(cancel)?;
                    progress.set_downloading(true);
                    crate::adapters::model_preparation::transfer_with_fallback(
                        cancel,
                        partial,
                        |partial| {
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
                            transfer(&mut response, partial, offset, artifact, cancel, |bytes| {
                                progress.bytes(completed + bytes);
                            })?;
                            Ok(())
                        },
                        |partial| {
                            crate::adapters::model_preparation::verify_open(
                                partial, artifact, cancel,
                            )
                        },
                        |partial| {
                            crate::adapters::model_git::download(
                                model,
                                artifact,
                                &partial_path,
                                partial,
                                cancel,
                                progress,
                            )
                        },
                    )?;
                    progress.set_downloading(false);
                    progress.message(format!("Verifying {} / {}", model.name, artifact.path));
                    Ok(())
                },
            )?;
            verified_files.push(files::stable_read(&final_path).map_err(|e| e.to_string())?);
        }
        completed += artifact.bytes;
        progress.bytes(completed);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
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
        let progress = Progress::cumulative(String::new(), bytes.len() as u64);
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
