//! Startup registration transaction, selected at compile time; no unsupported no-op.
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub(super) use windows::StartupChange;
