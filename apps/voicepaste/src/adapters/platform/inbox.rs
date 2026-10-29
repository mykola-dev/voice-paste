//! Bounded input batch. No GUI, native callbacks, handles or IO dependencies.
use std::collections::VecDeque;
use voicepaste_core::SessionId;

const HOTKEY_LIMIT: usize = 8;

pub(super) struct Pending<T> {
    // Registration id and an adapter-owned target transferred with the command.
    pub hotkeys: VecDeque<(usize, T)>,
    pub overflow: bool,
    pub cancel: Option<SessionId>,
    pub tray: bool,
    pub settings: bool,
    pub quit: bool,
}

impl<T> Default for Pending<T> {
    fn default() -> Self {
        Self {
            hotkeys: VecDeque::new(),
            overflow: false,
            cancel: None,
            tray: false,
            settings: false,
            quit: false,
        }
    }
}

impl<T> Pending<T> {
    pub fn hotkey(&mut self, id: usize, target: T) {
        if self.hotkeys.len() < HOTKEY_LIMIT {
            self.hotkeys.push_back((id, target));
        } else {
            self.overflow = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_and_drained_targets_are_released_without_clone_or_default() {
        use std::{cell::Cell, rc::Rc};
        struct Target(Rc<Cell<usize>>);
        impl Drop for Target {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let dropped = Rc::new(Cell::new(0));
        let mut pending = Pending::default();
        for id in 0..HOTKEY_LIMIT + 1 {
            pending.hotkey(id, Target(dropped.clone()));
        }
        assert_eq!(dropped.get(), 1);
        let batch = std::mem::take(&mut pending);
        assert_eq!(dropped.get(), 1);
        drop(batch);
        assert_eq!(dropped.get(), HOTKEY_LIMIT + 1);
        drop(pending);
        assert_eq!(dropped.get(), HOTKEY_LIMIT + 1);
    }

    #[test]
    fn overflow_preserves_order_and_control_slots() {
        let mut pending = Pending::default();
        for id in 0..HOTKEY_LIMIT + 2 {
            pending.hotkey(id, id + 100);
        }
        pending.cancel = Some(SessionId(7));
        pending.quit = true;
        pending.settings = true;
        pending.tray = true;
        assert!(pending.overflow);
        assert_eq!(
            pending.hotkeys.iter().copied().collect::<Vec<_>>(),
            (0..HOTKEY_LIMIT)
                .map(|id| (id, id + 100))
                .collect::<Vec<_>>()
        );
        let batch = std::mem::take(&mut pending);
        assert_eq!(batch.cancel, Some(SessionId(7)));
        assert!(batch.quit && batch.settings && batch.tray);
        assert!(pending.hotkeys.is_empty());
        assert!(!pending.overflow && !pending.quit && !pending.settings && !pending.tray);
        assert_eq!(pending.cancel, None);
        pending.hotkey(42, 99);
        assert_eq!(pending.hotkeys.pop_front(), Some((42, 99)));
    }
}
