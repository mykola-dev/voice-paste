#[cfg(target_os = "windows")]
mod application_icon;
pub mod audio;
#[cfg(target_os = "windows")]
mod diagnostics;
#[cfg(target_os = "windows")]
mod files;
#[cfg(target_os = "windows")]
mod hotkey;
#[cfg(target_os = "windows")]
pub mod launch;
#[cfg(target_os = "windows")]
mod memory;
#[cfg(target_os = "windows")]
pub mod models;
#[cfg(target_os = "macos")]
#[path = "models/macos.rs"]
pub mod models;
pub mod overlay;
pub mod paste;
#[cfg(target_os = "windows")]
pub mod platform;
pub(crate) mod settings_theme;
pub mod settings_window;
#[cfg(target_os = "windows")]
mod startup;
pub mod statistics;
#[cfg(target_os = "windows")]
pub mod storage;
#[cfg(target_os = "macos")]
pub mod storage {
    pub use crate::macos::storage::Storage;
}
#[cfg(target_os = "windows")]
pub mod tray_icons;
pub mod worker;

#[cfg(all(test, target_os = "windows"))]
mod boundaries;
