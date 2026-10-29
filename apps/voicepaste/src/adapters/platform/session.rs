//! GPUI session coordination. Native resources and callbacks belong to adapters.
use super::{Host, OverlayLifecycle, SettingsActivation, menu::TrayAction};
use crate::adapters::{overlay::OverlayWindow, settings_window::SettingsWindow};
use crate::runtime::{Command, Event, Runtime};
use gpui_kit::QuitMode;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use voicepaste_core::{
    State,
    settings::{Hotkey, Settings},
};

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
            if let Err(error) = crate::adapters::settings_theme::install(cx) {
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
    let mut window = Host::create(Hotkey::parse(&initial.hotkey)?, !settings_only)?;
    let (levels, clock) = {
        let runtime = runtime.borrow();
        (runtime.levels.clone(), runtime.clock)
    };
    let overlay = cx.update(|cx| {
        OverlayWindow::open(
            &initial.indicator_color,
            initial.overlay_appearance(),
            levels,
            clock,
            cx,
        )
    })?;
    let mut overlay_lifecycle = OverlayLifecycle::attach(&overlay, cx)?;
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
        // Runtime polling keeps its own deadline. Rendered overlay frames wake
        // only the native lifecycle, without waiting for (or restarting) it.
        let mut poll_timer =
            std::pin::pin!(cx.background_executor().timer(Duration::from_millis(20)));
        loop {
            let runtime_due = std::future::poll_fn(|task| {
                use std::{future::Future, task::Poll};
                if poll_timer.as_mut().poll(task).is_ready() {
                    Poll::Ready(true)
                } else {
                    overlay.poll_frame(task).map(|()| false)
                }
            })
            .await;
            if runtime_due {
                break;
            }
            if let Err(error) = overlay_lifecycle.sync(&overlay, cx) {
                window.notify(&format!("Recording indicator failed: {error}"), true);
            }
        }
        let native = window.drain();
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
            if window.accepts_hotkey(id) && !matches!(state, State::Transcribing(_)) {
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
            match window.menu(state) {
                Ok((choice, target)) => match choice {
                    Some(TrayAction::Toggle) => send(
                        &runtime,
                        &window,
                        Command::Toggle {
                            now: clock.elapsed(),
                            target,
                        },
                    ),
                    Some(TrayAction::Settings) => open_settings = true,
                    Some(TrayAction::Quit) => break,
                    None => (),
                },
                Err(error) => window.notify(&error, true),
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
                    activate_settings = Some(SettingsActivation::snapshot(settings, cx)?);
                } else {
                    generation = generation
                        .checked_add(1)
                        .ok_or("Settings window counter exhausted")?;
                    match SettingsWindow::open(saved.clone(), generation, &log_path, cx) {
                        Ok(opened) => {
                            activate_settings = Some(SettingsActivation::snapshot(&opened, cx)?);
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
                    Event::StatisticsReset { id, result } => {
                        if let Some(settings) = &settings
                            && settings.id == id
                        {
                            settings.statistics_reset(&result, cx);
                        }
                    }
                    Event::Statistics { id, filter, result } => {
                        if let Some(settings) = &settings
                            && settings.id == id
                        {
                            settings.statistics(filter, result, cx);
                        }
                    }
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
                        if overlay_lifecycle.enabled() {
                            overlay.update(state, model_downloading, cx);
                        }
                    }
                    Event::SettingsSaved { id, result } if pending_save == Some(id) => {
                        pending_save = None;
                        window.finish_hotkey(result.is_ok());
                        if let Ok(value) = &result {
                            saved = value.clone();
                            overlay.appearance(
                                &value.indicator_color,
                                value.overlay_appearance(),
                                cx,
                            );
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
                        if overlay_lifecycle.enabled() {
                            overlay.update(state, model_downloading, cx);
                        }
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
                && settings.take_statistics_reset(cx)
                && let Err(error) = runtime
                    .borrow()
                    .send(Command::ResetStatistics { id: settings.id })
            {
                settings.statistics_reset(&Err(error), cx);
            }
            if let Some(settings) = &settings
                && let Some(filter) =
                    settings.statistics_query(last_memory.elapsed() >= Duration::from_secs(2), cx)
                && runtime
                    .borrow()
                    .send(Command::Statistics {
                        id: settings.id,
                        filter: filter.clone(),
                    })
                    .is_ok()
            {
                settings.statistics_queried(filter, cx);
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
        if let Some(activation) = activate_settings
            && let Err(error) = activation.apply(settings.as_ref())
        {
            window.notify(&error, true);
        }
        // This adapter owns native snapshots and all reentrant shell operations.
        // Invoke it only outside App/runtime/entity borrows.
        if let Err(error) = overlay_lifecycle.sync(&overlay, cx) {
            window.notify(&format!("Recording indicator failed: {error}"), true);
        }
    }
    cx.update(|cx| {
        if let Some(settings) = settings.take() {
            settings.close(cx);
        }
    });
    overlay_lifecycle.close(overlay, cx);
    window.state(State::Idle);
    Ok(())
}

fn send(runtime: &RefCell<Runtime>, window: &Host, command: Command) {
    if let Err(error) = runtime.borrow().send(command) {
        window.notify(&error, true);
    }
}
