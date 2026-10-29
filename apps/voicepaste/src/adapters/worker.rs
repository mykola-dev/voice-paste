use super::{audio::AudioFile, storage::Storage};
use std::{
    fs::OpenOptions,
    io::{BufReader, Read, Write},
    mem::{size_of, zeroed},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    ptr::null,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use voicepaste_application::{Submission, Transcription, TranscriptionEvent};
use voicepaste_core::{
    SessionId,
    settings::{Settings, TranscriptionOptions},
};
use voicepaste_protocol::{Generation, Request, RequestId, Response, VERSION};
use windows_sys::Win32::{Foundation::*, System::JobObjects::*};

struct Job(HANDLE);
impl Drop for Job {
    fn drop(&mut self) {
        /* SAFETY: owned kill-on-close job, containing only our child. */
        unsafe {
            CloseHandle(self.0);
        }
    }
}
impl Job {
    fn new() -> Result<Self, String> {
        // SAFETY: zero-initialized POD, owned handle configured before assignment.
        unsafe {
            let handle = CreateJobObjectW(null(), null());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let job = Self(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            Ok(job)
        }
    }
}

struct Process {
    child: Child,
    job: Option<Job>,
    stdin: Option<ChildStdin>,
    responses: Receiver<Result<Response, String>>,
    readers: Vec<JoinHandle<()>>,
}
impl Process {
    fn spawn(storage: &Arc<Storage>) -> Result<Self, String> {
        let runtime = storage.root.join("runtime");
        let job = Job::new()?;
        let mut child = Command::new(runtime.join("voicepaste-stt-worker.exe"))
            .current_dir(&runtime)
            .creation_flags(0x08000000)
            .env(
                "PATH",
                std::env::var_os("SystemRoot")
                    .map(PathBuf::from)
                    .ok_or("SystemRoot missing")?
                    .join("System32"),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Cannot launch native STT worker: {e}"))?;
        // SAFETY: handle refers to our newly spawned child, waiting for Initialize
        // on stdin. Parent exit closes pipes even before job assignment completes.
        if unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) } == 0 {
            let error = std::io::Error::last_os_error();
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("Cannot own STT worker job: {error}"));
        }
        let (sender, responses) = mpsc::sync_channel(16);
        let stdout = child.stdout.take().ok_or("Worker stdout missing")?;
        let mut stderr = child.stderr.take().ok_or("Worker stderr missing")?;
        let stdin = child.stdin.take();
        let mut process = Self {
            child,
            job: Some(job),
            stdin,
            responses,
            readers: Vec::new(),
        };
        process.readers.push(
            thread::Builder::new()
                .name("stt-stdout".into())
                .spawn(move || {
                    let mut stdout = BufReader::new(stdout);
                    loop {
                        match voicepaste_protocol::read::<Response>(&mut stdout) {
                            Ok(Some(response)) => {
                                if sender.try_send(Ok(response)).is_err() {
                                    break;
                                }
                            }
                            result => {
                                let error = match result {
                                    Err(error) => error,
                                    _ => "Native worker output closed".into(),
                                };
                                let _ = sender.try_send(Err(error));
                                break;
                            }
                        }
                    }
                })
                .map_err(|e| e.to_string())?,
        );
        let storage = storage.clone();
        process.readers.push(
            thread::Builder::new()
                .name("stt-stderr".into())
                .spawn(move || {
                    let mut buffer = [0u8; 4096];
                    loop {
                        match stderr.read(&mut buffer) {
                            Ok(0) | Err(_) => break,
                            Ok(count) => storage.log(&format!(
                                "STT: {}",
                                String::from_utf8_lossy(&buffer[..count])
                            )),
                        }
                    }
                })
                .map_err(|e| e.to_string())?,
        );
        Ok(process)
    }
    fn send(&mut self, request: &Request) -> Result<(), String> {
        voicepaste_protocol::write(self.stdin.as_mut().ok_or("Worker stdin closed")?, request)
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.send(&Request::Shutdown);
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                _ if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                _ => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
        // Close the job before joining readers: any owned descendant inheriting
        // a pipe must also exit, otherwise EOF could wait on that descendant.
        drop(self.job.take());
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}

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
    pending: Option<(RequestId, Submission<AudioFile>)>,
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
        let statistics_filter = voicepaste_core::statistics::Filter::from_settings(settings)?;
        self.generation.0 = self
            .generation
            .0
            .checked_add(1)
            .ok_or("Worker generation exhausted")?;
        let mut process = Process::spawn(&self.storage)?;
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
    fn failed(&mut self, message: String) -> TranscriptionEvent {
        let session = self.pending.as_ref().map(|(_, s)| s.session);
        self.release();
        TranscriptionEvent::Failed { session, message }
    }

    pub fn memory_status(&self) -> String {
        super::memory::status(
            self.process
                .as_ref()
                .map(|process| process.child.as_raw_handle()),
            self.ready,
        )
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
            target: 0,
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

impl Transcription<AudioFile> for Worker {
    fn preload(&mut self, settings: &Settings) -> Result<(), String> {
        self.initialize(settings)
    }
    fn submit(&mut self, submission: Submission<AudioFile>) -> Result<(), String> {
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
    fn poll(&mut self) -> Option<TranscriptionEvent> {
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Some(self.failed("Native worker timed out".into()));
        }
        let result = match self.process.as_ref()?.responses.try_recv() {
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
                if !matches!(device.as_str(), "cpu" | "cuda")
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
                let statistics = voicepaste_core::statistics::Filter::from_settings(
                    &submission.settings,
                )
                .map(|filter| {
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
impl Drop for Worker {
    fn drop(&mut self) {
        self.release();
    }
}
