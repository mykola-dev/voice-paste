//! Owned native registrations. Standalone right modifiers require the existing
//! low-level hook: RegisterHotKey's modifier masks cannot distinguish sides.
use std::{
    ptr::{null, null_mut},
    sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering},
};
use voicepaste_core::settings::Hotkey;
use windows_sys::Win32::{
    Foundation::*, System::Threading::CreateMutexW, UI::Input::KeyboardAndMouse::*,
};

static HOOK_KEY: AtomicU32 = AtomicU32::new(0);
static HOOK_ID: AtomicI32 = AtomicI32::new(0);
static HELD: AtomicBool = AtomicBool::new(false);

pub struct Binding {
    pub key: Hotkey,
    pub id: i32,
    hwnd: HWND,
    reservation: HANDLE,
}

fn needs_hook(key: Hotkey) -> bool {
    key.modifiers == 0 && matches!(key.key, 0xa3 | 0xa5)
}

impl Binding {
    pub fn reserve(hwnd: HWND, id: i32, key: Hotkey) -> Result<Self, String> {
        let mut reservation = null_mut();
        // SAFETY: GUI-thread-owned HWND; named mutex handle only reserves a
        // standalone right key among Rust hosts, without taking mutex ownership.
        unsafe {
            if needs_hook(key) {
                let name: Vec<u16> = format!("Local\\VoicePasteRust.Hotkey.{}", key.key)
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                reservation = CreateMutexW(null(), 0, name.as_ptr());
                let error = GetLastError();
                if reservation.is_null() {
                    return Err(std::io::Error::from_raw_os_error(error as i32).to_string());
                }
                if error == ERROR_ALREADY_EXISTS {
                    CloseHandle(reservation);
                    return Err("That hotkey is already in use by another Rust VoicePaste.".into());
                }
            } else if RegisterHotKey(hwnd, id, key.modifiers | MOD_NOREPEAT, key.key) == 0 {
                return Err("That hotkey is already in use (possibly by legacy VoicePaste). Nothing was stopped. Choose another key in VoicePaste.exe --settings.".into());
            }
        }
        Ok(Self {
            key,
            id,
            hwnd,
            reservation,
        })
    }

    pub fn activate(&self) {
        let key = if needs_hook(self.key) {
            self.key.key
        } else {
            0
        };
        // SAFETY: read current physical state to avoid treating an already held
        // key as a new press after Save. This adapter never injects keyboard input.
        HELD.store(
            key != 0 && unsafe { GetAsyncKeyState(key as i32) } < 0,
            Ordering::Release,
        );
        HOOK_ID.store(self.id, Ordering::Release);
        HOOK_KEY.store(key, Ordering::Release);
    }
}
impl Drop for Binding {
    fn drop(&mut self) {
        // A staged binding never changes the active hook until Save succeeds.
        if HOOK_ID.load(Ordering::Acquire) == self.id {
            HOOK_KEY.store(0, Ordering::Release);
        }
        // SAFETY: this binding owns exactly one registration or reservation.
        unsafe {
            if self.reservation.is_null() {
                UnregisterHotKey(self.hwnd, self.id);
            } else {
                CloseHandle(self.reservation);
            }
        }
    }
}

pub fn hook_event(key: u32, down: bool) -> (bool, Option<i32>) {
    if key != HOOK_KEY.load(Ordering::Acquire) || key == 0 {
        return (false, None);
    }
    let was_held = HELD.swap(down, Ordering::AcqRel);
    (
        true,
        (down && !was_held).then(|| HOOK_ID.load(Ordering::Acquire)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn right_key_hook_distinguishes_side_and_ignores_autorepeat() {
        for key in [0xa3, 0xa5] {
            HOOK_ID.store(2, Ordering::Release);
            HOOK_KEY.store(key, Ordering::Release);
            HELD.store(false, Ordering::Release);
            assert_eq!(hook_event(key - 1, true), (false, None));
            assert_eq!(hook_event(key, true), (true, Some(2)));
            assert_eq!(hook_event(key, true), (true, None));
            assert_eq!(hook_event(key, false), (true, None));
            assert_eq!(hook_event(key, true), (true, Some(2)));
        }
        HOOK_KEY.store(0, Ordering::Release);
        HELD.store(false, Ordering::Release);
    }
    #[test]
    fn only_standalone_right_modifiers_use_hook() {
        for key in ["RightAlt", "RightCtrl"] {
            assert!(needs_hook(Hotkey::parse(key).unwrap()));
        }
        for key in ["ScrollLock", "Numpad0", "F8", "F12", "Ctrl+Alt+Space"] {
            assert!(!needs_hook(Hotkey::parse(key).unwrap()));
        }
    }
}
