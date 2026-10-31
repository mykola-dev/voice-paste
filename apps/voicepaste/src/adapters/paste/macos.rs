use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use voicepaste_application::Paste;
use voicepaste_core::settings::PasteShortcut;

unsafe extern "C" {
    fn vp_focus_capture() -> i32;
    fn vp_paste_prepare(bytes: *const u8, length: usize, pid: i32, error: *mut u32) -> bool;
    fn vp_paste_post(error: *mut u32) -> bool;
}

/// Borrowed process snapshot, confined to the adapter; no native handle in core.
pub struct FocusTarget {
    pid: i32,
}
impl FocusTarget {
    pub fn current() -> Self {
        Self { pid: 0 }
    }
    pub fn capture() -> Self {
        // SAFETY: read-only native frontmost application query.
        Self {
            pid: unsafe { vp_focus_capture() },
        }
    }
    pub(crate) fn from_pid(pid: i32) -> Self {
        Self { pid }
    }
}
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
        if shortcut != PasteShortcut::CmdV {
            return Err("Only Cmd+V is supported on macOS; clipboard was not changed".into());
        }
        let mut error = 0;
        // SAFETY: bytes live through this bounded synchronous call; native code
        // copies text, owns events and restores no stale native object pointer.
        if unsafe { vp_paste_prepare(text.as_ptr(), text.len(), target.pid, &mut error) } {
            if self.shutdown.load(Ordering::Acquire) {
                return Err("Paste cancelled by shutdown; result remains on the clipboard".into());
            }
            // SAFETY: native code owns/releases both keyboard events. This
            // second call follows the post-activation shutdown check.
            if unsafe { vp_paste_post(&mut error) } {
                return Ok(());
            }
        }
        Err(match error {
            1 => "Invalid Unicode text",
            2 => "Cannot write the clipboard",
            3 => "Accessibility permission is required for Cmd+V; text remains on the clipboard for manual paste",
            _ => "Cannot create paste events; text remains on the clipboard for manual paste",
        }.into())
    }
}
