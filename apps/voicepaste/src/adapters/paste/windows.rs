//! Borrowed Windows focus snapshot, never an owned HWND or a wire identifier.
mod clipboard;
pub use clipboard::ClipboardPaste;
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow},
};

/// Moves from input callback to recording to pending transcription to paste.
/// This does not keep the foreign window alive. Windows validates the handle at
/// restore time; failure intentionally leaves current focus as the paste target.
/// No Clone/Copy: application code transfers the snapshot, never inspects it.
#[derive(Default)]
pub struct FocusTarget {
    // Integer representation is confined to this adapter so the snapshot can
    // cross the host's command channel. It grants no right to dereference HWND.
    window: usize,
}

impl FocusTarget {
    /// No restoration requested (e.g. file-only transcription with no paste).
    pub fn current() -> Self {
        Self::default()
    }

    pub fn capture() -> Self {
        // SAFETY: query only; does not activate or keep the returned window alive.
        Self::from_window(unsafe { GetForegroundWindow() } as usize)
    }

    pub(super) fn from_window(window: usize) -> Self {
        Self { window }
    }

    pub(super) fn restore(&self) {
        if self.window != 0 {
            // SAFETY: Windows validates the borrowed handle. Best effort, as in
            // the existing behavior contract; denial does not abort clipboard IO.
            unsafe {
                SetForegroundWindow(self.window as HWND);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_target_requests_no_native_restoration_and_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<FocusTarget>();
        let target = FocusTarget::current();
        assert_eq!(target.window, 0);
        target.restore(); // no input, activation or clipboard access
    }
}
