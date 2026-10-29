//! Model HTTP transport selection. Provisioning owns range/resume/error policy.
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub(super) use windows::Response;

fn validate_range(value: &str, offset: u64, total: u64) -> Result<(), String> {
    if value != format!("bytes {offset}-{}/{total}", total.saturating_sub(1)) {
        return Err("Server returned an unexpected Content-Range; partial retained".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resumed_download_requires_the_exact_pinned_range() {
        assert!(validate_range("bytes 3-5/6", 3, 6).is_ok());
        for value in [
            "bytes 0-5/6",
            "bytes 3-4/6",
            "bytes 3-5/7",
            "bytes 3-5/*",
            "items 3-5/6",
            "",
        ] {
            assert!(validate_range(value, 3, 6).is_err(), "{value}");
        }
    }
}
