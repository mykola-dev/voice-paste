use windows_sys::Win32::{
    Storage::FileSystem::{FILE_TYPE_DISK, FILE_TYPE_PIPE, GetFileType},
    System::Console::*,
    UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW},
};

pub fn attach_parent_console() {
    // SAFETY: borrowed inherited handles. AttachConsole resets std handles, so
    // preserve file/pipe redirection. Never allocate a console for GUI launch.
    unsafe {
        let redirected = [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].into_iter().any(|id| {
            matches!(
                GetFileType(GetStdHandle(id)),
                FILE_TYPE_DISK | FILE_TYPE_PIPE
            )
        });
        if !redirected {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

pub fn startup_error(error: &str) {
    let message: Vec<u16> = error.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "VoicePaste Rust — startup error"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: live, NUL-terminated UTF-16 strings; explicit startup error only.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_ICONERROR,
        );
    }
}
