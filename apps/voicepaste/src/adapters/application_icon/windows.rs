//! Stable application identity, independent of notification-area state.
use std::ptr::null;
use windows_sys::Win32::{
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{HICON, LoadIconW},
};

/// Resource 1 is also used by Explorer and the pinned GPUI Windows backend.
/// LoadIcon returns a shared resource handle; callers must not destroy it.
pub fn load() -> Result<HICON, String> {
    // SAFETY: resource 1 is embedded by resources.rc in this executable.
    let icon = unsafe { LoadIconW(GetModuleHandleW(null()), std::ptr::without_provenance(1)) };
    if icon.is_null() {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(icon)
    }
}
