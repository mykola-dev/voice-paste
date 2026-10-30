//! Capture-only composition until the Metal worker is connected. Reuses the
//! production audio adapter and application state machine; never fabricates STT.
use crate::adapters::audio::Microphone;
use crossbeam_channel::{Receiver, Sender, bounded};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use voicepaste_application::{AudioCapture, Dictation};
use voicepaste_core::{SessionId, State, settings::Settings, waveform::Measurement};

enum Command {
    Toggle { now: Duration, settings: Settings },
    Cancel(SessionId),
}

pub struct Event {
    pub state: State,
    pub message: Result<&'static str, String>,
}

pub struct Capture {
    commands: Sender<Command>,
    pub events: Receiver<Event>,
    // Bounded display handoff. Dropping a meter sample never drops PCM.
    pub levels: Receiver<Measurement>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Capture {
    pub fn start(temp: PathBuf, clock: Instant) -> Result<Self, String> {
        let (commands, receiver) = bounded(1);
        let (sender, events) = bounded(8);
        let (meter, levels) = bounded(128);
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop = shutdown.clone();
        let thread = thread::Builder::new()
            .name("macos-capture".into())
            .spawn(move || {
                // CoreAudio streams are created, used and destroyed on their owner
                // thread. No device enumeration, disk writes or joins on GPUI.
                let microphone = Microphone::new(temp, meter, clock);
                run(microphone, receiver, sender, &stop, |audio| audio.remove());
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            commands,
            events,
            levels,
            shutdown,
            thread: Some(thread),
        })
    }

    pub fn toggle(&self, now: Duration, settings: Settings) -> Result<(), String> {
        self.commands
            .try_send(Command::Toggle { now, settings })
            .map_err(|error| format!("Capture command was not accepted: {error}"))
    }

    pub fn cancel(&self, session: SessionId) -> Result<(), String> {
        self.commands
            .try_send(Command::Cancel(session))
            .map_err(|error| format!("Capture cancellation was not accepted: {error}"))
    }
}

fn run<C: AudioCapture>(
    capture: C,
    receiver: Receiver<Command>,
    sender: Sender<Event>,
    stop: &AtomicBool,
    mut dispose: impl FnMut(C::Audio) -> Result<(), String>,
) {
    let mut app: Dictation<C, ()> = Dictation::new(capture);
    while !stop.load(Ordering::Acquire) {
        let message = match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(Command::Toggle { now, settings }) => {
                match app.toggle(now, &settings, ()) {
                    Ok(Some(submission)) => {
                        // The same owned temporary AudioFile as production. No
                        // transcript/paste; explicit cleanup reports failures.
                        let id = submission.session;
                        let result = dispose(submission.audio);
                        app.complete(id);
                        result.map(|()|
                            "Capture completed; temporary audio discarded. No transcription or paste.",
                        )
                    }
                    Ok(None) if matches!(app.state(), State::Recording(_)) => {
                        Ok("Capture preview — microphone recording; no transcription or paste.")
                    }
                    Ok(None) => Ok("Capture preview idle; toggle ignored by debounce."),
                    Err(error) => Err(error),
                }
            }
            Ok(Command::Cancel(id)) => app
                .cancel(id)
                .map(|()| "Recording cancelled; temporary audio discarded."),
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if !app.recording_failed() {
                    continue;
                }
                if let State::Recording(id) = app.state() {
                    let cleanup = app.cancel(id);
                    Err(match cleanup {
                        Ok(()) => "Microphone failed; recording discarded".into(),
                        Err(error) => format!("Microphone failed; {error}"),
                    })
                } else {
                    continue;
                }
            }
        };
        let mut event = Event {
            state: app.state(),
            message,
        };
        loop {
            match sender.send_timeout(event, Duration::from_millis(50)) {
                Ok(()) => break,
                Err(crossbeam_channel::SendTimeoutError::Timeout(value))
                    if !stop.load(Ordering::Acquire) =>
                {
                    event = value
                }
                Err(_) => {
                    let _ = app.shutdown();
                    return;
                }
            }
        }
    }
    // Cancels an active recording on all exits, including a full event queue.
    let _ = app.shutdown();
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    struct Owned(Arc<AtomicUsize>);
    impl Drop for Owned {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    struct Fake(Arc<AtomicUsize>, Option<Sender<()>>);
    impl AudioCapture for Fake {
        type Recording = Owned;
        type Audio = Owned;
        fn start(&mut self, _: SessionId) -> Result<Owned, String> {
            if let Some(started) = &self.1 {
                started.send(()).unwrap();
            }
            Ok(Owned(self.0.clone()))
        }
        fn finish(&mut self, recording: Owned) -> Result<Owned, String> {
            Ok(recording)
        }
        fn discard(&mut self, recording: Owned) -> Result<(), String> {
            drop(recording);
            Ok(())
        }
        fn failed(&self, _: &Owned) -> bool {
            false
        }
    }

    #[test]
    fn capture_completion_cancel_and_disconnect_release_audio() {
        let drops = Arc::new(AtomicUsize::new(0));
        let (commands, receiver) = bounded(8);
        let (sender, events) = bounded(8);
        let settings =
            Settings::initial_for(voicepaste_core::settings::SettingsPlatform::Macos).unwrap();
        for second in [0, 1, 2] {
            commands
                .send(Command::Toggle {
                    now: Duration::from_secs(second),
                    settings: settings.clone(),
                })
                .unwrap();
        }
        commands.send(Command::Cancel(SessionId(99))).unwrap();
        commands.send(Command::Cancel(SessionId(2))).unwrap();
        commands
            .send(Command::Toggle {
                now: Duration::from_secs(3),
                settings,
            })
            .unwrap();
        drop(commands);
        run(
            Fake(drops.clone(), None),
            receiver,
            sender,
            &AtomicBool::new(false),
            |audio| {
                drop(audio);
                Ok(())
            },
        );
        let states: Vec<_> = events.try_iter().map(|event| event.state).collect();
        assert_eq!(
            states,
            [
                State::Recording(SessionId(1)),
                State::Idle,
                State::Recording(SessionId(2)),
                State::Recording(SessionId(2)),
                State::Idle,
                State::Recording(SessionId(3))
            ]
        );
        assert_eq!(drops.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn shutdown_with_full_delivery_queue_discards_active_recording() {
        let drops = Arc::new(AtomicUsize::new(0));
        let (commands, receiver) = bounded(1);
        let (sender, _events) = bounded(1);
        sender
            .send(Event {
                state: State::Idle,
                message: Ok("full"),
            })
            .unwrap();
        let (started, wait_started) = bounded(0);
        commands
            .send(Command::Toggle {
                now: Duration::ZERO,
                settings: Settings::initial().unwrap(),
            })
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let owner_stop = stop.clone();
        let owner_drops = drops.clone();
        let thread = thread::spawn(move || {
            run(
                Fake(owner_drops, Some(started)),
                receiver,
                sender,
                &owner_stop,
                |audio| {
                    drop(audio);
                    Ok(())
                },
            )
        });
        // Signal proves the recording started; delivery remains full throughout.
        wait_started.recv_timeout(Duration::from_secs(5)).unwrap();
        stop.store(true, Ordering::Release);
        thread.join().unwrap();
        assert_eq!(drops.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn failed_audio_cleanup_is_not_reported_as_success_and_returns_to_idle() {
        let drops = Arc::new(AtomicUsize::new(0));
        let (commands, receiver) = bounded(2);
        let (sender, events) = bounded(2);
        for second in [0, 1] {
            commands
                .send(Command::Toggle {
                    now: Duration::from_secs(second),
                    settings: Settings::initial().unwrap(),
                })
                .unwrap();
        }
        drop(commands);
        run(
            Fake(drops.clone(), None),
            receiver,
            sender,
            &AtomicBool::new(false),
            |audio| {
                drop(audio);
                Err("cleanup fixture failure".into())
            },
        );
        let completed = events.try_iter().last().unwrap();
        assert_eq!(completed.state, State::Idle);
        assert_eq!(completed.message.unwrap_err(), "cleanup fixture failure");
        assert_eq!(drops.load(Ordering::Relaxed), 1);
    }
}
