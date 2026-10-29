use std::time::Duration;
pub mod settings;
use voicepaste_core::{
    SessionId, State,
    settings::{PasteShortcut, Settings},
};

/// The consumer carries an adapter-owned target by value, without interpreting
/// it. No native handles, cloning or ownership of another app's window required.
pub trait Paste<T> {
    fn paste(&mut self, text: &str, target: T, shortcut: PasteShortcut) -> Result<(), String>;
}

pub enum TranscriptionEvent<T> {
    Ready {
        device: String,
        statistics: Option<voicepaste_core::statistics::ModelLoad>,
    },
    Completed {
        session: SessionId,
        text: String,
        target: T,
        shortcut: PasteShortcut,
        statistics: Option<voicepaste_core::statistics::Sample>,
    },
    Failed {
        session: Option<SessionId>,
        message: String,
    },
}

pub trait Transcription<A, T> {
    fn preload(&mut self, settings: &Settings) -> Result<(), String>;
    fn submit(&mut self, submission: Submission<A, T>) -> Result<(), String>;
    fn poll(&mut self) -> Option<TranscriptionEvent<T>>;
    fn release(&mut self);
}

pub trait AudioCapture {
    type Recording;
    type Audio;
    fn start(&mut self, session: SessionId) -> Result<Self::Recording, String>;
    fn finish(&mut self, recording: Self::Recording) -> Result<Self::Audio, String>;
    fn discard(&mut self, recording: Self::Recording) -> Result<(), String>;
    fn failed(&self, recording: &Self::Recording) -> bool;
}

pub struct Submission<A, T> {
    pub session: SessionId,
    pub audio: A,
    pub settings: Settings,
    pub target: T,
}

struct Active<R, T> {
    id: SessionId,
    recording: R,
    settings: Settings,
    target: T,
}

pub struct Dictation<C: AudioCapture, T> {
    capture: C,
    state: State,
    active: Option<Active<C::Recording, T>>,
    next_id: u64,
    last_toggle: Option<Duration>,
}

impl<C: AudioCapture, T> Dictation<C, T> {
    pub fn new(capture: C) -> Self {
        Self {
            capture,
            state: State::Idle,
            active: None,
            next_id: 0,
            last_toggle: None,
        }
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn recording_failed(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| self.capture.failed(&active.recording))
    }

    pub fn toggle(
        &mut self,
        now: Duration,
        settings: &Settings,
        target: T,
    ) -> Result<Option<Submission<C::Audio, T>>, String> {
        if self
            .last_toggle
            .is_some_and(|last| now.saturating_sub(last) < Duration::from_millis(300))
        {
            return Ok(None);
        }
        self.last_toggle = Some(now);
        match self.state {
            State::Idle => {
                settings.validate()?;
                self.next_id = self
                    .next_id
                    .checked_add(1)
                    .ok_or("Session counter exhausted")?;
                let id = SessionId(self.next_id);
                let recording = self.capture.start(id)?;
                self.active = Some(Active {
                    id,
                    recording,
                    settings: settings.clone(),
                    target,
                });
                self.state = State::Recording(id);
                Ok(None)
            }
            State::Recording(_) => {
                let active = self.active.take().ok_or("Missing recording owner")?;
                self.state = State::Idle;
                let audio = self.capture.finish(active.recording)?;
                self.state = State::Transcribing(active.id);
                Ok(Some(Submission {
                    session: active.id,
                    audio,
                    settings: active.settings,
                    target: active.target,
                }))
            }
            State::Transcribing(_) => Ok(None),
        }
    }

    pub fn cancel(&mut self, id: SessionId) -> Result<(), String> {
        if self.state != State::Recording(id) {
            return Ok(());
        }
        self.state = State::Idle;
        if let Some(active) = self.active.take() {
            self.capture.discard(active.recording)?;
        }
        Ok(())
    }

    // Only a matching completion permits paste. A stale response cannot finish a
    // newer session; the adapter must check this before clipboard mutation.
    pub fn complete(&mut self, id: SessionId) -> bool {
        if self.state != State::Transcribing(id) {
            return false;
        }
        self.state = State::Idle;
        true
    }

    pub fn deliver(
        &mut self,
        id: SessionId,
        text: &str,
        target: T,
        shortcut: PasteShortcut,
        paste: &mut impl Paste<T>,
    ) -> Result<bool, String> {
        if !self.complete(id) {
            return Ok(false);
        }
        if text.trim().is_empty() {
            return Err("No speech detected".into());
        }
        paste.paste(text, target, shortcut)?;
        Ok(true)
    }

    pub fn shutdown(&mut self) -> Result<(), String> {
        self.state = State::Idle;
        if let Some(active) = self.active.take() {
            self.capture.discard(active.recording)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    // Intentionally neither Copy, Clone nor Default, and not an OS identifier.
    struct Target {
        name: &'static str,
        released: Rc<RefCell<Vec<&'static str>>>,
    }
    impl Drop for Target {
        fn drop(&mut self) {
            self.released.borrow_mut().push(self.name);
        }
    }
    struct TargetPaste(Vec<&'static str>);
    impl Paste<Target> for TargetPaste {
        fn paste(&mut self, _: &str, target: Target, _: PasteShortcut) -> Result<(), String> {
            self.0.push(target.name);
            Ok(())
        }
    }

    #[test]
    fn opaque_target_moves_from_start_through_completion_and_is_released_once() {
        let released = Rc::new(RefCell::new(Vec::new()));
        let target = |name| Target {
            name,
            released: released.clone(),
        };
        let settings = Settings::initial().unwrap();
        let mut app = Dictation::new(Capture);
        let mut paste = TargetPaste(Vec::new());
        app.toggle(Duration::ZERO, &settings, target("start"))
            .unwrap();
        app.toggle(Duration::from_millis(10), &settings, target("debounced"))
            .unwrap();
        let submission = app
            .toggle(Duration::from_secs(1), &settings, target("stop"))
            .unwrap()
            .unwrap();
        assert_eq!(&*released.borrow(), &["debounced", "stop"]);
        assert_eq!(submission.target.name, "start");
        assert!(
            !app.deliver(
                SessionId(99),
                "stale",
                target("stale"),
                settings.paste_shortcut,
                &mut paste
            )
            .unwrap()
        );
        assert!(paste.0.is_empty());
        assert!(
            app.deliver(
                submission.session,
                "Привіт",
                submission.target,
                settings.paste_shortcut,
                &mut paste
            )
            .unwrap()
        );
        assert_eq!(paste.0, ["start"]);
        assert_eq!(
            &*released.borrow(),
            &["debounced", "stop", "stale", "start"]
        );
    }

    #[test]
    fn cancel_and_shutdown_release_owned_targets_without_paste() {
        let released = Rc::new(RefCell::new(Vec::new()));
        let target = |name| Target {
            name,
            released: released.clone(),
        };
        let settings = Settings::initial().unwrap();
        let mut app = Dictation::new(Capture);
        app.toggle(Duration::ZERO, &settings, target("cancel"))
            .unwrap();
        app.cancel(SessionId(1)).unwrap();
        app.toggle(Duration::from_secs(1), &settings, target("shutdown"))
            .unwrap();
        app.shutdown().unwrap();
        assert_eq!(&*released.borrow(), &["cancel", "shutdown"]);
    }
    struct Clipboard {
        calls: usize,
        fail: bool,
    }
    impl Paste<usize> for Clipboard {
        fn paste(&mut self, _: &str, _: usize, _: PasteShortcut) -> Result<(), String> {
            self.calls += 1;
            if self.fail {
                Err("Injection blocked; clipboard retained".into())
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn stale_and_empty_results_never_mutate_clipboard() {
        let mut app = Dictation::new(Capture);
        let settings = Settings::initial().unwrap();
        let mut paste = Clipboard {
            calls: 0,
            fail: false,
        };
        app.toggle(Duration::ZERO, &settings, 1).unwrap();
        let submission = app
            .toggle(Duration::from_secs(1), &settings, 1)
            .unwrap()
            .unwrap();
        assert!(
            !app.deliver(
                SessionId(999),
                "late",
                1,
                settings.paste_shortcut,
                &mut paste
            )
            .unwrap()
        );
        assert_eq!(app.state(), State::Transcribing(submission.session));
        assert!(
            app.deliver(
                submission.session,
                "  ",
                1,
                settings.paste_shortcut,
                &mut paste
            )
            .is_err()
        );
        assert_eq!(paste.calls, 0);
        assert_eq!(app.state(), State::Idle);
    }
    #[test]
    fn failed_paste_recovers_without_repeating_injection() {
        let mut app = Dictation::new(Capture);
        let settings = Settings::initial().unwrap();
        let mut paste = Clipboard {
            calls: 0,
            fail: true,
        };
        app.toggle(Duration::ZERO, &settings, 1).unwrap();
        let submission = app
            .toggle(Duration::from_secs(1), &settings, 1)
            .unwrap()
            .unwrap();
        assert!(
            app.deliver(
                submission.session,
                "Привіт",
                1,
                settings.paste_shortcut,
                &mut paste
            )
            .is_err()
        );
        assert_eq!(paste.calls, 1);
        assert_eq!(app.state(), State::Idle);
        assert!(
            !app.deliver(
                submission.session,
                "duplicate",
                1,
                settings.paste_shortcut,
                &mut paste
            )
            .unwrap()
        );
        assert_eq!(paste.calls, 1);
    }
    struct Capture;
    impl AudioCapture for Capture {
        type Recording = SessionId;
        type Audio = SessionId;
        fn start(&mut self, id: SessionId) -> Result<SessionId, String> {
            Ok(id)
        }
        fn finish(&mut self, id: SessionId) -> Result<SessionId, String> {
            Ok(id)
        }
        fn discard(&mut self, _: SessionId) -> Result<(), String> {
            Ok(())
        }
        fn failed(&self, _: &SessionId) -> bool {
            false
        }
    }
    #[test]
    fn debounce_snapshot_busy_and_stale_completion() {
        let mut app = Dictation::new(Capture);
        let settings = Settings::initial().unwrap();
        app.toggle(Duration::ZERO, &settings, 42).unwrap();
        app.toggle(Duration::from_millis(299), &settings, 43)
            .unwrap();
        assert_eq!(app.state(), State::Recording(SessionId(1)));
        let mut changed = settings.clone();
        changed.beam_size = 7;
        let audio = app
            .toggle(Duration::from_millis(300), &changed, 43)
            .unwrap()
            .unwrap();
        assert_eq!(audio.settings, settings);
        assert_eq!(audio.target, 42);
        assert!(
            app.toggle(Duration::from_secs(1), &settings, 0)
                .unwrap()
                .is_none()
        );
        assert!(!app.complete(SessionId(2)));
        assert!(app.complete(audio.session));
    }
    #[test]
    fn cancel_is_recording_and_session_specific() {
        let mut app = Dictation::new(Capture);
        let settings = Settings::initial().unwrap();
        app.toggle(Duration::ZERO, &settings, 0).unwrap();
        app.cancel(SessionId(9)).unwrap();
        assert_eq!(app.state(), State::Recording(SessionId(1)));
        app.cancel(SessionId(1)).unwrap();
        assert_eq!(app.state(), State::Idle);
        app.toggle(Duration::from_secs(1), &settings, 0).unwrap();
        app.cancel(SessionId(1)).unwrap();
        assert_eq!(app.state(), State::Recording(SessionId(2)));
        app.shutdown().unwrap();
        assert_eq!(app.state(), State::Idle);
    }
}
