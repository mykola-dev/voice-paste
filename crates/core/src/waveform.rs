//! Timestamped, bounded display history. Rendering never feeds values back into audio.
use std::{collections::VecDeque, time::Duration};

pub const CAPACITY: usize = 512;
pub const VISIBLE_SECONDS: f64 = 3.24;

#[derive(Clone, Copy)]
pub struct Measurement {
    pub session: crate::SessionId,
    pub timestamp: Duration,
    /// The audio meter's unamplified RMS, independently of the recorded PCM.
    pub level: f32,
}

/// Renderer-independent waveform envelope at one horizontal position.
pub struct Column {
    pub level: f32,
    pub opacity: f32,
}

pub struct Waveform {
    samples: VecDeque<(f64, f64)>,
    last: Option<(f64, f64)>,
    smoothed: f64,
    frame: f64,
}

impl Waveform {
    pub fn new(now: Duration) -> Self {
        let mut result = Self {
            samples: VecDeque::with_capacity(CAPACITY),
            last: None,
            smoothed: 0.0,
            frame: now.as_secs_f64(),
        };
        result.reset(now);
        result
    }
    pub fn reset(&mut self, now: Duration) {
        self.frame = now.as_secs_f64();
        self.samples.clear();
        self.samples.extend([
            (self.frame - VISIBLE_SECONDS, 0.0),
            (self.frame - 0.06, 0.0),
            (self.frame, 0.0),
        ]);
        self.last = None;
        self.smoothed = 0.0;
    }
    pub fn advance(&mut self, now: Duration) {
        let time = now.as_secs_f64();
        if time - self.frame >= VISIBLE_SECONDS {
            self.reset(now);
        }
        self.frame = time;
        // Keep two predecessors for the tangent at the clipped left edge.
        while self.samples.len() > 3 && self.samples[2].0 <= time - VISIBLE_SECONDS {
            self.samples.pop_front();
        }
    }
    pub fn add(&mut self, timestamp: Duration, input: f32, amplification: u8) {
        let time = timestamp.as_secs_f64();
        if time > self.frame
            || self.frame - time >= VISIBLE_SECONDS
            || self.last.is_some_and(|(previous, _)| time < previous)
        {
            return;
        }
        let input = display_level(f64::from(input), amplification);
        self.smoothed = release(
            self.smoothed,
            input,
            self.last.map_or(0.0, |(previous, _)| time - previous),
        );
        if self.last.is_none() {
            while self.samples.len() > 1
                && self.samples.back().is_some_and(|sample| sample.0 > time)
            {
                self.samples.pop_back();
            }
        }
        if self.samples.back().is_some_and(|sample| sample.0 == time) {
            self.samples.pop_back();
        }
        if self.samples.len() == CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back((time, self.smoothed));
        self.last = Some((time, input));
    }
    /// Sample the display envelope; GPU/bitmap layout and colors belong to the UI.
    pub fn profile(&self, width: usize) -> Vec<Column> {
        if width == 0 || self.samples.len() < 2 {
            return Vec::new();
        }
        let mut ages = [0.0; CAPACITY];
        let mut levels = [0.0; CAPACITY];
        let mut tangents = [0.0; CAPACITY];
        let count = self.samples.len();
        for (index, &(time, level)) in self.samples.iter().enumerate() {
            ages[index] = self.frame - time;
            levels[index] = level;
        }
        build_tangents(&ages[..count], &levels[..count], &mut tangents[..count]);
        let live = self.last.map_or(0.0, |(time, input)| {
            release(self.smoothed, input, self.frame - time)
        });
        let mut next = 1;
        (0..width)
            .map(|x| {
                let age = VISIBLE_SECONDS * (1.0 - (x as f64 + 0.5) / width as f64);
                while next < count - 1 && age < ages[next] {
                    next += 1;
                }
                let seconds = ages[next - 1] - ages[next];
                let level = if age < ages[count - 1] {
                    live + (levels[count - 1] - live) * age / ages[count - 1]
                } else {
                    interpolate(
                        levels[next - 1],
                        levels[next],
                        tangents[next - 1],
                        tangents[next],
                        seconds,
                        (ages[next - 1] - age) / seconds,
                    )
                };
                Column {
                    level: level as f32,
                    opacity: (0.5 + 0.5 * (level / 0.9).clamp(0.0, 1.0)) as f32,
                }
            })
            .collect()
    }
}

fn release(previous: f64, input: f64, elapsed: f64) -> f64 {
    if input >= previous {
        input
    } else {
        input + (previous - input) * (-elapsed.max(0.0) / 0.14).exp()
    }
}
fn display_level(level: f64, amplification: u8) -> f64 {
    if !level.is_finite() {
        return 0.0;
    }
    (level * f64::from(amplification)).clamp(0.0, 1.0)
}
fn build_tangents(ages: &[f64], levels: &[f64], tangents: &mut [f64]) {
    let count = levels.len();
    tangents[0] = (levels[1] - levels[0]) / (ages[0] - ages[1]);
    tangents[count - 1] =
        (levels[count - 1] - levels[count - 2]) / (ages[count - 2] - ages[count - 1]);
    for i in 1..count - 1 {
        let before = ages[i - 1] - ages[i];
        let after = ages[i] - ages[i + 1];
        let left = (levels[i] - levels[i - 1]) / before;
        let right = (levels[i + 1] - levels[i]) / after;
        tangents[i] = if left == 0.0 || right == 0.0 || left.signum() != right.signum() {
            0.0
        } else {
            let a = 2.0 * after + before;
            let b = after + 2.0 * before;
            (a + b) / (a / left + b / right)
        };
    }
}
fn interpolate(from: f64, to: f64, a: f64, b: f64, seconds: f64, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let t2 = t * t;
    let t3 = t2 * t;
    ((2.0 * t3 - 3.0 * t2 + 1.0) * from
        + (t3 - 2.0 * t2 + t) * seconds * a
        + (-2.0 * t3 + 3.0 * t2) * to
        + (t3 - t2) * seconds * b)
        .clamp(from.min(to), from.max(to))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_amplification_is_applied_once_before_release_and_rendering() {
        for amplification in [1, 20, 30, 50] {
            let mut wave = Waveform::new(Duration::ZERO);
            wave.add(Duration::ZERO, 0.001, amplification);
            let expected = f64::from(0.001f32) * f64::from(amplification);
            assert!((wave.smoothed - expected).abs() < 1e-12);
            // Sample a constant segment, not the interpolated rise from the
            // seeded silent history at the first pixel-center timestamp.
            wave.advance(Duration::from_secs(1));
            wave.add(Duration::from_secs(1), 0.001, amplification);
            let actual = wave.profile(224).last().unwrap().level;
            assert!((f64::from(actual) - expected).abs() < 1e-8);
            wave.reset(Duration::ZERO);
            wave.add(Duration::ZERO, 0., amplification);
            assert!(wave.profile(224).iter().all(|column| column.level == 0.));
            wave.add(Duration::ZERO, 1., amplification);
            assert_eq!(wave.smoothed, 1.);
        }
    }
    #[test]
    fn raw_rms_uses_full_slider_gain_at_every_level() {
        for gain in [1, 10, 20, 30, 50] {
            for raw in [0., 1e-8, 0.001, 0.0025, 0.004, 0.005, 0.2, 1., 2.] {
                let expected = (raw * f64::from(gain)).min(1.);
                assert!((display_level(raw, gain) - expected).abs() < 1e-12);
            }
            for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1] {
                assert_eq!(display_level(invalid, gain), 0.);
            }
            let mut previous = 0.;
            for step in 0..=10000 {
                let level = display_level(f64::from(step) / 10000., gain);
                assert!(level.is_finite() && (0. ..=1.).contains(&level));
                assert!(level + 1e-12 >= previous);
                previous = level;
            }
        }
    }
    #[test]
    fn quiet_samples_are_never_attenuated_and_gain_one_is_identity() {
        for raw in [1e-8, 0.0001, 0.001, 0.0025, 0.005, 0.02, 0.5, 1.] {
            assert_eq!(display_level(raw, 1), raw);
            for gain in [10, 20, 30] {
                let level = display_level(raw, gain);
                assert!(level >= raw);
                assert!(level <= (raw * f64::from(gain)).min(1.));
            }
        }
        assert!((display_level(1e-8, 30) / 1e-8 - 30.).abs() < 1e-12);
    }
    #[test]
    fn profile_is_finite_bounded_and_preserves_live_release_at_fractional_scales() {
        let mut wave = Waveform::new(Duration::ZERO);
        for (millis, input) in [(0, 0.2), (20, f32::NAN), (40, 0.01), (80, 0.2)] {
            let now = Duration::from_millis(millis);
            wave.advance(now);
            wave.add(now, input, 20);
        }
        wave.advance(Duration::from_millis(220));
        for columns in [224, 280, 336, 392, 448] {
            let profile = wave.profile(columns);
            assert_eq!(profile.len(), columns);
            assert!(profile.iter().all(|c| c.level.is_finite()
                && (0.0..=1.0).contains(&c.level)
                && (0.5..=1.0).contains(&c.opacity)));
            assert!(profile.last().unwrap().level > 0.);
            assert_eq!(profile[0].level, 0.);
        }
        assert!(wave.profile(0).is_empty());
        wave.advance(Duration::from_secs(5));
        assert!(wave.profile(392).iter().all(|c| c.level == 0.));
    }
    #[test]
    fn actual_measurements_are_bounded_and_frames_do_not_repeat_input() {
        let mut wave = Waveform::new(Duration::ZERO);
        for index in 1..=1000 {
            let now = Duration::from_millis(index);
            wave.advance(now);
            wave.add(now, 0.1, 20);
        }
        assert_eq!(wave.samples.len(), CAPACITY);
        let count = wave.samples.len();
        wave.advance(Duration::from_millis(1100));
        assert_eq!(wave.samples.len(), count);
        wave.add(Duration::from_millis(999), 1.0, 20);
        assert_eq!(wave.samples.len(), count);
        wave.advance(Duration::from_secs(5));
        assert_eq!(wave.samples.len(), 3);
        assert!(wave.last.is_none());
    }
    #[test]
    fn silence_baseline_is_full_width_and_history_owns_its_opacity() {
        let wave = Waveform::new(Duration::ZERO);
        let profile = wave.profile(224);
        assert_eq!(profile.len(), 224);
        for column in profile {
            assert_eq!(column.level, 0.);
            assert_eq!(column.opacity, 0.5);
        }
        assert_eq!(release(1.0, 0.0, 0.14), (-1.0f64).exp());
        for t in 0..=100 {
            assert!((0.1..=0.8).contains(&interpolate(0.1, 0.8, 2.0, 1.0, 0.1, t as f64 / 100.0)));
        }
    }
}
