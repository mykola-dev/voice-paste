//! Application Support persistence and current-user login item. No bundle writes.
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use voicepaste_application::settings::SettingsRepository;
use voicepaste_core::settings::{Settings, SettingsPlatform};

pub struct Storage {
    pub settings: Settings,
    pub root: PathBuf,
    pub temp: PathBuf,
    pub log_path: PathBuf,
    login_item: PathBuf,
    executable: PathBuf,
}

impl Storage {
    pub fn open() -> Result<Self, String> {
        let home = std::env::var_os("HOME")
            .filter(|home| !home.is_empty())
            .ok_or("HOME is unavailable; refusing a fallback data location")?;
        let home = PathBuf::from(home);
        if !home.is_absolute() {
            return Err("HOME is not an absolute path; refusing a fallback data location".into());
        }
        let library = home.join("Library");
        Self::open_at(
            library.join("Application Support/VoicePaste"),
            library.join("Logs/VoicePaste/voicepaste.log"),
            library.join("LaunchAgents/dev.mykola.voicepaste.plist"),
            std::env::current_exe().map_err(|e| e.to_string())?,
        )
    }

    pub(crate) fn open_at(
        root: PathBuf,
        log_path: PathBuf,
        login_item: PathBuf,
        executable: PathBuf,
    ) -> Result<Self, String> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&root)
            .map_err(|e| format!("Cannot create Application Support folder: {e}"))?;
        let _lock = lock_config(&root)?;
        let config = root.join("config.json");
        reject_link(&config)?;
        let defaults = Settings::initial_for(SettingsPlatform::Macos)?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&config)
        {
            Ok(mut file) => {
                let bytes = serde_json::to_vec_pretty(&defaults).map_err(|e| e.to_string())?;
                if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
                    drop(file);
                    let _ = fs::remove_file(&config);
                    return Err(format!("Cannot create config: {error}"));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(format!("Cannot open config: {error}")),
        }
        let settings = read_settings(&config)?;
        let temp = root.join("temp");
        reject_link(&temp)?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&temp)
            .map_err(|e| e.to_string())?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root.join("data"))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            settings,
            root,
            temp,
            log_path,
            login_item,
            executable,
        })
    }

    pub fn log(&self, message: &str) {
        // Never log prompts/settings payloads. Bounded rotation keeps preview logs local.
        let Some(parent) = self.log_path.parent() else {
            return;
        };
        if fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .is_err()
        {
            return;
        }
        if fs::metadata(&self.log_path).is_ok_and(|metadata| metadata.len() > 1_000_000) {
            let _ = fs::rename(&self.log_path, self.log_path.with_extension("previous.log"));
        }
        if let Ok(mut file) = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(&self.log_path)
        {
            let _ = writeln!(file, "{message}");
        }
    }
}

fn read_settings(path: &Path) -> Result<Settings, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let settings = Settings::from_json_for(&text, SettingsPlatform::Macos)?;
    settings.validate_for(SettingsPlatform::Macos)?;
    Ok(settings)
}

fn reject_link(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(format!("Refusing symbolic link: {}", path.display()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn lock_config(root: &Path) -> Result<File, String> {
    let path = root.join("config.lock");
    reject_link(&path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(|e| e.to_string())?;
    lock.try_lock()
        .map_err(|e| format!("Settings are locked: {e}"))?;
    Ok(lock)
}

fn publish(path: &Path, bytes: &[u8]) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    reject_link(path)?;
    let parent = path.parent().ok_or("Missing settings parent directory")?;
    let temporary = parent.join(format!(
        ".voicepaste-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temporary, path).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

const AGENT_MARKER: &str = "<!-- VoicePaste Rust login item -->";

fn login_plist(executable: &Path) -> Result<String, String> {
    let executable = executable
        .to_str()
        .ok_or("Login-item executable path is not UTF-8")?;
    let escaped = executable
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{AGENT_MARKER}\n<plist version=\"1.0\"><dict><key>Label</key><string>dev.mykola.voicepaste</string><key>ProgramArguments</key><array><string>{escaped}</string></array><key>RunAtLoad</key><true/><key>LimitLoadToSessionType</key><string>Aqua</string></dict></plist>\n"
    ))
}

impl SettingsRepository for Storage {
    fn save(&self, settings: &Settings) -> Result<(), String> {
        settings.validate_for(SettingsPlatform::Macos)?;
        let _lock = lock_config(&self.root)?;
        let config = self.root.join("config.json");
        reject_link(&config)?;
        let previous = read_settings(&config)?;
        let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
        // Re-save the enabled item after a bundle move, even if the checkbox
        // stayed on. A failed config publication still restores exact old bytes.
        let change_login =
            previous.start_at_login != settings.start_at_login || settings.start_at_login;
        let old_agent = if change_login {
            reject_link(&self.login_item)?;
            let previous = match fs::read(&self.login_item) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(format!("Cannot read login item: {error}")),
            };
            if previous
                .as_ref()
                .is_some_and(|bytes| !String::from_utf8_lossy(bytes).contains(AGENT_MARKER))
            {
                return Err(
                    "An existing login item is not owned by VoicePaste Rust; it was not changed"
                        .into(),
                );
            }
            if settings.start_at_login {
                let parent = self.login_item.parent().ok_or("Login item has no parent")?;
                fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(parent)
                    .map_err(|e| e.to_string())?;
                publish(&self.login_item, login_plist(&self.executable)?.as_bytes())?;
            } else if previous.is_some() {
                fs::remove_file(&self.login_item).map_err(|e| e.to_string())?;
            }
            previous
        } else {
            None
        };
        if let Err(error) = publish(&config, &bytes) {
            if change_login {
                let rollback = match old_agent {
                    Some(bytes) => publish(&self.login_item, &bytes),
                    None => match fs::remove_file(&self.login_item) {
                        Ok(()) => Ok(()),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                        Err(error) => Err(error.to_string()),
                    },
                };
                rollback.map_err(|rollback| {
                    format!("{error}; login-item rollback failed: {rollback}")
                })?;
            }
            return Err(error);
        }
        // Rename is the commit point. A directory-sync failure after it is a
        // durability warning, not a false Save failure with already-changed data.
        if let Err(error) = File::open(&self.root).and_then(|directory| directory.sync_all()) {
            self.log(&format!(
                "Settings committed; directory sync failed: {error}"
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_incompatible_config_is_not_reseeded_or_imported() {
        let root =
            std::env::temp_dir().join(format!("voicepaste-mac-existing-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _owned = Owned(root.clone());
        let config = root.join("config.json");
        let windows = serde_json::to_vec(&Settings::initial().unwrap()).unwrap();
        fs::write(&config, &windows).unwrap();
        fs::write(root.join("mac.config.json"), "old Swift configuration").unwrap();
        assert!(
            Storage::open_at(
                root.clone(),
                root.join("log"),
                root.join("agent"),
                root.join("exe")
            )
            .is_err()
        );
        assert_eq!(fs::read(&config).unwrap(), windows);
        assert_eq!(
            fs::read_to_string(root.join("mac.config.json")).unwrap(),
            "old Swift configuration"
        );
        fs::remove_file(&config).unwrap();
        let lock = lock_config(&root).unwrap();
        assert!(
            Storage::open_at(
                root.clone(),
                root.join("log"),
                root.join("agent"),
                root.join("exe")
            )
            .is_err()
        );
        assert!(
            !config.exists(),
            "another initializer must not observe a partial seed"
        );
        drop(lock);
        let storage = Storage::open_at(
            root.clone(),
            root.join("log"),
            root.join("agent"),
            root.join("exe"),
        )
        .unwrap();
        assert_eq!(
            storage.settings,
            Settings::initial_for(SettingsPlatform::Macos).unwrap()
        );
    }

    #[test]
    fn save_is_local_preserves_prompts_and_login_changes_respect_ownership() {
        let root =
            std::env::temp_dir().join(format!("voicepaste-mac-settings-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _owned = Owned(root.clone());
        let data = root.join("data");
        let agent = root.join("agents/voicepaste.plist");
        let storage = Storage::open_at(
            data.clone(),
            root.join("log"),
            agent.clone(),
            root.join("My & App/VoicePaste"),
        )
        .unwrap();
        let mut changed = storage.settings.clone();
        changed.custom_initial_prompt = "Vocabulary\r\nУкраїнська".into();
        changed.start_at_login = true;
        storage.save(&changed).unwrap();
        assert_eq!(read_settings(&data.join("config.json")).unwrap(), changed);
        assert!(
            fs::read_to_string(&agent)
                .unwrap()
                .contains("My &amp; App/VoicePaste")
        );
        let before = fs::read(data.join("config.json")).unwrap();
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(data.join("config.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        changed.beam_size = 8;
        assert!(storage.save(&changed).is_err());
        assert_eq!(fs::read(data.join("config.json")).unwrap(), before);
        drop(lock);
        changed.start_at_login = false;
        storage.save(&changed).unwrap();
        assert!(!agent.exists());
        fs::write(&agent, "unrelated user work").unwrap();
        changed.start_at_login = true;
        assert!(storage.save(&changed).is_err());
        assert_eq!(fs::read_to_string(&agent).unwrap(), "unrelated user work");
        changed.device = voicepaste_core::settings::Device::CudaOnly;
        assert!(storage.save(&changed).is_err());
    }
}
