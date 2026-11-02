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
        // Initial provisioning remains automatic; this is an explicit retry
        // after a failed/cancelled preparation, shared with recording intents.
        Some(confirm_download)
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

    pub fn error_dialog(&self, message: &str) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let text: Vec<u16> = message
            .chars()
            .filter(|c| *c != '\0')
            .take(4096)
            .collect::<String>()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let title: Vec<u16> = "VoicePaste".encode_utf16().chain(Some(0)).collect();
        // SAFETY: synchronous UI-thread dialog outside framework/state borrows;
        // UTF-16 buffers remain live until it returns.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
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

fn confirm_download(model: &str) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDOK, MB_ICONQUESTION, MB_OKCANCEL, MessageBoxW,
    };
    let text: Vec<u16> = model.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "Prepare selected model"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: UI-thread dialog outside framework/state borrows; buffers outlive call.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OKCANCEL | MB_ICONQUESTION,
        ) == IDOK
    }
}
