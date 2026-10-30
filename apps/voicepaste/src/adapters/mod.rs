#[cfg(target_os = "windows")]
mod application_icon;
#[cfg(target_os = "windows")]
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
#[cfg(target_os = "windows")]
pub mod overlay;
#[cfg(target_os = "windows")]
pub mod paste;
#[cfg(target_os = "windows")]
pub mod platform;
mod settings_theme;
pub mod settings_window;
#[cfg(target_os = "windows")]
mod startup;
#[cfg(target_os = "windows")]
pub mod statistics;
#[cfg(target_os = "windows")]
pub mod storage;
#[cfg(target_os = "windows")]
pub mod tray_icons;
#[cfg(target_os = "windows")]
pub mod worker;

#[cfg(all(test, target_os = "windows"))]
mod boundaries;
