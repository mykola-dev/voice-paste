//! Win32 ownership/callbacks. Commands are queued; frame callbacks do bounded UI work.
use super::hotkey::Binding;
use super::overlay::{FRAME_TIMER, Overlay};
use super::overlay_frames::Frames;
use super::settings_window::SettingsWindow;
use super::tray_icons::{self, Lips};
use crate::runtime::{Command, Event, Runtime};
use gpui_kit::QuitMode;
use std::{
    cell::RefCell,
    collections::VecDeque,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use voicepaste_core::{
    SessionId, State,
    settings::{Hotkey, Settings},
};
use windows_sys::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
};

const TRAY: u32 = WM_APP + 1;
const CANCEL: u32 = WM_APP + 2;
const TRAY_ACTION: u32 = WM_APP + 3;
const TOGGLE: u32 = 1;
const SETTINGS: u32 = 2;
const QUIT: u32 = 3;
const TRAY_TOOLTIP: &str = "Voice Paste";
static WINDOW: AtomicUsize = AtomicUsize::new(0);
static SESSION: AtomicU64 = AtomicU64::new(0);
static ESC_HELD: AtomicBool = AtomicBool::new(false);
static ESC_CAPTURED: AtomicBool = AtomicBool::new(false);

// Native callbacks never enter GPUI or borrow the host/application. Toggle
// transitions retain order in a bounded inbox; overflow is reported explicitly.
// Escape and Quit have their own slots and cannot be evicted.
#[derive(Default)]
struct PendingNative {
    hotkeys: VecDeque<(usize, usize)>,
    overflow: bool,
    cancel: Option<SessionId>,
    tray: bool,
    settings: bool,
    quit: bool,
}
thread_local! {
    static PENDING: RefCell<PendingNative> = RefCell::default();
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn put<const N: usize>(buffer: &mut [u16; N], text: &str) {
    buffer.fill(0);
    for (slot, unit) in buffer[..N - 1].iter_mut().zip(text.encode_utf16()) {
        *slot = unit;
    }
}

struct Window {
    hwnd: HWND,
    hook: HHOOK,
    idle: HICON,
    busy: HICON,
    icon_size: usize,
    active_key: Option<Binding>,
    pending_key: Option<Binding>,
}

impl Window {
    fn create(hotkey: Hotkey, register: bool) -> Result<Self, String> {
        // SAFETY: all Win32 handles are owned on this message-loop thread;
        // frame context is attached later, with a stable Box and a detach guard.
        unsafe {
            let instance = GetModuleHandleW(null());
            let class_name = wide("VoicePasteRustHost");
            let mut class: WNDCLASSW = zeroed();
            class.lpfnWndProc = Some(window_proc);
            class.hInstance = instance;
            class.hIcon = super::application_icon::load()?;
            class.lpszClassName = class_name.as_ptr();
            if RegisterClassW(&class) == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class_name.as_ptr(),
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
                active_key: None,
                pending_key: None,
            };
            window.rebuild_icons(instance)?;
            if register {
                let binding = Binding::reserve(hwnd, 1, hotkey)?;
                binding.activate();
                window.active_key = Some(binding);
            }
            WINDOW.store(hwnd as usize, Ordering::Release);
            ESC_HELD.store(GetAsyncKeyState(VK_ESCAPE as i32) < 0, Ordering::Release);
            window.hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), instance, 0);
            if window.hook.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let mut data = window.tray_data();
            data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            data.uCallbackMessage = TRAY;
            data.hIcon = window.idle;
            put(&mut data.szTip, TRAY_TOOLTIP);
            if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
                return Err("Cannot create tray icon".into());
            }
            SetTimer(hwnd, 1, 50, None);
            Ok(window)
        }
    }
    fn stage_hotkey(&mut self, key: Hotkey) -> Result<(), String> {
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
        // Keep the old binding live until persistence succeeds.
        self.pending_key = Some(Binding::reserve(self.hwnd, id, key)?);
        Ok(())
    }
    fn finish_hotkey(&mut self, success: bool) {
        if let Some(binding) = self.pending_key.take()
            && success
        {
            binding.activate();
            self.active_key = Some(binding);
        }
    }
    fn rebuild_icons(&mut self, instance: HINSTANCE) -> Result<(), String> {
        // SAFETY: old handles are owned by this thread and destroyed after
        // the replacements succeed, so the tray never points at a dead icon.
        unsafe {
            let idle = tray_icons::create_icon(instance, self.icon_size, Lips::Bitten)?;
            let busy = match tray_icons::create_icon(instance, self.icon_size, Lips::Open) {
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
        }
        Ok(())
    }
    fn ensure_dpi(&mut self) -> Result<bool, String> {
        let size = tray_icons::system_icon_size(self.hwnd);
        if size == self.icon_size {
            return Ok(false);
        }
        self.icon_size = size;
        // SAFETY: same contract as recolor.
        let instance = unsafe { GetModuleHandleW(null()) };
        self.rebuild_icons(instance)?;
        Ok(true)
    }
    fn model_status(&self, _message: &str, preparing: bool, state: State) {
        if state != State::Idle {
            return;
        }
        let mut data = self.tray_data();
        data.uFlags = NIF_ICON | NIF_TIP;
        data.hIcon = if preparing { self.busy } else { self.idle };
        put(&mut data.szTip, TRAY_TOOLTIP);
        // SAFETY: owned tray identifier and live icon handles.
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
    fn tray_data(&self) -> NOTIFYICONDATAW {
        // SAFETY: zero is a valid starting representation for this Win32 POD.
        let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = 1;
        data
    }
    fn state(&self, state: State) {
        SESSION.store(
            match state {
                State::Recording(id) => id.0,
                _ => 0,
            },
            Ordering::Release,
        );
        let mut data = self.tray_data();
        data.uFlags = NIF_ICON | NIF_TIP;
        data.hIcon = match Lips::from(state) {
            Lips::Open => self.busy,
            Lips::Bitten => self.idle,
        };
        put(&mut data.szTip, TRAY_TOOLTIP);
        // SAFETY: data and owned icons remain valid for the synchronous call.
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
    fn notify(&self, message: &str, error: bool) {
        let mut data = self.tray_data();
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = if error { NIIF_ERROR } else { NIIF_INFO };
        put(&mut data.szInfoTitle, "Voice Paste");
        put(&mut data.szInfo, message);
        // SAFETY: data contains owned inline UTF-16 arrays and a live HWND.
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        SESSION.store(0, Ordering::Release);
        WINDOW.store(0, Ordering::Release);
        // SAFETY: resources are owned by this thread; no callback may outlive hook.
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

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: all messages not consumed by our loop use the OS default handler.
    unsafe {
        if message == TRAY {
            // Shell callbacks may be sent synchronously (and thus never appear
            // as a GetMessage result). Queue work for the owning application
            // loop, outside this potentially reentrant window procedure.
            PostMessageW(hwnd, TRAY_ACTION, w, l);
            return 0;
        }
        if message == WM_TIMER && w == FRAME_TIMER {
            // TrackPopupMenu dispatches here too; never depend on the outer
            // GetMessage loop for animation or the bounded audio-level handoff.
            Frames::dispatch(hwnd);
            return 0;
        }
        if message == WM_CLOSE {
            PENDING.with(|p| p.borrow_mut().quit = true);
            return 0;
        }
        if message == WM_HOTKEY {
            let target = GetForegroundWindow() as usize;
            PENDING.with(|p| {
                let mut p = p.borrow_mut();
                if p.hotkeys.len() < 8 {
                    p.hotkeys.push_back((w, target));
                } else {
                    p.overflow = true;
                }
            });
            return 0;
        }
        if message == CANCEL {
            PENDING.with(|p| p.borrow_mut().cancel = Some(SessionId(w as u64)));
            return 0;
        }
        if message == TRAY_ACTION {
            PENDING.with(|p| {
                let mut pending = p.borrow_mut();
                match l as u32 {
                    WM_LBUTTONUP => pending.settings = true,
                    WM_RBUTTONUP | WM_CONTEXTMENU => pending.tray = true,
                    _ => (),
                }
            });
            return 0;
        }
        DefWindowProcW(hwnd, message, w, l)
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: Windows supplies KBDLLHOOKSTRUCT for nonnegative keyboard-hook codes.
    unsafe {
        if code >= 0 {
            let event = &*(l as *const KBDLLHOOKSTRUCT);
            let down = matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let (captured, toggle) = super::hotkey::hook_event(event.vkCode, down);
            if let Some(id) = toggle {
                PostMessageW(
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
                    PostMessageW(
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

pub fn run(
    runtime: Runtime,
    initial: &Settings,
    log_path: &str,
    capture_preview: bool,
    settings_only: bool,
) -> Result<(), String> {
    let initial = initial.clone();
    let log_path = log_path.to_owned();
    let outcome = Rc::new(RefCell::new(Ok(())));
    let result = outcome.clone();
    // Runtime remains explicitly owned outside the GPUI task, so every exit
    // (including OS/app quit) joins it after the UI and native frame context stop.
    let runtime = Rc::new(RefCell::new(runtime));
    let background = runtime.clone();
    // Kit's RetainedLayoutCleanup global closes an async channel on drop. Its
    // receiver must still have a live dispatcher: otherwise waking it drops the
    // receive future synchronously under event-listener's own notification lock.
    // Retain the platform outside Application so all app globals drop first.
    let platform = gpui_kit::platform::current_platform(false);
    gpui_kit::Application::with_platform(platform.clone())
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            cx.set_quit_mode(QuitMode::Explicit);
            gpui_kit::init(cx);
            if let Err(error) = super::settings_theme::install(cx) {
                *result.borrow_mut() = Err(error);
                cx.quit();
                return;
            }
            cx.spawn(async move |cx| {
                let session = ui_session(
                    background,
                    initial,
                    log_path,
                    capture_preview,
                    settings_only,
                    cx,
                )
                .await;
                *result.borrow_mut() = session;
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    runtime.borrow_mut().shutdown();
    drop(platform);
    outcome.borrow_mut().clone()
}

async fn ui_session(
    runtime: Rc<RefCell<Runtime>>,
    initial: Settings,
    log_path: String,
    capture_preview: bool,
    settings_only: bool,
    cx: &mut gpui_kit::AsyncApp,
) -> Result<(), String> {
    let mut window = Window::create(Hotkey::parse(&initial.hotkey)?, !settings_only)?;
    let (levels, clock) = {
        let runtime = runtime.borrow();
        (runtime.levels.clone(), runtime.clock)
    };
    let frames = Frames::new(
        window.hwnd,
        Overlay::new(window.hwnd, &initial.indicator_color, clock.elapsed())?,
        levels,
        clock,
    );
    frames.appearance(&initial.indicator_color, initial.overlay_appearance());
    let mut state = State::Idle;
    let mut saved = initial;
    let mut generation = 1u64;
    let mut settings = None;
    let mut open_settings = settings_only;
    let mut pending_save = None;
    let mut model_message = if capture_preview || settings_only {
        "Setup/capture preview — no model loading"
    } else {
        "Preparing model..."
    }
    .to_owned();
    let mut model_preparing = !(capture_preview || settings_only);
    let mut model_downloading = false;
    let mut last_memory = Instant::now() - Duration::from_secs(2);
    let mut last_dpi = Instant::now() - Duration::from_secs(2);
    if capture_preview {
        window.notify("Capture preview: records microphone and discards on Stop. No transcription/paste in this explicit mode.", false);
    }
    loop {
        // One GPUI-owned message loop, no competing GetMessage loop. Each poll
        // has bounded work. Native modal calls below run OUTSIDE cx.update and
        // outside any application/entity/RefCell borrow.
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
        let native = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
        if native.quit {
            break;
        }
        if let Some(id) = native.cancel {
            send(&runtime, &window, Command::Cancel(id));
        }
        if native.overflow {
            window.notify("Hotkey queue is full; an extra toggle was rejected.", true);
        }
        if native.settings {
            open_settings = true;
        }
        for (id, target) in native.hotkeys {
            if window
                .active_key
                .as_ref()
                .is_some_and(|active| id == active.id as usize)
                && !matches!(state, State::Transcribing(_))
            {
                send(
                    &runtime,
                    &window,
                    Command::Toggle {
                        now: clock.elapsed(),
                        target,
                    },
                );
            }
        }
        if native.tray {
            let (choice, target) = tray_menu(&window, state);
            match choice {
                TOGGLE => send(
                    &runtime,
                    &window,
                    Command::Toggle {
                        now: clock.elapsed(),
                        target,
                    },
                ),
                SETTINGS => open_settings = true,
                QUIT => break,
                _ => (),
            }
        }
        let mut activate_settings = None;
        cx.update(|cx| -> Result<(), String> {
            if settings.as_ref().is_some_and(SettingsWindow::is_closed) {
                settings.take();
                if settings_only && pending_save.is_none() {
                    cx.quit();
                    return Ok(());
                }
            }
            if open_settings {
                open_settings = false;
                if let Some(settings) = &settings {
                    activate_settings = Some(settings.native_handle(cx)?);
                } else {
                    generation = generation
                        .checked_add(1)
                        .ok_or("Settings window counter exhausted")?;
                    match SettingsWindow::open(saved.clone(), generation, &log_path, cx) {
                        Ok(opened) => {
                            activate_settings = Some(opened.native_handle(cx)?);
                            settings = Some(opened);
                            last_memory = Instant::now() - Duration::from_secs(2);
                        }
                        Err(error) => window.notify(&error, true),
                    }
                }
            }
            for _ in 0..64 {
                let event = match runtime.borrow().events.try_recv() {
                    Ok(event) => event,
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        return Err("Dictation thread exited unexpectedly".into());
                    }
                };
                match event {
                    Event::ModelAvailability { id, model, ready } => {
                        if let Some(settings) = &settings
                            && settings.id == id
                        {
                            settings.availability(&model, ready, cx);
                        }
                    }
                    Event::State(next) => {
                        state = next;
                        window.state(state);
                        window.model_status(&model_message, model_preparing, state);
                        frames.update(state, model_downloading, clock.elapsed());
                    }
                    Event::SettingsSaved { id, result } if pending_save == Some(id) => {
                        pending_save = None;
                        window.finish_hotkey(result.is_ok());
                        if let Ok(value) = &result {
                            saved = value.clone();
                            frames.appearance(&value.indicator_color, value.overlay_appearance());
                        }
                        if let Some(settings) = &settings
                            && settings.id == id
                        {
                            settings.saved(&result, cx);
                        }
                    }
                    Event::SettingsSaved { .. } => (),
                    Event::Memory { id, message } => {
                        if let Some(settings) = &settings
                            && settings.id == id
                        {
                            settings.memory(&message, cx);
                        }
                    }
                    Event::ModelStatus {
                        message,
                        preparing,
                        downloading,
                    } => {
                        model_message = message;
                        model_preparing = preparing;
                        model_downloading = downloading;
                        window.model_status(&model_message, model_preparing, state);
                        frames.update(state, model_downloading, clock.elapsed());
                        if let Some(settings) = &settings {
                            // Recheck downloaded files, without replacing a draft's status
                            // with progress belonging to the saved/active model.
                            let model = settings
                                .model_query(cx)
                                .unwrap_or_else(|| saved.model.clone());
                            if runtime
                                .borrow()
                                .send(Command::ModelAvailability {
                                    id: settings.id,
                                    model,
                                })
                                .is_ok()
                            {
                                settings.model_queried(cx);
                            }
                        }
                    }
                    Event::Error(error) => window.notify(&error, true),
                    Event::Info(info) => window.notify(&info, false),
                    Event::CaptureCompleted => window.notify(
                        "Capture preview finished; WAV removed without transcription or paste.",
                        false,
                    ),
                }
            }
            if let Some(settings) = &settings
                && let Some(model) = settings.model_query(cx)
                && runtime
                    .borrow()
                    .send(Command::ModelAvailability {
                        id: settings.id,
                        model,
                    })
                    .is_ok()
            {
                settings.model_queried(cx);
            }
            if let Some(settings) = &settings
                && let Some(draft) = settings.take_save(cx)
            {
                let result = Hotkey::parse(&draft.hotkey)
                    .and_then(|key| window.stage_hotkey(key))
                    .and_then(|()| {
                        runtime.borrow().send(Command::SaveSettings {
                            id: settings.id,
                            settings: draft,
                        })
                    });
                if let Err(error) = result {
                    window.finish_hotkey(false);
                    settings.saved(&Err(error), cx);
                } else {
                    pending_save = Some(settings.id);
                }
            }
            if let Some(settings) = &settings
                && last_memory.elapsed() >= Duration::from_secs(2)
            {
                let _ = runtime.borrow().send(Command::Memory { id: settings.id });
                last_memory = Instant::now();
            }
            // DPI-aware tray: rebuild the exact-size lips when the monitor scale
            // changes. Bounded to one check per 2s, independent of Settings.
            if last_dpi.elapsed() >= Duration::from_secs(2) {
                match window.ensure_dpi() {
                    Ok(true) => window.state(state),
                    Ok(false) => {}
                    Err(error) => window.notify(&error, true),
                }
                last_dpi = Instant::now();
            }
            Ok(())
        })?;
        if let Some(hwnd) = activate_settings
            && settings
                .as_ref()
                .is_some_and(|settings| !settings.is_closed())
            && let Err(error) = show_settings(hwnd as HWND)
        {
            window.notify(&error, true);
        }
        if let Some(error) = frames.take_error() {
            window.notify(&format!("Recording indicator failed: {error}"), true);
        }
    }
    cx.update(|cx| {
        if let Some(settings) = settings.take() {
            settings.close(cx);
        }
    });
    drop(frames);
    window.state(State::Idle);
    Ok(())
}

fn send(runtime: &RefCell<Runtime>, window: &Window, command: Command) {
    if let Err(error) = runtime.borrow().send(command) {
        window.notify(&error, true);
    }
}

fn show_settings(hwnd: HWND) -> Result<(), String> {
    // SAFETY: the Settings owner supplied its live HWND on this UI thread. Run
    // only outside cx.update/RefCell guards: show/foreground can reenter Win32.
    // SetWindowPos explicitly unhides it even if STARTUPINFO requested SW_HIDE.
    // Do not use GPUI activate_window: the pinned backend injects Alt via SendInput.
    reveal_settings(hwnd)?;
    unsafe {
        // Foreground permission is best-effort. A denial must leave Settings
        // visible, never trigger synthetic keyboard input or attach input queues.
        SetForegroundWindow(hwnd);
    }
    Ok(())
}

fn reveal_settings(hwnd: HWND) -> Result<(), String> {
    // SAFETY: same live, UI-thread-owned HWND contract as show_settings.
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

fn tray_menu(window: &Window, state: State) -> (u32, usize) {
    // SAFETY: menu is owned and destroyed on the UI thread. No GPUI/App borrow
    // crosses TrackPopupMenu's reentrant message loop. Overlay timer stays live.
    unsafe {
        let target = GetForegroundWindow() as usize;
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return (0, target);
        }
        AppendMenuW(
            menu,
            MF_STRING
                | if matches!(state, State::Transcribing(_)) {
                    MF_GRAYED
                } else {
                    0
                },
            TOGGLE as usize,
            wide(if matches!(state, State::Recording(_)) {
                "Stop recording"
            } else {
                "Start recording"
            })
            .as_ptr(),
        );
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(
            menu,
            MF_STRING,
            SETTINGS as usize,
            wide("Settings...").as_ptr(),
        );
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, QUIT as usize, wide("Quit").as_ptr());
        let mut point: POINT = zeroed();
        GetCursorPos(&mut point);
        SetForegroundWindow(window.hwnd);
        let choice = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            window.hwnd,
            null(),
        );
        DestroyMenu(menu);
        PostMessageW(window.hwnd, WM_NULL, 0, 0);
        (choice as u32, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_reveal_unhides_existing_window_without_foreground_input() {
        // Zero-area, nonactivating tool window: native visibility regression,
        // not an interactive Settings harness and never steals user focus.
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
                GetModuleHandleW(null()),
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
    fn tray_left_opens_settings_and_right_queues_only_context_menu() {
        for (message, settings, menu) in [
            (WM_LBUTTONUP, true, false),
            (WM_RBUTTONUP, false, true),
            (WM_CONTEXTMENU, false, true),
            (WM_MOUSEMOVE, false, false),
        ] {
            PENDING.with(|p| *p.borrow_mut() = PendingNative::default());
            // SAFETY: TRAY_ACTION only queues state, never dereferences the HWND.
            unsafe {
                window_proc(null_mut(), TRAY_ACTION, 1, message as isize);
            }
            PENDING.with(|p| {
                let pending = p.borrow();
                assert_eq!(pending.settings, settings);
                assert_eq!(pending.tray, menu);
                assert!(pending.hotkeys.is_empty());
            });
        }
    }

    #[test]
    fn synchronous_shell_callback_is_queued_for_application_loop() {
        // No real tray registration or input: exercise the actual window proc
        // with Shell's synchronous delivery, not an already-posted imitation.
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
            let (_sender, receiver) = crossbeam_channel::bounded(1);
            let frames = Frames::new(
                hwnd,
                Overlay::new(hwnd, "#E9563D", Duration::ZERO).unwrap(),
                receiver,
                Instant::now(),
            );
            // A synchronously dispatched frame must not need platform::run's
            // outer loop. This is the same callback route used by native menus.
            SendMessageW(hwnd, WM_TIMER, FRAME_TIMER, 0);
            assert_eq!(frames.ticks(), 1);
            let mut result: MSG = zeroed();
            SendMessageW(hwnd, TRAY, 1, WM_RBUTTONUP as isize);
            let received = PeekMessageW(&mut result, hwnd, TRAY_ACTION, TRAY_ACTION, PM_REMOVE);
            drop(frames);
            assert_eq!(GetWindowLongPtrW(hwnd, GWLP_USERDATA), 0);
            DestroyWindow(hwnd);
            UnregisterClassW(name, instance);
            assert_ne!(received, 0);
            assert_eq!(result.lParam, WM_RBUTTONUP as isize);
        }
    }

    #[test]
    #[ignore = "Interactive: briefly shows an owned native popup and busy overlay, no microphone/model/paste"]
    fn native_modal_menu_keeps_dispatching_overlay_frames() {
        unsafe extern "system" fn procedure(
            hwnd: HWND,
            message: u32,
            w: WPARAM,
            l: LPARAM,
        ) -> LRESULT {
            unsafe {
                if message == WM_TIMER && w == 99 {
                    EndMenu();
                    return 0;
                }
                window_proc(hwnd, message, w, l)
            }
        }
        // SAFETY: this test owns all native UI resources on this thread. The
        // dismissal timer is dispatched by the same modal loop, not a renderer thread.
        unsafe {
            let instance = GetModuleHandleW(null());
            let name = windows_sys::w!("VoicePasteRustModalFramesTest");
            let mut class: WNDCLASSW = zeroed();
            class.hInstance = instance;
            class.lpszClassName = name;
            class.lpfnWndProc = Some(procedure);
            assert_ne!(RegisterClassW(&class), 0);
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                name,
                name,
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
            assert!(!hwnd.is_null());
            let (_sender, receiver) = crossbeam_channel::bounded(1);
            let frames = Frames::new(
                hwnd,
                Overlay::new(hwnd, "#E9563D", Duration::ZERO).unwrap(),
                receiver,
                Instant::now(),
            );
            frames.update(State::Idle, true, Duration::ZERO);
            let menu = CreatePopupMenu();
            AppendMenuW(menu, MF_STRING, 1, windows_sys::w!("Animation test"));
            SetForegroundWindow(hwnd);
            SetTimer(hwnd, 99, 750, None);
            let started = Instant::now();
            TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, 40, 40, 0, hwnd, null());
            let ticks = frames.ticks();
            let error = frames.take_error();
            KillTimer(hwnd, 99);
            DestroyMenu(menu);
            drop(frames);
            DestroyWindow(hwnd);
            UnregisterClassW(name, instance);
            assert!(
                started.elapsed() >= Duration::from_millis(500),
                "Menu closed too early to qualify animation"
            );
            assert!(ticks >= 5, "No modal frame delivery: {ticks}");
            assert!(error.is_none(), "{error:?}");
            eprintln!("Native modal menu: {ticks} rendered timer frames while open");
        }
    }
}
