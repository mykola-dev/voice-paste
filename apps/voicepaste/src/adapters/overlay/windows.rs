//! Windows window policy + backdrop only; GPUI owns foreground pixels and frames.
//! All mutating native calls run outside App/entity/RefCell borrows.
use super::state::{CORNER_RADIUS, HEIGHT, MARGIN, WIDTH};
use gpui_kit::{AnyWindowHandle, App};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    mem::{size_of, zeroed},
    ptr::null_mut,
};
use voicepaste_core::settings::OverlayAppearance;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Dwm::*,
    Graphics::Gdi::*,
    System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
    UI::{
        HiDpi::*,
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};
mod backdrop;
use backdrop::{Backdrop, Context};

const SUBCLASS: usize = 0x56504f;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Geometry {
    x: i32,
    y: i32,
    width: usize,
    height: usize,
    dpi: u32,
}

pub struct Shell {
    hwnd: HWND,
    visible: bool,
    // Material must drop before its compositor/dispatcher.
    backdrop: Option<Backdrop>,
    context: Option<Context>,
    geometry: Option<Geometry>,
}

impl Shell {
    /// Snapshot only: caller releases App before calling new/prepare/show/hide.
    pub fn window_handle(handle: AnyWindowHandle, cx: &mut App) -> Result<HWND, String> {
        handle
            .update(cx, |_, window, _| {
                match window.window_handle().map_err(|e| e.to_string())?.as_raw() {
                    RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get() as HWND),
                    _ => Err("Overlay does not have a Windows window handle".into()),
                }
            })
            .map_err(|e| e.to_string())?
    }

    pub fn new(hwnd: HWND) -> Result<Self, String> {
        // SAFETY: GPUI owns this live, hidden HWND on the current UI thread.
        // The subclass contains no borrowed state and is detached before closure.
        unsafe {
            if IsWindowVisible(hwnd) != 0 {
                return Err("Overlay must be configured before it is shown".into());
            }
            SetLastError(0);
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let previous = SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                (style | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST) as isize,
            );
            if previous == 0 && GetLastError() != 0 {
                return Err(last_error("configure overlay"));
            }
            suppress_native_frame(hwnd)?;
            disable_foreground_accent(hwnd)?;
            if SetWindowSubclass(hwnd, Some(nonactivating), SUBCLASS, 0) == 0 {
                return Err(last_error("protect overlay focus"));
            }
        }
        Ok(Self {
            hwnd,
            visible: false,
            backdrop: None,
            context: None,
            geometry: None,
        })
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Position hidden foreground and material. Returns whether tint lives in the material.
    pub fn prepare(&mut self, appearance: OverlayAppearance) -> Result<bool, String> {
        let geometry = placement()?;
        unsafe {
            if SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                geometry.x,
                geometry.y,
                geometry.width as i32,
                geometry.height as i32,
                SWP_NOACTIVATE | SWP_FRAMECHANGED,
            ) == 0
            {
                return Err(last_error("position overlay"));
            }
        }
        self.geometry = Some(geometry);
        self.backdrop.take();
        let alpha = appearance.background_alpha();
        if appearance.acrylic && alpha > 0 {
            if self.context.is_none() {
                self.context = Context::new().ok();
            }
            self.backdrop = self
                .context
                .as_ref()
                .and_then(|context| Backdrop::new(null_mut(), context, alpha));
            if self
                .backdrop
                .as_ref()
                .is_some_and(|backdrop| !place_backdrop(backdrop, geometry))
            {
                self.backdrop.take();
            }
        }
        Ok(self.backdrop.is_some())
    }

    /// Warm the hidden swapchain through GPUI's normal frame callback, then reveal.
    /// Synchronous dispatch is safe here: no application/entity guard is held.
    pub fn show(&mut self) -> Result<bool, String> {
        unsafe {
            SendMessageW(self.hwnd, WM_PAINT, 0, 0);
            if SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            ) == 0
            {
                return Err(last_error("show overlay"));
            }
        }
        self.visible = true;
        if self
            .backdrop
            .as_ref()
            .is_some_and(|backdrop| !backdrop.show_behind(self.hwnd))
        {
            self.backdrop.take();
        }
        Ok(self.backdrop.is_some())
    }

    pub fn hide(&mut self) {
        unsafe {
            ShowWindow(self.hwnd, SW_HIDE);
        }
        if let Some(backdrop) = &self.backdrop {
            backdrop.hide();
        }
        self.visible = false;
    }

    /// Follow GPUI's actual client geometry/DPI after native DPI changes. No frame timer.
    /// Returns true when an unavailable material needs the shared translucent fallback.
    pub fn sync_geometry(&mut self) -> Result<bool, String> {
        if !self.visible {
            return Ok(false);
        }
        let geometry = unsafe {
            let mut rect: RECT = zeroed();
            let mut origin = POINT { x: 0, y: 0 };
            if GetClientRect(self.hwnd, &mut rect) == 0
                || ClientToScreen(self.hwnd, &mut origin) == 0
            {
                return Err(last_error("read overlay geometry"));
            }
            Geometry {
                x: origin.x,
                y: origin.y,
                width: (rect.right - rect.left).max(0) as usize,
                height: (rect.bottom - rect.top).max(0) as usize,
                dpi: GetDpiForWindow(self.hwnd),
            }
        };
        if self.geometry == Some(geometry) {
            return Ok(false);
        }
        self.geometry = Some(geometry);
        if self
            .backdrop
            .as_ref()
            .is_some_and(|backdrop| !place_backdrop(backdrop, geometry))
        {
            self.backdrop.take();
            return Ok(true);
        }
        Ok(false)
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        // Async task cancellation may occur during App teardown. Never call
        // ShowWindow on GPUI's HWND here: that could reenter an already-borrowed
        // App. Normal shutdown hides explicitly outside App; GPUI owns closing
        // its foreground on all other exits. The backdrop has no GPUI callbacks.
        if let Some(backdrop) = &self.backdrop {
            backdrop.hide();
        }
        // SAFETY: composition root keeps the GPUI window alive until Shell drops.
        unsafe {
            RemoveWindowSubclass(self.hwnd, Some(nonactivating), SUBCLASS);
        }
    }
}

fn place_backdrop(backdrop: &Backdrop, geometry: Geometry) -> bool {
    backdrop.place(
        POINT {
            x: geometry.x,
            y: geometry.y,
        },
        geometry.width,
        geometry.height,
        f64::from(geometry.dpi) / 96.,
    )
}

fn placement() -> Result<Geometry, String> {
    // GPUI establishes PerMonitorV2 before opening any windows. Do not override
    // thread awareness while dispatching framework callbacks.
    unsafe {
        let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return Err(last_error("locate overlay work area"));
        }
        let (mut dpi, mut y) = (96, 96);
        let result = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut y);
        if result < 0 {
            return Err(format!("Cannot read overlay monitor DPI: {result}"));
        }
        let scale = f64::from(dpi) / 96.;
        let width = (f64::from(WIDTH) * scale).round() as usize;
        let height = (f64::from(HEIGHT) * scale).round() as usize;
        let margin = (f64::from(MARGIN) * scale).round() as i32;
        Ok(Geometry {
            x: info.rcWork.right - margin - width as i32,
            y: info.rcWork.top + margin,
            width,
            height,
            dpi,
        })
    }
}

fn last_error(action: &str) -> String {
    format!("Cannot {action}: {}", std::io::Error::last_os_error())
}

/// Only the shared rounded GPUI border is visible, never a second rectangular
/// non-client frame/shadow around either native composition surface.
fn suppress_native_frame(hwnd: HWND) -> Result<(), String> {
    let policy = DWMNCRP_DISABLED;
    // SAFETY: live UI-thread HWND, correctly sized DWM policy value.
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY as u32,
            (&policy as *const DWMNCRENDERINGPOLICY).cast(),
            size_of::<DWMNCRENDERINGPOLICY>() as u32,
        )
    };
    if result < 0 {
        return Err(format!("Cannot disable overlay native frame: {result}"));
    }
    Ok(())
}

fn disable_foreground_accent(hwnd: HWND) -> Result<(), String> {
    // The pinned GPUI Transparent backend installs ACCENT_ENABLE_TRANSPARENTGRADIENT
    // on the rectangular HWND. Our foreground needs only the premultiplied-alpha
    // DirectComposition swapchain; all backdrop effects live in the other HWND.
    #[repr(C)]
    struct Accent {
        state: u32,
        flags: u32,
        color: u32,
        animation: u32,
    }
    #[repr(C)]
    struct Attribute {
        attribute: u32,
        data: *const Accent,
        size: usize,
    }
    type SetAttribute = unsafe extern "system" fn(HWND, *const Attribute) -> i32;
    // SAFETY: optional user32 entry point, same ABI/layout as the pinned backend.
    // Stack data lives through the synchronous call; module handle is borrowed.
    unsafe {
        let module = GetModuleHandleW(windows_sys::w!("user32.dll"));
        let Some(function) =
            GetProcAddress(module, c"SetWindowCompositionAttribute".as_ptr().cast())
        else {
            return Ok(()); // No accent API: GPUI could not have installed one either.
        };
        let set: SetAttribute = std::mem::transmute(function);
        let accent = Accent {
            state: 0,
            flags: 0,
            color: 0,
            animation: 0,
        };
        let attribute = Attribute {
            attribute: 19,
            data: &accent,
            size: size_of::<Accent>(),
        };
        if set(hwnd, &attribute) == 0 {
            return Err(last_error("disable overlay foreground accent"));
        }
    }
    Ok(())
}

unsafe extern "system" fn nonactivating(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    // No host state/borrows cross the reentrant GPUI/default window procedure.
    unsafe {
        match message {
            WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
            WM_NCHITTEST => HTTRANSPARENT as isize,
            _ => DefSubclassProc(hwnd, message, w, l),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null;

    #[test]
    fn shell_configures_hidden_window_and_blocks_mouse_activation() {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                windows_sys::w!("STATIC"),
                windows_sys::w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                null_mut(),
                null(),
            );
            assert!(!hwnd.is_null());
            let foreground = GetForegroundWindow();
            let mut shell = Shell::new(hwnd).unwrap();
            let mut nc_enabled: i32 = 1;
            assert!(
                DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_NCRENDERING_ENABLED as u32,
                    (&mut nc_enabled as *mut i32).cast(),
                    size_of::<i32>() as u32
                ) >= 0
            );
            assert_eq!(
                nc_enabled, 0,
                "only the GPUI border may frame the foreground"
            );
            assert_ne!(
                GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_NOACTIVATE,
                0
            );
            assert_eq!(
                SendMessageW(hwnd, WM_MOUSEACTIVATE, 0, 0),
                MA_NOACTIVATE as isize
            );
            assert_eq!(
                SendMessageW(hwnd, WM_NCHITTEST, 0, 0),
                HTTRANSPARENT as isize
            );
            for (acrylic, transparency) in [(false, 57), (true, 100)] {
                assert!(
                    !shell
                        .prepare(OverlayAppearance {
                            acrylic,
                            transparency
                        })
                        .unwrap()
                );
                assert!(shell.context.is_none());
                assert_eq!(IsWindowVisible(hwnd), 0);
            }
            assert_eq!(GetForegroundWindow(), foreground);
            drop(shell);
            assert_ne!(IsWindow(hwnd), 0, "GPUI, not the adapter, owns the HWND");
            DestroyWindow(hwnd);
        }
    }
}
