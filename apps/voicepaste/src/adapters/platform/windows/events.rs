//! Windows callback bridge. Never enters GPUI or borrows the host/runtime.
use super::super::inbox::Pending;
use crate::adapters::hotkey;
use crate::adapters::paste::FocusTarget;
use std::{
    cell::{Cell, RefCell},
    ptr::null_mut,
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
use voicepaste_core::{SessionId, State};
use windows_sys::Win32::{
    Foundation::*,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub(in crate::adapters::platform) const TRAY: u32 = WM_APP + 1;
const CANCEL: u32 = WM_APP + 2;
const TRAY_ACTION: u32 = WM_APP + 3;
static WINDOW: AtomicUsize = AtomicUsize::new(0);
static SESSION: AtomicU64 = AtomicU64::new(0);
static ESC_HELD: AtomicBool = AtomicBool::new(false);
static ESC_CAPTURED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static PENDING: RefCell<Pending<FocusTarget>> = RefCell::new(Pending::default());
    static DELIVERY_WARNING: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn delivery_warning() {
    DELIVERY_WARNING.with(|warning| warning.set(true));
}

pub(super) fn take_delivery_warning() -> bool {
    DELIVERY_WARNING.with(|warning| warning.replace(false))
}

fn queue_tray(l: LPARAM) {
    PENDING.with(|p| {
        let mut pending = p.borrow_mut();
        match l as u32 {
            WM_LBUTTONUP => pending.settings = true,
            WM_RBUTTONUP | WM_CONTEXTMENU => pending.tray = true,
            _ => (),
        }
    });
}

fn defer(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) {
    // SAFETY: same-thread host destination; PostMessage never retains a borrow.
    if unsafe { PostMessageW(hwnd, message, w, l) } != 0 {
        return;
    }
    retain_failed_post(message, w, l);
}

fn retain_failed_post(message: u32, w: WPARAM, l: LPARAM) {
    delivery_warning();
    // These callbacks run on the installing UI thread. Posting is only a bridge
    // to this bounded inbox, not a reason to lose accepted input on OS failure.
    match message {
        WM_HOTKEY => {
            let target = FocusTarget::capture();
            PENDING.with(|p| p.borrow_mut().hotkey(w, target));
        }
        CANCEL => PENDING.with(|p| p.borrow_mut().cancel = Some(SessionId(w as u64))),
        TRAY_ACTION => queue_tray(l),
        _ => (),
    }
}

pub(in crate::adapters::platform) fn attach(hwnd: HWND) {
    WINDOW.store(hwnd as usize, Ordering::Release);
    // SAFETY: querying key state does not inject input or activate a window.
    ESC_HELD.store(
        unsafe { GetAsyncKeyState(VK_ESCAPE as i32) } < 0,
        Ordering::Release,
    );
}

pub(in crate::adapters::platform) fn detach() {
    SESSION.store(0, Ordering::Release);
    WINDOW.store(0, Ordering::Release);
    ESC_HELD.store(false, Ordering::Release);
    ESC_CAPTURED.store(false, Ordering::Release);
    drop(drain());
    take_delivery_warning();
}

pub(in crate::adapters::platform) fn recording(state: State) {
    SESSION.store(
        match state {
            State::Recording(id) => id.0,
            _ => 0,
        },
        Ordering::Release,
    );
}

pub(in crate::adapters::platform) fn drain() -> Pending<FocusTarget> {
    PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()))
}

pub(in crate::adapters::platform) unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    // SAFETY: unconsumed messages go to the OS default handler without a borrow.
    unsafe {
        if message == TRAY {
            // Shell can send synchronously. Defer work beyond its reentrant call.
            defer(hwnd, TRAY_ACTION, w, l);
            return 0;
        }
        if message == WM_CLOSE {
            PENDING.with(|p| p.borrow_mut().quit = true);
            return 0;
        }
        if message == WM_HOTKEY {
            let target = FocusTarget::capture();
            PENDING.with(|p| p.borrow_mut().hotkey(w, target));
            return 0;
        }
        if message == CANCEL {
            PENDING.with(|p| p.borrow_mut().cancel = Some(SessionId(w as u64)));
            return 0;
        }
        if message == TRAY_ACTION {
            queue_tray(l);
            return 0;
        }
        DefWindowProcW(hwnd, message, w, l)
    }
}

pub(in crate::adapters::platform) unsafe extern "system" fn keyboard_hook(
    code: i32,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    // SAFETY: Windows supplies KBDLLHOOKSTRUCT for nonnegative hook codes.
    unsafe {
        if code >= 0 {
            let event = &*(l as *const KBDLLHOOKSTRUCT);
            let down = matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let (captured, toggle) = hotkey::hook_event(event.vkCode, down);
            if let Some(id) = toggle {
                defer(
                    WINDOW.load(Ordering::Acquire) as HWND,
                    WM_HOTKEY,
                    id as usize,
                    0,
                );
            }
            if captured {
                return 1;
            }
        }
        if code >= 0 && (*(l as *const KBDLLHOOKSTRUCT)).vkCode == VK_ESCAPE as u32 {
            let down = w as u32 == WM_KEYDOWN || w as u32 == WM_SYSKEYDOWN;
            let was_held = ESC_HELD.swap(down, Ordering::AcqRel);
            if down && !was_held {
                let session = SESSION.load(Ordering::Acquire);
                if session != 0 {
                    ESC_CAPTURED.store(true, Ordering::Release);
                    defer(
                        WINDOW.load(Ordering::Acquire) as HWND,
                        CANCEL,
                        session as usize,
                        0,
                    );
                }
            }
            if ESC_CAPTURED.load(Ordering::Acquire) {
                if !down {
                    ESC_CAPTURED.store(false, Ordering::Release);
                }
                return 1;
            }
        }
        CallNextHookEx(null_mut(), code, w, l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{mem::zeroed, ptr::null};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    #[test]
    fn failed_post_retains_bounded_input_and_coalesces_a_delivery_warning() {
        drain();
        take_delivery_warning();
        // Exercise the failure branch directly: special HWND values can have
        // broadcast/thread-message semantics rather than fail deterministically.
        retain_failed_post(WM_HOTKEY, 2, 0);
        retain_failed_post(CANCEL, 41, 0);
        retain_failed_post(TRAY_ACTION, 0, WM_LBUTTONUP as isize);
        retain_failed_post(TRAY_ACTION, 0, WM_RBUTTONUP as isize);
        let pending = drain();
        assert_eq!(pending.hotkeys.len(), 1);
        assert_eq!(pending.hotkeys[0].0, 2);
        assert_eq!(pending.cancel, Some(SessionId(41)));
        assert!(pending.settings && pending.tray);
        assert!(take_delivery_warning());
        assert!(!take_delivery_warning());
        assert!(drain().hotkeys.is_empty());
    }

    #[test]
    fn tray_left_opens_settings_and_right_queues_only_context_menu() {
        for (message, settings, menu) in [
            (WM_LBUTTONUP, true, false),
            (WM_RBUTTONUP, false, true),
            (WM_CONTEXTMENU, false, true),
            (WM_MOUSEMOVE, false, false),
        ] {
            drain();
            // SAFETY: this message only queues state, never dereferences HWND.
            unsafe {
                window_proc(null_mut(), TRAY_ACTION, 1, message as isize);
            }
            let pending = drain();
            assert_eq!(pending.settings, settings);
            assert_eq!(pending.tray, menu);
            assert!(pending.hotkeys.is_empty());
        }
    }

    #[test]
    fn synchronous_shell_callback_is_queued_for_application_loop() {
        // No real tray/input; actual synchronous Shell-style delivery.
        unsafe {
            let instance = GetModuleHandleW(null());
            let name = windows_sys::w!("VoicePasteRustTrayCallbackTest");
            let mut class: WNDCLASSW = zeroed();
            class.hInstance = instance;
            class.lpszClassName = name;
            class.lpfnWndProc = Some(window_proc);
            assert_ne!(RegisterClassW(&class), 0);
            let hwnd = CreateWindowExW(
                0,
                name,
                name,
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                instance,
                null(),
            );
            assert!(!hwnd.is_null());
            let mut result: MSG = zeroed();
            SendMessageW(hwnd, TRAY, 1, WM_RBUTTONUP as isize);
            let received = PeekMessageW(&mut result, hwnd, TRAY_ACTION, TRAY_ACTION, PM_REMOVE);
            DestroyWindow(hwnd);
            UnregisterClassW(name, instance);
            assert_ne!(received, 0);
            assert_eq!(result.lParam, WM_RBUTTONUP as isize);
        }
    }

    #[test]
    fn draining_delivers_close_cancel_and_overflow_without_losing_hotkeys() {
        drain();
        // SAFETY: handled messages only queue state; no input or real HWND needed.
        unsafe {
            for id in 0..10 {
                window_proc(null_mut(), WM_HOTKEY, id, 0);
            }
            window_proc(null_mut(), CANCEL, 77, 0);
            window_proc(null_mut(), WM_CLOSE, 0, 0);
        }
        let pending = drain();
        assert_eq!(
            pending
                .hotkeys
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            (0..8).collect::<Vec<_>>()
        );
        assert!(pending.quit && pending.overflow);
        assert_eq!(pending.cancel, Some(SessionId(77)));
        let empty = drain();
        assert!(empty.hotkeys.is_empty());
        assert!(!empty.quit && !empty.overflow);
        assert_eq!(empty.cancel, None);
    }
}
