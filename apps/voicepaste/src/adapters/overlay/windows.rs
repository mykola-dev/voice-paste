//! UI-thread-owned, per-pixel-alpha native overlay. No audio, storage or STT IO.
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    time::Duration,
};
use voicepaste_core::{
    State,
    settings::OverlayAppearance,
    waveform::{Measurement, Waveform, premultiply},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};
mod backdrop;
use backdrop::{Backdrop, Context as BackdropContext};

const WIDTH: f64 = 224.0;
const HEIGHT: f64 = 100.0;
const CORNER_RADIUS: f64 = 16.0;
// Supersample only the small timer mask, never the whole animated surface.
const TEXT_SAMPLES: usize = 3;
pub const FRAME_TIMER: usize = 2;

struct DpiContext(DPI_AWARENESS_CONTEXT);
impl DpiContext {
    fn enter() -> Self {
        // SAFETY: restored on this same GUI thread before returning to settings.
        Self(unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) })
    }
}
impl Drop for DpiContext {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }
}

struct Dib {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    width: usize,
    height: usize,
}
impl Dib {
    fn new(width: usize, height: usize) -> Result<Self, String> {
        // SAFETY: owned memory DC and top-down 32-bit DIB; selection restored before deletion.
        unsafe {
            let mut value = Self {
                dc: CreateCompatibleDC(null_mut()),
                bitmap: null_mut(),
                old: null_mut(),
                bits: null_mut(),
                width,
                height,
            };
            if value.dc.is_null() {
                return Err("Cannot create overlay DC".into());
            }
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = width as i32;
            info.bmiHeader.biHeight = -(height as i32);
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits = null_mut();
            value.bitmap =
                CreateDIBSection(value.dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            value.bits = bits.cast();
            if value.bitmap.is_null() || value.bits.is_null() {
                return Err("Cannot allocate overlay pixels".into());
            }
            value.old = SelectObject(value.dc, value.bitmap);
            if value.old.is_null() || value.old as isize == -1 {
                return Err("Cannot select overlay pixels".into());
            }
            Ok(value)
        }
    }
    fn pixels(&mut self) -> &mut [u32] {
        // SAFETY: uniquely borrowed live DIB, exact pixel capacity. GDI has been
        // flushed before reading text pixels; main pixels are CPU-written only.
        unsafe { std::slice::from_raw_parts_mut(self.bits, self.width * self.height) }
    }
}
impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            if !self.old.is_null() && self.old as isize != -1 {
                SelectObject(self.dc, self.old);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
        }
    }
}

struct Surface {
    image: Dib,
    text: Dib,
    font: HFONT,
    old_font: HGDIOBJ,
    scale: f64,
    wave: Vec<u32>,
    background: Vec<u32>,
    timer_mask: Vec<u8>,
    timer_width: usize,
    timer_height: usize,
    timer_seconds: Option<u64>,
}
impl Surface {
    fn new(dpi: u32) -> Result<Self, String> {
        let scale = dpi as f64 / 96.0;
        let width = (WIDTH * scale).round() as usize;
        let height = (HEIGHT * scale).round() as usize;
        let timer_width = (66.0 * scale).round() as usize;
        let timer_height = (22.0 * scale).round() as usize;
        let mut value = Self {
            image: Dib::new(width, height)?,
            text: Dib::new(timer_width * TEXT_SAMPLES, timer_height * TEXT_SAMPLES)?,
            font: null_mut(),
            old_font: null_mut(),
            scale,
            wave: vec![0; width * (30.0 * scale).round() as usize],
            background: vec![0; width * height],
            timer_mask: vec![0; timer_width * timer_height],
            timer_width,
            timer_height,
            timer_seconds: None,
        };
        // Grayscale AA, not ClearType: each glyph's coverage becomes alpha over
        // the translucent body. No opaque GDI text rectangle in the layered HWND.
        unsafe {
            value.font = CreateFontW(
                -(16.0 * scale * TEXT_SAMPLES as f64).round() as i32,
                0,
                0,
                0,
                600,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                ANTIALIASED_QUALITY as u32,
                DEFAULT_PITCH as u32,
                windows_sys::w!("Segoe UI"),
            );
            if value.font.is_null() {
                return Err("Cannot create overlay timer font".into());
            }
            value.old_font = SelectObject(value.text.dc, value.font);
            SetTextColor(value.text.dc, 0xffffff);
            SetBkColor(value.text.dc, 0);
            SetBkMode(value.text.dc, TRANSPARENT as i32);
        }
        value.background(200);
        Ok(value)
    }
    fn background(&mut self, body_alpha: u8) {
        let width = self.image.width;
        let height = self.image.height;
        let scale = self.scale;
        for y in 0..height {
            for x in 0..width {
                let px = (x as f64 + 0.5) / scale;
                let py = (y as f64 + 0.5) / scale;
                let outer = rounded_coverage(px, py, 0.0, 0.0, WIDTH, HEIGHT, CORNER_RADIUS, scale);
                let inner = rounded_coverage(
                    px,
                    py,
                    1.0,
                    1.0,
                    WIDTH - 2.0,
                    HEIGHT - 2.0,
                    CORNER_RADIUS - 1.0,
                    scale,
                );
                // Slightly more transparent body (78%, previously 88%); keep
                // timer, waveform and recording indicator independently sharp.
                let outer_alpha = (255.0 * outer).round() as u8;
                let border_alpha = (255.0 * (outer - inner).max(0.0)).round() as u8;
                // At 0% transparency independent rounding can sum to 256 at an
                // antialiased edge. Quantize within the outer coverage budget.
                let body_coverage = ((f64::from(body_alpha) * inner).round() as u8)
                    .min(outer_alpha.saturating_sub(border_alpha));
                let body = premultiply(0xffffff, body_coverage);
                let border = premultiply(0x999999, border_alpha);
                // Adjacent coverage, not two overlaid translucent layers.
                self.background[y * width + x] = body + border;
            }
        }
    }
    fn use_acrylic_background(&mut self) {
        // Tint now belongs entirely to the material so the shared transparency
        // percentage has no hidden extra wash. Border/content remain separate.
        self.background(0);
    }
    fn update_timer(&mut self, seconds: u64) {
        if self.timer_seconds == Some(seconds) {
            return;
        }
        let timer: Vec<u16> = format!("{:02}:{:02}", seconds / 60, seconds % 60)
            .encode_utf16()
            .collect();
        self.text.pixels().fill(0);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: self.text.width as i32,
            bottom: self.text.height as i32,
        };
        unsafe {
            DrawTextW(
                self.text.dc,
                timer.as_ptr(),
                timer.len() as i32,
                &mut rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
            GdiFlush();
        }
        let stride = self.text.width;
        let source = self.text.pixels();
        for y in 0..self.timer_height {
            for x in 0..self.timer_width {
                let mut coverage = 0;
                for dy in 0..TEXT_SAMPLES {
                    for dx in 0..TEXT_SAMPLES {
                        coverage +=
                            source[(y * TEXT_SAMPLES + dy) * stride + x * TEXT_SAMPLES + dx] & 255;
                    }
                }
                self.timer_mask[y * self.timer_width + x] =
                    ((coverage + 4) / (TEXT_SAMPLES * TEXT_SAMPLES) as u32) as u8;
            }
        }
        self.timer_seconds = Some(seconds);
    }
    fn render(&mut self, state: State, elapsed: Duration, wave: &Waveform, rgb: u32) {
        let width = self.image.width;
        let height = self.image.height;
        let scale = self.scale;
        if matches!(state, State::Recording(_)) {
            self.update_timer(elapsed.as_secs());
        }
        let pixels = self.image.pixels();
        pixels.copy_from_slice(&self.background);
        let time = elapsed.as_secs_f64();
        if matches!(state, State::Recording(_)) {
            let opacity = 1.0 - 0.7 * triangle(time, 0.8);
            for y in (20.0 * scale).floor() as usize..(36.0 * scale).ceil() as usize {
                for x in (76.0 * scale).floor() as usize..(92.0 * scale).ceil() as usize {
                    let distance = ((x as f64 + 0.5 - 84.0 * scale).powi(2)
                        + (y as f64 + 0.5 - 28.0 * scale).powi(2))
                    .sqrt();
                    let alpha = ((6.0 * scale + 0.5 - distance).clamp(0.0, 1.0) * 255.0 * opacity)
                        .round() as u8;
                    blend(&mut pixels[y * width + x], premultiply(0xff0000, alpha));
                }
            }
            let left = (98.0 * scale).round() as usize;
            let top = (17.0 * scale).round() as usize;
            for y in 0..self.timer_height {
                for x in 0..self.timer_width {
                    blend(
                        &mut pixels[(top + y) * width + left + x],
                        premultiply(0x111111, self.timer_mask[y * self.timer_width + x]),
                    );
                }
            }
            let wave_height = self.wave.len() / width;
            wave.rasterize(width, wave_height, 30.0, rgb, &mut self.wave);
            let start = (49.0 * scale).round() as usize;
            for y in 0..wave_height {
                for x in 0..width {
                    blend(
                        &mut pixels[(start + y) * width + x],
                        self.wave[y * width + x],
                    );
                }
            }
        } else {
            for bar in 0..5 {
                let size = busy_size(time, bar);
                let left = 93.0 + bar as f64 * 8.0;
                let top = 50.0 - 20.0 * size;
                for y in (30.0 * scale).floor() as usize..(70.0 * scale).ceil() as usize {
                    for x in (left * scale).floor() as usize..((left + 6.0) * scale).ceil() as usize
                    {
                        let vertical = ((y + 1) as f64).min((100.0 - top) * scale)
                            - (y as f64).max(top * scale);
                        let horizontal = ((x + 1) as f64).min((left + 6.0) * scale)
                            - (x as f64).max(left * scale);
                        blend(
                            &mut pixels[y * width + x],
                            premultiply(
                                rgb,
                                (255.0 * vertical.clamp(0.0, 1.0) * horizontal.clamp(0.0, 1.0))
                                    .round() as u8,
                            ),
                        );
                    }
                }
            }
        }
        debug_assert_eq!(pixels.len(), width * height);
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            if !self.old_font.is_null() {
                SelectObject(self.text.dc, self.old_font);
            }
            if !self.font.is_null() {
                DeleteObject(self.font);
            }
        }
    }
}
fn triangle(time: f64, half: f64) -> f64 {
    let phase = (time / half).rem_euclid(2.0);
    if phase <= 1.0 { phase } else { 2.0 - phase }
}
fn busy_size(time: f64, bar: usize) -> f64 {
    // Slow the entire transcription timeline by 30%, including bar staggering.
    let phase = time * 0.70 - bar as f64 * 0.10;
    0.4 + 0.6
        * if phase < 0.0 {
            0.0
        } else {
            triangle(phase, 0.24)
        }
}
#[allow(clippy::too_many_arguments)] // Geometry of one rounded rectangle, not a platform abstraction.
fn rounded_coverage(
    x: f64,
    y: f64,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    radius: f64,
    scale: f64,
) -> f64 {
    let dx = (x - (left + width / 2.0)).abs() - (width / 2.0 - radius);
    let dy = (y - (top + height / 2.0)).abs() - (height / 2.0 - radius);
    let distance = dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius;
    (0.5 - distance * scale).clamp(0.0, 1.0)
}
fn blend(destination: &mut u32, source: u32) {
    let inverse = 255 - (source >> 24);
    let channel = |shift: u32| {
        ((source >> shift) & 255) + (((((*destination >> shift) & 255) * inverse) + 127) / 255)
    };
    *destination = (channel(24) << 24) | (channel(16) << 16) | (channel(8) << 8) | channel(0);
}

pub struct Overlay {
    hwnd: HWND,
    owner: HWND,
    state: State,
    preparing: bool,
    visible: bool,
    started: Duration,
    wave: Waveform,
    surface: Option<Surface>,
    position: POINT,
    rgb: u32,
    backdrop: Option<Backdrop>,
    backdrop_context: Option<BackdropContext>,
    appearance: OverlayAppearance,
}
impl Overlay {
    pub fn new(owner: HWND, color: &str, now: Duration) -> Result<Self, String> {
        let _dpi = DpiContext::enter();
        unsafe {
            let instance = GetModuleHandleW(null());
            let mut class: WNDCLASSW = zeroed();
            class.hInstance = instance;
            class.hIcon = crate::adapters::application_icon::load()?;
            class.lpfnWndProc = Some(overlay_proc);
            class.lpszClassName = windows_sys::w!("VoicePasteRustOverlay");
            if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_LAYERED
                    | WS_EX_TRANSPARENT,
                class.lpszClassName,
                windows_sys::w!("VoicePaste Rust status"),
                WS_POPUP,
                0,
                0,
                224,
                100,
                owner,
                null_mut(),
                instance,
                null(),
            );
            if hwnd.is_null() {
                return Err("Cannot create recording overlay".into());
            }
            let mut value = Self {
                hwnd,
                owner,
                state: State::Idle,
                preparing: false,
                visible: false,
                started: now,
                wave: Waveform::new(now),
                surface: None,
                position: POINT { x: 0, y: 0 },
                rgb: 0,
                backdrop: None,
                backdrop_context: None,
                appearance: OverlayAppearance::default(),
            };
            value.color(color);
            Ok(value)
        }
    }
    pub fn color(&mut self, color: &str) {
        if let Some(hex) = color.strip_prefix('#')
            && let Ok(rgb) = u32::from_str_radix(hex, 16)
        {
            self.rgb = rgb;
        }
    }
    pub fn appearance(&mut self, appearance: OverlayAppearance) {
        // Cache the next-show policy only: never rebuild an active recording's
        // surfaces or material in response to a Settings save.
        self.appearance = appearance;
    }
    pub fn update(&mut self, state: State, preparing: bool, now: Duration) -> Result<(), String> {
        let active = state != State::Idle || preparing;
        if state != self.state {
            self.wave.reset(now);
            self.started = now;
        }
        self.state = state;
        self.preparing = preparing;
        if !active {
            if self.visible {
                unsafe {
                    KillTimer(self.owner, FRAME_TIMER);
                    ShowWindow(self.hwnd, SW_HIDE);
                }
                if let Some(backdrop) = &self.backdrop {
                    backdrop.hide();
                }
                self.wave.reset(now);
            }
            self.visible = false;
            return Ok(());
        }
        if !self.visible {
            self.started = now;
            self.place()?;
            self.tick(now)?;
            unsafe {
                ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                // A 16-ms WM_TIMER can be rounded to ~31 ms by Windows' default
                // clock. The minimum timer interval avoids that half-rate step;
                // waveform positions still come from the monotonic clock.
                if SetTimer(self.owner, FRAME_TIMER, USER_TIMER_MINIMUM, None) == 0 {
                    return Err("Cannot start overlay frame timer".into());
                }
            }
            if self
                .backdrop
                .as_ref()
                .is_some_and(|backdrop| !backdrop.show_behind(self.hwnd))
            {
                self.backdrop.take();
                if let Some(surface) = &mut self.surface {
                    surface.background(self.appearance.background_alpha());
                }
                self.tick(now)?;
            }
            self.visible = true;
        }
        Ok(())
    }
    fn place(&mut self) -> Result<(), String> {
        let _dpi = DpiContext::enter();
        unsafe {
            let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY);
            let mut info: MONITORINFO = zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return Err("Cannot locate overlay work area".into());
            }
            let mut x = 96;
            let mut y = 96;
            let result = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y);
            if result < 0 {
                return Err(format!("Cannot read monitor DPI: {result}"));
            }
            let mut surface = Surface::new(x)?;
            let alpha = self.appearance.background_alpha();
            surface.background(alpha);
            let margin = (20.0 * surface.scale).round() as i32;
            self.position = POINT {
                x: info.rcWork.right - margin - surface.image.width as i32,
                y: info.rcWork.top + margin,
            };
            // Re-evaluate system preferences at each show, not on every frame.
            self.backdrop.take();
            if self.appearance.acrylic && alpha > 0 {
                if self.backdrop_context.is_none() {
                    self.backdrop_context = BackdropContext::new().ok();
                }
                self.backdrop = self
                    .backdrop_context
                    .as_ref()
                    .and_then(|context| Backdrop::new(self.owner, context, alpha));
            }
            // DWM material lives on an ordinary HWND behind layered content.
            if let Some(backdrop) = &self.backdrop {
                if backdrop.place(
                    self.position,
                    surface.image.width,
                    surface.image.height,
                    surface.scale,
                ) {
                    surface.use_acrylic_background();
                } else {
                    self.backdrop.take(); // retain the exact previous fallback pixels
                }
            }
            if SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                self.position.x,
                self.position.y,
                surface.image.width as i32,
                surface.image.height as i32,
                SWP_NOACTIVATE,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            self.surface = Some(surface);
            Ok(())
        }
    }
    pub fn measurement(&mut self, measurement: Measurement, now: Duration) {
        if self.state == State::Recording(measurement.session) {
            self.wave.advance(now);
            self.wave.add(measurement.timestamp, measurement.level);
        }
    }
    pub fn tick(&mut self, now: Duration) -> Result<(), String> {
        let _dpi = DpiContext::enter();
        self.wave.advance(now);
        let Some(surface) = &mut self.surface else {
            return Ok(());
        };
        surface.render(
            self.state,
            now.saturating_sub(self.started),
            &self.wave,
            self.rgb,
        );
        let size = SIZE {
            cx: surface.image.width as i32,
            cy: surface.image.height as i32,
        };
        let origin = POINT { x: 0, y: 0 };
        let alpha = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: live selected top-down premultiplied DIB; synchronous copy,
        // physical pixels under the overlay's temporary DPI-awareness context.
        if unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                null_mut(),
                &self.position,
                &size,
                surface.image.dc,
                &origin,
                0,
                &alpha,
                ULW_ALPHA,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }
}
impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            KillTimer(self.owner, FRAME_TIMER);
            DestroyWindow(self.hwnd);
        }
    }
}
unsafe extern "system" fn overlay_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // Nonactivation covers client and nonclient clicks; pass-through is best
    // effort and is not required for correctness or claimed across all apps.
    unsafe {
        match message {
            WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
            WM_NCHITTEST => HTTRANSPARENT as isize,
            WM_ERASEBKGND => 1,
            _ => DefWindowProcW(hwnd, message, w, l),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voicepaste_core::SessionId;

    #[test]
    fn acrylic_foreground_has_no_extra_wash_and_can_restore_exact_fallback() {
        for dpi in [96, 144, 168, 192] {
            let mut surface = Surface::new(dpi).unwrap();
            let original = surface.background.clone();
            let center = surface.image.height / 2 * surface.image.width + surface.image.width / 2;
            surface.use_acrylic_background();
            assert_eq!(
                surface.background[center], 0,
                "no hidden wash on top of the shared material tint"
            );
            for (before, after) in original.iter().zip(&surface.background) {
                if *before == 0 {
                    assert_eq!(*after, 0, "transparent corners stay transparent");
                }
            }
            surface.background(200);
            assert_eq!(surface.background, original);
        }
    }

    #[test]
    fn ordinary_and_fully_transparent_modes_do_not_create_material_resources() {
        let mut view = Overlay::new(null_mut(), "#E9563D", Duration::ZERO).unwrap();
        for (acrylic, transparency) in [(false, 57), (true, 100)] {
            view.appearance(OverlayAppearance {
                acrylic,
                transparency,
            });
            view.place().unwrap();
            assert!(view.backdrop.is_none());
            assert!(view.backdrop_context.is_none());
            assert!(!view.visible);
            assert_eq!(unsafe { IsWindowVisible(view.hwnd) }, 0);
            let surface = view.surface.as_ref().unwrap();
            let center = surface.image.height / 2 * surface.image.width + surface.image.width / 2;
            assert_eq!(
                surface.background[center] >> 24,
                u32::from(view.appearance.background_alpha())
            );
        }
    }

    #[test]
    fn shared_transparency_changes_only_body_not_border_or_busy_content() {
        for dpi in [96, 144, 168, 192] {
            let mut surface = Surface::new(dpi).unwrap();
            let wave = Waveform::new(Duration::ZERO);
            let border_index = surface.image.width / 2;
            let border = surface.background[border_index];
            for transparency in [0, 57, 100] {
                let alpha = OverlayAppearance {
                    acrylic: false,
                    transparency,
                }
                .background_alpha();
                surface.background(alpha);
                let center =
                    surface.image.height / 2 * surface.image.width + surface.image.width / 2;
                assert_eq!(surface.background[center] >> 24, u32::from(alpha));
                assert_eq!(surface.background[0], 0);
                assert_eq!(surface.background[border_index], border);
                surface.render(
                    State::Transcribing(SessionId(1)),
                    Duration::from_millis(500),
                    &wave,
                    0xe9563d,
                );
                let index = (50.0 * surface.scale) as usize * surface.image.width
                    + (95.0 * surface.scale) as usize;
                assert_eq!(surface.image.pixels()[index], 0xffe9563d);
            }
        }
    }

    #[test]
    fn overlay_uses_doubled_logical_corner_radius_at_each_dpi() {
        for dpi in [96, 144, 168, 192] {
            let surface = Surface::new(dpi).unwrap();
            let x = (3.0 * surface.scale) as usize;
            let y = (3.0 * surface.scale) as usize;
            assert_eq!(surface.background[y * surface.image.width + x], 0);
            let center = surface.background
                [(surface.image.height / 2) * surface.image.width + surface.image.width / 2];
            assert_eq!(center >> 24, 200, "Body transparency must not change");
        }
    }

    #[test]
    fn busy_animation_runs_at_70_percent_speed_including_stagger() {
        for bar in 0..5 {
            let offset = bar as f64 * 0.10;
            for (time, expected) in [(0.0, 0.4), (0.12, 0.7), (0.24, 1.0), (0.48, 0.4)] {
                assert!((busy_size((offset + time) / 0.70, bar) - expected).abs() < 1e-9);
            }
        }
        assert_eq!(busy_size(0.09, 1), 0.4);
        assert_eq!(busy_size(0.14, 1), 0.4);
        assert_eq!(triangle(0.8, 0.8), 1.0); // recording pulse unchanged
    }

    #[test]
    fn timer_has_fractional_coverage_and_busy_bars_use_selected_color() {
        for dpi in [96, 144, 192] {
            let mut surface = Surface::new(dpi).unwrap();
            let wave = Waveform::new(Duration::ZERO);
            surface.render(
                State::Recording(SessionId(1)),
                Duration::ZERO,
                &wave,
                0x00ff00,
            );
            let mut coverage = surface.timer_mask.clone();
            coverage.sort_unstable();
            coverage.dedup();
            assert!(
                coverage.len() > 16,
                "Timer must retain grayscale edge coverage"
            );
            let cached = surface.timer_mask.clone();
            surface.render(
                State::Recording(SessionId(1)),
                Duration::from_millis(500),
                &wave,
                0x00ff00,
            );
            assert_eq!(surface.timer_mask, cached);
            surface.render(
                State::Transcribing(SessionId(1)),
                Duration::from_millis(500),
                &wave,
                0x00ff00,
            );
            let x = (95.0 * surface.scale) as usize;
            let y = (50.0 * surface.scale) as usize;
            let index = y * surface.image.width + x;
            assert_eq!(surface.image.pixels()[index], 0xff00ff00);
        }
    }
}
