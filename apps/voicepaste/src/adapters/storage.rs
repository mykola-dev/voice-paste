use super::files;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use voicepaste_core::settings::{INITIAL_SETTINGS, Settings};

pub struct Storage {
    pub root: PathBuf,
    pub temp: PathBuf,
    pub settings: Settings,
}

impl Storage {
    pub fn open(root: &Path) -> Result<Self, String> {
        let data = root.join("data");
        let temp = data.join("temp");
        fs::create_dir_all(&temp)
            .map_err(|e| format!("Cannot create portable data folder: {e}"))?;
        let config = data.join("config.json");
        // create_new never truncates an existing config, including another
        // instance's config. A failed seed is removed rather than accepted.
        match files::exclusive_options()
            .write(true)
            .create_new(true)
            .open(&config)
        {
            Ok(mut file) => {
                if let Err(error) = file
                    .write_all(INITIAL_SETTINGS.as_bytes())
                    .and_then(|()| file.sync_all())
                {
                    drop(file);
                    let _ = fs::remove_file(&config);
                    return Err(format!("Cannot seed portable config: {error}"));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(format!("Cannot create config: {error}")),
        }
        let settings =
            Settings::from_json(&fs::read_to_string(&config).map_err(|e| e.to_string())?)?;
        Ok(Self {
            root: root.to_owned(),
            temp,
            settings,
        })
    }

    pub fn log(&self, message: &str) {
        super::diagnostics::log(message);
    }

    pub fn log_path() -> PathBuf {
        super::diagnostics::log_path()
    }
}

impl voicepaste_application::settings::SettingsRepository for Storage {
    fn save(&self, settings: &Settings) -> Result<(), String> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        settings.validate()?;
        let data = self.root.join("data");
        // Persistent lock file, exclusive handle: process exit releases ownership
        // without leaving a stale create_new lock or disallowing multiple hosts.
        let _lock = files::exclusive_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(data.join("config.lock"))
            .map_err(|e| format!("Settings are locked or unwritable: {e}"))?;
        let previous = Settings::from_json(
            &fs::read_to_string(data.join("config.json")).map_err(|e| e.to_string())?,
        )?;
        let temporary = data.join(format!(
            ".config-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = files::exclusive_options()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|e| e.to_string())?;
            drop(file);
            // Registry and config are committed as one Save operation. A failed
            // file replacement restores the exact prior Run entry; hotkey rollback
            // remains owned by the UI's existing save-completion path.
            let startup = super::startup::StartupChange::apply(
                previous.start_at_login,
                settings.start_at_login,
            )?;
            if let Err(error) = files::replace(&temporary, &data.join("config.json")) {
                if let Some(startup) = startup {
                    startup.rollback().map_err(|rollback| {
                        format!("{error}; startup rollback failed: {rollback}")
                    })?;
                }
                return Err(error);
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voicepaste_application::settings::SettingsRepository;
    #[test]
    fn atomic_save_preserves_config_when_locked_or_invalid() {
        let root = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join(format!("settings-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        struct OwnedTestRoot(PathBuf);
        impl Drop for OwnedTestRoot {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _owned = OwnedTestRoot(root.clone());
        let storage = Storage::open(&root).unwrap();
        assert_eq!(
            fs::read(root.join("data/config.json")).unwrap(),
            INITIAL_SETTINGS.as_bytes()
        );
        let mut changed = storage.settings.clone();
        changed.beam_size = 7;
        changed.custom_initial_prompt = "Saved vocabulary\r\nЗбережений словник".into();
        storage.save(&changed).unwrap();
        assert_eq!(Storage::open(&root).unwrap().settings, changed);
        let before = fs::read(root.join("data/config.json")).unwrap();
        let reopened = Storage::open(&root).unwrap();
        assert_eq!(
            reopened.settings.custom_initial_prompt,
            changed.custom_initial_prompt
        );
        assert_eq!(
            fs::read(root.join("data/config.json")).unwrap(),
            before,
            "opening an existing config must never reseed public defaults"
        );
        let lock = files::exclusive_options()
            .read(true)
            .write(true)
            .open(root.join("data/config.lock"))
            .unwrap();
        changed.beam_size = 8;
        assert!(storage.save(&changed).is_err());
        assert_eq!(fs::read(root.join("data/config.json")).unwrap(), before);
        drop(lock);
        // Permit the earlier config read but deny delete/replacement, so failure
        // happens at publication after the temporary file was written. Startup
        // stays disabled; this fixture never mutates the real Run value.
        let config_lock = files::stable_read(&root.join("data/config.json")).unwrap();
        assert!(storage.save(&changed).is_err());
        assert_eq!(fs::read(root.join("data/config.json")).unwrap(), before);
        drop(config_lock);
        assert_eq!(fs::read(root.join("data/config.json")).unwrap(), before);
        assert!(
            !fs::read_dir(root.join("data")).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".config-")
            }),
            "failed publication must remove its temporary file"
        );
        storage.save(&changed).unwrap();
        assert_eq!(Storage::open(&root).unwrap().settings, changed);
        changed.beam_size = 0;
        let before = fs::read(root.join("data/config.json")).unwrap();
        assert!(storage.save(&changed).is_err());
        assert_eq!(fs::read(root.join("data/config.json")).unwrap(), before);
    }
}
