//! Native menu, key-registration and tray effects for the shared session.
use super::Host;
use crate::adapters::{
    platform::menu::TrayAction,
    ui_session::{NativeIntents, RecordingPermission, RuntimeSession},
};
use std::time::{Duration, Instant};
use voicepaste_core::{
    State,
    settings::{Hotkey, Settings},
};

pub(in crate::adapters::platform) struct SessionHost {
    host: Host,
    last_dpi: Instant,
}

impl SessionHost {
    pub fn download_confirmation(&self) -> Option<fn(&str) -> bool> {
        // Windows retains its existing automatic model provisioning contract.
        None
    }

    pub fn new(initial: &Settings, _preview: bool, settings_only: bool) -> Result<Self, String> {
        Ok(Self {
            host: Host::create(Hotkey::parse(&initial.hotkey)?, !settings_only)?,
            last_dpi: Instant::now() - Duration::from_secs(2),
        })
    }

    pub fn drain(&mut self, _saved: &Settings, state: State) -> NativeIntents {
        let native = self.host.drain();
        let mut intents = NativeIntents {
            quit: native.quit,
            settings: native.settings,
            cancel: native.cancel,
            warning: native
                .overflow
                .then(|| "Hotkey queue is full; an extra toggle was rejected.".into()),
            ..Default::default()
        };
        for (id, target) in native.hotkeys {
            if self.host.accepts_hotkey(id) {
                intents.targets.push(target);
            }
        }
        if native.tray && !intents.quit {
            match self.host.menu(state) {
                Ok((Some(TrayAction::Toggle), target)) => intents.targets.push(target),
                Ok((Some(TrayAction::Settings), _)) => intents.settings = true,
                Ok((Some(TrayAction::Quit), _)) => intents.quit = true,
                Ok((None, _)) => (),
                Err(error) => intents.warning = Some(error),
            }
        }
        intents
    }

    pub fn recording_permission(&self, _request: bool) -> RecordingPermission {
        RecordingPermission::NotRequired
    }

    pub fn stage_settings(&mut self, _saved: &Settings, draft: &Settings) -> Result<(), String> {
        self.host.stage_hotkey(Hotkey::parse(&draft.hotkey)?)
    }

    pub fn finish_settings(&mut self, success: bool, _state: State) -> NativeIntents {
        self.host.finish_hotkey(success);
        NativeIntents::default()
    }

    pub fn state(&self, state: State) {
        self.host.state(state);
    }
    pub fn notify(&self, message: &str, error: bool) {
        self.host.notify(message, error);
    }

    pub fn refresh(
        &mut self,
        projection: &RuntimeSession,
        _permission_pending: bool,
        _message: &str,
    ) {
        self.host.model_status(
            &projection.model_message,
            projection.preparing,
            projection.state,
        );
        if self.last_dpi.elapsed() >= Duration::from_secs(2) {
            match self.host.ensure_dpi() {
                Ok(true) => self.host.state(projection.state),
                Ok(false) => (),
                Err(error) => self.host.notify(&error, true),
            }
            self.last_dpi = Instant::now();
        }
    }
}
