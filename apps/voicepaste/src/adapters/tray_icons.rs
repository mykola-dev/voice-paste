//! Raster lips extracted from the user-provided artwork (no procedural drawing).
//!
//! Shape encodes state: idle = toothy smile, recording = open mouth,
//! transcribing = bitten lip. Original proportions/colors are preserved.
//! Native-size premultiplied pixels cover standard Windows tray DPIs.

use voicepaste_core::State;
use windows_sys::Win32::{
    Foundation::{HINSTANCE, HWND},
    Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromWindow},
    UI::{
        HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
        WindowsAndMessaging::{CreateIcon, HICON},
    },
};

/// Tray face per application state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lips {
    Idle,
    Recording,
    Transcribing,
}

impl From<State> for Lips {
    fn from(state: State) -> Self {
        match state {
            State::Idle => Self::Idle,
            State::Recording(_) => Self::Recording,
            State::Transcribing(_) => Self::Transcribing,
        }
    }
}

/// Exact sizes shipped as premultiplied BGRA blobs. Standard DPIs land
/// exactly on these (96->16, 120->20, 144->24, 168->28, 192->32, 240->40,
/// 288->48); anything else snaps to the nearest entry.
pub const CANONICAL: [usize; 7] = [16, 20, 24, 28, 32, 40, 48];

/// Tray icon edge: 16px at 96 DPI, scaled and clamped to a sane Win32 range.
pub fn icon_size_for_dpi(dpi: u32) -> usize {
    let dpi = dpi.clamp(96, 288);
    ((16.0 * dpi as f64 / 96.0).round() as usize).clamp(16, 48)
}

/// Snap a requested size to the nearest shipped blob. Ties go larger:
/// a small system downscale stays invisible, an upscale would blur.
pub fn canonical_size(px: usize) -> usize {
    let mut best = CANONICAL[0];
    for &c in &CANONICAL[1..] {
        let (db, dc) = (best.abs_diff(px), c.abs_diff(px));
        if dc < db || (dc == db && c > best) {
            best = c;
        }
    }
    best
}

/// Current monitor DPI mapped to a shipped icon size. Falls back to 16px.
pub fn system_icon_size(hwnd: HWND) -> usize {
    // SAFETY: transient monitor/DPI query on the UI thread; no owned state.
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let (mut x, mut y) = (96u32, 96u32);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) == 0 {
            return canonical_size(icon_size_for_dpi(x));
        }
    }
    16
}

macro_rules! tray_blobs {
    ($(($state:ident, $file:literal, $size:literal)),*) => {
        /// Shipped premultiplied BGRA for a state and canonical size.
        /// Unknown sizes fall back to the 32px blob of the same state.
        fn blob_bytes(state: Lips, size: usize) -> &'static [u8] {
            match (state, size) {
                $((Lips::$state, $size) => include_bytes!(concat!(
                    "../../assets/tray/lips-",
                    $file,
                    "-",
                    stringify!($size),
                    ".bgra"
                )) as &[u8],)*
                _ => blob_bytes(state, 32),
            }
        }
    };
}

tray_blobs! {
    (Idle, "idle", 16), (Idle, "idle", 20), (Idle, "idle", 24), (Idle, "idle", 28),
    (Idle, "idle", 32), (Idle, "idle", 40), (Idle, "idle", 48),
    (Recording, "recording", 16), (Recording, "recording", 20), (Recording, "recording", 24),
    (Recording, "recording", 28), (Recording, "recording", 32), (Recording, "recording", 40),
    (Recording, "recording", 48),
    (Transcribing, "transcribing", 16), (Transcribing, "transcribing", 20),
    (Transcribing, "transcribing", 24), (Transcribing, "transcribing", 28),
    (Transcribing, "transcribing", 32), (Transcribing, "transcribing", 40),
    (Transcribing, "transcribing", 48)
}

/// Create a tray icon from the shipped blob nearest to `size`.
/// The AND mask is opaque wherever the blob has any alpha, so 1px
/// anti-aliased edges blend instead of clipping.
pub fn create_icon(instance: HINSTANCE, size: usize, state: Lips) -> Result<HICON, String> {
    let edge = canonical_size(size);
    let pixels = blob_bytes(state, edge);
    debug_assert_eq!(pixels.len(), edge * edge * 4);
    let row = edge.div_ceil(8);
    let mut mask = vec![255u8; row * edge];
    for y in 0..edge {
        for x in 0..edge {
            if pixels[(y * edge + x) * 4 + 3] > 0 {
                mask[y * row + x / 8] &= !(0x80 >> (x % 8));
            }
        }
    }
    // SAFETY: buffers exactly match the declared icon dimensions.
    let handle = unsafe {
        CreateIcon(
            instance,
            edge as i32,
            edge as i32,
            1,
            32,
            mask.as_ptr(),
            pixels.as_ptr(),
        )
    };
    if handle.is_null() {
        Err("Cannot create tray icon".into())
    } else {
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_size_tracks_dpi_and_snaps_to_shipped_blobs() {
        assert_eq!(icon_size_for_dpi(96), 16);
        assert_eq!(icon_size_for_dpi(168), 28);
        assert_eq!(icon_size_for_dpi(192), 32);
        assert_eq!(canonical_size(16), 16);
        assert_eq!(canonical_size(25), 24);
        assert_eq!(canonical_size(26), 28);
        assert_eq!(canonical_size(36), 40);
        assert_eq!(canonical_size(100), 48);
        for dpi in [96, 120, 144, 168, 192, 240, 288] {
            assert!(CANONICAL.contains(&canonical_size(icon_size_for_dpi(dpi))));
        }
    }

    #[test]
    fn every_state_ships_every_canonical_blob_with_distinct_pixels() {
        let states = [Lips::Idle, Lips::Recording, Lips::Transcribing];
        for state in states {
            for size in CANONICAL {
                assert_eq!(blob_bytes(state, size).len(), size * size * 4);
            }
        }
        // Unknown sizes fall back to the 32px blob of the same state.
        assert_eq!(blob_bytes(Lips::Idle, 25), blob_bytes(Lips::Idle, 32));
        for size in CANONICAL {
            let (idle, rec, bitten) = (
                blob_bytes(Lips::Idle, size),
                blob_bytes(Lips::Recording, size),
                blob_bytes(Lips::Transcribing, size),
            );
            assert_ne!(idle, rec);
            assert_ne!(idle, bitten);
            assert_ne!(rec, bitten);
        }
    }

    #[test]
    fn all_blobs_are_premultiplied_and_have_visible_artwork() {
        for state in [Lips::Idle, Lips::Recording, Lips::Transcribing] {
            for size in CANONICAL {
                let (pixels, remainder) = blob_bytes(state, size).as_chunks::<4>();
                assert!(remainder.is_empty());
                assert!(pixels.iter().any(|pixel| pixel[3] == 255));
                assert!(pixels.iter().any(|pixel| pixel[3] == 0));
                for pixel in pixels {
                    assert!(pixel[..3].iter().all(|channel| *channel <= pixel[3]));
                }
            }
        }
    }

    #[test]
    fn blobs_have_transparent_corners_and_opaque_centers() {
        for state in [Lips::Idle, Lips::Recording, Lips::Transcribing] {
            let edge = 32;
            let blob = blob_bytes(state, edge);
            for (x, y) in [(0, 0), (31, 0), (0, 31), (31, 31)] {
                assert_eq!(blob[(y * edge + x) * 4 + 3], 0, "{state:?} corner");
            }
            assert!(blob[(16 * edge + 16) * 4 + 3] > 200, "{state:?} center");
        }
    }
}
