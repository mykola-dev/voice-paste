//! Owns/reaps only the bundled worker child, with bounded IPC readers.
use std::{
    io::{BufReader, Read},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use voicepaste_protocol::{Request, Response};
unsafe extern "C" {
    fn vp_host_resident_bytes() -> u64;
    fn vp_process_resident_bytes(pid: i32) -> u64;
}
pub(super) fn memory(process: Option<&Process>, ready: bool) -> String {
    fn mib(bytes: u64) -> String {
        if bytes == u64::MAX {
            "unavailable".into()
        } else {
            format!("{:.1} MiB resident", bytes as f64 / 1048576.)
        }
    }
    // SAFETY: read-only native queries for this process and our owned child.
    let host = unsafe { vp_host_resident_bytes() };
    let child =
        process.map(|process| unsafe { vp_process_resident_bytes(process.child.id() as i32) });
    format!(
        "Host RAM: {}\nSTT worker: {}{}",
        mib(host),
        child.map(mib).unwrap_or_else(|| "not running".into()),
        if ready { " (model loaded)" } else { "" }
    )
}
pub(super) struct Process {
    child: Child,
    stdin: Option<ChildStdin>,
    responses: Receiver<Result<Response, String>>,
    readers: Vec<JoinHandle<()>>,
}
impl Process {
    pub(super) fn spawn(_: &Path) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let directory = exe.parent().ok_or("App executable has no parent")?;
        let mut child = Command::new(directory.join("voicepaste-stt-worker"))
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Cannot launch bundled Metal worker: {e}"))?;
        let (sender, responses) = mpsc::sync_channel(super::transport::RESPONSE_LIMIT);
        let stdin = child.stdin.take();
        let mut owner = Self {
            child,
            stdin,
            responses,
            readers: Vec::new(),
        };
        let stdout = owner.child.stdout.take().ok_or("Missing worker stdout")?;
        let mut stderr = owner.child.stderr.take().ok_or("Missing worker stderr")?;
        owner.readers.push(
            thread::Builder::new()
                .name("metal-stdout".into())
                .spawn(move || super::transport::read_responses(BufReader::new(stdout), sender))
                .map_err(|e| e.to_string())?,
        );
        owner.readers.push(
            thread::Builder::new()
                .name("metal-stderr".into())
                .spawn(move || {
                    let mut buffer = [0; 4096];
                    while let Ok(n) = stderr.read(&mut buffer) {
                        if n == 0 {
                            break;
                        }
                        eprint!("{}", String::from_utf8_lossy(&buffer[..n]));
                    }
                })
                .map_err(|e| e.to_string())?,
        );
        Ok(owner)
    }
    pub(super) fn send(&mut self, request: &Request) -> Result<(), String> {
        voicepaste_protocol::write(self.stdin.as_mut().ok_or("Worker stdin closed")?, request)
    }
    pub(super) fn poll(&self) -> Result<Result<Response, String>, mpsc::TryRecvError> {
        self.responses.try_recv()
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
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}
