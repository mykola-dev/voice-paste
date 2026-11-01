//! OS-independent presentation policy. Runtime remains the dictation state owner.
use std::time::Duration;
use voicepaste_core::{
    State,
    settings::OverlayAppearance,
    waveform::{Measurement, Waveform},
};

pub(super) const WIDTH: f32 = 224.;
pub(super) const HEIGHT: f32 = 100.;
pub(super) const CORNER_RADIUS: f32 = 16.;
pub(super) const MARGIN: f32 = 20.;
pub(super) const TRAVEL: f32 = WIDTH + MARGIN;
// Stop the native surface at the work-area edge, not on an adjacent monitor.
pub(super) const VIEWPORT_WIDTH: f32 = WIDTH + MARGIN;

pub(super) struct Presentation {
    pub state: State,
    pub active: bool,
    pub provisioning: Option<String>,
    pub cancel_requested: bool,
    preparation_generation: Option<u64>,
    cancel_pending: Option<u64>,
    pub started: Duration,
    pub wave: Waveform,
    pub color: u32,
    pub next_appearance: OverlayAppearance,
    pub appearance: OverlayAppearance,
}

impl Presentation {
    pub fn new(color: &str, appearance: OverlayAppearance, now: Duration) -> Self {
        let mut value = Self {
            state: State::Idle,
            active: false,
            provisioning: None,
            cancel_requested: false,
            preparation_generation: None,
            cancel_pending: None,
            started: now,
            wave: Waveform::new(now),
            color: 0xe9563d,
            next_appearance: appearance,
            appearance,
        };
        value.settings(color, appearance);
        value
    }

    pub fn settings(&mut self, color: &str, appearance: OverlayAppearance) {
        if let Some(hex) = color.strip_prefix('#')
            && hex.len() == 6
            && let Ok(rgb) = u32::from_str_radix(hex, 16)
        {
            self.color = rgb;
        }
        self.next_appearance = appearance;
    }

    #[cfg(test)]
    pub fn update(&mut self, state: State, downloading: bool, now: Duration) {
        self.update_status(state, downloading, "Preparing model...", now);
    }

    pub fn update_status(&mut self, state: State, preparing: bool, message: &str, now: Duration) {
        let active = state != State::Idle || preparing;
        if !preparing {
            self.cancel_requested = false;
        }
        // Keep the last foreground during exit; runtime ownership is unchanged.
        if active && (state != self.state || active != self.active) {
            self.wave.reset(now);
            self.started = now;
        }
        if active && !self.active {
            self.appearance = self.next_appearance;
        }
        if active {
            self.state = state;
            self.provisioning = preparing.then(|| {
                if message.trim().is_empty() {
                    "Preparing model...".to_owned()
                } else {
                    message.to_owned()
                }
            });
        }
        self.active = active;
    }

    pub fn interactive(&self) -> bool {
        self.active && self.state == State::Idle && self.provisioning.is_some()
    }

    pub fn set_preparation_generation(&mut self, generation: u64) {
        if self.preparation_generation != Some(generation) {
            self.preparation_generation = Some(generation);
            self.cancel_requested = false;
        }
        // A queued click belongs to its original generation even after restart.
    }

    pub fn request_cancel(&mut self) {
        if self.interactive()
            && !self.cancel_requested
            && self.cancel_pending.is_none()
            && let Some(generation) = self.preparation_generation
        {
            self.cancel_pending = Some(generation);
            self.cancel_requested = true;
        }
    }

    pub fn take_cancel(&mut self) -> Option<u64> {
        self.cancel_pending.take()
    }

    pub fn measurement(&mut self, value: Measurement, now: Duration) {
        if self.active && self.state == State::Recording(value.session) {
            self.wave.advance(now);
            self.wave.add(
                value.timestamp,
                value.level,
                self.appearance.waveform_amplification,
            );
        }
    }

    pub fn elapsed(&self, now: Duration) -> Duration {
        now.saturating_sub(self.started)
    }
}

pub(super) fn triangle(time: f64, half: f64) -> f64 {
    let phase = (time / half).rem_euclid(2.0);
    if phase <= 1.0 { phase } else { 2.0 - phase }
}

pub(super) fn busy_size(time: f64, bar: usize) -> f32 {
    let phase = time * 0.70 - bar as f64 * 0.10;
    (0.4 + 0.6
        * if phase < 0.0 {
            0.0
        } else {
            triangle(phase, 0.24)
        }) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use voicepaste_core::SessionId;

    #[test]
    fn appearance_applies_only_on_next_show_without_resetting_recording() {
        let original = OverlayAppearance::default();
        let changed = OverlayAppearance {
            acrylic: false,
            transparency: 100,
            waveform_amplification: 30,
        };
        let mut view = Presentation::new("#E9563D", original, Duration::ZERO);
        view.update(
            State::Recording(SessionId(1)),
            false,
            Duration::from_secs(1),
        );
        view.settings("#00ff00", changed);
        view.update(
            State::Recording(SessionId(1)),
            false,
            Duration::from_secs(2),
        );
        assert_eq!(view.appearance, original);
        assert_eq!(view.started, Duration::from_secs(1));
        assert_eq!(view.color, 0x00ff00);
        view.update(
            State::Transcribing(SessionId(1)),
            false,
            Duration::from_secs(3),
        );
        assert_eq!(view.appearance, original);
        view.update(State::Idle, false, Duration::from_secs(4));
        assert!(!view.active);
        view.update(State::Idle, true, Duration::from_secs(5));
        assert!(view.active);
        assert_eq!(view.appearance, changed);
        assert_eq!(view.elapsed(Duration::from_secs(6)), Duration::from_secs(1));
    }

    #[test]
    fn stale_sessions_do_not_change_the_waveform() {
        let mut view = Presentation::new("#E9563D", OverlayAppearance::default(), Duration::ZERO);
        view.update(State::Recording(SessionId(2)), false, Duration::ZERO);
        view.measurement(
            Measurement {
                session: SessionId(1),
                timestamp: Duration::ZERO,
                level: 1.,
            },
            Duration::ZERO,
        );
        assert!(
            view.wave
                .profile(224)
                .iter()
                .all(|column| column.level == 0.)
        );
    }

    #[test]
    fn busy_timeline_and_recording_pulse_are_preserved() {
        for bar in 0..5 {
            for (time, expected) in [(0.0, 0.4), (0.12, 0.7), (0.24, 1.0), (0.48, 0.4)] {
                assert!(
                    (busy_size((bar as f64 * 0.10 + time) / 0.70, bar) - expected).abs() < 1e-6
                );
            }
        }
        assert_eq!(busy_size(0.14, 1), 0.4);
        assert_eq!(triangle(0.8, 0.8), 1.0);
    }

    #[test]
    fn provisioning_retains_status_through_verification_and_terminal_exit() {
        let mut view = Presentation::new("#E9563D", OverlayAppearance::default(), Duration::ZERO);
        for (second, message) in [
            "Downloading large-v2 — 1.0 / 2.0 MiB",
            "Verifying large-v2",
            "Publishing verified model",
        ]
        .into_iter()
        .enumerate()
        {
            view.update_status(
                State::Idle,
                true,
                message,
                Duration::from_secs(second as u64),
            );
            assert!(view.active);
            assert!(view.interactive());
            assert_eq!(view.provisioning.as_deref(), Some(message));
            assert_eq!(
                view.started,
                Duration::ZERO,
                "progress must not restart entry"
            );
        }
        view.update_status(State::Idle, false, "Ready", Duration::from_secs(3));
        assert!(!view.active);
        assert!(!view.interactive());
        assert_eq!(
            view.provisioning.as_deref(),
            Some("Publishing verified model")
        );
    }

    #[test]
    fn cancel_intent_is_bounded_and_recording_is_never_interactive() {
        let mut view = Presentation::new("#E9563D", OverlayAppearance::default(), Duration::ZERO);
        view.set_preparation_generation(1);
        view.update_status(State::Idle, true, "Downloading", Duration::ZERO);
        view.request_cancel();
        view.request_cancel();
        assert_eq!(view.take_cancel(), Some(1));
        assert_eq!(view.take_cancel(), None);
        view.update_status(State::Idle, true, "Cancelling", Duration::from_secs(1));
        view.request_cancel();
        assert_eq!(view.take_cancel(), None);
        view.update_status(State::Idle, false, "Cancelled", Duration::from_secs(2));
        view.set_preparation_generation(2);
        view.update_status(State::Idle, true, "Retry", Duration::from_secs(3));
        view.request_cancel();
        assert_eq!(view.take_cancel(), Some(2));
        view.update_status(
            State::Recording(SessionId(1)),
            true,
            "Preparing",
            Duration::from_secs(4),
        );
        assert!(!view.interactive());
        view.request_cancel();
        assert_eq!(view.take_cancel(), None);
    }

    #[test]
    fn queued_cancel_keeps_old_generation_across_terminal_and_restart() {
        let mut view = Presentation::new("#E9563D", OverlayAppearance::default(), Duration::ZERO);
        view.set_preparation_generation(11);
        view.update_status(State::Idle, true, "Downloading", Duration::ZERO);
        view.request_cancel();
        assert!(view.cancel_requested);
        view.update_status(State::Idle, false, "Ready", Duration::from_secs(1));
        view.set_preparation_generation(12);
        view.update_status(
            State::Idle,
            true,
            "Downloading another model",
            Duration::from_secs(2),
        );
        assert!(!view.cancel_requested);
        // A second click must not overwrite the bounded old-generation intent.
        view.request_cancel();
        assert_eq!(view.take_cancel(), Some(11));
        assert_eq!(view.take_cancel(), None);
        view.request_cancel();
        assert_eq!(view.take_cancel(), Some(12));
        assert!(view.cancel_requested);
        view.set_preparation_generation(12);
        assert!(
            view.cancel_requested,
            "duplicate start must not reset cancellation"
        );
    }
}
