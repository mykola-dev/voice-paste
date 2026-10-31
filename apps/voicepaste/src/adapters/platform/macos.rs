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
use crate::{adapters::ui_session, bootstrap::CliPolicy, runtime::Runtime};
use native_session::SessionHost;
use std::sync::Arc;
use ui::{OverlayLifecycle, SettingsActivation};

pub(crate) const CLI_POLICY: CliPolicy = CliPolicy {
    check_message: None,
    max_files: Some(1),
    download_progress: false,
};
pub(crate) fn open_storage() -> Result<storage::Storage, String> {
    storage::Storage::open()
}
pub fn startup_error(message: &str) {
    host::startup_error(message);
}

pub(crate) fn run_session(
    runtime: Runtime,
    storage: Arc<storage::Storage>,
    capture: bool,
    settings_only: bool,
) -> Result<(), String> {
    ui_session::run(
        runtime,
        storage.settings.clone(),
        storage.log_path.display().to_string(),
        capture,
        settings_only,
        move |context, cx| {
            Box::pin(async move {
                let host = SessionHost::new(
                    storage,
                    &context.initial,
                    context.capture_preview,
                    context.settings_only,
                )?;
                session::ui_session(context, host, cx).await
            })
        },
    )
}
