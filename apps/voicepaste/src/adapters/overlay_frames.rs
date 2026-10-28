//! One frame owner on the host UI thread, including native nested menu loops.
use super::overlay::Overlay;
use crossbeam_channel::Receiver;
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};
use voicepaste_core::{
    State,
    settings::OverlayAppearance,
    waveform::{CAPACITY, Measurement},
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{GWLP_USERDATA, GetWindowLongPtrW, SetWindowLongPtrW},
};

struct Context {
    frame: RefCell<Frame>,
    levels: Receiver<Measurement>,
    clock: Instant,
}
struct Frame {
    overlay: Option<Overlay>,
    error: Option<String>,
    #[cfg(test)]
    ticks: usize,
}
impl Frame {
    fn finish(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            // Disable the timer/renderer once; the outer UI loop reports failure.
            self.overlay.take();
            self.error = Some(error);
        }
    }
}

pub struct Frames {
    hwnd: HWND,
    context: Box<Context>,
}
impl Frames {
    pub fn new(
        hwnd: HWND,
        overlay: Overlay,
        levels: Receiver<Measurement>,
        clock: Instant,
    ) -> Self {
        let value = Self {
            hwnd,
            context: Box::new(Context {
                frame: RefCell::new(Frame {
                    overlay: Some(overlay),
                    error: None,
                    #[cfg(test)]
                    ticks: 0,
                }),
                levels,
                clock,
            }),
        };
        // SAFETY: the host's user-data slot is reserved for this UI-thread-only
        // context. Box address stays stable; Drop detaches before freeing it.
        unsafe {
            SetWindowLongPtrW(
                hwnd,
                GWLP_USERDATA,
                (&*value.context as *const Context) as isize,
            );
        }
        value
    }
    pub fn update(&self, state: State, downloading: bool, now: Duration) {
        let mut frame = self.context.frame.borrow_mut();
        let result = frame
            .overlay
            .as_mut()
            .map_or(Ok(()), |view| view.update(state, downloading, now));
        frame.finish(result);
    }
    pub fn appearance(&self, color: &str, appearance: OverlayAppearance) {
        if let Some(view) = &mut self.context.frame.borrow_mut().overlay {
            view.color(color);
            view.appearance(appearance);
        }
    }
    pub fn take_error(&self) -> Option<String> {
        self.context.frame.borrow_mut().error.take()
    }

    /// Called only by the owning host's window procedure for its frame timer.
    /// The host must outlive Frames and must not use GWLP_USERDATA elsewhere.
    pub unsafe fn dispatch(hwnd: HWND) {
        // SAFETY: these invariants are upheld by platform::run's local owners.
        unsafe {
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Context;
            if pointer.is_null() {
                return;
            }
            let context = &*pointer;
            // Native render/show operations can dispatch synchronous messages.
            // Reentrant timer delivery skips a frame rather than aliasing state.
            let Ok(mut frame) = context.frame.try_borrow_mut() else {
                return;
            };
            let now = context.clock.elapsed();
            for measurement in context.levels.try_iter().take(CAPACITY) {
                if let Some(view) = &mut frame.overlay {
                    view.measurement(measurement, now);
                }
            }
            let result = frame.overlay.as_mut().map_or(Ok(()), |view| view.tick(now));
            frame.finish(result);
            #[cfg(test)]
            {
                frame.ticks += 1;
            }
        }
    }
    #[cfg(test)]
    pub fn ticks(&self) -> usize {
        self.context.frame.borrow().ticks
    }
}
impl Drop for Frames {
    fn drop(&mut self) {
        // SAFETY: host still lives on this thread; no callback can now see Box.
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
        }
    }
}
