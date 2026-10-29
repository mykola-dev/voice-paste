//! Native input selection. The application owns hotkey parsing/behavior policy.
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub(super) use windows::{Binding, hook_event};
