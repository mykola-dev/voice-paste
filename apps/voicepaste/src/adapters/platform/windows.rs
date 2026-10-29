//! Native menu tracking and Settings activation. Call outside GPUI/state borrows.
pub(super) mod events;
mod host;
pub(super) mod ui;
use super::menu::{self, MenuItem, TrayAction};
use crate::adapters::paste::FocusTarget;
pub(super) use host::Host;
use std::{
    mem::zeroed,
    ptr::{null, null_mut},
};
use voicepaste_core::State;
use windows_sys::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

const TOGGLE: usize = 1;
const SETTINGS: usize = 2;
const QUIT: usize = 3;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

struct Menu(HMENU);
impl Drop for Menu {
    fn drop(&mut self) {
        // SAFETY: this adapter uniquely owns the menu, including on build failure.
        unsafe {
            DestroyMenu(self.0);
        }
    }
}

fn create_menu(items: impl IntoIterator<Item = MenuItem>) -> Result<Menu, String> {
    // SAFETY: synchronous calls with live UTF-16 labels; RAII owns each menu.
    unsafe {
        let handle = CreatePopupMenu();
        if handle.is_null() {
            return Err(format!(
                "Cannot create tray menu: {}",
                std::io::Error::last_os_error()
            ));
        }
        let menu = Menu(handle);
        for item in items {
            let appended = match item {
                MenuItem::Separator => AppendMenuW(menu.0, MF_SEPARATOR, 0, null()),
                MenuItem::Command {
                    action,
                    label,
                    enabled,
                } => AppendMenuW(
                    menu.0,
                    MF_STRING | if enabled { 0 } else { MF_GRAYED },
                    action_id(action),
                    wide(label).as_ptr(),
                ),
            };
            if appended == 0 {
                return Err(format!(
                    "Cannot build tray menu: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        Ok(menu)
    }
}

pub(super) fn tray_menu(
    owner: HWND,
    state: State,
) -> Result<(Option<TrayAction>, FocusTarget), String> {
    let items = menu::items(state);
    // SAFETY: live UI-thread owner; menu is destroyed before return. No borrow
    // crosses TrackPopupMenu's nested loop. Capture paste target before activation.
    unsafe {
        let target = FocusTarget::capture();
        let menu = create_menu(items)?;
        let mut point: POINT = zeroed();
        if GetCursorPos(&mut point) == 0 {
            return Err(format!(
                "Cannot locate tray menu: {}",
                std::io::Error::last_os_error()
            ));
        }
        SetForegroundWindow(owner);
        let choice = TrackPopupMenu(
            menu.0,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            owner,
            null(),
        );
        drop(menu);
        if PostMessageW(owner, WM_NULL, 0, 0) == 0 {
            // Selection remains deliverable even if the native dismissal nudge
            // failed. Report it on the next bounded host drain, not in a callback.
            events::delivery_warning();
        }
        Ok((decode_action(choice as usize), target))
    }
}

fn decode_action(choice: usize) -> Option<TrayAction> {
    match choice {
        TOGGLE => Some(TrayAction::Toggle),
        SETTINGS => Some(TrayAction::Settings),
        QUIT => Some(TrayAction::Quit),
        _ => None,
    }
}

fn action_id(action: TrayAction) -> usize {
    match action {
        TrayAction::Toggle => TOGGLE,
        TrayAction::Settings => SETTINGS,
        TrayAction::Quit => QUIT,
    }
}

pub(super) fn show_settings(handle: isize) -> Result<(), String> {
    let hwnd = handle as HWND;
    reveal_settings(hwnd)?;
    // SAFETY: live Settings HWND on its owning thread, outside framework borrows.
    // Foreground permission is best-effort; never synthesize Alt or attach queues.
    unsafe {
        SetForegroundWindow(hwnd);
    }
    Ok(())
}

pub(super) fn reveal_settings(hwnd: HWND) -> Result<(), String> {
    // SAFETY: live UI-thread Settings HWND. Explicitly unhide even with SW_HIDE
    // startup info. Do not use pinned GPUI activation (it injects synthetic Alt).
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        if SetWindowPos(
            hwnd,
            null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        ) == 0
        {
            return Err(format!(
                "Cannot show Settings: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_build_preserves_items_and_drop_releases_the_native_menu() {
        for state in [
            State::Idle,
            State::Recording(voicepaste_core::SessionId(1)),
            State::Transcribing(voicepaste_core::SessionId(1)),
        ] {
            let items = menu::items(state);
            let expected = items.len();
            let menu = create_menu(items).unwrap();
            let handle = menu.0;
            assert_eq!(unsafe { GetMenuItemCount(handle) }, expected as i32);
            assert_eq!(unsafe { GetMenuItemID(handle, 0) }, TOGGLE as u32);
            let flags = unsafe { GetMenuState(handle, TOGGLE as u32, MF_BYCOMMAND) };
            assert_eq!(
                flags & MF_GRAYED != 0,
                matches!(state, State::Transcribing(_))
            );
            drop(menu);
            assert_eq!(unsafe { IsMenu(handle) }, 0);
        }
    }

    #[test]
    fn settings_reveal_unhides_existing_window_without_foreground_input() {
        // Zero-area, nonactivating window; never exercise interactive activation.
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                windows_sys::w!("STATIC"),
                windows_sys::w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null()),
                null(),
            );
            assert!(!hwnd.is_null());
            let foreground = GetForegroundWindow();
            let hidden = IsWindowVisible(hwnd) == 0;
            let first = reveal_settings(hwnd);
            let visible = IsWindowVisible(hwnd) != 0;
            ShowWindow(hwnd, SW_HIDE);
            let again = reveal_settings(hwnd);
            let visible_again = IsWindowVisible(hwnd) != 0;
            let final_foreground = GetForegroundWindow();
            DestroyWindow(hwnd);
            assert!(hidden && visible && visible_again);
            assert!(first.is_ok() && again.is_ok());
            assert_ne!(foreground, hwnd);
            assert_ne!(final_foreground, hwnd);
        }
    }

    #[test]
    fn native_ids_do_not_escape_as_application_commands() {
        for action in [TrayAction::Toggle, TrayAction::Settings, TrayAction::Quit] {
            assert_eq!(decode_action(action_id(action)), Some(action));
        }
        assert_eq!(decode_action(TOGGLE), Some(TrayAction::Toggle));
        assert_eq!(decode_action(SETTINGS), Some(TrayAction::Settings));
        assert_eq!(decode_action(QUIT), Some(TrayAction::Quit));
        assert_eq!(decode_action(0), None);
        assert_eq!(decode_action(usize::MAX), None);
    }
}
