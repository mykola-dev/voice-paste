use voicepaste_core::settings::Settings;

pub trait SettingsRepository {
    fn save(&self, settings: &Settings) -> Result<(), String>;
}

pub fn save(repository: &impl SettingsRepository, settings: Settings) -> Result<Settings, String> {
    settings.validate()?;
    repository.save(&settings)?;
    Ok(settings)
}

pub struct SettingsBoundary {
    current: Settings,
    pending: Option<Settings>,
}
impl SettingsBoundary {
    pub fn new(current: Settings) -> Self {
        Self {
            current,
            pending: None,
        }
    }
    pub fn current(&self) -> &Settings {
        &self.current
    }
    pub fn saved(&mut self, settings: Settings) {
        self.pending = Some(settings);
    }
    pub fn apply_at_idle(&mut self, idle: bool) -> bool {
        if idle && let Some(settings) = self.pending.take() {
            self.current = settings;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_session_keeps_previous_settings_until_idle() {
        let initial = Settings::initial().unwrap();
        let mut state = SettingsBoundary::new(initial.clone());
        let mut changed = initial.clone();
        changed.beam_size = 7;
        state.saved(changed.clone());
        assert!(!state.apply_at_idle(false));
        assert_eq!(state.current(), &initial);
        assert!(state.apply_at_idle(true));
        assert_eq!(state.current(), &changed);
    }
}
