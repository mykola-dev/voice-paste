//! macOS composition: one shared dictation runtime, native OS effects, GPUI UI.
#[path = "macos/host.rs"]
mod host;
#[path = "macos/input.rs"]
mod input;
#[path = "menu.rs"]
mod menu;
#[path = "macos/storage.rs"]
pub(crate) mod storage;

use crate::{
    adapters::{
        overlay::{OverlayWindow, macos::Shell},
        paste::FocusTarget,
        settings_window::SettingsWindow,
    },
    runtime::{Command, Event, Runtime},
};
use gpui_kit::QuitMode;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use voicepaste_core::{State, settings::Settings};

pub fn startup_error(message: &str) {
    host::startup_error(message);
}
pub fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let file_mode = args.len() == 2 && args[0] == "--transcribe-file";
    if !args.is_empty()
        && args != ["--settings"]
        && args != ["--capture-preview"]
        && args != ["--download-model"]
        && !file_mode
    {
        return Err("Usage: VoicePaste [--settings | --capture-preview | --download-model | --transcribe-file <wav>]".into());
    }
    let storage = Arc::new(storage::Storage::open()?);
    if args == ["--download-model"] || file_mode {
        let mut preparation = crate::adapters::models::Preparation::start_with_download(
            storage.root.clone(),
            &storage.settings.model,
            !file_mode,
        )?;
        loop {
            if let Some(result) = preparation.poll() {
                result?;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if file_mode {
            let mut worker = crate::adapters::worker::Worker::new(storage.clone());
            println!(
                "{}",
                worker.transcribe_file(std::path::Path::new(&args[1]), &storage.settings)?
            );
        }
        return Ok(());
    }
    let preview = args == ["--capture-preview"] || args == ["--settings"];
    let runtime = Runtime::start(storage.clone(), preview)?;
    let outcome = Rc::new(RefCell::new(Ok(())));
    let result = outcome.clone();
    // The outer owner joins capture/STT/provisioning only after GPUI returns.
    let runtime = Rc::new(RefCell::new(runtime));
    let background = runtime.clone();
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
                let session = session(storage.clone(), background, preview, cx).await;
                if let Err(error) = &session {
                    storage.log(error);
                }
                *result.borrow_mut() = session;
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    runtime.borrow_mut().shutdown();
    drop(runtime);
    drop(platform);
    outcome.borrow().clone()
}

async fn session(
    storage: Arc<storage::Storage>,
    runtime: Rc<RefCell<Runtime>>,
    preview: bool,
    cx: &mut gpui_kit::AsyncApp,
) -> Result<(), String> {
    let host = host::Host::new()?;
    if preview {
        host.enable_capture();
    } else {
        host.enable_dictation();
    }
    let mut saved = storage.settings.clone();
    let overlay = cx.update(|cx| {
        OverlayWindow::open(
            &saved.indicator_color,
            saved.overlay_appearance(),
            runtime.borrow().levels.clone(),
            runtime.borrow().clock,
            cx,
        )
    })?;
    let mut shell = Shell::attach(&overlay, cx)?;
    let mut downloading = false;
    let mut keys = input::Input::attach(&saved, false).ok();
    let mut next_keys = None;
    let mut state = State::Idle;
    let mut message = if preview {
        "Capture preview — no transcription or paste".into()
    } else {
        "Checking pinned model and VAD".into()
    };
    let log_path = storage.log_path.display().to_string();
    let mut generation = 1u64;
    let mut window =
        Some(cx.update(|cx| SettingsWindow::open(saved.clone(), generation, &log_path, cx))?);
    let mut pending_save = None;
    let mut permission_target = None;
    let mut last_memory = Instant::now() - Duration::from_secs(2);
    let mut open_settings = true;
    let mut quitting = false;
    loop {
        let mut timer = std::pin::pin!(cx.background_executor().timer(Duration::from_millis(20)));
        loop {
            let due = std::future::poll_fn(|task| {
                use std::{future::Future, task::Poll};
                if timer.as_mut().poll(task).is_ready() {
                    Poll::Ready(true)
                } else {
                    overlay.poll_frame(task).map(|()| false)
                }
            })
            .await;
            if due {
                break;
            }
            shell.sync(&overlay, cx);
        }
        let intents = host.drain();
        quitting |= intents.quit;
        if quitting && pending_save.is_none() {
            break;
        }
        open_settings |= intents.settings;
        let events: Vec<_> = runtime.borrow().events.try_iter().take(64).collect();
        for event in events {
            match event {
                Event::State(value) => {
                    state = value;
                    if let Some(keys) = &keys {
                        keys.arm(matches!(state, State::Recording(_)));
                    }
                    cx.update(|cx| overlay.update(state, downloading, cx));
                }
                Event::Error(error) | Event::Info(error) => {
                    storage.log(&error);
                    message = error;
                }
                Event::CaptureCompleted => {
                    message =
                        "Capture completed; temporary audio discarded. No transcription/paste."
                            .into()
                }
                Event::ModelStatus {
                    message: value,
                    downloading: value_downloading,
                    ..
                } => {
                    message = value;
                    downloading = value_downloading;
                    cx.update(|cx| overlay.update(state, downloading, cx));
                }
                Event::SettingsSaved { id, result } => {
                    if pending_save != Some(id) {
                        return Err("Unexpected Settings completion".into());
                    }
                    pending_save = None;
                    if let Ok(value) = &result {
                        saved = value.clone();
                        cx.update(|cx| {
                            overlay.appearance(
                                &saved.indicator_color,
                                saved.overlay_appearance(),
                                cx,
                            )
                        });
                        if let Some(replacement) = next_keys.take() {
                            replacement.active(true);
                            keys = Some(replacement);
                        }
                    } else {
                        next_keys.take();
                    }
                    cx.update(|cx| {
                        if let Some(window) = &window
                            && window.id == id
                        {
                            window.saved(&result, cx);
                        }
                    });
                }
                Event::Memory { id, message } => cx.update(|cx| {
                    if let Some(window) = &window
                        && window.id == id
                    {
                        window.memory(&message, cx);
                    }
                }),
                Event::ModelAvailability { id, model, ready } => cx.update(|cx| {
                    if let Some(window) = &window
                        && window.id == id
                    {
                        window.availability(&model, ready, cx);
                    }
                }),
                Event::Statistics { id, filter, result } => cx.update(|cx| {
                    if let Some(window) = &window
                        && window.id == id
                    {
                        window.statistics(filter, result, cx);
                    }
                }),
                Event::StatisticsReset { id, result } => cx.update(|cx| {
                    if let Some(window) = &window
                        && window.id == id
                    {
                        window.statistics_reset(&result, cx);
                    }
                }),
            }
        }
        if intents.enable_keys {
            match input::Input::attach(&saved, true) {
                Ok(value) => keys = Some(value),
                Err(error) => message = error,
            }
        }
        if intents.download && !preview && host.confirm_download(&saved.model) {
            runtime.borrow().send(Command::DownloadModel)?;
        }
        let mut targets = Vec::new();
        let mut cancel = intents.cancel;
        if let Some(keys) = &keys {
            let batch = keys.drain();
            targets = batch.targets;
            cancel |= batch.cancel;
            if batch.warning {
                message =
                    "Keyboard event queue overflowed or macOS disabled the tap; retry the key."
                        .into();
            }
        }
        if intents.toggle {
            targets.push(FocusTarget::capture());
        }
        if cancel && let State::Recording(id) = state {
            runtime.borrow().send(Command::Cancel(id))?;
        } else {
            for target in targets {
                if matches!(state, State::Recording(_))
                    || host.microphone_permission(false) == host::MicrophonePermission::Granted
                {
                    runtime.borrow().send(Command::Toggle {
                        now: runtime.borrow().clock.elapsed(),
                        target,
                    })?;
                } else if permission_target.is_none() {
                    match host.microphone_permission(true){
                        host::MicrophonePermission::Denied=>message="Microphone permission denied/restricted; check System Settings → Privacy & Security → Microphone.".into(),
                        _=>{permission_target=Some(target);message="Waiting for microphone permission…".into();}
                    }
                }
            }
        }
        if permission_target.is_some() {
            match host.microphone_permission(false) {
                host::MicrophonePermission::Granted => {
                    if let Some(target) = permission_target.take() {
                        runtime.borrow().send(Command::Toggle {
                            now: runtime.borrow().clock.elapsed(),
                            target,
                        })?;
                    }
                }
                host::MicrophonePermission::Denied => {
                    permission_target.take();
                    message = "Microphone permission denied/restricted".into();
                }
                _ => (),
            }
        }
        host.capture_status(
            matches!(state, State::Recording(_)),
            matches!(state, State::Transcribing(_)) || permission_target.is_some(),
            &message,
        );
        shell.sync(&overlay, cx);
        // Settings handoff takes values under GPUI, then performs native hotkey
        // setup and command delivery after App/entity borrows have unwound.
        let draft = cx.update(|cx| -> Result<Option<Settings>, String> {
            if window.as_ref().is_some_and(SettingsWindow::is_closed) {
                window.take();
            }
            if open_settings && pending_save.is_none() {
                open_settings = false;
                if window.is_none() {
                    generation = generation
                        .checked_add(1)
                        .ok_or("Settings generation exhausted")?;
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
            let Some(window) = &window else {
                return Ok(None);
            };
            window.backend_status(&message, cx);
            if let Some(model) = window.model_query(cx) {
                runtime.borrow().send(Command::ModelAvailability {
                    id: window.id,
                    model,
                })?;
                window.model_queried(cx);
            }
            if let Some(filter) = window.statistics_query(false, cx) {
                runtime.borrow().send(Command::Statistics {
                    id: window.id,
                    filter: filter.clone(),
                })?;
                window.statistics_queried(filter, cx);
            }
            if window.take_statistics_reset(cx) {
                runtime
                    .borrow()
                    .send(Command::ResetStatistics { id: window.id })?;
            }
            if last_memory.elapsed() >= Duration::from_secs(2) {
                runtime.borrow().send(Command::Memory { id: window.id })?;
                last_memory = Instant::now();
            }
            Ok(if pending_save.is_none() {
                window.take_save(cx)
            } else {
                None
            })
        })?;
        if let Some(draft) = draft {
            let replacement = if keys.is_some() && saved.hotkey != draft.hotkey {
                input::Input::attach(&draft, false).map(Some)
            } else {
                Ok(None)
            };
            let result = replacement.and_then(|replacement| {
                if let Some(value) = &replacement {
                    value.active(false);
                }
                runtime.borrow().send(Command::SaveSettings {
                    id: generation,
                    settings: draft,
                })?;
                next_keys = replacement;
                pending_save = Some(generation);
                Ok(())
            });
            if let Err(error) = result {
                cx.update(|cx| {
                    if let Some(window) = &window {
                        window.saved(&Err(error), cx);
                    }
                });
            }
        }
    }
    drop(keys);
    drop(next_keys);
    drop(shell);
    cx.update(|cx| overlay.close(cx));
    cx.update(|cx| {
        if let Some(window) = window.take() {
            window.close(cx);
        }
    });
    Ok(())
}
