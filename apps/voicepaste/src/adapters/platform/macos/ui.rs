//! Native lifecycle seam for the shared UI-session coordinator.
use crate::adapters::{
    overlay::{OverlayWindow, macos::Shell},
    settings_window::SettingsWindow,
};
use gpui_kit::{AnyWindowHandle, App, AsyncApp};

pub(super) struct SettingsActivation {
    window: AnyWindowHandle,
    generation: u64,
}
impl SettingsActivation {
    pub fn snapshot(settings: &SettingsWindow, _cx: &mut App) -> Result<Self, String> {
        Ok(Self {
            window: settings.handle(),
            generation: settings.id,
        })
    }
    pub fn apply(self, settings: Option<&SettingsWindow>, cx: &mut AsyncApp) -> Result<(), String> {
        if settings.is_some_and(|settings| settings.id == self.generation && !settings.is_closed())
        {
            cx.update(|cx| {
                self.window.update(cx, |_, window, cx| {
                    window.activate_window();
                    cx.activate(true);
                })
            })
            .map_err(|error| error.to_string())?;
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
