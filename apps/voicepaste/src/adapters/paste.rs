//! Paste implementation selection. Application transfers targets opaquely.
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{ClipboardPaste, FocusTarget};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{ClipboardPaste, FocusTarget};
