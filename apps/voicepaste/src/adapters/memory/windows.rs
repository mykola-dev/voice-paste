//! Borrowed process handles remain local to the Windows memory probe.
use std::{
    mem::{size_of, zeroed},
    os::windows::io::AsRawHandle,
    process::Child,
};
use windows_sys::Win32::{
    Foundation::HANDLE,
    System::{ProcessStatus::*, Threading::GetCurrentProcess},
};

fn working_set(process: HANDLE) -> Result<usize, String> {
    // SAFETY: live child or current-process pseudo handle, correctly sized output.
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = zeroed();
        counters.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(process, &mut counters, counters.cb) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(counters.WorkingSetSize)
    }
}

pub(super) fn host() -> Result<usize, String> {
    // SAFETY: borrowed pseudo handle, never closed.
    working_set(unsafe { GetCurrentProcess() })
}

pub(in crate::adapters) fn child(child: &Child) -> Result<usize, String> {
    working_set(child.as_raw_handle())
}
