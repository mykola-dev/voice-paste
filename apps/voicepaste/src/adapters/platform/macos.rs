//! macOS composition: shared application session with narrow native effects.
#[path = "macos/host.rs"]
mod host;
#[path = "macos/input.rs"]
mod input;
#[path = "menu.rs"]
mod menu;
#[path = "macos/session.rs"]
mod native_session;
#[path = "session.rs"]
mod session;
#[path = "macos/storage.rs"]
pub(crate) mod storage;
#[path = "macos/ui.rs"]
mod ui;
use native_session::SessionHost;
use ui::{OverlayLifecycle, SettingsActivation};

use crate::{adapters::ui_session, runtime::Runtime};
use std::{sync::Arc, time::Duration};

pub fn startup_error(message: &str) {
    host::startup_error(message);
}

pub fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let file_mode = args.len() == 2 && args[0] == "--transcribe-file";
    if !args.is_empty()
        && args != ["--settings"]
        && args != ["--capture-preview"]
        && args != ["--download-model"]
        && !file_mode
    {
        return Err("Usage: VoicePaste [--settings | --capture-preview | --download-model | --transcribe-file <wav>]".into());
    }
    let storage = Arc::new(storage::Storage::open()?);
    if args == ["--download-model"] || file_mode {
        let mut preparation = crate::adapters::models::Preparation::start_with_download(
            storage.root.clone(),
            &storage.settings.model,
            !file_mode,
        )?;
        loop {
            if let Some(result) = preparation.poll() {
                result?;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if file_mode {
            let mut worker = crate::adapters::worker::Worker::new(storage.clone());
            println!(
                "{}",
                worker.transcribe_file(std::path::Path::new(&args[1]), &storage.settings)?
            );
        }
        return Ok(());
    }
    let preview = args == ["--capture-preview"] || args == ["--settings"];
    let settings_only = args == ["--settings"];
    let runtime = Runtime::start(storage.clone(), preview)?;
    let session_storage = storage.clone();
    let result = ui_session::run(
        runtime,
        storage.settings.clone(),
        storage.log_path.display().to_string(),
        preview,
        settings_only,
        move |context, cx| {
            Box::pin(async move {
                let host = SessionHost::new(
                    session_storage,
                    &context.initial,
                    context.capture_preview,
                    context.settings_only,
                )?;
                session::ui_session(context, host, cx).await
            })
        },
    );
    if let Err(error) = &result {
        storage.log(error);
    }
    result
}
