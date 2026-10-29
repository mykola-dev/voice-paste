//! Resident RAM, including the always-running host, independent of residency policy.
use std::mem::{size_of, zeroed};
use windows_sys::Win32::{
    Foundation::HANDLE,
    System::{ProcessStatus::*, Threading::GetCurrentProcess},
};

fn working_set(process: HANDLE) -> Result<usize, String> {
    // SAFETY: callers supply either the current-process pseudo handle or an owned
    // live child-process handle. The sized output structure is writable.
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = zeroed();
        counters.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(process, &mut counters, counters.cb) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(counters.WorkingSetSize)
    }
}

pub fn status(worker: Option<HANDLE>, ready: bool) -> String {
    // SAFETY: GetCurrentProcess returns a borrowed pseudo handle; never close it.
    let host = working_set(unsafe { GetCurrentProcess() });
    let worker = worker.map(working_set).transpose();
    format_status(host, worker, ready)
}

fn format_status(
    host: Result<usize, String>,
    worker: Result<Option<usize>, String>,
    ready: bool,
) -> String {
    let mib = |bytes: usize| bytes as f64 / 1048576.;
    let host_label = match &host {
        Ok(bytes) => format!("Main application: {:.1} MiB", mib(*bytes)),
        Err(_) => "Main application: unavailable".into(),
    };
    let worker_label = match &worker {
        Ok(Some(bytes)) => format!(
            "Transcription worker: {:.1} MiB{}",
            mib(*bytes),
            if ready { "" } else { " (loading)" }
        ),
        Ok(None) => "Transcription worker: 0.0 MiB (model unloaded)".into(),
        Err(_) => "Transcription worker: unavailable".into(),
    };
    format!("{host_label}\n{worker_label}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unloaded_worker_still_shows_host_and_loading_counts_worker() {
        let idle = format_status(Ok(80 * 1048576), Ok(None), false);
        assert!(idle.contains("Main application: 80.0 MiB"));
        assert!(idle.contains("Transcription worker: 0.0 MiB (model unloaded)"));
        assert!(!idle.contains("Combined resident RAM"));
        assert_eq!(idle.lines().count(), 2);
        assert!(!idle.contains("GPU memory"));
        assert!(!idle.contains("WS sum"));
        let loaded = format_status(Ok(80 * 1048576), Ok(Some(200 * 1048576)), false);
        assert!(loaded.contains("Transcription worker: 200.0 MiB (loading)"));
        assert!(!loaded.contains("Combined resident RAM"));
        let failure = format_status(Ok(80 * 1048576), Err("process exited".into()), false);
        assert!(failure.contains("Transcription worker: unavailable"));
        assert!(!failure.contains("Combined resident RAM"));
    }
    #[test]
    fn current_host_is_measured_without_a_worker() {
        let text = status(None, false);
        assert!(!text.contains("Main application: unavailable"));
        assert!(!text.contains("Main application: 0.0 MiB"));
    }
}
