//! Native launch diagnostics, not application CLI parsing or recording behavior.
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{attach_parent_console, startup_error};
