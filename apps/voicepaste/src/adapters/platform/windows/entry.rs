//! Windows startup paths/composition only; CLI/application routing is shared.
use crate::{
    adapters::{platform, storage::Storage},
    bootstrap::CliPolicy,
    runtime::Runtime,
};
use std::sync::Arc;

pub(super) const CLI_POLICY: CliPolicy = CliPolicy {
    check_message: Some(
        "Portable settings valid; GPUI settings/recording overlay, model provisioning, STT and paste are connected. This check uses no network, microphone, GPU or clipboard.",
    ),
    max_files: None,
    download_progress: true,
};

pub(super) fn open_storage() -> Result<Storage, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    Storage::open(
        executable
            .parent()
            .ok_or("Executable has no parent folder")?,
    )
}

pub(super) fn run_session(
    runtime: Runtime,
    storage: Arc<Storage>,
    capture: bool,
    settings_only: bool,
) -> Result<(), String> {
    platform::run(
        runtime,
        &storage.settings,
        &Storage::log_path().display().to_string(),
        capture,
        settings_only,
    )
}
