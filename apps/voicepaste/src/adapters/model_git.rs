//! Shared pinned Git/LFS transfer. Only native child ownership is target-selected.
#[cfg(target_os = "macos")]
#[path = "model_git/macos.rs"]
mod native;
#[cfg(target_os = "windows")]
#[path = "model_git/windows.rs"]
mod native;
use crate::adapters::model_preparation::{Progress, check_cancel, verify};
use native::Process;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};
use voicepaste_core::models::{Artifact, Model};

fn run(directory: &Path, args: &[&str], stop: &AtomicBool) -> Result<(), String> {
    check_cancel(stop)?;
    let log = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut process = Process::spawn(
        Command::new("git")
            .current_dir(directory)
            .args([
                "-c",
                &format!(
                    "core.hooksPath={}",
                    directory.join(".git/no-hooks").display()
                ),
                "-c",
                "credential.interactive=never",
                "-c",
                "http.connectTimeout=15",
                "-c",
                "http.lowSpeedLimit=1024",
                "-c",
                "http.lowSpeedTime=30",
                "-c",
                "lfs.dialtimeout=15",
                "-c",
                "lfs.activitytimeout=30",
                "-c",
                "lfs.transfer.maxretries=1",
                "-c",
                "lfs.fetchrecentalways=false",
                "-c",
                "lfs.storage=lfs",
            ])
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_LFS_SKIP_SMUDGE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log.try_clone().map_err(|e| e.to_string())?),
    )
    .map_err(|e| format!("Could not start Git: {e}. Git and Git LFS must already be installed."))?;
    let started = Instant::now();
    loop {
        check_cancel(stop)?;
        if log.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024 {
            return Err("Git diagnostic output exceeded 64 KiB; transfer stopped".into());
        }
        if started.elapsed() > Duration::from_secs(30 * 60) {
            return Err("Git transfer deadline exceeded (30 minutes)".into());
        }
        match process.try_wait().map_err(|e| e.to_string())? {
            Some(exit) if exit.success() => return Ok(()),
            Some(exit) => {
                use std::io::{Seek, SeekFrom};
                let mut log = log;
                log.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
                let mut detail = String::new();
                log.take(4096).read_to_string(&mut detail).ok();
                return Err(format!(
                    "Git {exit}: {}. Git LFS must already be installed; no tools were installed automatically.",
                    detail.trim()
                ));
            }
            None => thread::sleep(Duration::from_millis(50)),
        }
    }
}

pub(crate) fn download(
    model: &Model,
    artifact: &Artifact,
    partial: &Path,
    output: &mut File,
    stop: &AtomicBool,
    status: &Progress,
) -> Result<(), String> {
    check_cancel(stop)?;
    status.clear_tracked_file();
    status.message(format!(
        "HTTPS unavailable or invalid; trying Hugging Face Git LFS: {}",
        artifact.path
    ));
    // Isolated source staging: never append Git bytes to an HTTP partial. TempDir
    // owns cleanup, while the existing HTTP partial survives failures/cancel.
    let staging = tempfile::Builder::new()
        .prefix(".git-download-")
        .tempdir_in(partial.parent().ok_or("Missing cache directory")?)
        .map_err(|e| e.to_string())?;
    let dir = staging.path();
    run(dir, &["lfs", "version"], stop)?;
    run(dir, &["init"], stop)?;
    let url = format!("https://huggingface.co/{}", model.repository);
    run(dir, &["remote", "add", "origin", &url], stop)?;
    run(
        dir,
        &["fetch", "--depth=1", "origin", &model.revision],
        stop,
    )?;
    run(dir, &["checkout", "--detach", "FETCH_HEAD"], stop)?;
    status.message(format!(
        "Downloading {} via Hugging Face Git LFS ({:.0} MiB)",
        artifact.path,
        artifact.bytes as f64 / 1048576.
    ));
    run(
        dir,
        &[
            "lfs",
            "fetch",
            "--include",
            &artifact.path,
            "--exclude",
            "",
            "origin",
            &model.revision,
        ],
        stop,
    )?;
    run(dir, &["lfs", "checkout", &artifact.path], stop)?;
    let source = dir.join(&artifact.path);
    status.track_file(source.clone(), artifact.bytes);
    status.message(format!("Verifying Git LFS / {}", artifact.path));
    copy_verified(&source, artifact, output, stop)?;
    status.track_file(partial.to_owned(), artifact.bytes);
    Ok(())
}

fn copy_verified(
    source: &Path,
    artifact: &Artifact,
    output: &mut File,
    stop: &AtomicBool,
) -> Result<(), String> {
    if !verify(source, artifact, stop)? {
        return Err("Git returned invalid weights or an LFS pointer; not published".into());
    }
    check_cancel(stop)?;
    // Copy only verified source bytes, starting from an empty HTTP staging file.
    // Using the caller's handle preserves Windows exclusive-sharing guarantees.
    let mut source = File::open(source).map_err(|e| e.to_string())?;
    output.set_len(0).map_err(|e| e.to_string())?;
    output.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut buffer = [0; 64 * 1024];
    loop {
        check_cancel(stop)?;
        let count = source.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_copy_replaces_http_staging_without_mixing_sources() {
        use sha2::{Digest, Sha256};
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("git.bin");
        let bytes = b"verified Git model weights";
        std::fs::write(&source, bytes).unwrap();
        let artifact = Artifact {
            path: "git.bin".into(),
            bytes: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        };
        let mut output = tempfile::tempfile().unwrap();
        output
            .write_all(b"old HTTP bytes, longer than the model")
            .unwrap();
        let stop = AtomicBool::new(false);
        copy_verified(&source, &artifact, &mut output, &stop).unwrap();
        assert!(
            crate::adapters::model_preparation::verify_open(&mut output, &artifact, &stop).unwrap()
        );
        std::fs::write(&source, b"version https://git-lfs.github.com/spec/v1").unwrap();
        assert!(copy_verified(&source, &artifact, &mut output, &stop).is_err());
        assert!(
            crate::adapters::model_preparation::verify_open(&mut output, &artifact, &stop).unwrap()
        );
        assert!(copy_verified(&source, &artifact, &mut output, &AtomicBool::new(true)).is_err());
    }

    #[test]
    fn pre_cancelled_git_never_starts_and_git_errors_are_reported() {
        let root = tempfile::tempdir().unwrap();
        assert!(
            run(root.path(), &["--version"], &AtomicBool::new(true))
                .unwrap_err()
                .contains("cancelled")
        );
        let error = run(
            root.path(),
            &["voicepaste-nonexistent-command"],
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(error.contains("not a git command"), "{error}");
        assert!(error.len() < 5000);
    }
}
