//! Best-effort bounded diagnostics. No transcript/prompt/audio is added here.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

pub(super) fn log_path() -> PathBuf {
    std::env::temp_dir().join("VoicePaste/voicepaste.log")
}

pub(super) fn log(message: &str) {
    let path = log_path();
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    if fs::metadata(&path).is_ok_and(|m| m.len() > 1024 * 1024) {
        let _ = fs::rename(&path, path.with_extension("previous.log"));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}
