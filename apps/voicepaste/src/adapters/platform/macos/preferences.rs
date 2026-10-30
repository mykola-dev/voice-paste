//! Background owner for Settings IO. One in-flight save, bounded delivery, joined shutdown.
use super::storage::Storage;
use crossbeam_channel::{Receiver, Sender, bounded};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use voicepaste_core::settings::Settings;

pub struct Completion {
    pub id: u64,
    pub result: Result<Settings, String>,
}

pub struct Preferences {
    commands: Sender<(u64, Settings)>,
    pub completions: Receiver<Completion>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Preferences {
    pub fn start(storage: Arc<Storage>) -> Result<Self, String> {
        let (commands, receiver) = bounded::<(u64, Settings)>(1);
        let (sender, completions) = bounded(1);
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop = shutdown.clone();
        let thread = thread::Builder::new()
            .name("macos-preferences".into())
            .spawn(move || {
                loop {
                    match receiver.recv_timeout(Duration::from_millis(50)) {
                        Ok((id, draft)) => {
                            let result = voicepaste_application::settings::save(&*storage, draft);
                            if let Err(error) = &result {
                                storage.log(&format!("Settings save failed: {error}"));
                            }
                            // UI permits at most one in-flight save. A full channel
                            // indicates an invariant failure, never block shutdown.
                            if sender.try_send(Completion { id, result }).is_err() {
                                break;
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                            if stop.load(Ordering::Acquire) {
                                break;
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            commands,
            completions,
            shutdown,
            thread: Some(thread),
        })
    }

    pub fn save(&self, id: u64, draft: Settings) -> Result<(), String> {
        self.commands
            .try_send((id, draft))
            .map_err(|error| format!("Cannot submit settings save: {error}"))
    }
}

impl Drop for Preferences {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        // Finish an accepted local Save before exit, rather than abandoning a
        // half-committed config/login-item operation. No model/network work here.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn shutdown_finishes_an_accepted_save_without_an_event_consumer() {
        let root =
            std::env::temp_dir().join(format!("voicepaste-mac-preferences-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _owned = Owned(root.clone());
        let storage = Arc::new(
            Storage::open_at(
                root.join("data"),
                root.join("log"),
                root.join("agent"),
                root.join("exe"),
            )
            .unwrap(),
        );
        let preferences = Preferences::start(storage.clone()).unwrap();
        let mut draft = storage.settings.clone();
        draft.custom_initial_prompt = "Accepted Save\r\nЗберегти".into();
        preferences.save(42, draft.clone()).unwrap();
        drop(preferences);
        let reopened = Storage::open_at(
            root.join("data"),
            root.join("log"),
            root.join("agent"),
            root.join("exe"),
        )
        .unwrap();
        assert_eq!(reopened.settings, draft);
        assert!(
            !root.join("agent").exists(),
            "disabled startup never creates a login item"
        );
    }
}
