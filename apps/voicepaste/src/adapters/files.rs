//! File-sharing and durable publication primitives selected for the host OS.
//! Consumers retain config/model policy; this module owns only filesystem effects.
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub(super) use windows::{exclusive_options, replace, replace_preserving, stable_read};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(super) use macos::replace_preserving;
