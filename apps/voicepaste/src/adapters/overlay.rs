//! Overlay boundary: native handles/rendering stay in the selected OS module.
//! macOS/Linux adapters are future work, not successful no-op stubs.
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{FRAME_TIMER, Overlay};
