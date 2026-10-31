//! Recording-intent ownership across asynchronous native permission handoff.
use super::RecordingPermission;
use voicepaste_core::{SessionId, State};

pub(crate) struct RecordingInput<T> {
    permission_target: Option<T>,
    pending_cancel: Option<SessionId>,
}

impl<T> Default for RecordingInput<T> {
    fn default() -> Self {
        Self {
            permission_target: None,
            pending_cancel: None,
        }
    }
}

impl<T> RecordingInput<T> {
    pub fn clear(&mut self) {
        self.permission_target.take();
        self.pending_cancel = None;
    }

    pub fn permission_pending(&self) -> bool {
        self.permission_target.is_some()
    }

    pub fn dispatch(
        &mut self,
        state: State,
        targets: Vec<T>,
        cancel: Option<SessionId>,
        mut permission: impl FnMut(bool) -> RecordingPermission,
        mut send_cancel: impl FnMut(SessionId) -> Result<bool, String>,
        mut send_toggle: impl FnMut(T) -> Result<bool, String>,
    ) -> Result<Option<String>, String> {
        self.pending_cancel = cancel.or(self.pending_cancel);
        if let Some(id) = self.pending_cancel {
            self.permission_target.take();
            if send_cancel(id)? {
                self.pending_cancel = None;
            }
            return Ok(None); // Cancellation wins over all toggles in this batch.
        }
        if matches!(state, State::Transcribing(_)) {
            self.permission_target.take();
            return Ok(None);
        }
        let mut message = None;
        for target in targets {
            // One explicit Start owns the permission handoff. Additional keys must
            // not become a second Start/Stop when its asynchronous grant arrives.
            if self.permission_target.is_some() {
                continue;
            }
            if matches!(state, State::Recording(_))
                || matches!(
                    permission(false),
                    RecordingPermission::NotRequired | RecordingPermission::Granted
                )
            {
                if !send_toggle(target)? {
                    message =
                        Some("Command queue busy; extra recording toggle was rejected.".into());
                }
            } else if permission(true) == RecordingPermission::Denied {
                message = Some("Microphone permission denied/restricted; check Privacy & Security → Microphone.".into());
            } else {
                self.permission_target = Some(target);
                message = Some("Waiting for microphone permission…".into());
            }
        }
        if self.permission_pending() {
            match permission(false) {
                RecordingPermission::Granted | RecordingPermission::NotRequired => {
                    if let Some(target) = self.permission_target.take()
                        && !send_toggle(target)?
                    {
                        message = Some("Command queue busy; press the recording key again.".into());
                    }
                }
                RecordingPermission::Denied => {
                    self.permission_target.take();
                    message = Some("Microphone permission denied/restricted".into());
                }
                RecordingPermission::Pending => (),
            }
        }
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permission_handoff_keeps_original_target_and_rejects_duplicate_start() {
        let mut input = RecordingInput::default();
        let mut sent = Vec::new();
        input
            .dispatch(
                State::Idle,
                vec![11, 12],
                None,
                |_| RecordingPermission::Pending,
                |_| panic!("no cancel"),
                |target| {
                    sent.push(target);
                    Ok(true)
                },
            )
            .unwrap();
        assert!(input.permission_pending());
        assert!(sent.is_empty());
        input
            .dispatch(
                State::Idle,
                vec![13],
                None,
                |_| RecordingPermission::Granted,
                |_| panic!("no cancel"),
                |target| {
                    sent.push(target);
                    Ok(true)
                },
            )
            .unwrap();
        assert_eq!(sent, [11]);
        assert!(!input.permission_pending());
    }

    #[test]
    fn cancel_wins_over_permission_and_toggles_and_retries_backpressure() {
        let mut input = RecordingInput {
            permission_target: Some(11),
            ..Default::default()
        };
        let id = SessionId(7);
        input
            .dispatch(
                State::Idle,
                vec![12],
                Some(id),
                |_| panic!("cancel wins"),
                |actual| {
                    assert_eq!(actual, id);
                    Ok(false)
                },
                |_| panic!("no toggle"),
            )
            .unwrap();
        assert!(!input.permission_pending());
        assert_eq!(input.pending_cancel, Some(id));
        input
            .dispatch(
                State::Idle,
                vec![13],
                None,
                |_| panic!("cancel retries first"),
                |actual| {
                    assert_eq!(actual, id);
                    Ok(true)
                },
                |_| panic!("no toggle"),
            )
            .unwrap();
        assert!(input.pending_cancel.is_none());
    }

    #[test]
    fn denied_or_busy_permission_completion_never_replays_an_old_target() {
        for permission in [RecordingPermission::Denied, RecordingPermission::Granted] {
            let mut input = RecordingInput {
                permission_target: Some(11),
                pending_cancel: None,
            };
            assert!(
                input
                    .dispatch(
                        State::Idle,
                        vec![],
                        None,
                        |_| permission,
                        |_| panic!("no cancel"),
                        |_| Ok(false)
                    )
                    .unwrap()
                    .is_some()
            );
            assert!(!input.permission_pending());
            input
                .dispatch(
                    State::Idle,
                    vec![],
                    None,
                    |_| panic!("no pending permission"),
                    |_| panic!("no cancel"),
                    |_| panic!("no stale toggle"),
                )
                .unwrap();
        }
    }

    #[test]
    fn recording_stop_does_not_request_permission_and_transcribing_ignores_toggles() {
        let mut input = RecordingInput::default();
        input
            .dispatch(
                State::Recording(SessionId(1)),
                vec![11],
                None,
                |_| panic!("Stop needs no permission"),
                |_| panic!("no cancel"),
                |target| {
                    assert_eq!(target, 11);
                    Ok(true)
                },
            )
            .unwrap();
        input.permission_target = Some(12);
        input
            .dispatch(
                State::Transcribing(SessionId(1)),
                vec![13],
                None,
                |_| panic!("busy"),
                |_| panic!("no cancel"),
                |_| panic!("busy"),
            )
            .unwrap();
        assert!(!input.permission_pending());
    }

    #[test]
    fn shutdown_discards_pending_intents_and_disconnect_is_not_backpressure() {
        let mut input = RecordingInput {
            permission_target: Some(11),
            pending_cancel: Some(SessionId(1)),
        };
        input.clear();
        assert!(!input.permission_pending());
        assert!(input.pending_cancel.is_none());
        assert!(
            input
                .dispatch(
                    State::Idle,
                    vec![12],
                    None,
                    |_| RecordingPermission::NotRequired,
                    |_| panic!("no cancel"),
                    |_| Err("disconnected".into())
                )
                .is_err()
        );
    }
}
