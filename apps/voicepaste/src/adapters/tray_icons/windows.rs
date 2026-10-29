//! Native DPI query and icon allocation; artwork/state/mask policy stays shared.
use super::{Lips, canonical_size, icon_data, icon_size_for_dpi};
use windows_sys::Win32::{
    Foundation::{HINSTANCE, HWND},
    Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromWindow},
    UI::{
        HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
        WindowsAndMessaging::{CreateIcon, HICON},
    },
};

pub(in crate::adapters) fn system_icon_size(hwnd: HWND) -> usize {
    // SAFETY: transient UI-thread monitor/DPI query, no owned resource.
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let (mut x, mut y) = (96, 96);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) == 0 {
            return canonical_size(icon_size_for_dpi(x));
        }
    }
    16
}

pub(in crate::adapters) fn create_icon(
    instance: HINSTANCE,
    size: usize,
    state: Lips,
) -> Result<HICON, String> {
    let (edge, pixels, mask) = icon_data(size, state);
    // SAFETY: buffers exactly match the declared icon dimensions and bit depth.
    let handle = unsafe {
        CreateIcon(
            instance,
            edge as i32,
            edge as i32,
            1,
            32,
            mask.as_ptr(),
            pixels.as_ptr(),
        )
    };
    if handle.is_null() {
        Err("Cannot create tray icon".into())
    } else {
        Ok(handle)
    }
}
