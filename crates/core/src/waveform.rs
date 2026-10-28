//! Timestamped, bounded display history. Rendering never feeds values back into audio.
use std::{collections::VecDeque, time::Duration};

pub const CAPACITY: usize = 512;
pub const VISIBLE_SECONDS: f64 = 3.24;

#[derive(Clone, Copy)]
pub struct Measurement {
    pub session: crate::SessionId,
    pub timestamp: Duration,
    /// The audio meter's RMS x5, independently of the recorded PCM.
    pub level: f32,
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
    pub fn add(&mut self, timestamp: Duration, input: f32) {
        let time = timestamp.as_secs_f64();
        if time > self.frame
            || self.frame - time >= VISIBLE_SECONDS
            || self.last.is_some_and(|(previous, _)| time < previous)
        {
            return;
        }
        let input = if input.is_finite() {
            (input as f64 * 5.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
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
    /// Premultiplied ARGB, with analytic vertical coverage and 4x horizontal sampling.
    pub fn rasterize(
        &self,
        width: usize,
        height: usize,
        height_dips: f64,
        rgb: u32,
        pixels: &mut [u32],
    ) {
        assert_eq!(pixels.len(), width * height);
        pixels.fill(0);
        if width == 0 || height == 0 || self.samples.len() < 2 {
            return;
        }
        let mut ages = [0.0; CAPACITY];
        let mut levels = [0.0; CAPACITY];
        let mut tangents = [0.0; CAPACITY];
        let count = self.samples.len();
        for (index, &(time, level)) in self.samples.iter().enumerate() {
            ages[index] = self.frame - time;
            levels[index] = display_level(level);
        }
        build_tangents(&ages[..count], &levels[..count], &mut tangents[..count]);
        let live = display_level(self.last.map_or(0.0, |(time, input)| {
            release(self.smoothed, input, self.frame - time)
        }));
        let center = height as f64 / 2.0;
        let baseline = (height as f64 / height_dips / 2.0).min(center);
        let mut next = 1;
        for x in 0..width {
            let mut tops = [0.0; 4];
            let mut bottoms = [0.0; 4];
            let mut opacities = [0.0; 4];
            for sub in 0..4 {
                let age =
                    VISIBLE_SECONDS * (1.0 - (x as f64 + (sub as f64 + 0.5) / 4.0) / width as f64);
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
                let half = baseline + level * (center - baseline);
                tops[sub] = center - half;
                bottoms[sub] = center + half;
                opacities[sub] = 0.5 + 0.5 * (level / 0.9).clamp(0.0, 1.0);
            }
            let first = tops
                .iter()
                .copied()
                .fold(height as f64, f64::min)
                .floor()
                .max(0.0) as usize;
            let end = bottoms
                .iter()
                .copied()
                .fold(0.0, f64::max)
                .ceil()
                .min(height as f64) as usize;
            for y in first..end {
                let mut alpha = 0.0;
                for sub in 0..4 {
                    alpha += (((y + 1) as f64).min(bottoms[sub]) - (y as f64).max(tops[sub]))
                        .clamp(0.0, 1.0)
                        * opacities[sub];
                }
                pixels[y * width + x] = premultiply(rgb, (alpha * 255.0 / 4.0).round() as u8);
            }
        }
    }
}

fn release(previous: f64, input: f64, elapsed: f64) -> f64 {
    if input >= previous {
        input
    } else {
        input + (previous - input) * (-elapsed.max(0.0) / 0.14).exp()
    }
}
fn display_level(level: f64) -> f64 {
    let t = (level / 0.1).clamp(0.0, 1.0);
    (level * (1.0 + t * t * (3.0 - 2.0 * t))).clamp(0.0, 1.0)
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
pub fn premultiply(rgb: u32, alpha: u8) -> u32 {
    let a = u32::from(alpha);
    let channel = |shift: u32| (((rgb >> shift) & 255u32) * a + 127) / 255;
    (a << 24) | (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_measurements_are_bounded_and_frames_do_not_repeat_input() {
        let mut wave = Waveform::new(Duration::ZERO);
        for index in 1..=1000 {
            let now = Duration::from_millis(index);
            wave.advance(now);
            wave.add(now, 0.1);
        }
        assert_eq!(wave.samples.len(), CAPACITY);
        let count = wave.samples.len();
        wave.advance(Duration::from_millis(1100));
        assert_eq!(wave.samples.len(), count);
        wave.add(Duration::from_millis(999), 1.0);
        assert_eq!(wave.samples.len(), count);
        wave.advance(Duration::from_secs(5));
        assert_eq!(wave.samples.len(), 3);
        assert!(wave.last.is_none());
    }
    #[test]
    fn silence_baseline_is_full_width_and_history_owns_its_opacity() {
        let wave = Waveform::new(Duration::ZERO);
        let mut pixels = vec![0; 224 * 30];
        wave.rasterize(224, 30, 30.0, 0xe9563d, &mut pixels);
        for x in 0..224 {
            assert_eq!(pixels[14 * 224 + x] >> 24, 64);
            assert_eq!(pixels[15 * 224 + x] >> 24, 64);
        }
        assert_eq!(release(1.0, 0.0, 0.14), (-1.0f64).exp());
        for t in 0..=100 {
            assert!((0.1..=0.8).contains(&interpolate(0.1, 0.8, 2.0, 1.0, 0.1, t as f64 / 100.0)));
        }
    }
}
