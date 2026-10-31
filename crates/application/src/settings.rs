use voicepaste_core::settings::Settings;

pub trait SettingsRepository {
    fn save(&self, settings: &Settings) -> Result<(), String>;
}

pub fn save(repository: &impl SettingsRepository, settings: Settings) -> Result<Settings, String> {
    settings.validate()?;
    repository.save(&settings)?;
    Ok(settings)
}

/// Startup state and config publication form one Save. Native adapters own the
/// snapshot and effects; only the ordering and rollback obligation are shared.
pub fn commit_with_startup<T>(
    apply: impl FnOnce() -> Result<T, String>,
    commit: impl FnOnce() -> Result<(), String>,
    rollback: impl FnOnce(T) -> Result<(), String>,
) -> Result<(), String> {
    let snapshot = apply()?;
    if let Err(error) = commit() {
        rollback(snapshot)
            .map_err(|rollback| format!("{error}; startup rollback failed: {rollback}"))?;
        return Err(error);
    }
    Ok(())
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
    fn startup_is_rolled_back_only_after_failed_config_publication() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        for fail in [false, true] {
            calls.borrow_mut().clear();
            let result = commit_with_startup(
                || {
                    calls.borrow_mut().push("apply");
                    Ok("exact native snapshot")
                },
                || {
                    calls.borrow_mut().push("commit");
                    if fail {
                        Err("publication failed".into())
                    } else {
                        Ok(())
                    }
                },
                |snapshot| {
                    assert_eq!(snapshot, "exact native snapshot");
                    calls.borrow_mut().push("rollback");
                    Ok(())
                },
            );
            assert_eq!(result.is_err(), fail);
            assert_eq!(
                *calls.borrow(),
                if fail {
                    vec!["apply", "commit", "rollback"]
                } else {
                    vec!["apply", "commit"]
                }
            );
        }
    }

    #[test]
    fn failed_startup_never_publishes_and_rollback_failure_retains_both_errors() {
        assert_eq!(
            commit_with_startup(
                || Err::<(), _>("startup failed".into()),
                || panic!("cannot commit"),
                |_| panic!("no snapshot to restore"),
            )
            .unwrap_err(),
            "startup failed"
        );
        let error = commit_with_startup(
            || Ok(()),
            || Err("config failed".into()),
            |_| Err("native restore failed".into()),
        )
        .unwrap_err();
        assert!(error.contains("config failed"));
        assert!(error.contains("native restore failed"));
    }
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
