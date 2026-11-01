//! Hidden, suspended-until-owned child tree. No Git or download policy.
use std::{
    io,
    mem::{size_of, zeroed},
    os::windows::{io::AsRawHandle, process::CommandExt},
    process::{Child, Command, ExitStatus},
    ptr::null,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: uniquely owned valid native handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub(super) struct Process {
    child: Child,
    job: Option<Handle>,
}

impl Process {
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        // SAFETY: POD configuration; every successful handle immediately has an owner.
        let job = unsafe {
            let handle = CreateJobObjectW(null(), null());
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Handle(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            job
        };
        let child = command
            .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
            .spawn()?;
        let owner = Self {
            child,
            job: Some(job),
        };
        // The child cannot launch helpers before job assignment. Unlike ordinary
        // spawn-then-assign, this also owns descendants on fast failure paths.
        unsafe {
            if AssignProcessToJobObject(
                owner
                    .job
                    .as_ref()
                    .ok_or_else(|| io::Error::other("Missing child job"))?
                    .0,
                owner.child.as_raw_handle(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        resume(owner.child.id())?;
        Ok(owner)
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }
}

fn resume(pid: u32) -> io::Result<()> {
    // std::process closes the primary thread handle. The suspended process has
    // exactly one thread; reopen only that owned process's thread for ResumeThread.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let snapshot = Handle(snapshot);
        let mut entry: THREADENTRY32 = zeroed();
        entry.dwSize = size_of::<THREADENTRY32>() as u32;
        let mut found = Thread32First(snapshot.0, &mut entry);
        while found != 0 {
            if entry.th32OwnerProcessID == pid {
                let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                if thread.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let thread = Handle(thread);
                if ResumeThread(thread.0) == u32::MAX {
                    return Err(io::Error::last_os_error());
                }
                return Ok(());
            }
            found = Thread32Next(snapshot.0, &mut entry);
        }
        Err(io::Error::other(
            "Could not find owned suspended child thread",
        ))
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        drop(self.job.take()); // terminate the entire owned tree, including helpers
        let _ = self.child.kill(); // also covers job-assignment failure
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, process::Stdio};

    #[test]
    fn hidden_child_is_job_owned_before_running_and_terminated_on_drop() {
        let command =
            PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/cmd.exe");
        let mut process = Process::spawn(
            Command::new(command)
                .args(["/D", "/C", "set /p voicepaste_wait="])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
        )
        .unwrap();
        assert!(process.try_wait().unwrap().is_none());
        // SAFETY: query only the owned child's process/job, retaining a query
        // handle to verify termination after its owner has been dropped.
        unsafe {
            let mut owned = 0;
            assert_ne!(
                IsProcessInJob(
                    process.child.as_raw_handle(),
                    process.job.as_ref().unwrap().0,
                    &mut owned
                ),
                0
            );
            assert_ne!(owned, 0);
            let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, process.child.id());
            assert!(!handle.is_null());
            let handle = Handle(handle);
            drop(process);
            assert_eq!(WaitForSingleObject(handle.0, 1000), WAIT_OBJECT_0);
        }
    }
}
