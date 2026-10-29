//! Shared, interruptible slide/fade timeline. Native adapters only apply its sample.
use std::time::Duration;

#[derive(Default)]
pub(super) struct Motion {
    from: f32,
    target: bool,
    started: Duration,
}

impl Motion {
    pub fn target(&mut self, visible: bool, now: Duration) {
        if visible != self.target {
            self.from = self.sample(now);
            self.started = now;
            self.target = visible;
        }
    }

    pub fn sample(&self, now: Duration) -> f32 {
        let destination = if self.target { 1. } else { 0. };
        let distance = (destination - self.from).abs();
        if distance == 0. {
            return destination;
        }
        let duration = if self.target { 0.220 } else { 0.160 } * distance;
        let t = (now.saturating_sub(self.started).as_secs_f32() / duration).clamp(0., 1.);
        let eased = if self.target {
            1. - (1. - t).powi(3)
        } else {
            t.powi(3)
        };
        self.from + (destination - self.from) * eased
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_time_not_frame_count_controls_both_transitions() {
        for hz in [60, 120, 144] {
            let mut motion = Motion::default();
            motion.target(true, Duration::ZERO);
            for frame in 0..hz {
                let now = Duration::from_secs_f64(frame as f64 / hz as f64);
                let t = (now.as_secs_f32() / 0.220).min(1.);
                assert!((motion.sample(now) - (1. - (1. - t).powi(3))).abs() < 1e-5);
            }
            // Dropped frames and a monitor change do not stretch either phase.
            assert_eq!(motion.sample(Duration::from_secs(1)), 1.);
            motion.target(false, Duration::from_secs(1));
            assert_eq!(motion.sample(Duration::from_millis(1160)), 0.);
        }
    }

    #[test]
    fn enter_decelerates_exit_accelerates_and_finishes() {
        let mut motion = Motion::default();
        motion.target(true, Duration::ZERO);
        assert_eq!(motion.sample(Duration::ZERO), 0.);
        assert!((motion.sample(Duration::from_millis(110)) - 0.875).abs() < 1e-5);
        assert_eq!(motion.sample(Duration::from_millis(220)), 1.);
        motion.target(false, Duration::from_millis(220));
        assert!((motion.sample(Duration::from_millis(300)) - 0.875).abs() < 1e-5);
        assert_eq!(motion.sample(Duration::from_millis(380)), 0.);
    }

    #[test]
    fn reversal_and_duplicate_targets_preserve_position() {
        let mut motion = Motion::default();
        motion.target(true, Duration::ZERO);
        let now = Duration::from_millis(70);
        let before = motion.sample(now);
        motion.target(true, now);
        assert_eq!(motion.sample(now), before);
        motion.target(false, now);
        assert_eq!(motion.sample(now), before);
        let later = Duration::from_millis(100);
        let before = motion.sample(later);
        motion.target(true, later);
        assert_eq!(motion.sample(later), before);
        assert_eq!(motion.sample(Duration::from_secs(1)), 1.);
    }

    #[test]
    fn completed_entry_stays_visible_until_an_explicit_exit() {
        let mut motion = Motion::default();
        motion.target(true, Duration::ZERO);
        for seconds in [1, 2, 60, 600, 3600] {
            let now = Duration::from_secs(seconds);
            motion.target(true, now);
            assert_eq!(motion.sample(now), 1.);
        }
    }
}
