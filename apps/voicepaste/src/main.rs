#![cfg_attr(all(not(test), target_os = "windows"), windows_subsystem = "windows")]
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
compile_error!(
    "VoicePaste supports Windows and the macOS launch preview; Linux is not implemented"
);
#[cfg(test)]
extern crate gpui_kit as gpui;
#[cfg(target_os = "windows")]
mod adapters;
#[cfg(target_os = "macos")]
#[path = "adapters/platform/macos.rs"]
mod macos;
#[cfg(target_os = "windows")]
mod runtime;
#[cfg(target_os = "macos")]
#[path = "adapters/settings_theme.rs"]
mod settings_theme;

#[cfg(target_os = "windows")]
use adapters::{platform, storage::Storage};
#[cfg(target_os = "windows")]
use std::{path::PathBuf, sync::Arc};
#[cfg(target_os = "windows")]
use voicepaste_core::settings::Hotkey;

#[cfg(target_os = "windows")]
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

#[cfg(target_os = "windows")]
fn main() -> std::process::ExitCode {
    // GUI launches never allocate a console, even in dev builds. Explicit CLI
    // modes may reuse the caller's console; inherited redirected handles remain
    // usable when no parent console exists (e.g. CI / Start-Process redirects).
    let cli = std::env::args_os()
        .skip(1)
        .any(|arg| arg != "--settings" && arg != "--capture-preview");
    if cli {
        adapters::launch::attach_parent_console();
    }
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            // Startup failure must be visible in every console-free GUI build.
            // This is an explicit error dialog, not a recording overlay.
            if !cli {
                adapters::launch::startup_error(&error);
            }
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "macos")]
fn main() -> std::process::ExitCode {
    match macos::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("VoicePaste launch preview failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
