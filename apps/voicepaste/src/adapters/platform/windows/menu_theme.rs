//! Best-effort native popup theme. Windows has no documented Win32 popup dark-mode API.
use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::ERROR_SUCCESS,
    System::{
        LibraryLoader::{GetModuleHandleW, GetProcAddress},
        Registry::*,
    },
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        WindowsAndMessaging::*,
    },
};

type SetMode = unsafe extern "system" fn(i32) -> i32;
type Flush = unsafe extern "system" fn();

pub(super) struct MenuTheme {
    previous: i32,
    set: SetMode,
    flush: Flush,
}
impl Drop for MenuTheme {
    fn drop(&mut self) {
        // Restore the process-wide preference once the nested menu loop ends.
        unsafe {
            (self.set)(self.previous);
            (self.flush)();
        }
    }
}

pub(super) fn system_menu_theme() -> Option<MenuTheme> {
    let version = windows_version::OsVersion::current();
    // Ordinal 135 has a different signature on older builds. Future builds
    // outside the verified ABI range fall back to the regular native menu.
    if version.major != 10 || !(18362..=26100).contains(&version.build) {
        return None;
    }
    unsafe {
        let mut contrast: HIGHCONTRASTW = std::mem::zeroed();
        contrast.cbSize = size_of::<HIGHCONTRASTW>() as u32;
        if SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            (&mut contrast as *mut HIGHCONTRASTW).cast(),
            0,
        ) == 0
            || contrast.dwFlags & HCF_HIGHCONTRASTON != 0
        {
            return None;
        }
        let mut light: u32 = 0;
        let mut size = size_of::<u32>() as u32;
        if RegGetValueW(
            HKEY_CURRENT_USER,
            windows_sys::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            windows_sys::w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut light as *mut u32).cast(),
            &mut size,
        ) != ERROR_SUCCESS
        {
            return None;
        }
        let module = GetModuleHandleW(windows_sys::w!("uxtheme.dll"));
        if module.is_null() {
            return None;
        }
        // Undocumented ordinals: use only on known Windows 10/11 builds, and
        // retain a no-op fallback if either entry point is absent.
        let set: SetMode = std::mem::transmute(GetProcAddress(module, 135usize as *const u8)?);
        let flush: Flush = std::mem::transmute(GetProcAddress(module, 136usize as *const u8)?);
        let previous = set(if light == 0 { 2 } else { 3 });
        flush();
        Some(MenuTheme {
            previous,
            set,
            flush,
        })
    }
}
