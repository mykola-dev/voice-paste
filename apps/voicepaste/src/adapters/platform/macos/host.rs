//! Owns one native menu-bar item; callback delivery never enters GPUI/Rust.
use super::menu::{self, MenuItem, TrayAction};
use std::{ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc};

unsafe extern "C" {
    fn vp_menu_create(png: *const u8, length: usize) -> *mut c_void;
    fn vp_menu_add(host: *mut c_void, title: *const std::ffi::c_char, action: u32, enabled: bool);
    fn vp_menu_drain(host: *mut c_void) -> u32;
    fn vp_menu_destroy(host: *mut c_void);
    fn vp_startup_error(message: *const std::ffi::c_char);
    fn vp_capture_menu_enable(host: *mut c_void);
    fn vp_capture_menu_update(
        host: *mut c_void,
        recording: bool,
        busy: bool,
        status: *const std::ffi::c_char,
    );
    fn vp_microphone_permission(host: *mut c_void, request: bool) -> u32;
    fn vp_dictation_menu_enable(host: *mut c_void);
    fn vp_confirm_download(model: *const std::ffi::c_char) -> bool;
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

pub struct Intents {
    pub settings: bool,
    pub quit: bool,
    pub toggle: bool,
    pub cancel: bool,
    pub download: bool,
    pub enable_keys: bool,
}

#[derive(PartialEq, Eq)]
pub enum MicrophonePermission {
    Undecided,
    Granted,
    Denied,
    Pending,
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

    pub fn drain(&self) -> Intents {
        // SAFETY: the retained native host is live and this is its UI thread.
        let flags = unsafe { vp_menu_drain(self.pointer.as_ptr()) };
        Intents {
            settings: flags & 1 != 0,
            quit: flags & 2 != 0,
            toggle: flags & 4 != 0,
            cancel: flags & 8 != 0,
            download: flags & 16 != 0,
            enable_keys: flags & 32 != 0,
        }
    }

    pub fn enable_capture(&self) {
        // SAFETY: live retained main-thread host; only called once in capture mode.
        unsafe {
            vp_capture_menu_enable(self.pointer.as_ptr());
        }
    }
    pub fn enable_dictation(&self) {
        // SAFETY: main-thread live menu owner; menu changes never enter Rust.
        unsafe {
            vp_dictation_menu_enable(self.pointer.as_ptr());
        }
    }
    pub fn confirm_download(&self, model: &str) -> bool {
        let Ok(model) = std::ffi::CString::new(model) else {
            return false;
        };
        // SAFETY: synchronous main-thread native alert; no framework borrow held.
        unsafe { vp_confirm_download(model.as_ptr()) }
    }

    pub fn capture_status(&self, recording: bool, busy: bool, message: &str) {
        let message = message.replace('\0', "");
        if let Ok(message) = std::ffi::CString::new(message) {
            // SAFETY: native adapter copies the string during this UI-thread call.
            unsafe {
                vp_capture_menu_update(self.pointer.as_ptr(), recording, busy, message.as_ptr());
            }
        }
    }

    pub fn microphone_permission(&self, request: bool) -> MicrophonePermission {
        // SAFETY: retained UI-thread host; async completion retains only native
        // state and never calls back through a borrowed Rust pointer.
        match unsafe { vp_microphone_permission(self.pointer.as_ptr(), request) } {
            1 => MicrophonePermission::Granted,
            2 => MicrophonePermission::Denied,
            3 => MicrophonePermission::Pending,
            _ => MicrophonePermission::Undecided,
        }
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
