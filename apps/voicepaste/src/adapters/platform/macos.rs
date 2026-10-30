//! Native macOS composition. Shared GPUI Settings, real local persistence and
//! menu-bar lifecycle; dictation adapters are deliberately not faked.
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
    if !args.is_empty() && args != ["--settings"] {
        return Err(
            "Usage: VoicePaste [--settings]. Recording/transcription/paste are not connected yet."
                .into(),
        );
    }
    let storage = Arc::new(storage::Storage::open()?);
    let preferences = preferences::Preferences::start(storage.clone())?;
    let outcome = Rc::new(RefCell::new(Ok(())));
    let result = outcome.clone();
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
                let session = session(storage.clone(), &preferences, cx).await;
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
    outcome.borrow().clone()
}

async fn session(
    storage: Arc<storage::Storage>,
    preferences: &preferences::Preferences,
    cx: &mut gpui_kit::AsyncApp,
) -> Result<(), String> {
    let host = host::Host::new()?;
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
        let (show, quit) = host.drain();
        quit_requested |= quit;
        if quit_requested && pending_save.is_none() {
            break;
        }
        open_settings |= show;
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
