//! UI-thread owner of the hidden host, tray icons, hook, timer and registrations.
use super::{events, tray_menu, wide};
use crate::adapters::platform::{inbox::Pending, menu::TrayAction};
use crate::adapters::{
    application_icon,
    hotkey::Binding,
    paste::FocusTarget,
    tray_icons::{self, Lips},
};
use std::{
    cell::Cell,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use voicepaste_core::{State, settings::Hotkey};
use windows_sys::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Shell::*, WindowsAndMessaging::*},
};

const TRAY_TOOLTIP: &str = "Voice Paste";

#[derive(Default)]
struct TrayIconCache(Cell<Option<Lips>>);

impl TrayIconCache {
    fn invalidate(&self) {
        self.0.set(None);
    }

    fn applied(&self, icon: Lips) {
        self.0.set(Some(icon));
    }

    fn update(&self, icon: Lips, send: impl FnOnce() -> bool) {
        if self.0.get() == Some(icon) {
            return;
        }
        self.invalidate();
        if send() {
            self.applied(icon);
        }
    }
}

#[derive(Default)]
struct TrayRestore(Cell<bool>);

impl TrayRestore {
    fn request(&self, cache: &TrayIconCache) {
        self.0.set(true);
        cache.invalidate();
    }

    fn attempt(&self, cache: &TrayIconCache, icon: Lips, send: impl FnOnce() -> bool) {
        if self.0.get() && send() {
            self.0.set(false);
            cache.applied(icon);
        }
    }
}

pub(in crate::adapters::platform) struct Host {
    hwnd: HWND,
    hook: HHOOK,
    idle: HICON,
    busy: HICON,
    icon_size: usize,
    tray_icon: TrayIconCache,
    restore: TrayRestore,
    active_key: Option<Binding>,
    pending_key: Option<Binding>,
    _class: WindowClass,
}

struct WindowClass {
    instance: HINSTANCE,
    name: Vec<u16>,
}
impl WindowClass {
    fn register(instance: HINSTANCE, name: Vec<u16>, icon: HICON) -> Result<Self, String> {
        let mut class: WNDCLASSW = unsafe { zeroed() };
        class.lpfnWndProc = Some(events::window_proc);
        class.hInstance = instance;
        class.hIcon = icon;
        class.lpszClassName = name.as_ptr();
        // SAFETY: owned name lives through registration and subsequent teardown.
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(Self { instance, name })
    }
}
impl Drop for WindowClass {
    fn drop(&mut self) {
        // SAFETY: the Host destroys its HWND before this owned class is dropped.
        unsafe {
            UnregisterClassW(self.name.as_ptr(), self.instance);
        }
    }
}

impl Host {
    pub(in crate::adapters::platform) fn create(
        hotkey: Hotkey,
        register: bool,
    ) -> Result<Self, String> {
        // SAFETY: handles are owned and used on the GPUI message-loop thread.
        unsafe {
            let instance = GetModuleHandleW(null());
            let class_name = wide("VoicePasteRustHost");
            let class = WindowClass::register(instance, class_name, application_icon::load()?)?;
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.name.as_ptr(),
                wide("VoicePaste Rust").as_ptr(),
                0,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            if hwnd.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let mut window = Self {
                hwnd,
                hook: null_mut(),
                idle: null_mut(),
                busy: null_mut(),
                icon_size: tray_icons::system_icon_size(hwnd),
                tray_icon: TrayIconCache::default(),
                restore: TrayRestore::default(),
                active_key: None,
                pending_key: None,
                _class: class,
            };
            window.rebuild_icons(instance, window.icon_size)?;
            if register {
                let binding = Binding::reserve(hwnd, 1, hotkey)?;
                binding.activate();
                window.active_key = Some(binding);
            }
            events::attach(hwnd)?;
            window.hook =
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(events::keyboard_hook), instance, 0);
            if window.hook.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let mut data = window.tray_data();
            data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            data.uCallbackMessage = events::TRAY;
            data.hIcon = window.idle;
            put(&mut data.szTip, TRAY_TOOLTIP);
            if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
                return Err("Cannot create tray icon".into());
            }
            window.tray_icon.applied(Lips::Bitten);
            if SetTimer(hwnd, 1, 50, None) == 0 {
                return Err(format!(
                    "Cannot create host timer: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(window)
        }
    }

    pub(in crate::adapters::platform) fn drain(&self) -> Pending<FocusTarget> {
        let pending = events::drain();
        if events::take_delivery_warning() {
            self.notify(
                "Windows message posting failed; input was queued directly where possible.",
                true,
            );
        }
        pending
    }

    pub(in crate::adapters::platform) fn accepts_hotkey(&self, id: usize) -> bool {
        self.active_key
            .as_ref()
            .is_some_and(|key| id == key.id as usize)
    }

    pub(in crate::adapters::platform) fn menu(
        &self,
        state: State,
    ) -> Result<(Option<TrayAction>, FocusTarget), String> {
        tray_menu(self.hwnd, state)
    }

    pub(in crate::adapters::platform) fn stage_hotkey(
        &mut self,
        key: Hotkey,
    ) -> Result<(), String> {
        if self.pending_key.is_some() {
            return Err("A hotkey change is already being saved".into());
        }
        if self
            .active_key
            .as_ref()
            .is_some_and(|active| active.key == key)
        {
            return Ok(());
        }
        let id = if self
            .active_key
            .as_ref()
            .is_some_and(|active| active.id == 1)
        {
            2
        } else {
            1
        };
        // Reserve before persistence; old binding stays live until commit.
        self.pending_key = Some(Binding::reserve(self.hwnd, id, key)?);
        Ok(())
    }

    pub(in crate::adapters::platform) fn finish_hotkey(&mut self, success: bool) {
        if let Some(binding) = self.pending_key.take()
            && success
        {
            binding.activate();
            self.active_key = Some(binding);
        }
    }

    fn rebuild_icons(&mut self, instance: HINSTANCE, size: usize) -> Result<(), String> {
        // SAFETY: owned icons are replaced only after both allocations succeed.
        unsafe {
            let idle = tray_icons::create_icon(instance, size, Lips::Bitten)?;
            let busy = match tray_icons::create_icon(instance, size, Lips::Open) {
                Ok(icon) => icon,
                Err(error) => {
                    DestroyIcon(idle);
                    return Err(error);
                }
            };
            if !self.idle.is_null() {
                DestroyIcon(self.idle);
            }
            if !self.busy.is_null() {
                DestroyIcon(self.busy);
            }
            self.idle = idle;
            self.busy = busy;
            self.icon_size = size;
            self.tray_icon.invalidate();
        }
        Ok(())
    }

    pub(in crate::adapters::platform) fn ensure_dpi(&mut self) -> Result<bool, String> {
        let size = tray_icons::system_icon_size(self.hwnd);
        if size == self.icon_size {
            return Ok(false);
        }
        // SAFETY: borrowed process module handle.
        self.rebuild_icons(unsafe { GetModuleHandleW(null()) }, size)?;
        Ok(true)
    }

    pub(in crate::adapters::platform) fn model_status(
        &self,
        _message: &str,
        preparing: bool,
        state: State,
    ) {
        self.update_icon(tray_face(state, preparing));
    }

    fn tray_data(&self) -> NOTIFYICONDATAW {
        // SAFETY: zero is valid initialization for this Win32 POD.
        let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = 1;
        data
    }

    pub(in crate::adapters::platform) fn state(&self, state: State) {
        events::recording(state);
        self.update_icon(Lips::from(state));
    }

    fn icon_data(&self, icon: Lips) -> NOTIFYICONDATAW {
        let mut data = self.tray_data();
        data.uFlags = NIF_ICON | NIF_TIP;
        data.hIcon = match icon {
            Lips::Open => self.busy,
            Lips::Bitten => self.idle,
        };
        put(&mut data.szTip, TRAY_TOOLTIP);
        data
    }

    fn update_icon(&self, icon: Lips) {
        self.tray_icon.update(icon, || {
            let data = self.icon_data(icon);
            // SAFETY: owned inline buffers and live icon handles; no borrow held.
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) != 0 }
        });
    }

    pub(in crate::adapters::platform) fn restore_tray(&self, icon: Lips) {
        if events::take_tray_restore() {
            self.restore.request(&self.tray_icon);
        }
        self.restore.attempt(&self.tray_icon, icon, || {
            let mut data = self.icon_data(icon);
            data.uFlags |= NIF_MESSAGE;
            data.uCallbackMessage = events::TRAY;
            // SAFETY: same-thread tray identifier and live icon; no borrow held.
            unsafe { Shell_NotifyIconW(NIM_ADD, &data) != 0 }
        });
    }

    pub(in crate::adapters::platform) fn notify(&self, message: &str, error: bool) {
        let mut data = self.tray_data();
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = if error { NIIF_ERROR } else { NIIF_INFO };
        put(&mut data.szInfoTitle, "Voice Paste");
        put(&mut data.szInfo, message);
        // SAFETY: inline buffers and live tray identifier.
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
}

pub(super) fn tray_face(state: State, preparing: bool) -> Lips {
    if state == State::Idle && preparing {
        Lips::Open
    } else {
        Lips::from(state)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        events::detach();
        // SAFETY: same-thread resources; detach/unhook before registrations/HWND.
        unsafe {
            if !self.hook.is_null() {
                UnhookWindowsHookEx(self.hook);
            }
            self.pending_key.take();
            self.active_key.take();
            KillTimer(self.hwnd, 1);
            Shell_NotifyIconW(NIM_DELETE, &self.tray_data());
            DestroyWindow(self.hwnd);
            if !self.idle.is_null() {
                DestroyIcon(self.idle);
            }
            if !self.busy.is_null() {
                DestroyIcon(self.busy);
            }
        }
    }
}

fn put<const N: usize>(buffer: &mut [u16; N], text: &str) {
    buffer.fill(0);
    for (slot, unit) in buffer[..N - 1].iter_mut().zip(text.encode_utf16()) {
        *slot = unit;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_cache_skips_only_successful_unchanged_updates_and_retries_failures() {
        let cache = TrayIconCache::default();
        let calls = Cell::new(0);
        let send = |success| {
            calls.set(calls.get() + 1);
            success
        };
        cache.update(Lips::Bitten, || send(true));
        cache.update(Lips::Bitten, || send(true));
        assert_eq!(calls.get(), 1);
        cache.update(Lips::Open, || send(false));
        cache.update(Lips::Open, || send(true));
        cache.update(Lips::Open, || send(true));
        assert_eq!(calls.get(), 3);
        // DPI rebuild or a forced restore must republish even the same face.
        cache.invalidate();
        cache.update(Lips::Open, || send(true));
        assert_eq!(calls.get(), 4);
        cache.update(Lips::Bitten, || send(true));
        assert_eq!(calls.get(), 5);
    }

    #[test]
    fn tray_face_accounts_for_preparation_and_all_runtime_states() {
        let id = voicepaste_core::SessionId(1);
        assert_eq!(tray_face(State::Idle, false), Lips::Bitten);
        assert_eq!(tray_face(State::Idle, true), Lips::Open);
        for state in [State::Recording(id), State::Transcribing(id)] {
            assert_eq!(tray_face(state, false), Lips::Open);
            assert_eq!(tray_face(state, true), Lips::Open);
        }
    }

    #[test]
    fn taskbar_restore_retries_failure_and_republishes_the_current_icon() {
        let cache = TrayIconCache::default();
        let restore = TrayRestore::default();
        let calls = Cell::new(0);
        let send = |success| {
            calls.set(calls.get() + 1);
            success
        };
        cache.applied(Lips::Bitten);
        restore.attempt(&cache, Lips::Bitten, || send(true));
        assert_eq!(calls.get(), 0);
        restore.request(&cache);
        restore.attempt(&cache, Lips::Bitten, || send(false));
        restore.attempt(&cache, Lips::Open, || send(true));
        restore.attempt(&cache, Lips::Open, || send(true));
        cache.update(Lips::Open, || send(true));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn window_class_is_released_on_failed_setup_and_can_be_registered_again() {
        let instance = unsafe { GetModuleHandleW(null()) };
        let name = wide("VoicePasteRustOwnedClassTest");
        for _ in 0..2 {
            let class = WindowClass::register(instance, name.clone(), null_mut()).unwrap();
            let mut info: WNDCLASSW = unsafe { zeroed() };
            assert_ne!(
                unsafe { GetClassInfoW(instance, name.as_ptr(), &mut info) },
                0
            );
            drop(class);
            assert_eq!(
                unsafe { GetClassInfoW(instance, name.as_ptr(), &mut info) },
                0
            );
        }
    }
}
