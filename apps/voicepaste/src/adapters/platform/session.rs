//! One application session; selected hosts implement native effects, not scenarios.
use super::{OverlayLifecycle, SessionHost, SettingsActivation};
use crate::adapters::{
    overlay::OverlayWindow,
    ui_session::{
        self, Context, DownloadConsent, RecordingInput, RuntimeSession, SessionEvent,
        SettingsSession, post,
    },
};
use crate::runtime::Command;
#[cfg(target_os = "windows")]
use crate::runtime::Runtime;
use voicepaste_core::State;
#[cfg(target_os = "windows")]
use voicepaste_core::settings::Settings;

#[cfg(target_os = "windows")]
pub fn run(
    runtime: Runtime,
    initial: &Settings,
    log_path: &str,
    capture_preview: bool,
    settings_only: bool,
) -> Result<(), String> {
    ui_session::run(
        runtime,
        initial.clone(),
        log_path.to_owned(),
        capture_preview,
        settings_only,
        |context, cx| {
            Box::pin(async move {
                let host = SessionHost::new(
                    &context.initial,
                    context.capture_preview,
                    context.settings_only,
                )?;
                ui_session(context, host, cx).await
            })
        },
    )
}

pub(super) async fn ui_session(
    context: Context,
    mut host: SessionHost,
    cx: &mut gpui_kit::AsyncApp,
) -> Result<(), String> {
    let Context {
        runtime,
        initial,
        log_path,
        capture_preview,
        settings_only,
    } = context;
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
            &initial.indicator_color,
            crate::adapters::settings_window::overlay_appearance(&initial),
            levels,
            clock,
            cx,
        )
    })?;
    let mut lifecycle = OverlayLifecycle::attach(&overlay, cx)?;
    let mut settings = SettingsSession::new(initial, log_path);
    let mut open_settings = settings_only;
    let mut projection = RuntimeSession::new(
        if capture_preview || settings_only {
            "Setup/capture preview — no model loading"
        } else {
            "Preparing model..."
        }
        .into(),
        !(capture_preview || settings_only),
    );
    let mut message = projection.model_message.clone();
    let mut input = RecordingInput::default();
    let mut quitting = false;
    let mut download_consent = DownloadConsent::default();
    let mut cancel_preparation = None;
    if capture_preview && !settings_only {
        host.notify("Capture preview: records microphone and discards on Stop. No transcription/paste in this explicit mode.", false);
    }
    loop {
        ui_session::wait_tick(&overlay, cx, |overlay, cx| {
            if let Err(error) = lifecycle.sync(overlay, cx) {
                host.notify(&format!("Recording indicator failed: {error}"), true);
            }
        })
        .await;
        // Replies precede native intents so saved registrations and recording state
        // are current. Every native effect runs after framework/runtime borrows end.
        let mut targets = Vec::new();
        let mut completion_cancel = None;
        for event in
            projection.dispatch(&runtime, &mut settings, &overlay, lifecycle.enabled(), cx)?
        {
            match event {
                SessionEvent::ModelPreparationFailed {
                    model,
                    message: error,
                } => {
                    download_consent.cancelled(model);
                    host.notify(&error, true);
                    // Model preparation never owns a live recording. Native
                    // modal presentation is limited to this terminal failure.
                    if projection.state == State::Idle {
                        host.error_dialog(&error);
                    }
                    message = error;
                }
                SessionEvent::ModelPreparationCancelled { model } => {
                    download_consent.cancelled(model)
                }
                SessionEvent::ModelReady { model } => download_consent.ready(&model),
                SessionEvent::StartupModelUnavailable { model } => {
                    download_consent.unavailable(model)
                }
                SessionEvent::State(state) => host.state(state),
                SessionEvent::SettingsSaved { success } => {
                    let pending = host.finish_settings(success, projection.state);
                    targets.extend(pending.targets);
                    completion_cancel = pending.cancel;
                    if let Some(warning) = pending.warning {
                        host.notify(&warning, true);
                        message = warning;
                    }
                }
                SessionEvent::ModelStatus { message: value, .. } => message = value,
                SessionEvent::Error(error) => {
                    host.notify(&error, true);
                    message = error;
                }
                SessionEvent::Info(info) => {
                    host.notify(&info, false);
                    message = info;
                }
                SessionEvent::CaptureCompleted => {
                    message = "Capture preview finished; temporary audio discarded without transcription or paste.".into();
                    host.notify(&message, false);
                }
            }
        }
        let native = host.drain(&settings.saved, projection.state);
        if let Some(generation) = cx.update(|cx| overlay.take_cancel(cx)) {
            cancel_preparation = Some(generation);
        }
        quitting |= native.quit;
        if quitting {
            // Accepted saves must commit/rollback native registration before exit.
            input.clear();
            if !settings.saving() {
                break;
            }
        }
        open_settings |= native.settings;
        if let Some(warning) = native.warning {
            host.notify(&warning, true);
            message = warning;
        }
        if !quitting {
            if projection.preparing && !native.targets.is_empty() {
                targets.clear();
                message = "Preparing the selected model; please wait or cancel preparation.".into();
            } else if !native.targets.is_empty() && download_consent.retry(&settings.saved.model) {
                // Recording is a retry intent, not permission to record before
                // provisioning completes. Consent remains model-scoped.
                targets.clear();
            } else {
                targets.extend(native.targets);
            }
            let startup_confirm = if download_consent.take_offer(
                &settings.saved.model,
                !projection.preparing && projection.state == State::Idle && !settings.saving(),
            ) {
                host.download_confirmation()
            } else {
                None
            };
            if let Some(confirm) = native.download.or(startup_confirm)
                && !capture_preview
                && !settings_only
            {
                if projection.state != State::Idle || projection.downloading {
                    message =
                        "Wait until dictation/download is idle, or cancel the download first."
                            .into();
                } else if confirm(&download_label(&settings.saved.model)) {
                    download_consent.approve(&settings.saved.model);
                }
            }
            if cancel_preparation.is_some() {
                download_consent.submitted();
            }
            if download_consent.pending(&settings.saved.model)
                && projection.state == State::Idle
                && post(&commands, Command::DownloadModel)?
            {
                download_consent.submitted();
            }
            if let Some(generation) = cancel_preparation {
                if post(&commands, Command::CancelModelDownload { generation })? {
                    cancel_preparation = None;
                } else {
                    message = "Waiting to cancel model preparation…".into();
                }
            }
            if let Some(status) = input.dispatch(
                projection.state,
                targets,
                native.cancel.or(completion_cancel),
                |request| host.recording_permission(request),
                |id| post(&commands, Command::Cancel(id)),
                |target| {
                    post(
                        &commands,
                        Command::Toggle {
                            now: clock.elapsed(),
                            target,
                        },
                    )
                },
            )? {
                host.notify(&status, false);
                message = status;
            }
        }
        let activation = cx.update(|cx| -> Result<_, String> {
            if !quitting
                && settings.maintain(&mut open_settings, cx)?
                && let Some(window) = settings.window()
            {
                return SettingsActivation::snapshot(window, cx).map(Some);
            }
            Ok(None)
        });
        match activation {
            Ok(Some(activation)) => {
                if let Err(error) = activation.apply(settings.window(), cx) {
                    host.notify(&error, true);
                }
            }
            Err(error) => host.notify(&error, true),
            Ok(None) => (),
        }
        let draft = cx.update(|cx| -> Result<_, String> {
            settings.queries(&commands, cx)?;
            Ok(if quitting {
                None
            } else {
                settings.take_save(cx)
            })
        })?;
        if let Some((id, draft)) = draft {
            let result = host
                .stage_settings(&settings.saved, &draft)
                .and_then(|()| settings.submit(id, draft, &commands));
            if let Err(error) = result {
                host.finish_settings(false, projection.state);
                cx.update(|cx| settings.failed(id, error, cx));
            }
        }
        if settings_only && settings.window().is_none() && !settings.saving() {
            break;
        }
        host.refresh(&projection, input.permission_pending(), &message);
        if let Err(error) = lifecycle.sync(&overlay, cx) {
            host.notify(&format!("Recording indicator failed: {error}"), true);
        }
    }
    cx.update(|cx| settings.close(cx));
    lifecycle.close(overlay, cx);
    host.state(State::Idle);
    Ok(())
}

fn download_label(name: &str) -> String {
    let platform = crate::adapters::settings_window::model_platform();
    match voicepaste_core::models::find_for(name, platform) {
        Ok(model) => format!(
            "Whisper {} — {:.2} GiB",
            model.name,
            model.files.iter().map(|file| file.bytes).sum::<u64>() as f64 / 1_073_741_824.0
        ),
        Err(_) => name.to_owned(),
    }
}

#[cfg(test)]
mod download_label_tests {
    #[test]
    fn confirmation_identifies_model_and_size() {
        let label = super::download_label("large-v2");
        assert!(label.starts_with("Whisper large-v2 — "));
        assert!(label.ends_with(" GiB"));
    }
}
