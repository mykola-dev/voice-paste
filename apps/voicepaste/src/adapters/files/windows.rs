use std::{
    fs::{File, OpenOptions},
    io,
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::Path,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_SHARE_READ, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, ReplaceFileW,
};

pub(in crate::adapters) fn exclusive_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.share_mode(0);
    options
}

pub(in crate::adapters) fn stable_read(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

pub(in crate::adapters) fn replace(source: &Path, destination: &Path) -> Result<(), String> {
    let source = wide(source);
    let destination = wide(destination);
    // SAFETY: live NUL-terminated same-directory paths supplied by the consumer.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

/// Publish a new artifact without overwrite, or atomically retain the old bytes
/// at the consumer-chosen backup path. Not the same policy as config replacement.
pub(in crate::adapters) fn replace_preserving(
    source: &Path,
    destination: &Path,
    backup: Option<&Path>,
) -> Result<(), String> {
    let source = wide(source);
    let destination = wide(destination);
    // SAFETY: live NUL-terminated paths, backup buffer lives through ReplaceFileW.
    let result = unsafe {
        if let Some(backup) = backup {
            let backup = wide(backup);
            ReplaceFileW(
                destination.as_ptr(),
                source.as_ptr(),
                backup.as_ptr(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        } else {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if result == 0 {
        Err(io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}
