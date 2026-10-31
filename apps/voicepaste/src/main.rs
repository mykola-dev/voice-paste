#![cfg_attr(all(not(test), target_os = "windows"), windows_subsystem = "windows")]
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
compile_error!("VoicePaste supports Windows and Apple Silicon macOS; Linux is not implemented");
#[cfg(test)]
extern crate gpui_kit as gpui;
mod adapters;
mod bootstrap;
mod runtime;

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
    match bootstrap::run() {
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
    match bootstrap::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("VoicePaste macOS preview failed: {error}");
            adapters::platform::startup_error(&error);
            std::process::ExitCode::FAILURE
        }
    }
}
