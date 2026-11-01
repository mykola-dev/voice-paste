use crate::adapters::{
    audio::Microphone,
    models::Preparation,
    paste::{ClipboardPaste, FocusTarget},
    storage::Storage,
    worker::Worker,
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
    Toggle {
        now: Duration,
        target: FocusTarget,
    },
    Cancel(SessionId),
    SaveSettings {
        id: u64,
        settings: Settings,
    },
    Memory {
        id: u64,
    },
    ModelAvailability {
        id: u64,
        model: String,
    },
    Statistics {
        id: u64,
        filter: voicepaste_core::statistics::Filter,
    },
    ResetStatistics {
        id: u64,
    },
    DownloadModel,
    CancelModelDownload {
        generation: u64,
    },
    Quit,
}
pub enum Event {
    PreparationStarted {
        generation: u64,
    },
    ModelPreparationFailed {
        model: String,
        message: String,
    },
    ModelPreparationCancelled {
        model: String,
    },
    ModelReady {
        model: String,
    },
    StartupModelUnavailable {
        model: String,
    },
    StatisticsReset {
        id: u64,
        result: Result<(), String>,
    },
    Statistics {
        id: u64,
        filter: voicepaste_core::statistics::Filter,
        result: Result<Vec<voicepaste_core::statistics::Summary>, String>,
    },
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
                let mut statistics = crate::adapters::statistics::Statistics::open(&storage.root.join("data/transcription-stats.sqlite"));
                if let Err(error) = &statistics { storage.log(&format!("Transcription statistics unavailable: {error}")); }
                let mut paste = ClipboardPaste {
                    shutdown: thread_shutdown.clone(),
                };
                let mut preferences = SettingsBoundary::new(storage.settings.clone());
                let mut model_ready = capture_preview;
                let mut preparation_generation = 1u64;
                let mut preparation = if capture_preview { None } else {
                    prepare(&storage,&preferences.current().model,&sender,&thread_shutdown)
                };
                let mut last_progress = Instant::now();
                let mut startup_check = !capture_preview;
                if preparation.is_some() && !reliable(&sender, Event::PreparationStarted { generation: preparation_generation }, &thread_shutdown) { return; }
                loop {
                    if thread_shutdown.load(Ordering::Acquire) {
                        // Stop capture before disk commits or native-child waits;
                        // quitting must not keep the microphone live behind UI teardown.
                        if let Err(error)=app.shutdown(){storage.log(&error);}
                        // An accepted Save remains a persistence obligation even
                        // when OS Quit ends the UI before its completion arrives.
                        while let Ok(command)=receiver.try_recv(){
                            if let Command::SaveSettings{settings,..}=command
                                && let Err(error)=voicepaste_application::settings::save(&*storage,settings){storage.log(&format!("Settings save during shutdown failed: {error}"));}
                        }
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
                            startup_check=true;
                            drop(preparation.take()); model_ready=false;
                            preparation_generation = preparation_generation.checked_add(1).expect("Preparation generation exhausted");
                            if !reliable(&sender, Event::PreparationStarted { generation: preparation_generation }, &thread_shutdown) { break; }
                            preparation=prepare(&storage,&current.model,&sender,&thread_shutdown);
                        } else if model_ready && !capture_preview && current.keep_in_memory
                            && let Err(error) = worker.preload(current) { report(&storage,&sender,error); }
                    }
                    match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(Command::Quit) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Ok(Command::CancelModelDownload { generation }) => {
                            if !current_preparation(generation, preparation_generation, preparation.is_some()) { continue; }
                            startup_check=false;
                            if preparation.is_some(){
                                drop(preparation.take());model_ready=false;
                                if !reliable(&sender, Event::ModelPreparationCancelled { model: preferences.current().model.clone() }, &thread_shutdown) { break; }
                                if !reliable(&sender, Event::ModelStatus{message:"Model preparation cancelled; partial download retained".into(),preparing:false,downloading:false}, &thread_shutdown) { break; }
                            }
                        }
                        Ok(Command::DownloadModel) => {
                            if app.state()!=State::Idle {report(&storage,&sender,"Wait until dictation is idle before downloading a model".into());continue;}
                            startup_check=false;
                            worker.release();drop(preparation.take());model_ready=false;
                            preparation_generation = preparation_generation.checked_add(1).expect("Preparation generation exhausted");
                            if !reliable(&sender, Event::PreparationStarted { generation: preparation_generation }, &thread_shutdown) { break; }
                            match crate::adapters::models::Preparation::start_with_download(storage.root.clone(),&preferences.current().model,true) {
                                Ok(task)=>preparation=Some(task),
                                Err(error)=>{
                                    storage.log(&error);
                                    if !reliable(&sender, Event::ModelStatus { message: error.clone(), preparing:false, downloading:false }, &thread_shutdown) { break; }
                                    if !reliable(&sender, Event::ModelPreparationFailed { model:preferences.current().model.clone(), message:error }, &thread_shutdown) { break; }
                                },
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => (),
                        Ok(Command::SaveSettings { id, settings }) => {
                            let result = voicepaste_application::settings::save(&*storage, settings);
                            if let Ok(settings) = &result { preferences.saved(settings.clone()); }
                            if !reliable(&sender, Event::SettingsSaved { id, result }, &thread_shutdown) { break; }
                        }
                        Ok(Command::Memory { id }) => { let _ = sender.try_send(Event::Memory { id, message: worker.memory_status() }); }
                        Ok(Command::Statistics { id, filter }) => {
                            let result = statistics.as_ref().map_err(Clone::clone).and_then(|db| db.query(&filter));
                            if !reliable(&sender, Event::Statistics { id, filter, result }, &thread_shutdown) { break; }
                        }
                        Ok(Command::ResetStatistics { id }) => {
                            let result = statistics.as_mut().map_err(|error| error.clone()).and_then(|db| db.reset());
                            if let Err(error) = &result { storage.log(&format!("Statistics reset failed: {error}")); }
                            if !reliable(&sender, Event::StatisticsReset { id, result }, &thread_shutdown) { break; }
                        }
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
                        let network_attempted = preparation.as_ref().is_some_and(|task| task.progress.network_attempted());
                        if let Some(task) = &preparation {
                            storage.log(&format!("Model preparation: {}; network_download={}", task.progress.snapshot(), task.progress.downloading()));
                        }
                        drop(preparation.take());
                        model_ready = result.is_ok();
                        if model_ready && !reliable(&sender, Event::ModelReady { model: preferences.current().model.clone() }, &thread_shutdown) { break; }
                        let missing_local = std::mem::take(&mut startup_check) && !model_ready;
                        if missing_local
                            && !reliable(&sender, Event::StartupModelUnavailable { model: preferences.current().model.clone() }, &thread_shutdown) { break; }
                        let message = match result {
                            Ok(_) => format!("{} is ready (verified local model)", preferences.current().model),
                            Err(error) => {
                                storage.log(&error);
                                if (!missing_local || network_attempted) && !error.contains("cancelled")
                                    && !reliable(&sender, Event::ModelPreparationFailed { model: preferences.current().model.clone(), message: format!("Could not prepare the selected model. Verified files and resumable downloads are retained. Try Start recording again to retry.\n\n{error}") }, &thread_shutdown) { break; }
                                error
                            },
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
                            TranscriptionEvent::Ready { device, statistics: sample } => {
                                if let Some(sample) = sample
                                    && let Err(error) = statistics.as_ref().map_err(Clone::clone).and_then(|db| db.record_load(&sample))
                                {
                                    storage.log(&format!("Model loading statistics not recorded: {error}"));
                                }
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
                                statistics: sample,
                            } => {
                                let accepted = app.state() == State::Transcribing(session);
                                if let Err(error) =
                                    app.deliver(session, &text, target, shortcut, &mut paste)
                                {
                                    report(&storage, &sender, error);
                                }
                                if accepted && let Some(sample) = sample
                                    && let Err(error) = statistics.as_ref().map_err(Clone::clone).and_then(|db| db.record(&sample))
                                {
                                    storage.log(&format!("Transcription statistics not recorded: {error}"));
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
                if let Err(error) = app.shutdown() {
                    storage.log(&error);
                }
                drop(preparation);
                worker.release();
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

    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        // The flag is authoritative, even with a full command queue. Wake an
        // idle receiver if possible; never block shutdown trying to enqueue Quit.
        let _ = self.commands.try_send(Command::Quit);
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

fn current_preparation(requested: u64, current: u64, active: bool) -> bool {
    active && requested == current
}

fn prepare(
    storage: &Storage,
    model: &str,
    sender: &SyncSender<Event>,
    shutdown: &AtomicBool,
) -> Option<Preparation> {
    match Preparation::start(storage.root.clone(), model) {
        Ok(task) => Some(task),
        Err(error) => {
            storage.log(&error);
            let _ = reliable(
                sender,
                Event::ModelStatus {
                    message: error.clone(),
                    preparing: false,
                    downloading: false,
                },
                shutdown,
            );
            let _ = reliable(
                sender,
                Event::ModelPreparationFailed {
                    model: model.into(),
                    message: error,
                },
                shutdown,
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_cannot_target_a_replacement_or_finished_preparation() {
        assert!(current_preparation(1, 1, true));
        assert!(!current_preparation(1, 2, true));
        assert!(!current_preparation(1, 1, false));
        assert!(!current_preparation(2, 3, true)); // retry of the same model
    }

    #[test]
    fn full_event_queue_does_not_prevent_shutdown_or_receiver_disconnect() {
        for disconnected in [false, true] {
            let (sender, receiver) = mpsc::sync_channel(1);
            let mut receiver = Some(receiver);
            assert!(sender.try_send(Event::CaptureCompleted).is_ok());
            let shutdown = Arc::new(AtomicBool::new(false));
            let signal = shutdown.clone();
            let (done, completed) = mpsc::channel();
            let task = thread::spawn(move || {
                done.send(reliable(&sender, Event::CaptureCompleted, &signal))
                    .unwrap();
            });
            if disconnected {
                drop(receiver.take());
            } else {
                shutdown.store(true, Ordering::Release);
            }
            let result = completed.recv_timeout(Duration::from_secs(1));
            // Always unblock the thread before joining, including a failed check.
            shutdown.store(true, Ordering::Release);
            drop(receiver);
            task.join().unwrap();
            assert!(!result.unwrap());
        }
    }

    #[test]
    fn shutdown_does_not_block_on_a_full_command_queue() {
        let (commands, receiver) = mpsc::sync_channel(1);
        commands.send(Command::Quit).unwrap();
        let (_, events) = mpsc::sync_channel(1);
        let (_, levels) = crossbeam_channel::bounded(1);
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut runtime = Runtime {
            commands,
            events,
            thread: None,
            clock: Instant::now(),
            levels,
            shutdown: shutdown.clone(),
        };
        let (done, completed) = mpsc::channel();
        let task = thread::spawn(move || {
            runtime.shutdown();
            done.send(()).unwrap();
        });
        let result = completed.recv_timeout(Duration::from_secs(1));
        // Unblock the old implementation before joining even when it fails.
        drop(receiver);
        task.join().unwrap();
        assert!(
            result.is_ok(),
            "shutdown waited for space in the command queue"
        );
        assert!(shutdown.load(Ordering::Acquire));
    }
}
