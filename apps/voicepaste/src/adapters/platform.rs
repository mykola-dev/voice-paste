//! Explicit platform composition. No speculative adapters for unsupported OSes.
mod inbox;
mod menu;
mod session;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
use windows::{
    Host,
    ui::{OverlayLifecycle, SettingsActivation},
};

pub use session::run;
