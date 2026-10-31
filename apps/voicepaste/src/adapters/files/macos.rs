use std::{fs, path::Path};

pub(in crate::adapters) fn private_options() -> fs::OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = fs::OpenOptions::new();
    options.mode(0o600);
    options
}

/// Cache writers hold the model lock. Retain the old inode before atomic rename;
/// a failed rename leaves both the completed artifact and staging bytes intact.
pub(in crate::adapters) fn replace_preserving(
    source: &Path,
    destination: &Path,
    backup: Option<&Path>,
) -> Result<(), String> {
    if let Some(backup) = backup {
        fs::hard_link(destination, backup).map_err(|e| e.to_string())?;
        fs::rename(source, destination).map_err(|e| e.to_string())?;
    } else {
        // Unlike rename, link refuses an unexpectedly existing destination.
        fs::hard_link(source, destination).map_err(|e| e.to_string())?;
        fs::remove_file(source).map_err(|e| e.to_string())?;
    }
    let parent = destination
        .parent()
        .ok_or("Missing model cache directory")?;
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| e.to_string())
}
