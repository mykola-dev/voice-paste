use super::FocusTarget;
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use voicepaste_application::Paste;
use voicepaste_core::settings::PasteShortcut;
use windows_sys::Win32::{
    Foundation::*,
    System::{DataExchange::*, Memory::*},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub struct ClipboardPaste {
    pub shutdown: Arc<AtomicBool>,
}
impl Paste<FocusTarget> for ClipboardPaste {
    fn paste(
        &mut self,
        text: &str,
        target: FocusTarget,
        shortcut: PasteShortcut,
    ) -> Result<(), String> {
        if self.shutdown.load(Ordering::Acquire) {
            return Err("Paste cancelled by shutdown".into());
        }
        target.restore();
        let mut last = String::new();
        for attempt in 0..6 {
            if self.shutdown.load(Ordering::Acquire) {
                return Err("Paste cancelled by shutdown".into());
            }
            match set_clipboard(text) {
                Ok(()) => {
                    thread::sleep(Duration::from_millis(100));
                    if self.shutdown.load(Ordering::Acquire) {
                        return Err("Paste cancelled by shutdown; text remains in clipboard".into());
                    }
                    return inject(shortcut);
                }
                Err(error) => last = error,
            }
            if attempt < 5 {
                thread::sleep(Duration::from_millis(10 << attempt));
            }
        }
        Err(format!("Cannot set clipboard: {last}"))
    }
}

struct Clipboard;
impl Drop for Clipboard {
    fn drop(&mut self) {
        /* SAFETY: this thread owns the open clipboard. */
        unsafe {
            CloseClipboard();
        }
    }
}
struct Memory(HGLOBAL);
impl Drop for Memory {
    fn drop(&mut self) {
        /* SAFETY: released only before ownership transfer. */
        unsafe {
            GlobalFree(self.0);
        }
    }
}

fn set_clipboard(text: &str) -> Result<(), String> {
    if text.contains('\0') {
        return Err("Transcript contains an embedded NUL".into());
    }
    let value: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    // SAFETY: clipboard/global memory lifecycle is confined to this thread.
    // Allocation and copy precede EmptyClipboard so allocation failure preserves
    // the old clipboard. A message-only owner avoids NULL-owner SetClipboardData.
    unsafe {
        let memory = Memory(GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, value.len() * 2));
        if memory.0.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let pointer = GlobalLock(memory.0);
        if pointer.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        std::ptr::copy_nonoverlapping(value.as_ptr(), pointer.cast::<u16>(), value.len());
        GlobalUnlock(memory.0);
        let owner = CreateWindowExW(
            0,
            windows_sys::w!("STATIC"),
            windows_sys::w!("VoicePaste Clipboard"),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            null_mut(),
            null(),
        );
        if owner.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        struct Window(HWND);
        impl Drop for Window {
            fn drop(&mut self) {
                /* SAFETY: owned message-only window. */
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }
        let _owner = Window(owner);
        if OpenClipboard(owner) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let _clipboard = Clipboard;
        if EmptyClipboard() == 0 || SetClipboardData(13, memory.0).is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        std::mem::forget(memory); // clipboard now owns the HGLOBAL
    }
    Ok(())
}

fn sequence(shortcut: PasteShortcut) -> Vec<(u16, bool)> {
    let keys: &[u16] = match shortcut {
        PasteShortcut::CtrlV => &[VK_CONTROL, 0x56],
        PasteShortcut::CtrlShiftV => &[VK_CONTROL, VK_SHIFT, 0x56],
        PasteShortcut::ShiftInsert => &[VK_SHIFT, VK_INSERT],
    };
    keys.iter()
        .map(|&key| (key, false))
        .chain(keys.iter().rev().map(|&key| (key, true)))
        .collect()
}
fn input(key: u16, up: bool) -> INPUT {
    // SAFETY: INPUT union is initialized using the matching INPUT_KEYBOARD tag.
    let mut value: INPUT = unsafe { zeroed() };
    value.r#type = INPUT_KEYBOARD;
    value.Anonymous.ki = KEYBDINPUT {
        wVk: key,
        wScan: 0,
        // Insert is the dedicated extended key, not keypad 0. Terminals can
        // distinguish them even when SendInput accepts the entire sequence.
        dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }
            | if key == VK_INSERT {
                KEYEVENTF_EXTENDEDKEY
            } else {
                0
            },
        time: 0,
        dwExtraInfo: 0,
    };
    value
}
fn inject(shortcut: PasteShortcut) -> Result<(), String> {
    let sequence = sequence(shortcut);
    let inputs: Vec<_> = sequence.iter().map(|&(key, up)| input(key, up)).collect();
    // SAFETY: valid contiguous INPUT array with correct ABI element size.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    } as usize;
    if sent == inputs.len() {
        return Ok(());
    }
    // Release only our successfully injected, still-held keys, in reverse order.
    let mut held = Vec::new();
    for &(key, up) in sequence.iter().take(sent.min(sequence.len())) {
        if up {
            held.retain(|&held| held != key);
        } else {
            held.push(key);
        }
    }
    let release: Vec<_> = held.into_iter().rev().map(|key| input(key, true)).collect();
    if !release.is_empty() {
        /* SAFETY: same valid INPUT layout. */
        unsafe {
            SendInput(
                release.len() as u32,
                release.as_ptr(),
                size_of::<INPUT>() as i32,
            );
        }
    }
    Err(
        "Paste input was blocked or incomplete. Text remains in the clipboard for manual paste."
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Interactive: briefly focuses an owned edit window and exercises the real clipboard/SendInput"]
    fn shift_insert_reaches_a_real_edit_control() {
        unsafe {
            // Save/restore existing Unicode text; never log the user's clipboard.
            assert_ne!(OpenClipboard(null_mut()), 0);
            let clipboard = Clipboard;
            let memory = GetClipboardData(13);
            assert!(
                !memory.is_null(),
                "Requires existing Unicode clipboard text"
            );
            let pointer = GlobalLock(memory);
            assert!(!pointer.is_null());
            let data = std::slice::from_raw_parts(pointer.cast::<u16>(), GlobalSize(memory) / 2);
            let length = data.iter().position(|&unit| unit == 0).unwrap();
            let previous_text = String::from_utf16(&data[..length]).unwrap();
            GlobalUnlock(memory);
            drop(clipboard);
            struct RestoreClipboard(String);
            impl Drop for RestoreClipboard {
                fn drop(&mut self) {
                    let _ = set_clipboard(&self.0);
                }
            }
            let _restore = RestoreClipboard(previous_text);
            let text = "VoicePaste Rust — перевірка вставки\r\nShift+Insert".to_owned();

            struct Target {
                window: HWND,
                previous: HWND,
            }
            impl Drop for Target {
                fn drop(&mut self) {
                    unsafe {
                        DestroyWindow(self.window);
                        SetForegroundWindow(self.previous);
                    }
                }
            }
            let target = Target {
                previous: GetForegroundWindow(),
                window: CreateWindowExW(
                    0,
                    windows_sys::w!("STATIC"),
                    windows_sys::w!("VoicePaste Rust paste check"),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    80,
                    80,
                    420,
                    160,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null(),
                ),
            };
            assert!(!target.window.is_null());
            let edit = CreateWindowExW(
                0,
                windows_sys::w!("EDIT"),
                windows_sys::w!(""),
                WS_CHILD | WS_VISIBLE | ES_MULTILINE as u32,
                0,
                0,
                400,
                120,
                target.window,
                null_mut(),
                null_mut(),
                null(),
            );
            assert!(!edit.is_null());
            SetForegroundWindow(target.window);
            SetFocus(edit);
            assert_eq!(GetFocus(), edit, "Controlled edit must have keyboard focus");
            assert_eq!(
                GetForegroundWindow(),
                target.window,
                "Controlled target must be foreground"
            );
            let destination = target.window as usize;
            let expected = text.clone();
            let task = thread::spawn(move || {
                ClipboardPaste {
                    shutdown: Arc::new(AtomicBool::new(false)),
                }
                .paste(
                    &text,
                    FocusTarget::from_window(destination),
                    PasteShortcut::ShiftInsert,
                )
            });
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            let mut actual = Vec::new();
            while std::time::Instant::now() < deadline {
                let mut message: MSG = zeroed();
                while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                actual.resize(GetWindowTextLengthW(edit) as usize + 1, 0);
                GetWindowTextW(edit, actual.as_mut_ptr(), actual.len() as i32);
                if task.is_finished() && actual.len() > 1 {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            task.join().unwrap().unwrap();
            // No text in assertion output, even on failure.
            let actual = String::from_utf16_lossy(&actual[..actual.len() - 1]);
            assert!(
                actual.replace("\r\n", "\n") == expected.replace("\r\n", "\n"),
                "Shift+Insert did not insert the clipboard into the owned edit"
            );
        }
    }
    #[test]
    fn shift_insert_uses_the_dedicated_extended_key_on_down_and_up() {
        for up in [false, true] {
            let event = input(VK_INSERT, up);
            // SAFETY: input initializes the keyboard member of INPUT.
            let flags = unsafe { event.Anonymous.ki.dwFlags };
            assert_eq!(flags & KEYEVENTF_EXTENDEDKEY, KEYEVENTF_EXTENDEDKEY);
            assert_eq!(flags & KEYEVENTF_KEYUP != 0, up);
        }
        assert_eq!(unsafe { input(VK_SHIFT, false).Anonymous.ki.dwFlags }, 0);
    }
    #[test]
    fn shortcuts_release_all_keys_in_reverse_order() {
        for shortcut in [
            PasteShortcut::CtrlV,
            PasteShortcut::CtrlShiftV,
            PasteShortcut::ShiftInsert,
        ] {
            let sequence = sequence(shortcut);
            let half = sequence.len() / 2;
            assert!(sequence[..half].iter().all(|&(_, up)| !up));
            assert_eq!(
                sequence[..half]
                    .iter()
                    .map(|&(key, _)| key)
                    .collect::<Vec<_>>(),
                sequence[half..]
                    .iter()
                    .rev()
                    .map(|&(key, up)| {
                        assert!(up);
                        key
                    })
                    .collect::<Vec<_>>()
            );
        }
    }
}
