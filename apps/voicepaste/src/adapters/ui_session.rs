//! Shared GPUI lifetime and Settings coordination. Native effects stay with callers.
mod download;
mod input;
use super::{overlay::OverlayWindow, settings_window::SettingsWindow};
use crate::runtime::{Command, Event, Runtime};
pub(crate) use download::DownloadRetry;
use gpui_kit::{App, AsyncApp, QuitMode};
pub(crate) use input::RecordingInput;
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError},
    time::{Duration, Instant},
};
use voicepaste_core::{State, settings::Settings};

/// Values only: native callbacks/menus never enter the runtime or GPUI session.
#[derive(Default)]
pub(crate) struct NativeIntents {
    pub quit: bool,
    pub settings: bool,
    pub targets: Vec<super::paste::FocusTarget>,
    pub cancel: Option<voicepaste_core::SessionId>,
    pub warning: Option<String>,
    pub download: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "Native permission policies differ; the coordinator handles every outcome on both hosts"
)]
pub(crate) enum RecordingPermission {
    /// No separate asynchronous native permission handoff; audio open reports errors.
    NotRequired,
    Granted,
    Denied,
    Pending,
}

/// Only non-Settings work leaves the shared reply router.
pub(crate) enum SessionEvent {
    ModelPreparationFailed {
        model: String,
        message: String,
    },
    ModelPreparationCancelled {
        model: String,
    },
    PreparationStarted {
        model: String,
    },
    ModelReady {
        model: String,
    },
    State(State),
    Error(String),
    Recovery {
        session: voicepaste_core::SessionId,
        message: String,
    },
    Info(String),
    CaptureCompleted,
    ModelStatus {
        message: String,
        preparing: bool,
        downloading: bool,
    },
    SettingsSaved {
        success: bool,
    },
}

pub(crate) struct Context {
    pub runtime: Rc<RefCell<Runtime>>,
    pub initial: Settings,
    pub log_path: String,
    pub capture_preview: bool,
    pub settings_only: bool,
}

const EVENT_BUDGET: usize = 64;

/// Deliver buffered replies before reporting disconnect, so an accepted Save's
/// native completion cannot disappear behind a terminal channel error.
fn receive_events(events: &Receiver<Event>) -> Result<Vec<Event>, String> {
    let mut batch = Vec::with_capacity(EVENT_BUDGET);
    for _ in 0..EVENT_BUDGET {
        match events.try_recv() {
            Ok(event) => batch.push(event),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) if !batch.is_empty() => break,
            Err(TryRecvError::Disconnected) => {
                return Err("Dictation thread exited unexpectedly".into());
            }
        }
    }
    Ok(batch)
}

/// One read-only runtime projection and bounded reply pump for both native hosts.
pub(crate) struct RuntimeSession {
    pub state: State,
    pub model_message: String,
    pub preparing: bool,
    pub downloading: bool,
    show_preparation: bool,
}

impl RuntimeSession {
    pub fn new(model_message: String, preparing: bool) -> Self {
        Self {
            state: State::Idle,
            model_message,
            preparing,
            downloading: false,
            show_preparation: false,
        }
    }

    fn project(&mut self, event: &SessionEvent) -> bool {
        match event {
            SessionEvent::State(state) => self.state = *state,
            SessionEvent::PreparationStarted { .. } => {
                self.preparing = true;
                self.downloading = false;
                self.show_preparation = false;
                self.model_message = "Preparing model...".into();
            }
            SessionEvent::ModelStatus {
                message,
                preparing,
                downloading,
            } => {
                self.model_message.clone_from(message);
                self.preparing = *preparing;
                self.downloading = *downloading;
                self.show_preparation = *preparing && (self.show_preparation || *downloading);
            }
            _ => return false,
        }
        true
    }

    pub fn dispatch(
        &mut self,
        runtime: &RefCell<Runtime>,
        settings: &mut SettingsSession,
        overlay: &OverlayWindow,
        overlay_enabled: bool,
        cx: &mut AsyncApp,
    ) -> Result<Vec<SessionEvent>, String> {
        let events = receive_events(&runtime.borrow().events)?;
        // Runtime borrow is released before framework/entity updates.
        Ok(cx.update(|cx| {
            let mut native = Vec::with_capacity(events.len());
            for event in events {
                let Some(event) = settings.event(event, overlay, cx) else {
                    continue;
                };
                if self.project(&event) && overlay_enabled {
                    overlay.update_status(
                        self.state,
                        self.show_preparation,
                        &self.model_message,
                        cx,
                    );
                }
                native.push(event);
            }
            native
        }))
    }
}

pub(crate) fn run(
    runtime: Runtime,
    initial: Settings,
    log_path: String,
    capture_preview: bool,
    settings_only: bool,
    session: impl for<'a> FnOnce(
        Context,
        &'a mut AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>>
    + 'static,
) -> Result<(), String> {
    let outcome = Rc::new(RefCell::new(Ok(())));
    let result = outcome.clone();
    // Shutdown joins outside the UI task, including native/application quit.
    let runtime = Rc::new(RefCell::new(runtime));
    let context = Context {
        runtime: runtime.clone(),
        initial,
        log_path,
        capture_preview,
        settings_only,
    };
    // Kit cleanup globals must drop before their async dispatcher/platform.
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
                let outcome = session(context, cx).await;
                *result.borrow_mut() = outcome;
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    runtime.borrow_mut().shutdown();
    drop(runtime);
    drop(platform);
    outcome.borrow().clone()
}

/// Full is backpressure, not a dead runtime. Callers retain/retry durable intents.
pub(crate) fn post(commands: &SyncSender<Command>, command: Command) -> Result<bool, String> {
    match commands.try_send(command) {
        Ok(()) => Ok(true),
        Err(TrySendError::Full(_)) => Ok(false),
        Err(TrySendError::Disconnected(_)) => Err("Dictation thread exited".into()),
    }
}

/// Native frame wakes never restart the independent runtime-poll deadline.
pub(crate) async fn wait_tick(
    overlay: &OverlayWindow,
    cx: &mut AsyncApp,
    mut frame: impl FnMut(&OverlayWindow, &mut AsyncApp),
) {
    let mut timer = std::pin::pin!(cx.background_executor().timer(Duration::from_millis(20)));
    loop {
        let due = std::future::poll_fn(|task| {
            use std::task::Poll;
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
        frame(overlay, cx);
    }
}

#[derive(Default)]
struct SaveState {
    generation: u64,
    pending: Option<u64>,
}

impl SaveState {
    fn next(&mut self) -> Result<u64, String> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("Settings window counter exhausted")?;
        Ok(self.generation)
    }

    fn complete(&mut self, id: u64) -> bool {
        if self.pending != Some(id) {
            return false;
        }
        self.pending = None;
        true
    }
}

pub(crate) struct SettingsSession {
    pub saved: Settings,
    window: Option<SettingsWindow>,
    saves: SaveState,
    log_path: String,
    last_memory: Instant,
}

impl SettingsSession {
    pub fn new(saved: Settings, log_path: String) -> Self {
        Self {
            saved,
            window: None,
            saves: SaveState::default(),
            log_path,
            last_memory: Instant::now() - Duration::from_secs(2),
        }
    }

    pub fn window(&self) -> Option<&SettingsWindow> {
        self.window.as_ref()
    }

    pub fn saving(&self) -> bool {
        self.saves.pending.is_some()
    }

    /// Return an activation intent; callers perform native activation separately.
    pub fn maintain(&mut self, open: &mut bool, cx: &mut App) -> Result<bool, String> {
        if self.window.as_ref().is_some_and(SettingsWindow::is_closed) {
            self.window.take();
        }
        if !*open || self.saving() {
            return Ok(false);
        }
        *open = false;
        if self.window.is_none() {
            let id = self.saves.next()?;
            self.window = Some(SettingsWindow::open(
                self.saved.clone(),
                id,
                &self.log_path,
                cx,
            )?);
            self.last_memory = Instant::now() - Duration::from_secs(2);
        }
        Ok(true)
    }

    /// Consume Settings replies once. Accepted Save replies return to the caller
    /// only for native hotkey commit/rollback; stale replies have no effects.
    pub fn event(
        &mut self,
        event: Event,
        overlay: &OverlayWindow,
        cx: &mut App,
    ) -> Option<SessionEvent> {
        match event {
            Event::ModelPreparationFailed { model, message } => {
                return Some(SessionEvent::ModelPreparationFailed { model, message });
            }
            Event::ModelPreparationCancelled { model } => {
                return Some(SessionEvent::ModelPreparationCancelled { model });
            }
            Event::ModelReady { model } => return Some(SessionEvent::ModelReady { model }),
            Event::PreparationStarted { generation, model } => {
                overlay.set_preparation_generation(generation, cx);
                return Some(SessionEvent::PreparationStarted { model });
            }
            Event::SettingsSaved { id, result } => {
                if !self.saves.complete(id) {
                    return None;
                }
                if let Ok(value) = &result {
                    self.saved = value.clone();
                    overlay.appearance(
                        &value.indicator_color,
                        super::settings_window::overlay_appearance(value),
                        cx,
                    );
                }
                if let Some(window) = self.window.as_ref().filter(|window| window.id == id) {
                    window.saved(&result, cx);
                }
                return Some(SessionEvent::SettingsSaved {
                    success: result.is_ok(),
                });
            }
            Event::Memory { id, message } => {
                if let Some(window) = self.window.as_ref().filter(|window| window.id == id) {
                    window.memory(&message, cx);
                }
            }
            Event::ModelAvailability { id, model, ready } => {
                if let Some(window) = self.window.as_ref().filter(|window| window.id == id) {
                    window.availability(&model, ready, cx);
                }
            }
            Event::Statistics { id, filter, result } => {
                if let Some(window) = self.window.as_ref().filter(|window| window.id == id) {
                    window.statistics(filter, result, cx);
                }
            }
            Event::StatisticsReset { id, result } => {
                if let Some(window) = self.window.as_ref().filter(|window| window.id == id) {
                    window.statistics_reset(&result, cx);
                }
            }
            Event::State(state) => return Some(SessionEvent::State(state)),
            Event::Error(error) => return Some(SessionEvent::Error(error)),
            Event::Recovery { session, message } => {
                return Some(SessionEvent::Recovery { session, message });
            }
            Event::Info(info) => return Some(SessionEvent::Info(info)),
            Event::CaptureCompleted => return Some(SessionEvent::CaptureCompleted),
            Event::ModelStatus {
                message,
                preparing,
                downloading,
            } => {
                if let Some(window) = &self.window {
                    window.refresh_model_query(cx);
                }
                return Some(SessionEvent::ModelStatus {
                    message,
                    preparing,
                    downloading,
                });
            }
        }
        None
    }

    pub fn queries(&mut self, commands: &SyncSender<Command>, cx: &mut App) -> Result<(), String> {
        let Some(window) = &self.window else {
            return Ok(());
        };
        if let Some(model) = window.model_query(cx)
            && post(
                commands,
                Command::ModelAvailability {
                    id: window.id,
                    model,
                },
            )?
        {
            window.model_queried(cx);
        }
        if window.take_statistics_reset(cx)
            && !post(commands, Command::ResetStatistics { id: window.id })?
        {
            window.statistics_reset(
                &Err("Command queue busy; retry statistics reset".into()),
                cx,
            );
        }
        let refresh = self.last_memory.elapsed() >= Duration::from_secs(2);
        if let Some(filter) = window.statistics_query(refresh, cx)
            && post(
                commands,
                Command::Statistics {
                    id: window.id,
                    filter: filter.clone(),
                },
            )?
        {
            window.statistics_queried(filter, cx);
        }
        if refresh && post(commands, Command::Memory { id: window.id })? {
            self.last_memory = Instant::now();
        }
        Ok(())
    }

    pub fn take_save(&self, cx: &mut App) -> Option<(u64, Settings)> {
        if self.saving() {
            return None;
        }
        let window = self.window.as_ref()?;
        window.take_save(cx).map(|draft| (window.id, draft))
    }

    /// Native staging must succeed before this call. No native effects run under App.
    pub fn submit(
        &mut self,
        id: u64,
        draft: Settings,
        commands: &SyncSender<Command>,
    ) -> Result<(), String> {
        if self.saving() {
            return Err("A Settings save is already pending".into());
        }
        if !post(
            commands,
            Command::SaveSettings {
                id,
                settings: draft,
            },
        )? {
            return Err("Command queue busy; Settings were not submitted. Try Apply again.".into());
        }
        self.saves.pending = Some(id);
        Ok(())
    }

    pub fn failed(&self, id: u64, error: String, cx: &mut App) {
        if let Some(window) = self.window.as_ref().filter(|window| window.id == id) {
            window.saved(&Err(error), cx);
        }
    }

    pub fn close(&mut self, cx: &mut App) {
        if let Some(window) = self.window.take() {
            window.close(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_model_preparation_stays_hidden_but_download_remains_visible() {
        let mut projection = RuntimeSession::new("Ready".into(), false);
        projection.project(&SessionEvent::PreparationStarted {
            model: "local".into(),
        });
        assert!(projection.preparing);
        assert!(!projection.show_preparation);
        projection.project(&SessionEvent::ModelStatus {
            message: "Checking".into(),
            preparing: true,
            downloading: false,
        });
        assert!(!projection.show_preparation);
        projection.project(&SessionEvent::ModelStatus {
            message: "Downloading".into(),
            preparing: true,
            downloading: true,
        });
        assert!(projection.show_preparation);
        projection.project(&SessionEvent::ModelStatus {
            message: "Verifying".into(),
            preparing: true,
            downloading: false,
        });
        assert!(projection.show_preparation);
        projection.project(&SessionEvent::ModelStatus {
            message: "Ready".into(),
            preparing: false,
            downloading: false,
        });
        assert!(!projection.show_preparation);
        projection.project(&SessionEvent::PreparationStarted {
            model: "other".into(),
        });
        assert!(!projection.show_preparation);
    }

    #[gpui_kit::test]
    fn download_retry_flow_handles_split_batches_and_bounded_commands(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let (events, receiver) = std::sync::mpsc::sync_channel(EVENT_BUDGET + 2);
        let (commands, command_receiver) = std::sync::mpsc::sync_channel(1);
        let (_meter, levels) = crossbeam_channel::bounded(1);
        cx.update(|cx| {
            gpui_kit::init(cx);
            super::super::settings_theme::install(cx).unwrap();
            let mut settings = SettingsSession::new(initial(), String::new());
            let model = settings.saved.model.clone();
            let overlay = OverlayWindow::open(&settings.saved.indicator_color, settings.saved.overlay_appearance(), levels, Instant::now(), cx).unwrap();
            let mut projection = RuntimeSession::new("Preparing".into(), true);
            let mut retry = DownloadRetry::default();
            let route = |settings: &mut SettingsSession, projection: &mut RuntimeSession, retry: &mut DownloadRetry, cx: &mut App| {
                for event in receive_events(&receiver).unwrap() {
                    if let Some(event) = settings.event(event, &overlay, cx) {
                        projection.project(&event);
                        retry.event(&event, &settings.saved.model);
                    }
                }
            };
            let send_pending = |projection: &RuntimeSession, retry: &mut DownloadRetry| {
                retry.submit(&model, !projection.preparing && !projection.downloading && projection.state == State::Idle, |model| post(&commands, Command::DownloadModel { model: model.into() })).unwrap();
            };
            // Automatic success produces no new command.
            events.send(Event::ModelReady { model: model.clone() }).unwrap();
            events.send(Event::ModelStatus { message: "Ready".into(), preparing: false, downloading: false }).unwrap();
            route(&mut settings, &mut projection, &mut retry, cx);
            send_pending(&projection, &mut retry);
            assert!(command_receiver.try_recv().is_err());

            // Simulate a fresh preparation and split its terminal failure/status
            // across the 64-event budget. Start is retained until status is idle.
            events.send(Event::PreparationStarted { generation: 2, model: model.clone() }).unwrap();
            for _ in 0..EVENT_BUDGET - 2 { events.send(Event::Info("filler".into())).unwrap(); }
            events.send(Event::ModelPreparationFailed { model: model.clone(), message: "fixture".into() }).unwrap();
            events.send(Event::ModelStatus { message: "Failed".into(), preparing: false, downloading: false }).unwrap();
            route(&mut settings, &mut projection, &mut retry, cx);
            send_pending(&projection, &mut retry);
            assert!(command_receiver.try_recv().is_err());
            assert!(retry.retry(&model));
            send_pending(&projection, &mut retry);
            assert!(command_receiver.try_recv().is_err());
            route(&mut settings, &mut projection, &mut retry, cx);
            commands.try_send(Command::Memory { id: 1 }).unwrap();
            send_pending(&projection, &mut retry);
            assert!(matches!(command_receiver.try_recv(), Ok(Command::Memory { id: 1 })));
            send_pending(&projection, &mut retry);
            assert!(matches!(command_receiver.try_recv(), Ok(Command::DownloadModel { model: actual }) if actual == model));
            assert!(retry.retry(&model));
            send_pending(&projection, &mut retry);
            assert!(command_receiver.try_recv().is_err());

            events.send(Event::PreparationStarted { generation: 3, model: model.clone() }).unwrap();
            events.send(Event::ModelPreparationCancelled { model: model.clone() }).unwrap();
            events.send(Event::ModelStatus { message: "Cancelled".into(), preparing: false, downloading: false }).unwrap();
            route(&mut settings, &mut projection, &mut retry, cx);
            send_pending(&projection, &mut retry);
            assert!(command_receiver.try_recv().is_err());
            assert!(retry.retry(&model));
            send_pending(&projection, &mut retry);
            assert!(matches!(command_receiver.try_recv(), Ok(Command::DownloadModel { model: actual }) if actual == model));
            overlay.close(cx);
        });
    }

    fn initial() -> Settings {
        #[cfg(target_os = "windows")]
        let platform = voicepaste_core::settings::SettingsPlatform::Windows;
        #[cfg(target_os = "macos")]
        let platform = voicepaste_core::settings::SettingsPlatform::Macos;
        Settings::initial_for(platform).unwrap()
    }

    #[test]
    fn runtime_event_pump_is_bounded_and_preserves_order() {
        let (sender, receiver) = std::sync::mpsc::sync_channel(EVENT_BUDGET + 2);
        for value in 0..EVENT_BUDGET + 2 {
            sender.send(Event::Info(value.to_string())).unwrap();
        }
        let first = receive_events(&receiver).unwrap();
        assert_eq!(first.len(), EVENT_BUDGET);
        for (index, event) in first.iter().enumerate() {
            assert!(matches!(event, Event::Info(value) if value == &index.to_string()));
        }
        let second = receive_events(&receiver).unwrap();
        assert_eq!(second.len(), 2);
        assert!(matches!(&second[0], Event::Info(value) if value == &EVENT_BUDGET.to_string()));
        assert!(receive_events(&receiver).unwrap().is_empty());
    }

    #[test]
    fn disconnect_is_reported_after_buffered_save_completion_is_delivered() {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        sender
            .send(Event::SettingsSaved {
                id: 7,
                result: Err("fixture failure".into()),
            })
            .unwrap();
        drop(sender);
        let events = receive_events(&receiver).unwrap();
        assert!(matches!(&events[..], [Event::SettingsSaved { id: 7, .. }]));
        assert!(receive_events(&receiver).is_err());
    }

    #[test]
    fn state_and_model_projection_share_overlay_update_decisions() {
        let mut session = RuntimeSession::new("Checking".into(), true);
        let recording = State::Recording(voicepaste_core::SessionId(3));
        assert!(session.project(&SessionEvent::State(recording)));
        assert_eq!(session.state, recording);
        assert!(session.project(&SessionEvent::ModelStatus {
            message: "Downloading".into(),
            preparing: true,
            downloading: true
        }));
        assert!(session.downloading);
        assert!(session.preparing);
        assert_eq!(session.model_message, "Downloading");
        assert!(session.project(&SessionEvent::ModelStatus {
            message: "Verifying downloaded model".into(),
            preparing: true,
            downloading: false,
        }));
        assert!(session.preparing);
        assert!(!session.downloading);
        assert_eq!(session.model_message, "Verifying downloaded model");
        assert!(!session.project(&SessionEvent::Error("unrelated error".into())));
        assert_eq!(session.model_message, "Verifying downloaded model");
        assert!(session.project(&SessionEvent::ModelStatus {
            message: "Ready".into(),
            preparing: false,
            downloading: false
        }));
        assert!(!session.downloading);
        assert!(!session.preparing);
        assert_eq!(session.state, recording);
    }

    #[gpui_kit::test]
    fn model_status_refresh_retries_full_queue_and_uses_open_draft_selection(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let (commands, receiver) = std::sync::mpsc::sync_channel(1);
        let (_meter, levels) = crossbeam_channel::bounded(1);
        cx.update(|cx| {
            gpui_kit::init(cx);
            super::super::settings_theme::install(cx).unwrap();
            let mut session = SettingsSession::new(initial(), String::new());
            let selected = session.saved.model.clone();
            let overlay = OverlayWindow::open(&session.saved.indicator_color, session.saved.overlay_appearance(), levels, Instant::now(), cx).unwrap();
            let mut open = true;
            session.maintain(&mut open, cx).unwrap();
            let window = session.window().unwrap();
            window.model_queried(cx);
            let id = window.id;
            // An existing draft's selected model, not the later saved projection,
            // must be queried when provisioning completes.
            session.saved.model = "different saved model".into();
            assert!(session.event(Event::ModelStatus { message: "Ready".into(), preparing: false, downloading: false }, &overlay, cx).is_some());
            commands.try_send(Command::Memory { id }).unwrap();
            session.queries(&commands, cx).unwrap();
            assert_eq!(session.window().unwrap().model_query(cx), Some(selected.clone()));
            receiver.try_recv().unwrap();
            session.queries(&commands, cx).unwrap();
            assert!(matches!(receiver.try_recv().unwrap(), Command::ModelAvailability { id: actual_id, model } if actual_id == id && model == selected));
            assert!(session.window().unwrap().model_query(cx).is_none());
            session.close(cx);
            overlay.close(cx);
        });
    }

    #[gpui_kit::test]
    fn pending_save_survives_closed_draft_and_reopen_uses_committed_settings(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let (commands, receiver) = std::sync::mpsc::sync_channel(8);
        let (_meter, levels) = crossbeam_channel::bounded(1);
        cx.update(|cx| {
            gpui_kit::init(cx);
            super::super::settings_theme::install(cx).unwrap();
            let mut session = SettingsSession::new(initial(), String::new());
            let overlay = OverlayWindow::open(&session.saved.indicator_color, session.saved.overlay_appearance(), levels, Instant::now(), cx).unwrap();
            let mut open = true;
            assert!(session.maintain(&mut open, cx).unwrap());
            let id = session.window().unwrap().id;
            let mut draft = session.saved.clone();
            draft.custom_initial_prompt = "Збережи цей prompt".into();
            session.submit(id, draft.clone(), &commands).unwrap();
            assert!(matches!(receiver.try_recv().unwrap(), Command::SaveSettings { id: value, .. } if value == id));
            session.close(cx);
            open = true;
            assert!(!session.maintain(&mut open, cx).unwrap());
            assert!(open);
            assert!(session.window().is_none());
            assert!(session.event(Event::SettingsSaved { id: id + 1, result: Ok(draft.clone()) }, &overlay, cx).is_none());
            assert!(session.saving());
            assert_ne!(session.saved.custom_initial_prompt, draft.custom_initial_prompt);
            assert!(matches!(session.event(Event::SettingsSaved { id, result: Ok(draft.clone()) }, &overlay, cx), Some(SessionEvent::SettingsSaved { success: true })));
            assert!(!session.saving());
            assert_eq!(session.saved, draft);
            assert!(session.maintain(&mut open, cx).unwrap());
            assert_ne!(session.window().unwrap().id, id);
            // Duplicate completions cannot mutate the reopened generation.
            assert!(session.event(Event::SettingsSaved { id, result: Err("late failure".into()) }, &overlay, cx).is_none());
            assert_eq!(session.saved, draft);
            session.close(cx);
            overlay.close(cx);
        });
    }

    #[gpui_kit::test]
    fn failed_save_keeps_saved_settings_and_releases_pending_obligation(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let (commands, _receiver) = std::sync::mpsc::sync_channel(8);
        let (_meter, levels) = crossbeam_channel::bounded(1);
        cx.update(|cx| {
            gpui_kit::init(cx);
            super::super::settings_theme::install(cx).unwrap();
            let initial = initial();
            let mut session = SettingsSession::new(initial.clone(), String::new());
            let overlay = OverlayWindow::open(
                &initial.indicator_color,
                initial.overlay_appearance(),
                levels,
                Instant::now(),
                cx,
            )
            .unwrap();
            let mut draft = initial.clone();
            draft.custom_initial_prompt = "unsaved".into();
            session.submit(1, draft, &commands).unwrap();
            assert!(matches!(
                session.event(
                    Event::SettingsSaved {
                        id: 1,
                        result: Err("disk unavailable".into())
                    },
                    &overlay,
                    cx
                ),
                Some(SessionEvent::SettingsSaved { success: false })
            ));
            assert!(!session.saving());
            assert_eq!(session.saved, initial);
            overlay.close(cx);
        });
    }

    #[test]
    fn stale_save_does_not_release_current_save() {
        let mut saves = SaveState {
            generation: 2,
            pending: Some(2),
        };
        assert!(!saves.complete(1));
        assert_eq!(saves.pending, Some(2));
        assert!(saves.complete(2));
        assert!(!saves.complete(2));
    }

    #[test]
    fn generations_never_wrap_or_reuse_closed_draft_identity() {
        let mut saves = SaveState::default();
        assert_eq!(saves.next().unwrap(), 1);
        assert_eq!(saves.next().unwrap(), 2);
        saves.generation = u64::MAX;
        assert!(saves.next().is_err());
        assert_eq!(saves.generation, u64::MAX);
    }

    #[test]
    fn full_queue_rejects_save_without_claiming_it_is_pending() {
        let (commands, receiver) = std::sync::mpsc::sync_channel(1);
        assert!(post(&commands, Command::Memory { id: 1 }).unwrap());
        let mut settings = SettingsSession::new(initial(), String::new());
        assert!(
            settings
                .submit(1, settings.saved.clone(), &commands)
                .is_err()
        );
        assert!(!settings.saving());
        receiver.try_recv().unwrap();
        settings
            .submit(1, settings.saved.clone(), &commands)
            .unwrap();
        assert!(settings.saving());
        assert!(
            settings
                .submit(2, settings.saved.clone(), &commands)
                .is_err()
        );
        drop(receiver);
        assert!(post(&commands, Command::Memory { id: 1 }).is_err());
    }
}
