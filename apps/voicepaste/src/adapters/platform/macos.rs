//! Native macOS composition. Shared GPUI Settings, real local persistence and
//! menu-bar lifecycle; dictation adapters are deliberately not faked.
#[path = "macos/capture.rs"]
mod capture;
#[path = "macos/host.rs"]
mod host;
#[path = "menu.rs"]
mod menu;
#[path = "macos/preferences.rs"]
mod preferences;
#[path = "macos/storage.rs"]
mod storage;

use crate::adapters::settings_window::SettingsWindow;
use gpui_kit::QuitMode;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

pub fn startup_error(message: &str) {
    host::startup_error(message);
}

pub fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !args.is_empty() && args != ["--settings"] && args != ["--capture-preview"] {
        return Err(
            "Usage: VoicePaste [--settings | --capture-preview]. Transcription/paste are not connected yet."
                .into(),
        );
    }
    let storage = Arc::new(storage::Storage::open()?);
    let preferences = Rc::new(preferences::Preferences::start(storage.clone())?);
    let clock = Instant::now();
    let capture = if args == ["--capture-preview"] {
        Some(Rc::new(capture::Capture::start(
            storage.capture_directory()?,
            clock,
        )?))
    } else {
        None
    };
    let outcome = Rc::new(RefCell::new(Ok(())));
    let result = outcome.clone();
    // Keep background owners outside GPUI; joining capture/writer/save threads
    // occurs after Application::run, never while a UI task is dispatching.
    let session_preferences = preferences.clone();
    let session_capture = capture.clone();
    // Dispatcher outlives all GPUI globals and native UI resources.
    let platform = gpui_kit::platform::current_platform(false);
    gpui_kit::Application::with_platform(platform.clone())
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            cx.set_quit_mode(QuitMode::Explicit);
            gpui_kit::init(cx);
            if let Err(error) = crate::adapters::settings_theme::install(cx) {
                *result.borrow_mut() = Err(error);
                cx.quit();
                return;
            }
            cx.spawn(async move |cx| {
                let session = session(
                    storage.clone(),
                    &session_preferences,
                    session_capture.as_deref(),
                    clock,
                    cx,
                )
                .await;
                if let Err(error) = &session {
                    storage.log(error);
                }
                *result.borrow_mut() = session;
                // Native menu is dropped before quitting the app. The background
                // preference owner is dropped/joined with this task on exit.
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    drop(platform);
    drop(capture);
    drop(preferences);
    outcome.borrow().clone()
}

async fn session(
    storage: Arc<storage::Storage>,
    preferences: &preferences::Preferences,
    capture: Option<&capture::Capture>,
    clock: Instant,
    cx: &mut gpui_kit::AsyncApp,
) -> Result<(), String> {
    let host = host::Host::new()?;
    if capture.is_some() {
        host.enable_capture();
    }
    let mut capture_state = voicepaste_core::State::Idle;
    let mut capture_pending = false;
    let mut permission_pending = false;
    let mut saved = storage.settings.clone();
    let log_path = storage.log_path.display().to_string();
    let mut generation = 1u64;
    let mut window =
        Some(cx.update(|cx| SettingsWindow::open(saved.clone(), generation, &log_path, cx))?);
    let mut pending_save = None;
    let mut last_memory = Instant::now() - Duration::from_secs(2);
    let mut open_settings = true;
    let mut quit_requested = false;
    loop {
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
        let intents = host.drain();
        quit_requested |= intents.quit;
        if quit_requested && pending_save.is_none() {
            break;
        }
        open_settings |= intents.settings;
        if let Some(capture) = capture {
            // Drain meters even before the shared overlay is connected; the
            // callback remains bounded and the audio queue is independent.
            for _ in 0..128 {
                if capture.levels.try_recv().is_err() {
                    break;
                }
            }
            for _ in 0..8 {
                let event = match capture.events.try_recv() {
                    Ok(event) => event,
                    Err(crossbeam_channel::TryRecvError::Empty) => break,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        return Err("Capture worker exited unexpectedly".into());
                    }
                };
                capture_pending = false;
                capture_state = event.state;
                let message = match &event.message {
                    Ok(message) => *message,
                    Err(error) => {
                        storage.log(error);
                        error.as_str()
                    }
                };
                host.capture_status(
                    matches!(capture_state, voicepaste_core::State::Recording(_)),
                    false,
                    message,
                );
            }
            if permission_pending {
                match host.microphone_permission(false) {
                    host::MicrophonePermission::Granted => {
                        permission_pending = false;
                        capture.toggle(clock.elapsed(), saved.clone())?;
                        capture_pending = true;
                        host.capture_status(false, true, "Opening microphone…");
                    }
                    host::MicrophonePermission::Denied => {
                        permission_pending = false;
                        host.capture_status(false, false, "Microphone denied/restricted. Check System Settings → Privacy & Security → Microphone.");
                    }
                    _ => (),
                }
            }
            if intents.cancel && !capture_pending {
                if let voicepaste_core::State::Recording(id) = capture_state {
                    capture.cancel(id)?;
                    capture_pending = true;
                    host.capture_status(true, true, "Cancelling recording…");
                }
            } else if intents.toggle && !capture_pending && !permission_pending {
                let permission = if matches!(capture_state, voicepaste_core::State::Recording(_)) {
                    host::MicrophonePermission::Granted // Stop never requests permission.
                } else {
                    host.microphone_permission(true)
                };
                match permission {
                    host::MicrophonePermission::Granted => {
                        capture.toggle(clock.elapsed(), saved.clone())?;
                        capture_pending = true;
                        host.capture_status(matches!(capture_state, voicepaste_core::State::Recording(_)), true, "Updating capture…");
                    }
                    host::MicrophonePermission::Denied => host.capture_status(false, false, "Microphone denied/restricted. Check System Settings → Privacy & Security → Microphone."),
                    _ => {
                        permission_pending = true;
                        host.capture_status(false, true, "Waiting for microphone permission…");
                    }
                }
            }
        }
        // Completion identity prevents a finished Save from closing/replacing a
        // different draft. Successful Apply updates the base even after Cancel.
        match preferences.completions.try_recv() {
            Ok(completion) => {
                if pending_save != Some(completion.id) {
                    return Err("Unexpected settings completion identity".into());
                }
                pending_save = None;
                if let Ok(value) = &completion.result {
                    saved = value.clone();
                }
                cx.update(|cx| {
                    if let Some(window) = &window
                        && window.id == completion.id
                    {
                        window.saved(&completion.result, cx);
                    }
                });
            }
            Err(crossbeam_channel::TryRecvError::Empty) => (),
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                return Err("Settings worker exited unexpectedly".into());
            }
        }
        cx.update(|cx| -> Result<(), String> {
            if window.as_ref().is_some_and(SettingsWindow::is_closed) {
                window.take();
            }
            if open_settings && pending_save.is_none() {
                open_settings = false;
                if window.is_none() {
                    generation = generation
                        .checked_add(1)
                        .ok_or("Settings window counter exhausted")?;
                    window = Some(SettingsWindow::open(
                        saved.clone(),
                        generation,
                        &log_path,
                        cx,
                    )?);
                }
                if let Some(window) = &window {
                    window
                        .handle()
                        .update(cx, |_, window, cx| {
                            window.activate_window();
                            cx.activate(true);
                        })
                        .map_err(|e| e.to_string())?;
                }
            }
            if let Some(window) = &window {
                window.backend_pending(cx);
                if pending_save.is_none()
                    && let Some(draft) = window.take_save(cx)
                {
                    match preferences.save(window.id, draft) {
                        Ok(()) => pending_save = Some(window.id),
                        Err(error) => window.saved(&Err(error), cx),
                    }
                }
                if last_memory.elapsed() >= Duration::from_secs(2) {
                    window.memory(&host.memory_status(), cx);
                    last_memory = Instant::now();
                }
            }
            Ok(())
        })?;
    }
    cx.update(|cx| {
        if let Some(window) = window.take() {
            window.close(cx);
        }
    });
    Ok(())
}
