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
fn post(commands: &std::sync::mpsc::SyncSender<Command>, command: Command) -> Result<bool, String> {
    match commands.try_send(command) {
        Ok(()) => Ok(true),
        Err(std::sync::mpsc::TrySendError::Full(_)) => Ok(false),
        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
            Err("Dictation thread exited".into())
        }
    }
}
fn replace_keys(
    keys: &mut Option<input::Input>,
    replacement: input::Input,
    state: State,
) -> input::Batch {
    let pending = keys.as_ref().map(input::Input::drain).unwrap_or_default();
    replacement.arm(state);
    replacement.active(true);
    *keys = Some(replacement);
    pending
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
    let settings_only = args == ["--settings"];
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
                let session =
                    session(storage.clone(), background, preview, settings_only, cx).await;
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
    settings_only: bool,
    cx: &mut gpui_kit::AsyncApp,
) -> Result<(), String> {
    let host = host::Host::new()?;
    if preview && !settings_only {
        host.enable_capture();
    } else if !settings_only {
        host.enable_dictation();
    }
    let mut saved = storage.settings.clone();
    let (levels, clock, commands) = {
        let runtime = runtime.borrow();
        (
            runtime.levels.clone(),
            runtime.clock,
            runtime.commands.clone(),
        )
    };
    let overlay = cx.update(|cx| {
        OverlayWindow::open(
            &saved.indicator_color,
            saved.overlay_appearance(),
            levels,
            clock,
            cx,
        )
    })?;
    let mut shell = Shell::attach(&overlay, cx)?;
    let mut downloading = false;
    let mut model_preparing = !preview;
    let mut keys = if settings_only {
        None
    } else {
        input::Input::attach(&saved, false).ok()
    };
    let mut next_keys: Option<input::Input> = None;
    let mut state = State::Idle;
    let mut message = if preview {
        "Capture preview — no transcription or paste".into()
    } else {
        "Checking pinned model and VAD".into()
    };
    let log_path = storage.log_path.display().to_string();
    let mut generation = 1u64;
    let mut window = if settings_only {
        Some(cx.update(|cx| SettingsWindow::open(saved.clone(), generation, &log_path, cx))?)
    } else {
        None
    };
    let mut pending_save = None;
    let mut permission_target = None;
    let mut pending_cancel = None;
    let mut last_memory = Instant::now() - Duration::from_secs(2);
    let mut open_settings = settings_only;
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
        let mut batch = input::Batch::default();
        let events: Vec<_> = runtime.borrow().events.try_iter().take(64).collect();
        for event in events {
            match event {
                Event::State(value) => {
                    state = value;
                    if let Some(keys) = &keys {
                        keys.arm(state);
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
                    preparing: value_preparing,
                    downloading: value_downloading,
                    ..
                } => {
                    message = value;
                    model_preparing = value_preparing;
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
                            batch.merge(replace_keys(&mut keys, replacement, state));
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
                Ok(value) => batch.merge(replace_keys(&mut keys, value, state)),
                Err(error) => message = error,
            }
        }
        if intents.download && !preview {
            if state != State::Idle || downloading {
                message =
                    "Wait until dictation/download is idle, or cancel the download first.".into();
            } else if host.confirm_download(&saved.model)
                && !post(&commands, Command::DownloadModel)?
            {
                message = "Command queue busy; download was not started.".into();
            }
        }
        if intents.cancel_download && !post(&commands, Command::CancelModelDownload)? {
            message = "Command queue busy; retry download cancellation.".into();
        }
        if let Some(keys) = &keys {
            batch.merge(keys.drain());
            if batch.warning {
                message =
                    "Keyboard event queue overflowed or macOS disabled the tap; retry the key."
                        .into();
            }
        }
        if intents.toggle {
            batch.merge(input::Batch {
                targets: vec![FocusTarget::capture()],
                ..Default::default()
            });
        }
        if batch.warning {
            message="Keyboard input overflowed or its tap was interrupted; an extra toggle was rejected.".into();
        }
        pending_cancel = intents.cancel.or(batch.cancel).or(pending_cancel);
        if let Some(id) = pending_cancel {
            if post(&commands, Command::Cancel(id))? {
                pending_cancel = None;
            }
        } else {
            for target in batch.targets {
                if matches!(state, State::Recording(_))
                    || host.microphone_permission(false) == host::MicrophonePermission::Granted
                {
                    if !post(
                        &commands,
                        Command::Toggle {
                            now: clock.elapsed(),
                            target,
                        },
                    )? {
                        message = "Command queue busy; extra recording toggle was rejected.".into();
                    }
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
                        if !post(
                            &commands,
                            Command::Toggle {
                                now: clock.elapsed(),
                                target,
                            },
                        )? {
                            message = "Command queue busy; press the recording key again.".into();
                        }
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
            state,
            matches!(state, State::Transcribing(_)) || permission_target.is_some(),
            state != State::Idle || model_preparing,
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
            window.backend_status(&saved.model, &message, cx);
            if let Some(model) = window.model_query(cx) {
                if post(
                    &commands,
                    Command::ModelAvailability {
                        id: window.id,
                        model,
                    },
                )? {
                    window.model_queried(cx);
                }
            }
            if let Some(filter) = window.statistics_query(false, cx) {
                if post(
                    &commands,
                    Command::Statistics {
                        id: window.id,
                        filter: filter.clone(),
                    },
                )? {
                    window.statistics_queried(filter, cx);
                }
            }
            if window.take_statistics_reset(cx) {
                if !post(&commands, Command::ResetStatistics { id: window.id })? {
                    window.statistics_reset(
                        &Err("Command queue busy; retry statistics reset".into()),
                        cx,
                    );
                }
            }
            if last_memory.elapsed() >= Duration::from_secs(2) {
                let _ = post(&commands, Command::Memory { id: window.id })?;
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
                if !post(
                    &commands,
                    Command::SaveSettings {
                        id: generation,
                        settings: draft,
                    },
                )? {
                    return Err(
                        "Command queue busy; Settings were not submitted. Try Apply again.".into(),
                    );
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saturated_input_commands_are_rejected_without_treating_runtime_as_dead() {
        let (commands, receiver) = std::sync::mpsc::sync_channel(1);
        assert!(post(&commands, Command::Cancel(voicepaste_core::SessionId(1))).unwrap());
        assert!(!post(&commands, Command::Cancel(voicepaste_core::SessionId(2))).unwrap());
        drop(receiver);
        assert!(post(&commands, Command::Cancel(voicepaste_core::SessionId(2))).is_err());
    }
}
