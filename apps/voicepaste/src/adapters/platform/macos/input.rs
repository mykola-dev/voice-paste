//! Main-thread tap owner and adapter-local focus snapshots.
use crate::adapters::paste::FocusTarget;
use std::{ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc};
use voicepaste_core::settings::{Hotkey, Settings};
unsafe extern "C" {
    fn vp_input_create(key: u16, prompt: bool) -> *mut c_void;
    fn vp_input_arm(input: *mut c_void, session: u64);
    fn vp_input_active(input: *mut c_void, active: bool);
    fn vp_input_drain(
        input: *mut c_void,
        pids: *mut i32,
        flags: *mut u32,
        session: *mut u64,
    ) -> u32;
    fn vp_input_destroy(input: *mut c_void);
}
pub struct Input {
    pointer: NonNull<c_void>,
    _main: PhantomData<Rc<()>>,
}
#[derive(Default)]
pub struct Batch {
    pub targets: Vec<FocusTarget>,
    pub cancel: Option<voicepaste_core::SessionId>,
    pub warning: bool,
}
impl Batch {
    pub fn merge(&mut self, mut batch: Self) {
        let remaining = 8usize.saturating_sub(self.targets.len());
        self.warning |= batch.warning || batch.targets.len() > remaining;
        let take = batch.targets.len().min(remaining);
        self.targets.extend(batch.targets.drain(..take));
        self.cancel = batch.cancel.or(self.cancel);
    }
}
fn recording_key(settings: &Settings) -> Result<u16, String> {
    let hotkey = Hotkey::parse(&settings.hotkey)?;
    if hotkey.modifiers != 0 {
        return Err("macOS recording keys must be standalone".into());
    }
    match hotkey.key {
        0xA3 => Ok(62),
        0xA5 => Ok(61),
        0x77 => Ok(100),
        0x7B => Ok(111),
        _ => Err("Unsupported macOS recording key".into()),
    }
}
impl Input {
    pub fn attach(settings: &Settings, prompt: bool) -> Result<Self, String> {
        let key = recording_key(settings)?;
        // SAFETY: UI-thread creation transfers one native owner; callback never
        // calls Rust. Permission denial returns null without installing a tap.
        let pointer = unsafe { vp_input_create(key, prompt) };
        Ok(Self {pointer:NonNull::new(pointer).ok_or("Global recording keys require Accessibility permission. Grant VoicePaste access in System Settings → Privacy & Security → Accessibility, then use Enable recording keys in the menu.")?,_main:PhantomData})
    }
    pub fn arm(&self, state: voicepaste_core::State) {
        let session = if let voicepaste_core::State::Recording(id) = state {
            id.0
        } else {
            0
        };
        // SAFETY: live main-thread native owner; no reentrant Rust callbacks.
        unsafe {
            vp_input_arm(self.pointer.as_ptr(), session);
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
        let mut session = 0;
        // SAFETY: native queue is bounded to eight and copies into this array.
        let count = unsafe {
            vp_input_drain(
                self.pointer.as_ptr(),
                pids.as_mut_ptr(),
                &mut flags,
                &mut session,
            )
        }
        .min(8);
        Batch {
            targets: pids[..count as usize]
                .iter()
                .map(|pid| FocusTarget::from_pid(*pid))
                .collect(),
            cancel: (session != 0).then_some(voicepaste_core::SessionId(session)),
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_presets_map_to_native_single_keys_without_requesting_tcc() {
        let mut settings =
            Settings::initial_for(voicepaste_core::settings::SettingsPlatform::Macos).unwrap();
        for (key, code) in [
            ("RightCtrl", 62),
            ("RightAlt", 61),
            ("F8", 100),
            ("F12", 111),
        ] {
            settings.hotkey = key.into();
            assert_eq!(recording_key(&settings).unwrap(), code);
        }
        for key in ["Ctrl+F8", "ScrollLock", "Numpad0"] {
            settings.hotkey = key.into();
            assert!(recording_key(&settings).is_err());
        }
    }
    #[test]
    fn replacement_inboxes_remain_bounded_and_keep_the_latest_cancel_identity() {
        let mut batch = Batch {
            targets: (0..8).map(FocusTarget::from_pid).collect(),
            cancel: Some(voicepaste_core::SessionId(1)),
            warning: false,
        };
        batch.merge(Batch {
            targets: (8..16).map(FocusTarget::from_pid).collect(),
            cancel: Some(voicepaste_core::SessionId(2)),
            warning: false,
        });
        assert_eq!(batch.targets.len(), 8);
        assert!(batch.warning);
        assert_eq!(batch.cancel, Some(voicepaste_core::SessionId(2)));
    }
}
