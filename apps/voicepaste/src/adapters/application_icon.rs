//! Native executable-resource icon selection.
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub(super) use windows::load;
