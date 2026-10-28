use crate::adapters::{
    audio::Microphone, models::Preparation, paste::ClipboardPaste, storage::Storage, worker::Worker,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use voicepaste_application::settings::SettingsBoundary;
use voicepaste_application::{Dictation, Transcription, TranscriptionEvent};
use voicepaste_core::settings::{Settings, TranscriptionOptions};
use voicepaste_core::{SessionId, State};

pub enum Command {
    Toggle { now: Duration, target: usize },
    Cancel(SessionId),
    SaveSettings { id: u64, settings: Settings },
    Memory { id: u64 },
    ModelAvailability { id: u64, model: String },
    Quit,
}
pub enum Event {
    ModelAvailability {
        id: u64,
        model: String,
        ready: bool,
    },
    State(State),
    Error(String),
    CaptureCompleted,
    Info(String),
    SettingsSaved {
        id: u64,
        result: Result<Settings, String>,
    },
    Memory {
        id: u64,
        message: String,
    },
    ModelStatus {
        message: String,
        preparing: bool,
        downloading: bool,
    },
}

pub struct Runtime {
    pub commands: SyncSender<Command>,
    pub events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
    pub clock: Instant,
    pub levels: crossbeam_channel::Receiver<voicepaste_core::waveform::Measurement>,
    shutdown: Arc<AtomicBool>,
}

impl Runtime {
    pub fn start(storage: Arc<Storage>, capture_preview: bool) -> Result<Self, String> {
        let clock = Instant::now();
        let (meter, levels) = crossbeam_channel::bounded(voicepaste_core::waveform::CAPACITY);
        let (commands, receiver) = mpsc::sync_channel(8);
        let (sender, events) = mpsc::sync_channel(64);
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = shutdown.clone();
        let thread = thread::Builder::new()
            .name("dictation".into())
            .spawn(move || {
                let microphone = Microphone::new(storage.temp.clone(), meter, clock);
                let mut app = Dictation::new(microphone);
                let mut worker = Worker::new(storage.clone());
                let mut paste = ClipboardPaste {
                    shutdown: thread_shutdown.clone(),
                };
                let mut preferences = SettingsBoundary::new(storage.settings.clone());
                let mut model_ready = capture_preview;
                let mut preparation = if capture_preview { None } else {
                    prepare(&storage,&preferences.current().model,&sender)
                };
                let mut last_progress = Instant::now();
                loop {
                    if thread_shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    // Commit deferred preferences before accepting another Start,
                    // including the first command after a transcription finished.
                    let previous = preferences.current().clone();
                    if preferences.apply_at_idle(app.state() == State::Idle) {
                        let current = preferences.current();
                        if previous.model != current.model || TranscriptionOptions::from(&previous) != TranscriptionOptions::from(current) || !current.keep_in_memory {
                            worker.release();
                        }
                        if previous.model != current.model && !capture_preview {
                            drop(preparation.take()); model_ready=false;
                            preparation=prepare(&storage,&current.model,&sender);
                        } else if model_ready && !capture_preview && current.keep_in_memory
                            && let Err(error) = worker.preload(current) { report(&storage,&sender,error); }
                    }
                    match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(Command::Quit) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => (),
                        Ok(Command::SaveSettings { id, settings }) => {
                            let result = voicepaste_application::settings::save(&*storage, settings);
                            if let Ok(settings) = &result { preferences.saved(settings.clone()); }
                            if !reliable(&sender, Event::SettingsSaved { id, result }, &thread_shutdown) { break; }
                        }
                        Ok(Command::Memory { id }) => { let _ = sender.try_send(Event::Memory { id, message: worker.memory_status() }); }
                        Ok(Command::ModelAvailability { id, model }) => {
                            // Read-only inventory; never provisions or loads a draft selection.
                            let ready = crate::adapters::models::local_path(&storage.root, &model).is_ok();
                            if !reliable(&sender, Event::ModelAvailability { id, model, ready }, &thread_shutdown) { break; }
                        }
                        Ok(Command::Cancel(id)) => {
                            if let Err(error) = app.cancel(id) {
                                report(&storage, &sender, error);
                            }
                            let _ = sender.try_send(Event::State(app.state()));
                        }
                        Ok(Command::Toggle { now, target }) => {
                            if preferences.current().debug_logging {
                                storage.log(&format!("Toggle at {} ms; before {:?}", now.as_millis(), app.state()));
                            }
                            if !model_ready {
                                let _ = sender.try_send(Event::Info("Model is not ready. Check Settings for status. Restart VoicePaste to retry a failed download; partial files are retained.".into()));
                                continue;
                            }
                            match app.toggle(now, preferences.current(), target) {
                                Ok(Some(submission)) => {
                                    let id = submission.session;
                                    let _ = sender.try_send(Event::State(app.state()));
                                    if capture_preview {
                                        match submission.audio.remove() {
                                            Ok(()) => {
                                                let _ = sender.try_send(Event::CaptureCompleted);
                                            }
                                            Err(error) => report(&storage, &sender, error),
                                        }
                                        app.complete(id);
                                    } else if let Err(error) = worker.submit(submission) {
                                        app.complete(id);
                                        report(&storage, &sender, error);
                                    }
                                }
                                Ok(None) => {
                                    if !capture_preview
                                        && matches!(app.state(), State::Recording(_))
                                        && let Err(error) = worker.preload(preferences.current())
                                    {
                                        report(&storage, &sender, error);
                                    }
                                }
                                Err(error) => report(&storage, &sender, error),
                            }
                            let _ = sender.try_send(Event::State(app.state()));
                            if preferences.current().debug_logging {
                                storage.log(&format!("Toggle finished; state {:?}", app.state()));
                            }
                        }
                    }
                    if let Some(result) = preparation.as_mut().and_then(Preparation::poll) {
                        if let Some(task) = &preparation {
                            storage.log(&format!("Model preparation: {}; network_download={}", task.progress.snapshot(), task.progress.downloading()));
                        }
                        drop(preparation.take());
                        model_ready = result.is_ok();
                        let message = match result {
                            Ok(_) => format!("{} is ready (verified local model)", preferences.current().model),
                            Err(error) => { if !error.contains("cancelled") {report(&storage,&sender,error.clone());} else {storage.log(&error);} error },
                        };
                        if !reliable(&sender,Event::ModelStatus {message,preparing:false,downloading:false},&thread_shutdown) { break; }
                        if model_ready && preferences.current().keep_in_memory
                            && let Err(error) = worker.preload(preferences.current()) { report(&storage,&sender,error); }
                    }
                    if last_progress.elapsed() >= Duration::from_millis(250) {
                        if let Some(task) = &preparation { let _ = sender.try_send(Event::ModelStatus { message:task.progress.snapshot(), preparing:true, downloading:task.progress.downloading() }); }
                        last_progress=Instant::now();
                    }
                    while let Some(event) = worker.poll() {
                        match event {
                            TranscriptionEvent::Ready { device } => {
                                storage.log(&format!("Native worker ready on {device}"));
                                if preferences.current().device
                                    == voicepaste_core::settings::Device::CudaAuto
                                    && device == "cpu"
                                {
                                    let _ = sender.try_send(Event::Info(
                                        "CUDA unavailable; auto mode selected CPU INT8".into(),
                                    ));
                                }
                            }
                            TranscriptionEvent::Completed {
                                session,
                                text,
                                target,
                                shortcut,
                            } => {
                                if let Err(error) =
                                    app.deliver(session, &text, target, shortcut, &mut paste)
                                {
                                    report(&storage, &sender, error);
                                }
                                let _ = sender.try_send(Event::State(app.state()));
                            }
                            TranscriptionEvent::Failed { session, message } => {
                                if let Some(session) = session {
                                    app.complete(session);
                                }
                                report(&storage, &sender, message);
                                let _ = sender.try_send(Event::State(app.state()));
                            }
                        }
                    }
                    if !preferences.current().keep_in_memory && app.state() == State::Idle {
                        worker.release();
                    }
                    if let State::Recording(id) = app.state() {
                        // Device-loss/overflow is also detected without waiting for Stop.
                        if app.recording_failed() {
                            let _ = app.cancel(id);
                            report(
                                &storage,
                                &sender,
                                "Microphone failed or audio buffer overflowed; capture discarded"
                                    .into(),
                            );
                            let _ = sender.try_send(Event::State(app.state()));
                        }
                    }
                }
                drop(preparation);
                worker.release();
                if let Err(error) = app.shutdown() {
                    storage.log(&error);
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            commands,
            events,
            thread: Some(thread),
            clock,
            levels,
            shutdown,
        })
    }

    pub fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                TrySendError::Full(_) => "Command queue busy; try again".into(),
                TrySendError::Disconnected(_) => "Dictation thread exited".into(),
            })
    }

    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        // Quit is reliable; normal callbacks use try_send and never block.
        let _ = self.commands.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn reliable(sender: &SyncSender<Event>, mut event: Event, shutdown: &AtomicBool) -> bool {
    loop {
        if shutdown.load(Ordering::Acquire) {
            return false;
        }
        match sender.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(returned)) => {
                event = returned;
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn report(storage: &Storage, sender: &SyncSender<Event>, error: String) {
    storage.log(&error);
    let _ = sender.try_send(Event::Error(error));
}

fn prepare(storage: &Storage, model: &str, sender: &SyncSender<Event>) -> Option<Preparation> {
    match Preparation::start(storage.root.clone(), model) {
        Ok(task) => Some(task),
        Err(error) => {
            let _ = sender.try_send(Event::ModelStatus {
                message: error.clone(),
                preparing: false,
                downloading: false,
            });
            report(storage, sender, error);
            None
        }
    }
}
