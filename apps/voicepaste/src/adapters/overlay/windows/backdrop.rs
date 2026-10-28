//! Compositor-clipped frosted backdrop behind the existing native foreground.
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Dwm::*,
    System::{LibraryLoader::GetModuleHandleW, Registry::*},
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        WindowsAndMessaging::*,
    },
};
mod composition;
pub(super) use composition::Context;

pub(super) struct Backdrop(HWND, composition::Material);
impl Backdrop {
    pub(super) fn new(owner: HWND, context: &Context, alpha: u8) -> Option<Self> {
        // SAFETY: synchronous calls, stack-owned buffers and UI-thread-owned HWND.
        unsafe {
            if !effects_allowed() {
                return None;
            }
            let instance = GetModuleHandleW(null());
            let mut class: WNDCLASSW = zeroed();
            class.hInstance = instance;
            class.lpfnWndProc = Some(backdrop_proc);
            class.lpszClassName = windows_sys::w!("VoicePasteRustBackdrop");
            if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return None;
            }
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOREDIRECTIONBITMAP,
                class.lpszClassName,
                windows_sys::w!("VoicePaste Acrylic"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                owner,
                null_mut(),
                instance,
                null(),
            );
            if hwnd.is_null() {
                return None;
            }
            match composition::Material::new(context, hwnd, alpha) {
                Ok(material) => Some(Self(hwnd, material)),
                Err(_) => {
                    DestroyWindow(hwnd);
                    None
                }
            }
        }
    }
    pub(super) fn place(&self, position: POINT, width: usize, height: usize, scale: f64) -> bool {
        unsafe {
            if !effects_allowed() {
                return false;
            }
            if self.1.resize(width, height, scale).is_err() {
                return false;
            }
            SetWindowPos(
                self.0,
                HWND_TOPMOST,
                position.x,
                position.y,
                width as i32,
                height as i32,
                SWP_NOACTIVATE,
            ) != 0
        }
    }
    pub(super) fn show_behind(&self, foreground: HWND) -> bool {
        unsafe {
            SetWindowPos(
                self.0,
                foreground,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            ) != 0
        }
    }
    pub(super) fn hide(&self) {
        unsafe {
            ShowWindow(self.0, SW_HIDE);
        }
    }
}
impl Drop for Backdrop {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0);
        }
    }
}

unsafe fn effects_allowed() -> bool {
    unsafe {
        let mut composition = 0;
        if DwmIsCompositionEnabled(&mut composition) < 0 || composition == 0 {
            return false;
        }
        let mut contrast: HIGHCONTRASTW = zeroed();
        contrast.cbSize = size_of::<HIGHCONTRASTW>() as u32;
        if SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            (&mut contrast as *mut HIGHCONTRASTW).cast(),
            0,
        ) == 0
            || contrast.dwFlags & HCF_HIGHCONTRASTON != 0
        {
            return false;
        }
        let mut enabled: u32 = 1;
        let mut bytes = size_of::<u32>() as u32;
        let result = RegGetValueW(
            HKEY_CURRENT_USER,
            windows_sys::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            windows_sys::w!("EnableTransparency"),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut enabled as *mut u32).cast(),
            &mut bytes,
        );
        (result == ERROR_SUCCESS && enabled != 0) || result == ERROR_FILE_NOT_FOUND
    }
}
unsafe extern "system" fn backdrop_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
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

    #[test]
    fn enabled_windows_10_material_does_not_silently_skip_composition() {
        let version = windows_version::OsVersion::current();
        if version.major != 10
            || !(17763..22621).contains(&version.build)
            || !unsafe { effects_allowed() }
        {
            return;
        }
        let context = Context::new().expect("composition context on supported Windows 10");
        let host = Backdrop::new(null_mut(), &context, 110)
            .expect("enabled Windows 10 must exercise the clipped compositor path");
        assert_eq!(unsafe { IsWindowVisible(host.0) }, 0);
    }

    #[test]
    fn supported_material_host_is_hidden_nonactivating_and_released() {
        // Never show/focus a window. Unsupported OS/preferences use fallback.
        let context = Context::new().unwrap();
        let Some(host) = Backdrop::new(null_mut(), &context, 110) else {
            return;
        };
        let hwnd = host.0;
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            assert_ne!(style & WS_EX_NOACTIVATE, 0);
            assert_eq!(style & WS_EX_LAYERED, 0);
            assert_eq!(IsWindowVisible(hwnd), 0);
            assert_ne!(style & WS_EX_NOREDIRECTIONBITMAP, 0);
            for scale in [1.0, 1.5, 1.75, 2.0] {
                assert!(host.place(
                    POINT { x: 0, y: 0 },
                    (224.0 * scale) as usize,
                    (100.0 * scale) as usize,
                    scale
                ));
            }
        }
        drop(host);
        assert_eq!(unsafe { IsWindow(hwnd) }, 0);
    }
}
