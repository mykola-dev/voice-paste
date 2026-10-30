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
