//! Windows window policy + backdrop only; GPUI owns foreground pixels and frames.
//! All mutating native calls run outside App/entity/RefCell borrows.
use super::state::{CORNER_RADIUS, HEIGHT, MARGIN, TRAVEL, VIEWPORT_WIDTH, WIDTH};
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
    opacity: Option<f32>,
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
            make_frameless_popup(hwnd)?;
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
            opacity: None,
        })
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Position hidden foreground and material. Returns whether tint lives in the material.
    pub fn prepare(&mut self, appearance: OverlayAppearance) -> Result<bool, String> {
        let geometry = placement()?;
        position_foreground(self.hwnd, geometry)?;
        self.geometry = Some(geometry);
        self.opacity = None;
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
        self.warm()?;
        unsafe {
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
        // First reveal may deliver startup geometry messages too. Reconcile
        // before revealing the separate material, never stretch it to a host.
        self.warm()?;
        if self
            .backdrop
            .as_ref()
            .is_some_and(|backdrop| !backdrop.show_behind(self.hwnd))
        {
            self.backdrop.take();
        }
        Ok(self.backdrop.is_some())
    }

    fn warm(&mut self) -> Result<(), String> {
        // No App/entity guards: GPUI's first paint can synchronously initialize
        // the surface and deliver native geometry changes. Validate afterwards,
        // not only before the first paint, and redraw any corrected viewport.
        unsafe {
            SendMessageW(self.hwnd, WM_PAINT, 0, 0);
        }
        let (resized, _) = self.synchronize_geometry()?;
        if resized {
            unsafe {
                SendMessageW(self.hwnd, WM_PAINT, 0, 0);
            }
        }
        Ok(())
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

    /// Apply the shared timeline to the material inside its fixed host. Never
    /// move either HWND during animation: client geometry is not an animation anchor.
    pub fn animate(&mut self, opacity: f32) -> bool {
        let Some(geometry) = self.geometry else {
            return false;
        };
        if self.opacity == Some(opacity) {
            return false;
        }
        self.opacity = Some(opacity);
        let offset = TRAVEL * geometry.dpi as f32 / 96. * (1. - opacity);
        if self
            .backdrop
            .as_ref()
            .is_some_and(|backdrop| !backdrop.transition(opacity, offset))
        {
            self.backdrop.take();
            return true;
        }
        false
    }

    /// Follow GPUI's actual client geometry/DPI after native DPI changes. No frame timer.
    /// Returns true when an unavailable material needs the shared translucent fallback.
    pub fn sync_geometry(&mut self) -> Result<bool, String> {
        if !self.visible {
            return Ok(false);
        }
        self.synchronize_geometry()
            .map(|(_, lost_material)| lost_material)
    }

    /// Returns (viewport resized, material lost). Host pixels are observed, not
    /// accepted as the panel's logical size: this is a fixed-size popup.
    fn synchronize_geometry(&mut self) -> Result<(bool, bool), String> {
        let mut geometry = read_geometry(self.hwnd)?;
        let expected = viewport_pixels(geometry.dpi);
        let resized = (geometry.width, geometry.height) != expected;
        if resized {
            // SAFETY: UI thread, no App/entity guards; normal WM_SIZE updates
            // GPUI's viewport and renderer. No fake layout/scale-factor update.
            unsafe {
                if SetWindowPos(
                    self.hwnd,
                    null_mut(),
                    0,
                    0,
                    expected.0 as i32,
                    expected.1 as i32,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                ) == 0
                {
                    return Err(last_error("restore canonical overlay viewport"));
                }
            }
            geometry = read_geometry(self.hwnd)?;
            if (geometry.width, geometry.height) != viewport_pixels(geometry.dpi) {
                return Err("Overlay viewport did not settle at its current DPI".into());
            }
        }
        if self.geometry == Some(geometry) {
            return Ok((resized, false));
        }
        self.geometry = Some(geometry);
        self.opacity = None;
        if self
            .backdrop
            .as_ref()
            .is_some_and(|backdrop| !place_backdrop(backdrop, geometry))
        {
            self.backdrop.take();
            return Ok((resized, true));
        }
        Ok((resized, false))
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

fn viewport_pixels(dpi: u32) -> (usize, usize) {
    let scale = f64::from(dpi) / 96.;
    (
        (f64::from(VIEWPORT_WIDTH) * scale).round() as usize,
        (f64::from(HEIGHT) * scale).round() as usize,
    )
}

fn read_geometry(hwnd: HWND) -> Result<Geometry, String> {
    // SAFETY: caller owns the live foreground on this UI thread.
    unsafe {
        let mut rect: RECT = zeroed();
        let mut origin = POINT { x: 0, y: 0 };
        if GetClientRect(hwnd, &mut rect) == 0 || ClientToScreen(hwnd, &mut origin) == 0 {
            return Err(last_error("read overlay geometry"));
        }
        Ok(Geometry {
            x: origin.x,
            y: origin.y,
            width: (rect.right - rect.left).max(0) as usize,
            height: (rect.bottom - rect.top).max(0) as usize,
            dpi: GetDpiForWindow(hwnd),
        })
    }
}

fn position_foreground(hwnd: HWND, geometry: Geometry) -> Result<(), String> {
    // SAFETY: hidden UI-thread-owned foreground, outside App/entity borrows.
    // Moving monitors synchronously delivers WM_DPICHANGED. GPUI applies the
    // suggested RECT in a nested SetWindowPos, so combining the move and final
    // size lets that RECT overwrite/double-scale our already target-DPI size.
    // Finish the DPI transition first, then apply the canonical viewport once.
    unsafe {
        if SetWindowPos(
            hwnd,
            null_mut(),
            geometry.x,
            geometry.y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        ) == 0
        {
            return Err(last_error("move overlay to target monitor"));
        }
        if SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            geometry.x,
            geometry.y,
            geometry.width as i32,
            geometry.height as i32,
            SWP_NOACTIVATE | SWP_FRAMECHANGED,
        ) == 0
        {
            return Err(last_error("size overlay after DPI transition"));
        }
    }
    Ok(())
}

fn placement() -> Result<Geometry, String> {
    // GPUI establishes PerMonitorV2 before opening any windows. Do not override
    // thread awareness while dispatching framework callbacks.
    let monitor = unsafe { MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY) };
    monitor_placement(monitor)
}

fn monitor_placement(monitor: HMONITOR) -> Result<Geometry, String> {
    unsafe {
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
        let panel_width = (f64::from(WIDTH) * scale).round() as i32;
        let (width, height) = viewport_pixels(dpi);
        let margin = (f64::from(MARGIN) * scale).round() as i32;
        Ok(Geometry {
            x: info.rcWork.right - margin - panel_width,
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

/// GPUI's pinned PopUp backend passes style 0 to CreateWindowEx. Windows adds
/// WS_CAPTION to an overlapped window; hiding its DWM decoration does NOT remove
/// the non-client insets. GPUI removes only the top inset in WM_NCCALCSIZE, leaving
/// sides/bottom. Native outer placement would then truncate the GPUI panel while
/// the material follows the shorter client area. Both surfaces must be popups
/// whose client rect exactly matches the requested viewport, before first show.
fn make_frameless_popup(hwnd: HWND) -> Result<(), String> {
    // SAFETY: live, hidden UI-thread HWND, called outside all App/entity borrows.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let style = (style & !(WS_CAPTION | WS_THICKFRAME | WS_BORDER | WS_DLGFRAME)) | WS_POPUP;
        SetLastError(0);
        if SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize) == 0 && GetLastError() != 0 {
            return Err(last_error("remove overlay non-client insets"));
        }
        if SetWindowPos(
            hwnd,
            null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOMOVE | SWP_NOSIZE,
        ) == 0
        {
            return Err(last_error("refresh frameless overlay viewport"));
        }
    }
    Ok(())
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
    use std::{cell::Cell, ptr::null};

    struct Awareness(DPI_AWARENESS_CONTEXT);
    impl Drop for Awareness {
        fn drop(&mut self) {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }

    struct HiddenWindow(HWND);
    impl Drop for HiddenWindow {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }

    #[derive(Default)]
    struct DpiFixture {
        changes: Cell<u32>,
        size: Cell<(u32, i32, i32)>,
        failed: Cell<bool>,
    }

    unsafe extern "system" fn dpi_fixture(
        hwnd: HWND,
        message: u32,
        w: WPARAM,
        l: LPARAM,
        _: usize,
        data: usize,
    ) -> LRESULT {
        // Mirrors pinned GPUI's real WM_DPICHANGED -> suggested RECT -> nested
        // WM_SIZE path. Cells are snapshots, never guards across SetWindowPos.
        unsafe {
            let state = &*(data as *const DpiFixture);
            if message == WM_DPICHANGED {
                state.changes.set(state.changes.get() + 1);
                let rect = *(l as *const RECT);
                let placed = SetWindowPos(
                    hwnd,
                    null_mut(),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                state.failed.set(placed == 0);
                return 0;
            }
            if message == WM_SIZE {
                let mut rect: RECT = zeroed();
                GetClientRect(hwnd, &mut rect);
                state
                    .size
                    .set((GetDpiForWindow(hwnd), rect.right, rect.bottom));
            }
            DefSubclassProc(hwnd, message, w, l)
        }
    }

    unsafe extern "system" fn collect_monitors(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> i32 {
        unsafe {
            (*(data as *mut Vec<HMONITOR>)).push(monitor);
        }
        1
    }

    #[test]
    fn first_hidden_placement_across_real_mixed_dpi_monitors_keeps_canonical_viewport() {
        unsafe {
            let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            assert!(!previous.is_null());
            let _awareness = Awareness(previous);
            let mut monitors = Vec::new();
            assert_ne!(
                EnumDisplayMonitors(
                    null_mut(),
                    null(),
                    Some(collect_monitors),
                    (&mut monitors as *mut Vec<HMONITOR>) as LPARAM
                ),
                0
            );
            let placements: Vec<_> = monitors
                .into_iter()
                .map(|monitor| monitor_placement(monitor).unwrap())
                .collect();
            let Some((first, second)) = placements.iter().find_map(|first| {
                placements
                    .iter()
                    .find(|second| second.dpi != first.dpi)
                    .map(|second| (*first, *second))
            }) else {
                eprintln!("Mixed-DPI native regression requires two monitors with different DPI");
                return;
            };
            let state = DpiFixture::default();
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                windows_sys::w!("STATIC"),
                windows_sys::w!(""),
                WS_POPUP,
                first.x,
                first.y,
                first.width as i32,
                first.height as i32,
                null_mut(),
                null_mut(),
                null_mut(),
                null(),
            );
            assert!(!hwnd.is_null());
            // Destroy the subclass/window before its stack-owned callback data.
            let _window = HiddenWindow(hwnd);
            assert_ne!(
                SetWindowSubclass(
                    hwnd,
                    Some(dpi_fixture),
                    SUBCLASS + 1,
                    (&state as *const DpiFixture) as usize
                ),
                0
            );
            let foreground = GetForegroundWindow();
            for target in [second, first, second, first] {
                let changes = state.changes.get();
                position_foreground(hwnd, target).unwrap();
                assert!(!state.failed.get());
                assert!(
                    state.changes.get() > changes,
                    "must exercise real WM_DPICHANGED"
                );
                let mut client: RECT = zeroed();
                let mut origin = POINT { x: 0, y: 0 };
                assert_ne!(GetClientRect(hwnd, &mut client), 0);
                assert_ne!(ClientToScreen(hwnd, &mut origin), 0);
                assert_eq!(GetDpiForWindow(hwnd), target.dpi);
                assert_eq!((origin.x, origin.y), (target.x, target.y));
                assert_eq!(
                    (client.right, client.bottom),
                    (target.width as i32, target.height as i32),
                    "the first show must not need a second placement to repair its size"
                );
                assert_eq!(
                    state.size.get(),
                    (target.dpi, target.width as i32, target.height as i32),
                    "renderer resize must receive canonical pixels at the new DPI"
                );
                assert_eq!(IsWindowVisible(hwnd), 0);
                assert_eq!(GetForegroundWindow(), foreground);
            }
        }
    }

    #[test]
    fn shell_configures_hidden_window_and_blocks_mouse_activation() {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                windows_sys::w!("STATIC"),
                windows_sys::w!(""),
                // Match the pinned GPUI PopUp backend's zero creation style.
                // Windows supplies an overlapped non-client frame for it.
                WS_OVERLAPPED,
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
            assert_eq!(
                GetWindowLongPtrW(hwnd, GWL_STYLE) as u32
                    & (WS_CAPTION | WS_THICKFRAME | WS_BORDER | WS_DLGFRAME),
                0,
                "disabling DWM decoration alone must not leave invisible client insets"
            );
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
                            transparency,
                            ..OverlayAppearance::default()
                        })
                        .unwrap()
                );
                assert!(shell.context.is_none());
                assert_eq!(IsWindowVisible(hwnd), 0);
                let anchor = shell.geometry.unwrap();
                // Exercise animation + geometry feedback without revealing a window.
                shell.visible = true;
                for opacity in [0., 0.4, 0.8, 1., 1., 1., 0.8, 0.4, 0.] {
                    assert!(!shell.animate(opacity));
                    assert!(!shell.sync_geometry().unwrap());
                    let mut rect: RECT = zeroed();
                    assert_ne!(GetWindowRect(hwnd, &mut rect), 0);
                    assert_eq!(rect.left, anchor.x);
                    assert_eq!(rect.top, anchor.y);
                    let mut client: RECT = zeroed();
                    let mut origin = POINT { x: 0, y: 0 };
                    assert_ne!(GetClientRect(hwnd, &mut client), 0);
                    assert_ne!(ClientToScreen(hwnd, &mut origin), 0);
                    assert_eq!((origin.x, origin.y), (rect.left, rect.top));
                    assert_eq!(client.right - client.left, rect.right - rect.left);
                    assert_eq!(client.bottom - client.top, rect.bottom - rect.top);
                    assert_eq!(
                        anchor.height,
                        (HEIGHT * anchor.dpi as f32 / 96.).round() as usize
                    );
                    assert_eq!(IsWindowVisible(hwnd), 0);
                    assert!(shell.geometry == Some(anchor));
                }
                shell.visible = false;
            }
            assert_eq!(GetForegroundWindow(), foreground);
            drop(shell);
            assert_ne!(IsWindow(hwnd), 0, "GPUI, not the adapter, owns the HWND");
            DestroyWindow(hwnd);
        }
    }
}
