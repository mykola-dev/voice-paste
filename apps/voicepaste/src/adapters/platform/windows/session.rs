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

    pub fn retry_without_vad(&self, message: &str) -> bool {
        use windows_sys::Win32::UI::Controls::{
            TASKDIALOG_BUTTON, TASKDIALOGCONFIG, TaskDialogIndirect,
        };
        let wide = |value: &str| -> Vec<u16> {
            value
                .chars()
                .filter(|c| *c != '\0')
                .take(4096)
                .collect::<String>()
                .encode_utf16()
                .chain(Some(0))
                .collect()
        };
        let title = wide("VoicePaste");
        let heading = wide("Transcription failed");
        let content = wide(message);
        let retry = wide("Try without VAD");
        let cancel = wide("Cancel");
        let buttons = [
            TASKDIALOG_BUTTON {
                nButtonID: 100,
                pszButtonText: retry.as_ptr(),
            },
            TASKDIALOG_BUTTON {
                nButtonID: 2,
                pszButtonText: cancel.as_ptr(),
            },
        ];
        let config = TASKDIALOGCONFIG {
            cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
            pszWindowTitle: title.as_ptr(),
            pszMainInstruction: heading.as_ptr(),
            pszContent: content.as_ptr(),
            cButtons: buttons.len() as u32,
            pButtons: buttons.as_ptr(),
            nDefaultButton: 2,
            ..Default::default()
        };
        let mut result = 2;
        // SAFETY: UI thread, no state guards held; strings and buttons remain live.
        let status = unsafe {
            TaskDialogIndirect(
                &config,
                &mut result,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if status >= 0 {
            return result == 100;
        }
        // Task Dialog requires common-controls v6; retain a usable recovery
        // choice on systems where the native dialog is unavailable.
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            IDOK, MB_ICONERROR, MB_OKCANCEL, MessageBoxW,
        };
        let fallback = wide(&format!(
            "{message}\n\nPress OK to try without VAD, or Cancel to keep the recording."
        ));
        // SAFETY: same UI thread and live UTF-16 buffers as above.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                fallback.as_ptr(),
                title.as_ptr(),
                MB_OKCANCEL | MB_ICONERROR,
            ) == IDOK
        }
    }

    pub fn refresh(
        &mut self,
        projection: &RuntimeSession,
        _permission_pending: bool,
        _message: &str,
    ) {
        if self.last_dpi.elapsed() >= Duration::from_secs(2) {
            match self.host.ensure_dpi() {
                Ok(_) => (),
                Err(error) => self.host.notify(&error, true),
            }
            self.last_dpi = Instant::now();
        }
        self.host.model_status(
            &projection.model_message,
            projection.preparing,
            projection.state,
        );
        self.host.restore_tray(super::host::tray_face(
            projection.state,
            projection.preparing,
        ));
    }
}
