//! Main-thread tap owner and adapter-local focus snapshots.
use crate::adapters::paste::FocusTarget;
use std::{ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc};
use voicepaste_core::settings::{Hotkey, Settings};
unsafe extern "C" {
    fn vp_input_create(key: u16, prompt: bool) -> *mut c_void;
    fn vp_input_arm(input: *mut c_void, recording: bool);
    fn vp_input_active(input: *mut c_void, active: bool);
    fn vp_input_drain(input: *mut c_void, pids: *mut i32, flags: *mut u32) -> u32;
    fn vp_input_destroy(input: *mut c_void);
}
pub struct Input {
    pointer: NonNull<c_void>,
    _main: PhantomData<Rc<()>>,
}
pub struct Batch {
    pub targets: Vec<FocusTarget>,
    pub cancel: bool,
    pub warning: bool,
}
impl Input {
    pub fn attach(settings: &Settings, prompt: bool) -> Result<Self, String> {
        let key = match Hotkey::parse(&settings.hotkey)?.key {
            0xA3 => 62,
            0xA5 => 61,
            0x77 => 100,
            0x7B => 111,
            _ => return Err("Unsupported macOS recording key".into()),
        };
        // SAFETY: UI-thread creation transfers one native owner; callback never
        // calls Rust. Permission denial returns null without installing a tap.
        let pointer = unsafe { vp_input_create(key, prompt) };
        Ok(Self {pointer:NonNull::new(pointer).ok_or("Global recording keys require Accessibility permission. Grant VoicePaste access in System Settings → Privacy & Security → Accessibility, then use Enable recording keys in the menu.")?,_main:PhantomData})
    }
    pub fn arm(&self, recording: bool) {
        // SAFETY: live main-thread native owner; no reentrant Rust callbacks.
        unsafe {
            vp_input_arm(self.pointer.as_ptr(), recording);
        }
    }
    pub fn active(&self, active: bool) {
        // SAFETY: main-thread live native tap owner.
        unsafe {
            vp_input_active(self.pointer.as_ptr(), active);
        }
    }
    pub fn drain(&self) -> Batch {
        let mut pids = [0; 8];
        let mut flags = 0;
        // SAFETY: native queue is bounded to eight and copies into this array.
        let count =
            unsafe { vp_input_drain(self.pointer.as_ptr(), pids.as_mut_ptr(), &mut flags) }.min(8);
        Batch {
            targets: pids[..count as usize]
                .iter()
                .map(|pid| FocusTarget::from_pid(*pid))
                .collect(),
            cancel: flags & 1 != 0,
            warning: flags & 6 != 0,
        }
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        // SAFETY: single UI-thread owner, detach/invalidate before releasing context.
        unsafe {
            vp_input_destroy(self.pointer.as_ptr());
        }
    }
}
