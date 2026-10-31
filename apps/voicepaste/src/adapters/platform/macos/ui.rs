//! Native lifecycle seam for the shared UI-session coordinator.
use crate::adapters::{
    overlay::{OverlayWindow, macos::Shell},
    settings_window::SettingsWindow,
};
use gpui_kit::{App, AsyncApp};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{ffi::c_void, ptr::NonNull};

unsafe extern "C" {
    fn vp_settings_activate(view: *mut c_void) -> bool;
}

pub(super) struct SettingsActivation {
    view: NonNull<c_void>,
    generation: u64,
}
impl SettingsActivation {
    pub fn snapshot(settings: &SettingsWindow, cx: &mut App) -> Result<Self, String> {
        let view = settings
            .handle()
            .update(cx, |_, window, _| {
                match window
                    .window_handle()
                    .map_err(|error| error.to_string())?
                    .as_raw()
                {
                    RawWindowHandle::AppKit(handle) => Ok(handle.ns_view),
                    _ => Err("Settings does not have an AppKit view".to_owned()),
                }
            })
            .map_err(|error| error.to_string())??;
        Ok(Self {
            view,
            generation: settings.id,
        })
    }
    pub fn apply(
        self,
        settings: Option<&SettingsWindow>,
        _cx: &mut AsyncApp,
    ) -> Result<(), String> {
        if settings.is_some_and(|settings| settings.id == self.generation && !settings.is_closed())
        {
            // SAFETY: main-thread snapshot of the same still-open GPUI Settings
            // generation. Native activation occurs with no App/entity borrow.
            if !unsafe { vp_settings_activate(self.view.as_ptr()) } {
                return Err("Cannot activate the macOS Settings window".into());
            }
        }
        Ok(())
    }
}

pub(super) struct OverlayLifecycle {
    shell: Shell,
}
impl OverlayLifecycle {
    pub fn attach(overlay: &OverlayWindow, cx: &mut AsyncApp) -> Result<Self, String> {
        Ok(Self {
            shell: Shell::attach(overlay, cx)?,
        })
    }
    pub fn enabled(&self) -> bool {
        true
    }
    pub fn sync(&mut self, overlay: &OverlayWindow, cx: &mut AsyncApp) -> Result<(), String> {
        self.shell.sync(overlay, cx);
        Ok(())
    }
    pub fn close(self, overlay: OverlayWindow, cx: &mut AsyncApp) {
        drop(self);
        cx.update(|cx| overlay.close(cx));
    }
}
