#![cfg_attr(not(test), windows_subsystem = "windows")]
#[cfg(test)]
extern crate gpui_kit as gpui;
mod adapters;
mod runtime;

use adapters::{platform, storage::Storage};
use std::{path::PathBuf, sync::Arc};
use voicepaste_core::settings::Hotkey;

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let file_mode = args.len() >= 2 && args[0] == "--transcribe-file";
    let settings_only = args == ["--settings"];
    let download_mode = args == ["--download-model"];
    if !args.is_empty()
        && args != ["--check"]
        && args != ["--capture-preview"]
        && !file_mode
        && !settings_only
        && !download_mode
    {
        return Err(
            "Usage: voicepaste [--check | --settings | --download-model | --capture-preview | --transcribe-file <wav>]".into(),
        );
    }
    let root: PathBuf = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Executable has no parent folder")?
        .to_owned();
    let storage = Arc::new(Storage::open(&root)?);
    Hotkey::parse(&storage.settings.hotkey)?;
    if args == ["--check"] {
        println!(
            "Portable settings valid; GPUI settings/recording overlay, model provisioning, STT and paste are connected. This check uses no network, microphone, GPU or clipboard."
        );
        return Ok(());
    }
    if file_mode {
        let mut worker = adapters::worker::Worker::new(storage.clone());
        for file in &args[1..] {
            println!(
                "{}",
                worker.transcribe_file(std::path::Path::new(file), &storage.settings)?
            );
        }
        return Ok(());
    }
    if download_mode {
        let mut preparation =
            adapters::models::Preparation::start(root.clone(), &storage.settings.model)?;
        loop {
            if let Some(result) = preparation.poll() {
                println!("Model ready: {}", result?.display());
                return Ok(());
            }
            println!("{}", preparation.progress.snapshot());
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
    let log_path = Storage::log_path().display().to_string();
    let capture_preview = args == ["--capture-preview"];
    let runtime = runtime::Runtime::start(storage.clone(), capture_preview || settings_only)?;
    let result = platform::run(
        runtime,
        &storage.settings,
        &log_path,
        capture_preview,
        settings_only,
    );
    if let Err(ref error) = result {
        storage.log(error);
    }
    result
}

fn main() -> std::process::ExitCode {
    // GUI launches never allocate a console, even in dev builds. Explicit CLI
    // modes may reuse the caller's console; inherited redirected handles remain
    // usable when no parent console exists (e.g. CI / Start-Process redirects).
    let cli = std::env::args_os()
        .skip(1)
        .any(|arg| arg != "--settings" && arg != "--capture-preview");
    if cli {
        // SAFETY: inspect inherited handles without owning/closing them. Attaching
        // resets standard handles, so preserve explicit file/pipe redirection.
        unsafe {
            use windows_sys::Win32::{
                Storage::FileSystem::{FILE_TYPE_DISK, FILE_TYPE_PIPE, GetFileType},
                System::Console::*,
            };
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
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            // Startup failure must be visible in every console-free GUI build.
            // This is an explicit error dialog, not a recording overlay.
            if !cli {
                let message: Vec<u16> = error.encode_utf16().chain(Some(0)).collect();
                let title: Vec<u16> = "VoicePaste Rust — startup error"
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                // SAFETY: both strings are live, NUL-terminated UTF-16 buffers.
                unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                        std::ptr::null_mut(),
                        message.as_ptr(),
                        title.as_ptr(),
                        0x10,
                    );
                }
            }
            std::process::ExitCode::FAILURE
        }
    }
}
