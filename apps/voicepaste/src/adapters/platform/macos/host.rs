//! Owns one native menu-bar item; callback delivery never enters GPUI/Rust.
use super::menu::{self, MenuItem, TrayAction};
use std::{ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc};

unsafe extern "C" {
    fn vp_menu_create(png: *const u8, length: usize) -> *mut c_void;
    fn vp_menu_add(host: *mut c_void, title: *const std::ffi::c_char, action: u32, enabled: bool);
    fn vp_menu_drain(host: *mut c_void) -> u32;
    fn vp_menu_destroy(host: *mut c_void);
    fn vp_host_resident_bytes() -> u64;
    fn vp_startup_error(message: *const std::ffi::c_char);
}

pub(super) fn startup_error(message: &str) {
    let message: String = message
        .chars()
        .filter(|character| *character != '\0')
        .take(4096)
        .collect();
    if let Ok(message) = std::ffi::CString::new(message) {
        // SAFETY: process entry point calls this on the main thread. AppKit
        // copies the valid UTF-8 message for its startup-only modal alert.
        unsafe {
            vp_startup_error(message.as_ptr());
        }
    }
}

pub struct Host {
    pointer: NonNull<c_void>,
    // AppKit owns UI-thread callbacks; never transfer this handle to a worker.
    _main_thread: PhantomData<Rc<()>>,
}

impl Host {
    pub fn new() -> Result<Self, String> {
        let icon = include_bytes!("../../../../assets/tray/masters/lips-bitten.png");
        // SAFETY: called from GPUI's main-thread session; native adapter copies
        // the static PNG bytes and returns one retained object owned by Host.
        let pointer = unsafe { vp_menu_create(icon.as_ptr(), icon.len()) };
        let host = Self {
            pointer: NonNull::new(pointer).ok_or("Cannot create the macOS menu-bar icon")?,
            _main_thread: PhantomData,
        };
        for item in menu::items(voicepaste_core::State::Idle) {
            match item {
                MenuItem::Separator => {
                    // SAFETY: live UI-thread host; null title denotes separator.
                    unsafe {
                        vp_menu_add(host.pointer.as_ptr(), std::ptr::null(), 0, false);
                    }
                }
                MenuItem::Command {
                    action,
                    label,
                    enabled,
                } => {
                    let title = std::ffi::CString::new(label).map_err(|e| e.to_string())?;
                    let code = match action {
                        TrayAction::Settings => 1,
                        TrayAction::Quit => 2,
                        TrayAction::Toggle => 4,
                    };
                    // SAFETY: native adapter copies the label synchronously. The
                    // shared recording command stays disabled until connected.
                    unsafe {
                        vp_menu_add(
                            host.pointer.as_ptr(),
                            title.as_ptr(),
                            code,
                            enabled && action != TrayAction::Toggle,
                        );
                    }
                }
            }
        }
        Ok(host)
    }

    pub fn drain(&self) -> (bool, bool) {
        // SAFETY: the retained native host is live and this is its UI thread.
        let flags = unsafe { vp_menu_drain(self.pointer.as_ptr()) };
        (flags & 1 != 0, flags & 2 != 0)
    }

    pub fn memory_status(&self) -> String {
        // SAFETY: read-only task_info query for this process; no native pointer.
        let bytes = unsafe { vp_host_resident_bytes() };
        let host = if bytes == u64::MAX {
            "unavailable".into()
        } else {
            format!("{:.1} MiB resident", bytes as f64 / 1048576.)
        };
        format!("Host RAM: {host}\nSTT worker: not connected yet")
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // SAFETY: exactly one owner; callbacks are detached before native release.
        unsafe {
            vp_menu_destroy(self.pointer.as_ptr());
        }
    }
}
