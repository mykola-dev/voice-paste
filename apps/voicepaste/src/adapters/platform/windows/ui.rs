//! GPUI/native lifecycle seam. Only snapshots run under App borrows; native
//! mutations and shell teardown run after each update returns on the UI thread.
use crate::adapters::{
    overlay::{OverlayWindow, windows::Shell},
    settings_window::SettingsWindow,
};
use gpui_kit::{App, AsyncApp};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use voicepaste_core::State;

/// One-poll activation intent. It borrows no App and never owns the GPUI window.
pub(in crate::adapters::platform) struct SettingsActivation {
    hwnd: isize,
    generation: u64,
}

impl SettingsActivation {
    pub fn snapshot(settings: &SettingsWindow, cx: &mut App) -> Result<Self, String> {
        let hwnd = settings
            .handle()
            .update(cx, |_, window, _| {
                match window.window_handle().map_err(|e| e.to_string())?.as_raw() {
                    RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get()),
                    _ => Err("Settings does not have a Windows window handle".to_owned()),
                }
            })
            .map_err(|e| e.to_string())??;
        Ok(Self {
            hwnd,
            generation: settings.id,
        })
    }

    /// Completion processing may have closed Settings since the snapshot.
    pub fn apply(self, settings: Option<&SettingsWindow>) -> Result<(), String> {
        if settings.is_some_and(|settings| settings.id == self.generation && !settings.is_closed())
        {
            super::show_settings(self.hwnd)?;
        }
        Ok(())
    }
}

pub(in crate::adapters::platform) struct OverlayLifecycle {
    shell: Shell,
    enabled: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Show,
    Hide,
    Sync,
}

fn action(active: bool, pending: bool, visible: bool) -> Action {
    if active && (pending || !visible) {
        Action::Show
    } else if !active && visible {
        Action::Hide
    } else {
        Action::Sync
    }
}

impl OverlayLifecycle {
    pub fn attach(overlay: &OverlayWindow, cx: &mut AsyncApp) -> Result<Self, String> {
        let hwnd = cx.update(|cx| Shell::window_handle(overlay.handle(), cx))?;
        Ok(Self {
            shell: Shell::new(hwnd)?,
            enabled: true,
        })
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Preserve prepare → shared material → warm/show → fallback ordering.
    /// Failure disables only the indicator, never the active dictation.
    pub fn sync(&mut self, overlay: &OverlayWindow, cx: &mut AsyncApp) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        let result = self.sync_enabled(overlay, cx);
        if result.is_err() {
            self.enabled = false;
            self.shell.hide();
            cx.update(|cx| overlay.update(State::Idle, false, cx));
        }
        result
    }

    fn sync_enabled(&mut self, overlay: &OverlayWindow, cx: &mut AsyncApp) -> Result<(), String> {
        let opacity = cx.update(|cx| overlay.animation(cx));
        let active = cx.update(|cx| overlay.active(cx));
        let pending = cx.update(|cx| overlay.show_pending(cx));
        match action(active, pending, self.shell.visible()) {
            Action::Show => {
                if self.shell.visible() {
                    self.shell.hide();
                }
                let appearance = cx.update(|cx| overlay.current_appearance(cx));
                let material = self.shell.prepare(appearance)?;
                cx.update(|cx| overlay.material(material, cx));
                if self.shell.animate(opacity) {
                    cx.update(|cx| overlay.material(false, cx));
                }
                let shown_material = self.shell.show()?;
                // Warming the foreground may have rendered a newer sample.
                let opacity = cx.update(|cx| overlay.animation(cx));
                let lost_material = self.shell.animate(opacity);
                if material != shown_material || lost_material {
                    cx.update(|cx| overlay.material(shown_material && !lost_material, cx));
                }
            }
            Action::Hide => self.shell.hide(),
            Action::Sync => {
                if self.shell.sync_geometry()? {
                    cx.update(|cx| overlay.material(false, cx));
                }
                if self.shell.animate(opacity) {
                    cx.update(|cx| overlay.material(false, cx));
                }
            }
        }
        Ok(())
    }

    /// Detach native subclass/material before GPUI closes its foreground.
    /// On task cancellation Shell::drop deliberately avoids GPUI reentry.
    pub fn close(mut self, overlay: OverlayWindow, cx: &mut AsyncApp) {
        self.shell.hide();
        drop(self);
        cx.update(|cx| overlay.close(cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_appearance_restarts_visible_overlay_only_while_active() {
        assert_eq!(action(true, true, true), Action::Show);
        assert_eq!(action(true, true, false), Action::Show);
        assert_eq!(action(false, true, true), Action::Hide);
        assert_eq!(action(false, true, false), Action::Sync);
    }

    #[test]
    fn stable_overlay_syncs_geometry_and_inactive_overlay_hides() {
        assert_eq!(action(true, false, false), Action::Show);
        assert_eq!(action(true, false, true), Action::Sync);
        assert_eq!(action(false, false, true), Action::Hide);
        assert_eq!(action(false, false, false), Action::Sync);
    }
}
