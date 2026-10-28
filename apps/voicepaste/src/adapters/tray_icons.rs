//! Raster lips extracted from the user-provided artwork (no procedural drawing).
//!
//! Idle uses the bitten lip; recording and transcription use the open mouth.
//! Original proportions/colors are preserved without extra inset.
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
    Bitten,
    Open,
}

impl From<State> for Lips {
    fn from(state: State) -> Self {
        match state {
            State::Idle => Self::Bitten,
            State::Recording(_) | State::Transcribing(_) => Self::Open,
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
    (Bitten, "bitten", 16), (Bitten, "bitten", 20), (Bitten, "bitten", 24),
    (Bitten, "bitten", 28), (Bitten, "bitten", 32), (Bitten, "bitten", 40),
    (Bitten, "bitten", 48),
    (Open, "open", 16), (Open, "open", 20), (Open, "open", 24),
    (Open, "open", 28), (Open, "open", 32), (Open, "open", 40), (Open, "open", 48)
}

// CreateIcon expects WORD-aligned monochrome scanlines, not packed rows.
fn mask_stride(edge: usize) -> usize {
    edge.div_ceil(16) * 2
}

/// Create a tray icon from the shipped blob nearest to `size`.
/// The AND mask is opaque wherever the blob has any alpha, so 1px
/// anti-aliased edges blend instead of clipping.
pub fn create_icon(instance: HINSTANCE, size: usize, state: Lips) -> Result<HICON, String> {
    let edge = canonical_size(size);
    let pixels = blob_bytes(state, edge);
    debug_assert_eq!(pixels.len(), edge * edge * 4);
    let row = mask_stride(edge);
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
        let states = [Lips::Bitten, Lips::Open];
        for state in states {
            for size in CANONICAL {
                assert_eq!(blob_bytes(state, size).len(), size * size * 4);
            }
        }
        // Unknown sizes fall back to the 32px blob of the same state.
        assert_eq!(blob_bytes(Lips::Bitten, 25), blob_bytes(Lips::Bitten, 32));
        for size in CANONICAL {
            assert_ne!(blob_bytes(Lips::Bitten, size), blob_bytes(Lips::Open, size));
        }
    }

    #[test]
    fn all_blobs_are_premultiplied_and_have_visible_artwork() {
        for state in [Lips::Bitten, Lips::Open] {
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
        for state in [Lips::Bitten, Lips::Open] {
            let edge = 32;
            let blob = blob_bytes(state, edge);
            for (x, y) in [(0, 0), (31, 0), (0, 31), (31, 31)] {
                assert_eq!(blob[(y * edge + x) * 4 + 3], 0, "{state:?} corner");
            }
            assert!(blob[(16 * edge + 16) * 4 + 3] > 200, "{state:?} center");
        }
    }

    #[test]
    fn state_mapping_uses_only_two_faces() {
        let session = voicepaste_core::SessionId(1);
        assert_eq!(Lips::from(State::Idle), Lips::Bitten);
        assert_eq!(Lips::from(State::Recording(session)), Lips::Open);
        assert_eq!(Lips::from(State::Transcribing(session)), Lips::Open);
    }

    #[test]
    fn monochrome_mask_rows_are_word_aligned() {
        for (edge, stride) in [
            (16, 2),
            (20, 4),
            (24, 4),
            (28, 4),
            (32, 4),
            (40, 6),
            (48, 6),
        ] {
            assert_eq!(mask_stride(edge), stride);
        }
    }

    #[test]
    fn artwork_fills_the_square_without_extra_inset() {
        for state in [Lips::Bitten, Lips::Open] {
            for edge in CANONICAL {
                let pixels = blob_bytes(state, edge);
                // Both retained masters are wider than tall: their visible bounds
                // reach both horizontal edges, with proportional vertical padding.
                for x in [0, edge - 1] {
                    assert!(
                        (0..edge).any(|y| pixels[(y * edge + x) * 4 + 3] > 0),
                        "{state:?} {edge}px has an extra horizontal inset"
                    );
                }
            }
        }
    }
}
