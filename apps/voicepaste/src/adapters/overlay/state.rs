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

pub(super) struct Presentation {
    pub state: State,
    pub active: bool,
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

    pub fn update(&mut self, state: State, downloading: bool, now: Duration) {
        let active = state != State::Idle || downloading;
        if state != self.state || active != self.active {
            self.wave.reset(now);
            self.started = now;
        }
        if active && !self.active {
            self.appearance = self.next_appearance;
        }
        self.state = state;
        self.active = active;
    }

    pub fn measurement(&mut self, value: Measurement, now: Duration) {
        if self.state == State::Recording(value.session) {
            self.wave.advance(now);
            self.wave.add(value.timestamp, value.level);
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
}
