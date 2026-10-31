use super::{audio::AudioFile, paste::FocusTarget, storage::Storage};
use std::{
    fs::OpenOptions,
    io::Write,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
use voicepaste_application::{Submission, Transcription, TranscriptionEvent};
use voicepaste_core::{
    SessionId,
    settings::{Settings, TranscriptionOptions},
};
use voicepaste_protocol::{Generation, Request, RequestId, Response, VERSION};
mod transport;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows::Process;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::Process;

pub struct Worker {
    storage: Arc<Storage>,
    process: Option<Process>,
    generation: Generation,
    next_request: u64,
    ready: bool,
    actual_device: String,
    statistics_filter: Option<voicepaste_core::statistics::Filter>,
    options: Option<TranscriptionOptions>,
    model: String,
    pending: Option<(RequestId, Submission<AudioFile, FocusTarget>)>,
    sent: bool,
    deadline: Option<Instant>,
}

impl Worker {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            process: None,
            generation: Generation(0),
            next_request: 0,
            ready: false,
            actual_device: String::new(),
            statistics_filter: None,
            options: None,
            model: String::new(),
            pending: None,
            sent: false,
            deadline: None,
        }
    }
    fn initialize(&mut self, settings: &Settings) -> Result<(), String> {
        let options = TranscriptionOptions::from(settings);
        if self.process.is_some()
            && self.options.as_ref() == Some(&options)
            && self.model == settings.model
        {
            return Ok(());
        }
        self.release();
        let model = super::models::local_path(&self.storage.root, &settings.model)?;
        let statistics_filter = super::models::filter(settings)?;
        self.generation.0 = self
            .generation
            .0
            .checked_add(1)
            .ok_or("Worker generation exhausted")?;
        let mut process = Process::spawn(&self.storage.root)?;
        process.send(&Request::Initialize {
            version: VERSION,
            generation: self.generation,
            model,
            audio_root: self.storage.temp.clone(),
            options: options.clone(),
        })?;
        self.process = Some(process);
        self.options = Some(options);
        self.model = settings.model.clone();
        self.statistics_filter = Some(statistics_filter);
        self.deadline = Some(Instant::now() + Duration::from_secs(180));
        Ok(())
    }
    fn send_pending(&mut self) -> Result<(), String> {
        if self.ready
            && !self.sent
            && let Some((id, submission)) = &self.pending
        {
            self.process
                .as_mut()
                .ok_or("Worker not running")?
                .send(&Request::Transcribe {
                    generation: self.generation,
                    id: *id,
                    session: submission.session,
                    audio: submission.audio.path.clone(),
                })?;
            self.sent = true;
            self.deadline = Some(Instant::now() + Duration::from_secs(600));
        }
        Ok(())
    }
    fn failed(&mut self, message: String) -> TranscriptionEvent<FocusTarget> {
        let session = self.pending.as_ref().map(|(_, s)| s.session);
        self.release();
        TranscriptionEvent::Failed { session, message }
    }

    pub fn memory_status(&self) -> String {
        #[cfg(target_os = "windows")]
        {
            super::memory::status(
                self.process.as_ref().map(Process::working_set).transpose(),
                self.ready,
            )
        }
        #[cfg(target_os = "macos")]
        {
            macos::memory(self.process.as_ref(), self.ready)
        }
    }

    // No mic, clipboard or injection: an explicit file route through the same
    // resident worker transport, useful while bringing up the application.
    pub fn transcribe_file(
        &mut self,
        path: &std::path::Path,
        settings: &Settings,
    ) -> Result<String, String> {
        let destination = self.storage.temp.join(format!(
            "file-request-{}-{}.wav",
            std::process::id(),
            self.next_request
        ));
        let mut input = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|e| e.to_string())?;
        let owned = AudioFile { path: destination };
        std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
        drop(output);
        self.submit(Submission {
            session: SessionId(1),
            audio: owned,
            settings: settings.clone(),
            target: FocusTarget::current(),
        })?;
        loop {
            match self.poll() {
                Some(TranscriptionEvent::Completed { text, .. }) => return Ok(text),
                Some(TranscriptionEvent::Failed { message, .. }) => return Err(message),
                _ => thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}

impl Transcription<AudioFile, FocusTarget> for Worker {
    fn preload(&mut self, settings: &Settings) -> Result<(), String> {
        self.initialize(settings)
    }
    fn submit(&mut self, submission: Submission<AudioFile, FocusTarget>) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("Transcription already in progress".into());
        }
        self.initialize(&submission.settings)?;
        self.next_request = self
            .next_request
            .checked_add(1)
            .ok_or("Worker request counter exhausted")?;
        self.pending = Some((RequestId(self.next_request), submission));
        if let Err(error) = self.send_pending() {
            self.release();
            return Err(error);
        }
        Ok(())
    }
    fn poll(&mut self) -> Option<TranscriptionEvent<FocusTarget>> {
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Some(self.failed("Native worker timed out".into()));
        }
        let result = match self.process.as_ref()?.poll() {
            Ok(response) => response,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                return Some(self.failed("Native worker output disconnected".into()));
            }
        };
        let response = match result {
            Ok(response) => response,
            Err(error) => return Some(self.failed(error)),
        };
        self.receive(response)
    }
    fn release(&mut self) {
        // Kill/join child before dropping the audio file it could still be using.
        drop(self.process.take());
        drop(self.pending.take());
        self.ready = false;
        self.actual_device.clear();
        self.statistics_filter = None;
        self.sent = false;
        self.deadline = None;
    }
}

impl Worker {
    fn receive(&mut self, response: Response) -> Option<TranscriptionEvent<FocusTarget>> {
        match response {
            Response::Ready {
                version,
                generation,
                device,
                mels,
                model_loading_us,
            } => {
                if version != VERSION
                    || generation != self.generation
                    || !matches!(mels, 80 | 128)
                    || self.ready
                {
                    return Some(self.failed("Invalid native worker readiness message".into()));
                }
                if !valid_device(self.options.as_ref().map(|o| o.device), &device)
                    || (self
                        .options
                        .as_ref()
                        .is_some_and(|o| o.device == voicepaste_core::settings::Device::CudaOnly)
                        && device != "cuda")
                {
                    return Some(self.failed("Native worker violated device policy".into()));
                }
                self.ready = true;
                self.actual_device = device.clone();
                self.deadline = None;
                if let Err(error) = self.send_pending() {
                    return Some(self.failed(error));
                }
                let statistics = self.statistics_filter.clone().map(|filter| {
                    voicepaste_core::statistics::ModelLoad {
                        filter,
                        device: device.clone(),
                        loading_us: model_loading_us,
                    }
                });
                Some(TranscriptionEvent::Ready { device, statistics })
            }
            Response::Result {
                generation,
                id,
                session,
                text,
                measurement,
            } => {
                if generation != self.generation
                    || !self.sent
                    || !self
                        .pending
                        .as_ref()
                        .is_some_and(|(expected, s)| *expected == id && s.session == session)
                {
                    return Some(self.failed("Mismatched native worker result".into()));
                }
                let (_, submission) = self.pending.take()?;
                self.sent = false;
                self.deadline = None;
                // Export numeric metadata only. The session owner decides whether
                // this completion is still current before persisting a sample.
                let statistics = super::models::filter(&submission.settings).map(|filter| {
                    voicepaste_core::statistics::Sample::new(
                        filter,
                        &self.actual_device,
                        measurement,
                        &text,
                    )
                });
                let statistics = match statistics {
                    Ok(sample) => sample,
                    Err(error) => {
                        self.storage
                            .log(&format!("Transcription statistics unavailable: {error}"));
                        None
                    }
                };
                Some(TranscriptionEvent::Completed {
                    session,
                    text,
                    target: submission.target,
                    shortcut: submission.settings.paste_shortcut,
                    statistics,
                })
            }
            Response::Error {
                generation,
                id,
                message,
            } => {
                if generation != self.generation
                    || (id.is_some()
                        && !self
                            .pending
                            .as_ref()
                            .is_some_and(|(expected, _)| Some(*expected) == id))
                {
                    return Some(self.failed("Mismatched native worker error".into()));
                }
                Some(self.failed(message))
            }
        }
    }
}

fn valid_device(policy: Option<voicepaste_core::settings::Device>, actual: &str) -> bool {
    use voicepaste_core::settings::Device;
    match policy {
        Some(Device::MetalOnly) => actual == "metal",
        Some(Device::Cpu) => actual == "cpu",
        Some(Device::CudaOnly) => actual == "cuda",
        Some(Device::CudaAuto) => matches!(actual, "cpu" | "cuda"),
        None => false,
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    use voicepaste_core::{settings::Device, statistics::Measurement};
    #[test]
    fn readiness_enforces_exact_device_policy_including_no_metal_fallback() {
        assert!(valid_device(Some(Device::MetalOnly), "metal"));
        for device in ["cpu", "cuda", ""] {
            assert!(!valid_device(Some(Device::MetalOnly), device));
        }
        assert!(!valid_device(Some(Device::Cpu), "metal"));
        assert!(!valid_device(Some(Device::CudaAuto), "metal"));
    }

    struct Fixture {
        worker: Worker,
        root: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .join(format!(
                    "worker-state-test-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir(&root).unwrap();
            #[cfg(target_os = "windows")]
            let storage = Arc::new(Storage::open(&root).unwrap());
            #[cfg(target_os = "macos")]
            let storage = Arc::new(
                Storage::open_at(
                    root.join("data"),
                    root.join("test.log"),
                    root.join("agent.plist"),
                    root.join("app"),
                )
                .unwrap(),
            );
            let mut worker = Worker::new(storage);
            worker.generation = Generation(7);
            worker.options = Some(TranscriptionOptions::from(&worker.storage.settings));
            Self { worker, root }
        }
        fn pending(&mut self) -> PathBuf {
            let path = self.worker.storage.temp.join("owned.wav");
            fs::write(&path, b"owned audio fixture, not microphone data").unwrap();
            self.worker.pending = Some((
                RequestId(11),
                Submission {
                    session: SessionId(3),
                    audio: AudioFile { path: path.clone() },
                    settings: self.worker.storage.settings.clone(),
                    target: FocusTarget::current(),
                },
            ));
            self.worker.sent = true;
            self.worker.ready = true;
            self.worker.actual_device = "cpu".into();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.worker.release();
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn result(generation: u64, request: u64, session: u64) -> Response {
        Response::Result {
            generation: Generation(generation),
            id: RequestId(request),
            session: SessionId(session),
            text: "Привіт, бро".into(),
            measurement: Measurement {
                inference_us: 100,
                audio_samples: 16000,
                vad_us: None,
                preparation_us: Some(20),
            },
        }
    }

    #[test]
    fn matching_result_returns_metadata_and_releases_audio_without_paste() {
        let mut fixture = Fixture::new();
        let audio = fixture.pending();
        let event = fixture.worker.receive(result(7, 11, 3));
        assert!(
            matches!(event, Some(TranscriptionEvent::Completed { session: SessionId(3), text, statistics: Some(_), .. }) if text == "Привіт, бро")
        );
        assert!(!audio.exists());
        assert!(fixture.worker.pending.is_none());
        assert!(fixture.worker.deadline.is_none());
        assert!(!fixture.worker.sent);
    }

    #[test]
    fn stale_generation_request_or_session_fail_and_release_pending_audio() {
        for ids in [(6, 11, 3), (7, 10, 3), (7, 11, 2)] {
            let mut fixture = Fixture::new();
            let audio = fixture.pending();
            assert!(matches!(
                fixture.worker.receive(result(ids.0, ids.1, ids.2)),
                Some(TranscriptionEvent::Failed {
                    session: Some(SessionId(3)),
                    ..
                })
            ));
            assert!(!audio.exists());
            assert!(!fixture.worker.ready);
        }
    }

    #[test]
    fn invalid_readiness_and_cuda_policy_fail_without_submission() {
        for (version, generation, mels, device) in [
            (VERSION + 1, 7, 80, "cuda"),
            (VERSION, 6, 80, "cuda"),
            (VERSION, 7, 81, "cuda"),
            (VERSION, 7, 80, "cpu"),
            (VERSION, 7, 80, "unknown"),
        ] {
            let mut fixture = Fixture::new();
            fixture.worker.options.as_mut().unwrap().device = Device::CudaOnly;
            assert!(matches!(
                fixture.worker.receive(Response::Ready {
                    version,
                    generation: Generation(generation),
                    mels,
                    device: device.into(),
                    model_loading_us: 20,
                }),
                Some(TranscriptionEvent::Failed { session: None, .. })
            ));
            assert!(!fixture.worker.ready);
        }
    }

    #[test]
    fn timeout_and_explicit_release_clear_owned_submission() {
        let mut fixture = Fixture::new();
        let audio = fixture.pending();
        fixture.worker.deadline = Some(Instant::now());
        assert!(matches!(
            fixture.worker.poll(),
            Some(TranscriptionEvent::Failed {
                session: Some(SessionId(3)),
                ..
            })
        ));
        assert!(!audio.exists());
        let audio = fixture.pending();
        fixture.worker.release();
        assert!(!audio.exists());
        assert!(fixture.worker.poll().is_none());
    }
}
