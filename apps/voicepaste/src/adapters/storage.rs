use std::os::windows::fs::OpenOptionsExt;
use std::{
    fs::{self, OpenOptions},
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
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
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
        let path = Self::log_path();
        if let Some(parent) = path.parent()
            && fs::create_dir_all(parent).is_err()
        {
            return;
        }
        // Bound diagnostics without recording audio, transcript or prompt.
        if fs::metadata(&path).is_ok_and(|m| m.len() > 1024 * 1024) {
            let _ = fs::rename(&path, path.with_extension("previous.log"));
        }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{message}");
        }
    }

    pub fn log_path() -> PathBuf {
        std::env::temp_dir().join("VoicePaste/voicepaste.log")
    }
}

impl voicepaste_application::settings::SettingsRepository for Storage {
    fn save(&self, settings: &Settings) -> Result<(), String> {
        use std::sync::atomic::{AtomicU64, Ordering};
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        settings.validate()?;
        let data = self.root.join("data");
        // Persistent lock file, exclusive handle: process exit releases ownership
        // without leaving a stale create_new lock or disallowing multiple hosts.
        let _lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
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
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|e| e.to_string())?;
            drop(file);
            use std::os::windows::ffi::OsStrExt;
            let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let destination: Vec<u16> = data
                .join("config.json")
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            // Registry and config are committed as one Save operation. A failed
            // file replacement restores the exact prior Run entry; hotkey rollback
            // remains owned by the UI's existing save-completion path.
            let startup = super::startup::StartupChange::apply(
                previous.start_at_login,
                settings.start_at_login,
            )?;
            // SAFETY: live NUL-terminated same-directory paths; failed replacement
            // leaves the previous config intact. Never fall back to AppData.
            if unsafe {
                MoveFileExW(
                    source.as_ptr(),
                    destination.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                let error = std::io::Error::last_os_error().to_string();
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
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(root.join("data/config.lock"))
            .unwrap();
        changed.beam_size = 8;
        assert!(storage.save(&changed).is_err());
        assert_eq!(fs::read(root.join("data/config.json")).unwrap(), before);
        drop(lock);
        changed.beam_size = 0;
        assert!(storage.save(&changed).is_err());
        assert_eq!(fs::read(root.join("data/config.json")).unwrap(), before);
    }
}
