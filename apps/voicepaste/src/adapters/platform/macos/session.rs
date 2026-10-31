//! AppKit menus, permission query and event-tap ownership; no application loop.
use super::{host, input, storage};
use crate::adapters::{
    paste::FocusTarget,
    ui_session::{NativeIntents, RecordingPermission, RuntimeSession},
};
use std::sync::Arc;
use voicepaste_core::{State, settings::Settings};

pub(super) struct SessionHost {
    host: host::Host,
    storage: Arc<storage::Storage>,
    keys: Option<input::Input>,
    next_keys: Option<input::Input>,
}

fn intents(batch: input::Batch) -> NativeIntents {
    NativeIntents {
        targets: batch.targets,
        cancel: batch.cancel,
        warning: batch.warning.then(|| "Keyboard input overflowed or its tap was interrupted; an extra toggle was rejected.".into()),
        ..Default::default()
    }
}

impl SessionHost {
    pub fn new(
        storage: Arc<storage::Storage>,
        initial: &Settings,
        preview: bool,
        settings_only: bool,
    ) -> Result<Self, String> {
        let host = host::Host::new()?;
        if !settings_only {
            if preview {
                host.enable_capture();
            } else {
                host.enable_dictation();
            }
        }
        Ok(Self {
            host,
            storage,
            keys: if settings_only {
                None
            } else {
                input::Input::attach(initial, false).ok()
            },
            next_keys: None,
        })
    }

    fn replace_keys(&mut self, replacement: input::Input, state: State) -> input::Batch {
        let pending = self
            .keys
            .as_ref()
            .map(input::Input::drain)
            .unwrap_or_default();
        replacement.arm(state);
        replacement.active(true);
        self.keys = Some(replacement);
        pending
    }

    pub fn drain(&mut self, saved: &Settings, state: State) -> NativeIntents {
        let native = self.host.drain();
        let mut batch = input::Batch::default();
        let mut warning = None;
        if native.enable_keys {
            match input::Input::attach(saved, true) {
                Ok(value) => batch.merge(self.replace_keys(value, state)),
                Err(error) => warning = Some(error),
            }
        }
        if let Some(keys) = &self.keys {
            batch.merge(keys.drain());
        }
        if native.toggle {
            batch.targets.push(FocusTarget::capture());
        }
        let mut result = intents(batch);
        result.quit = native.quit;
        result.settings = native.settings;
        result.cancel = native.cancel.or(result.cancel);
        result.warning = warning.or(result.warning);
        result.download = native
            .download
            .then_some(host::Host::confirm_download as fn(&str) -> bool);
        result.cancel_download = native.cancel_download;
        result
    }

    pub fn recording_permission(&self, request: bool) -> RecordingPermission {
        match self.host.microphone_permission(request) {
            host::MicrophonePermission::Granted => RecordingPermission::Granted,
            host::MicrophonePermission::Denied => RecordingPermission::Denied,
            host::MicrophonePermission::Undecided | host::MicrophonePermission::Pending => {
                RecordingPermission::Pending
            }
        }
    }

    pub fn stage_settings(&mut self, saved: &Settings, draft: &Settings) -> Result<(), String> {
        self.next_keys = if self.keys.is_some() && saved.hotkey != draft.hotkey {
            let replacement = input::Input::attach(draft, false)?;
            replacement.active(false);
            Some(replacement)
        } else {
            None
        };
        Ok(())
    }

    pub fn finish_settings(&mut self, success: bool, state: State) -> NativeIntents {
        match (success, self.next_keys.take()) {
            (true, Some(replacement)) => intents(self.replace_keys(replacement, state)),
            _ => NativeIntents::default(),
        }
    }

    pub fn state(&self, state: State) {
        if let Some(keys) = &self.keys {
            keys.arm(state);
        }
    }
    pub fn notify(&self, message: &str, _error: bool) {
        self.storage.log(message);
    }

    pub fn refresh(
        &mut self,
        projection: &RuntimeSession,
        permission_pending: bool,
        message: &str,
    ) {
        self.host.capture_status(
            projection.state,
            matches!(projection.state, State::Transcribing(_)) || permission_pending,
            projection.state != State::Idle || projection.preparing,
            message,
        );
    }
}
