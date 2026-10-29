//! UI-thread latest-frame notification. One pending wake, never a frame queue.
use std::{
    cell::{Cell, RefCell},
    task::{Context, Poll, Waker},
    time::Duration,
};

#[derive(Default)]
pub(super) struct Frames {
    pending: Cell<bool>,
    waiter: RefCell<Option<Waker>>,
}

/// Gate global compositor callbacks to the window's current display. Deadlines
/// retain their phase through jitter; missed frames never slow the timeline.
#[derive(Default)]
pub(super) struct Cadence {
    interval: Option<Duration>,
    next: Duration,
}

impl Cadence {
    pub fn due(&mut self, now: Duration, interval: Option<Duration>) -> bool {
        let Some(interval) = interval.filter(|value| !value.is_zero()) else {
            self.interval = None;
            return true;
        };
        if self.interval != Some(interval) {
            self.interval = Some(interval);
            self.next = now + interval;
            return true;
        }
        // Vblank delivery has jitter. This tolerance avoids accidentally halving
        // the rate when a native callback arrives just before its ideal deadline.
        if now + interval / 4 < self.next {
            return false;
        }
        let skipped =
            (now + interval / 4).saturating_sub(self.next).as_nanos() / interval.as_nanos();
        self.next += interval.mul_f64((skipped + 1) as f64);
        true
    }
}

impl Frames {
    pub fn publish(&self) {
        self.pending.set(true);
        let waiter = self.waiter.borrow_mut().take();
        if let Some(waiter) = waiter {
            waiter.wake();
        }
    }

    pub fn poll(&self, cx: &mut Context<'_>) -> Poll<()> {
        if self.pending.replace(false) {
            self.waiter.borrow_mut().take();
            Poll::Ready(())
        } else {
            *self.waiter.borrow_mut() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::Wake,
    };

    #[derive(Default)]
    struct WakeCount(AtomicUsize);
    impl Wake for WakeCount {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn latest_frame_coalesces_and_wakes_without_a_queue() {
        let frames = Frames::default();
        let counter = Arc::new(WakeCount::default());
        let waker = Waker::from(counter.clone());
        let mut cx = Context::from_waker(&waker);
        assert!(frames.poll(&mut cx).is_pending());
        for _ in 0..1000 {
            frames.publish();
        }
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);
        assert!(frames.poll(&mut cx).is_ready());
        assert!(frames.poll(&mut cx).is_pending());
        frames.publish();
        assert_eq!(counter.0.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn cadence_tracks_display_rate_changes_and_skips_missed_deadlines() {
        for hz in [60, 120, 144] {
            let mut cadence = Cadence::default();
            let interval = Duration::from_secs_f64(1. / hz as f64);
            let count = (0..240)
                .filter(|frame| {
                    cadence.due(
                        Duration::from_secs_f64(*frame as f64 / 240.),
                        Some(interval),
                    )
                })
                .count();
            assert_eq!(count, hz);
            assert!(cadence.due(Duration::from_secs(10), Some(interval)));
            assert!(!cadence.due(Duration::from_secs(10), Some(interval)));
            let changed = Duration::from_secs_f64(1. / 75.);
            assert!(cadence.due(Duration::from_secs(10), Some(changed)));
            assert!(!cadence.due(Duration::from_secs(10), Some(changed)));
        }
    }
}
