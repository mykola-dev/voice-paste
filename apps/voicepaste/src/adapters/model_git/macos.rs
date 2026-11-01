//! macOS executable discovery and owned subprocess group; no download policy.
use std::{
    io,
    os::unix::process::CommandExt,
    process::{Child, Command, ExitStatus},
};

pub(super) struct Process(Child);
impl Process {
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        // Finder does not inherit an interactive shell's Homebrew PATH.
        let mut paths: Vec<_> =
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
        paths.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(Into::into));
        let path = std::env::join_paths(paths).map_err(io::Error::other)?;
        command.env("PATH", path).process_group(0).spawn().map(Self)
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.0.try_wait()
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: process_group(0) creates an owned group including Git helpers.
        unsafe { libc::kill(-(self.0.id() as i32), libc::SIGKILL) };
        let _ = self.0.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drop_terminates_and_reaps_owned_child() {
        let process = Process::spawn(Command::new("/bin/sleep").arg("60")).unwrap();
        let id = process.0.id();
        drop(process);
        // SAFETY: signal 0 queries this previously owned child's lifetime.
        assert_eq!(unsafe { libc::kill(id as i32, 0) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    }
}
