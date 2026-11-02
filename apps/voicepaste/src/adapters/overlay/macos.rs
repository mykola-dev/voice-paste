//! Native nonactivating placement/backdrop; shared GPUI owns every foreground frame.
use super::OverlayWindow;
use gpui_kit::AsyncApp;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc};
unsafe extern "C" {
    fn vp_overlay_create(view: *mut c_void) -> *mut c_void;
    fn vp_overlay_prepare(shell: *mut c_void, blur: bool, alpha: f64) -> bool;
    fn vp_overlay_present(shell: *mut c_void, active: bool, opacity: f64, interactive: bool);
    fn vp_overlay_destroy(shell: *mut c_void);
}
fn material_alpha(appearance: voicepaste_core::settings::OverlayAppearance) -> f64 {
    f64::from(appearance.background_alpha()) / 255.
}
pub struct Shell {
    pointer: NonNull<c_void>,
    visible: bool,
    _main: PhantomData<Rc<()>>,
}
impl Shell {
    pub fn attach(overlay: &OverlayWindow, cx: &mut AsyncApp) -> Result<Self, String> {
        let view = cx.update(|cx| {
            overlay
                .handle()
                .update(cx, |_, window, _| {
                    match window.window_handle().map_err(|e| e.to_string())?.as_raw() {
                        RawWindowHandle::AppKit(handle) => {
                            Ok::<*mut c_void, String>(handle.ns_view.as_ptr())
                        }
                        _ => Err("Overlay does not have an AppKit view".into()),
                    }
                })
                .map_err(|e| e.to_string())?
        })?;
        // SAFETY: GPUI's live hidden NSPanel view, on UI thread. Native retains
        // window until shell Drop, which precedes closing the GPUI window.
        let pointer = unsafe { vp_overlay_create(view) };
        Ok(Self {
            pointer: NonNull::new(pointer).ok_or("Cannot configure macOS overlay")?,
            visible: false,
            _main: PhantomData,
        })
    }
    pub fn sync(&mut self, overlay: &OverlayWindow, cx: &mut AsyncApp) {
        let (active, pending, appearance, opacity, interactive) = cx.update(|cx| {
            (
                overlay.active(cx),
                overlay.show_pending(cx),
                overlay.current_appearance(cx),
                overlay.animation(cx),
                overlay.provisioning_interactive(cx),
            )
        });
        if !active && !self.visible {
            return;
        }
        if active && (pending || !self.visible) {
            // SAFETY: native mutations run after releasing all GPUI borrows.
            let material = unsafe {
                vp_overlay_prepare(
                    self.pointer.as_ptr(),
                    appearance.acrylic,
                    material_alpha(appearance),
                )
            };
            cx.update(|cx| overlay.material(material, cx));
        }
        // SAFETY: same owned main-thread shell. Animation uses only the last
        // rendered shared sample; native code never advances an independent clock.
        unsafe {
            vp_overlay_present(
                self.pointer.as_ptr(),
                active,
                f64::from(opacity),
                interactive,
            );
        }
        self.visible = active;
    }
}
impl Drop for Shell {
    fn drop(&mut self) {
        // SAFETY: detached before GPUI closes its foreground; native releases backdrop.
        unsafe {
            vp_overlay_destroy(self.pointer.as_ptr());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" {
        fn vp_overlay_origin(
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            left: *mut f64,
            bottom: *mut f64,
        );
        fn vp_overlay_material_opacity(alpha: f64, animation: f64) -> f64;
    }

    #[test]
    fn native_appkit_anchor_matches_top_right_work_area_in_global_points() {
        for (x, y, width, height) in [
            (0., 0., 1440., 875.),
            (-1920., -120., 1920., 1080.),
            (1440., 900., 1728., 1079.),
        ] {
            let (mut left, mut bottom) = (0., 0.);
            // SAFETY: pure native geometry helper, valid output pointers, no AppKit effects.
            unsafe { vp_overlay_origin(x, y, width, height, &mut left, &mut bottom) };
            assert_eq!(x + width - (left + 224.), 20.);
            assert_eq!(y + height - (bottom + 100.), 20.);
            assert_eq!(
                left + 244.,
                x + width,
                "viewport stops at the work-area edge"
            );
        }
    }
    #[test]
    fn native_tint_uses_unit_alpha_not_byte_alpha() {
        for transparency in 0..=100 {
            let appearance = voicepaste_core::settings::OverlayAppearance {
                transparency,
                ..Default::default()
            };
            let alpha = material_alpha(appearance);
            assert!((0.0..=1.0).contains(&alpha));
            assert!(
                (alpha - (1.0 - f64::from(transparency) / 100.)).abs() <= 0.5 / 255. + f64::EPSILON
            );
        }
    }

    #[test]
    fn material_fades_with_transparency_and_animation() {
        for transparency in 0..=100 {
            let appearance = voicepaste_core::settings::OverlayAppearance {
                transparency,
                ..Default::default()
            };
            let alpha = material_alpha(appearance);
            for animation in [0., 0.25, 0.5, 1.] {
                // SAFETY: pure native arithmetic; no AppKit state or pointers.
                let actual = unsafe { vp_overlay_material_opacity(alpha, animation) };
                assert!((actual - alpha * animation).abs() < 1e-12);
            }
        }
        // SAFETY: same pure native helper, including defensive input clamping.
        assert_eq!(unsafe { vp_overlay_material_opacity(0., 1.) }, 0.);
        assert_eq!(unsafe { vp_overlay_material_opacity(1., 1.) }, 1.);
        assert_eq!(unsafe { vp_overlay_material_opacity(2., -1.) }, 0.);
    }
}
