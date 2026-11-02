//! Validation selected at the native composition boundary.
use voicepaste_core::settings::Settings;

#[cfg(any(target_os = "windows", test))]
pub(crate) fn windows(settings: &Settings) -> Result<(), String> {
    // Preserve the historical Windows runtime/storage contract, including
    // parser-supported custom hotkeys and previously accepted saved values.
    settings.validate()
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn macos(settings: &Settings) -> Result<(), String> {
    settings.validate_for(voicepaste_core::settings::SettingsPlatform::Macos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use voicepaste_core::settings::{Device, PasteShortcut, SettingsPlatform};

    #[test]
    fn platform_policies_validate_models_and_preserve_windows_compatibility() {
        let mut settings = Settings::initial_for(SettingsPlatform::Windows).unwrap();
        settings.hotkey = "Ctrl+Alt+Space".into();
        settings.custom_initial_prompt = "Збережений prompt".into();
        assert!(windows(&settings).is_ok());
        settings.device = Device::MetalOnly;
        settings.paste_shortcut = PasteShortcut::CmdV;
        assert!(windows(&settings).is_ok());
        settings.model = "unsupported".into();
        assert!(windows(&settings).is_err());

        let mut settings = Settings::initial_for(SettingsPlatform::Macos).unwrap();
        settings.custom_initial_prompt = "Збережений prompt".into();
        assert!(macos(&settings).is_ok());
        for model in voicepaste_core::models::catalog_for(SettingsPlatform::Macos).unwrap() {
            settings.model = model.name;
            assert!(macos(&settings).is_ok());
        }
        let mut invalid = settings.clone();
        invalid.model = "unsupported".into();
        assert!(macos(&invalid).is_err());
        invalid = settings.clone();
        invalid.device = Device::CudaOnly;
        assert!(macos(&invalid).is_err());
        invalid = settings.clone();
        invalid.hotkey = "Ctrl+Alt+Space".into();
        assert!(macos(&invalid).is_err());
        invalid = settings.clone();
        invalid.paste_shortcut = PasteShortcut::CtrlV;
        assert!(macos(&invalid).is_err());
        assert_eq!(settings.custom_initial_prompt, "Збережений prompt");
    }

    #[test]
    fn macos_policy_rejects_invalid_settings_before_save_or_capture() {
        use std::{cell::Cell, time::Duration};
        use voicepaste_application::{AudioCapture, Dictation, settings::SettingsRepository};
        use voicepaste_core::{SessionId, State};

        struct Repository(Cell<bool>);
        impl SettingsRepository for Repository {
            fn save(&self, _: &Settings) -> Result<(), String> {
                self.0.set(true);
                Ok(())
            }
        }
        struct Capture;
        impl AudioCapture for Capture {
            type Recording = ();
            type Audio = ();
            fn start(&mut self, _: SessionId) -> Result<(), String> {
                Ok(())
            }
            fn finish(&mut self, _: ()) -> Result<(), String> {
                Ok(())
            }
            fn discard(&mut self, _: ()) -> Result<(), String> {
                Ok(())
            }
            fn failed(&self, _: &()) -> bool {
                false
            }
        }

        let mut settings = Settings::initial_for(SettingsPlatform::Macos).unwrap();
        settings.device = Device::CudaOnly;
        let repository = Repository(Cell::new(false));
        assert!(
            voicepaste_application::settings::save(&repository, settings.clone(), macos).is_err()
        );
        assert!(!repository.0.get());
        let mut dictation = Dictation::new(Capture, macos);
        assert!(dictation.toggle(Duration::ZERO, &settings, ()).is_err());
        assert_eq!(dictation.state(), State::Idle);
    }
}
