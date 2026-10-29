//! Owns the Windows child/job/pipes/readers; no native identity escapes to Worker.
use crate::adapters::{diagnostics, memory};
use std::{
    io::{BufReader, Read},
    mem::{size_of, zeroed},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    ptr::null,
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use voicepaste_protocol::{Request, Response};
use windows_sys::Win32::{Foundation::*, System::JobObjects::*};

struct Job(HANDLE);
impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: owned kill-on-close job, containing only our child tree.
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

pub(super) struct Process {
    child: Child,
    job: Option<Job>,
    stdin: Option<ChildStdin>,
    responses: Receiver<Result<Response, String>>,
    readers: Vec<JoinHandle<()>>,
}
impl Process {
    pub(super) fn spawn(root: &Path) -> Result<Self, String> {
        let runtime = root.join("runtime");
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
        // SAFETY: our new child waits for Initialize on stdin. No foreign process.
        if unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) } == 0 {
            let error = std::io::Error::last_os_error();
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("Cannot own STT worker job: {error}"));
        }
        let (sender, responses) = mpsc::sync_channel(super::transport::RESPONSE_LIMIT);
        // Establish the owner before any fallible pipe/thread setup: every failure
        // kills/waits the child, closes the job and joins already-started readers.
        let stdin = child.stdin.take();
        let mut process = Self {
            child,
            job: Some(job),
            stdin,
            responses,
            readers: Vec::new(),
        };
        let stdout = process.child.stdout.take().ok_or("Worker stdout missing")?;
        let mut stderr = process.child.stderr.take().ok_or("Worker stderr missing")?;
        process.readers.push(
            thread::Builder::new()
                .name("stt-stdout".into())
                .spawn(move || {
                    super::transport::read_responses(BufReader::new(stdout), sender);
                })
                .map_err(|e| e.to_string())?,
        );
        process.readers.push(
            thread::Builder::new()
                .name("stt-stderr".into())
                .spawn(move || {
                    let mut buffer = [0u8; 4096];
                    loop {
                        match stderr.read(&mut buffer) {
                            Ok(0) | Err(_) => break,
                            Ok(count) => diagnostics::log(&format!(
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

    pub(super) fn send(&mut self, request: &Request) -> Result<(), String> {
        voicepaste_protocol::write(self.stdin.as_mut().ok_or("Worker stdin closed")?, request)
    }

    pub(super) fn poll(&self) -> Result<Result<Response, String>, mpsc::TryRecvError> {
        self.responses.try_recv()
    }

    pub(super) fn working_set(&self) -> Result<usize, String> {
        memory::child(&self.child)
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
        // Owned descendants must release inherited pipes before reader joins.
        drop(self.job.take());
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, WaitForSingleObject};

    // Child is this test executable, not a deployed app or loaded ML worker.
    // This bounded fixture is inert in normal tests and has no UI/clipboard/ML.
    #[test]
    fn owned_process_fixture() {
        match std::env::var("VOICEPASTE_OWNED_PROCESS_FIXTURE").as_deref() {
            Ok("blocked") => loop {
                thread::park();
            },
            Ok("eof") => {
                let _ = std::io::stdin().read_to_end(&mut Vec::new());
            }
            _ => (),
        }
    }

    struct Probe(HANDLE);
    impl Drop for Probe {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn check_cleanup(mode: &str) {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "adapters::worker::windows::tests::owned_process_fixture",
                "--nocapture",
            ])
            .env("VOICEPASTE_OWNED_PROCESS_FIXTURE", mode)
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let job = Job::new().unwrap();
        let mut probe = Probe(std::ptr::null_mut());
        unsafe {
            assert_ne!(AssignProcessToJobObject(job.0, child.as_raw_handle()), 0);
            assert_ne!(
                DuplicateHandle(
                    GetCurrentProcess(),
                    child.as_raw_handle(),
                    GetCurrentProcess(),
                    &mut probe.0,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS
                ),
                0
            );
        }
        let stdout = child.stdout.take().unwrap();
        let joined = Arc::new(AtomicBool::new(false));
        let finished = joined.clone();
        let reader = thread::spawn(move || {
            let _ = BufReader::new(stdout).read_to_end(&mut Vec::new());
            finished.store(true, Ordering::Release);
        });
        let (_, responses) = mpsc::sync_channel(1);
        let stdin = child.stdin.take();
        let process = Process {
            child,
            job: Some(job),
            stdin,
            responses,
            readers: vec![reader],
        };
        drop(process);
        assert_eq!(unsafe { WaitForSingleObject(probe.0, 2000) }, WAIT_OBJECT_0);
        assert!(joined.load(Ordering::Acquire));
    }

    #[test]
    fn graceful_drop_closes_stdin_reaps_child_and_joins_reader() {
        check_cleanup("eof");
    }

    #[test]
    fn forced_drop_kills_only_owned_child_and_joins_reader() {
        check_cleanup("blocked");
    }
}
