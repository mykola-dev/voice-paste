//! Model-scoped explicit retries retained under command backpressure.
use super::SessionEvent;

#[derive(Default)]
pub(crate) struct DownloadRetry {
    missing: Option<String>,
    pending: Option<String>,
    in_flight: Option<String>,
}

impl DownloadRetry {
    pub fn event(&mut self, event: &SessionEvent, selected: &str) {
        match event {
            SessionEvent::ModelPreparationFailed { model, .. }
            | SessionEvent::ModelPreparationCancelled { model } => {
                if model == selected {
                    self.missing = Some(model.clone());
                }
                if self.in_flight.as_ref() == Some(model) {
                    self.in_flight = None;
                }
            }
            SessionEvent::PreparationStarted { model } => {
                if self.missing.as_ref() == Some(model) {
                    self.missing = None;
                }
            }
            SessionEvent::ModelReady { model } => {
                if self.missing.as_ref() == Some(model) {
                    self.missing = None;
                }
                if self.pending.as_ref() == Some(model) {
                    self.pending = None;
                }
                if self.in_flight.as_ref() == Some(model) {
                    self.in_flight = None;
                }
            }
            _ => (),
        }
    }

    /// A Start for a missing model is a retry, never a recording request.
    pub fn retry(&mut self, model: &str) -> bool {
        if self.missing.as_deref() == Some(model) || self.in_flight.as_deref() == Some(model) {
            if self.in_flight.as_deref() != Some(model) {
                self.request(model);
            }
            true
        } else {
            false
        }
    }

    pub fn request(&mut self, model: &str) {
        if self.in_flight.as_deref() != Some(model) {
            self.pending = Some(model.into());
        }
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    pub fn submit(
        &mut self,
        model: &str,
        available: bool,
        post: impl FnOnce(&str) -> Result<bool, String>,
    ) -> Result<(), String> {
        if self
            .in_flight
            .as_deref()
            .is_some_and(|pending| pending != model)
        {
            self.in_flight = None;
        }
        if self
            .pending
            .as_deref()
            .is_some_and(|pending| pending != model)
        {
            self.pending = None;
        }
        if self.pending.is_some() && available && post(model)? {
            self.pending = None;
            self.in_flight = Some(model.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_is_explicit_model_scoped_and_retained_until_submission() {
        let mut retry = DownloadRetry::default();
        retry.event(
            &SessionEvent::ModelPreparationCancelled {
                model: "large-v3".into(),
            },
            "large-v3",
        );
        assert!(!retry.retry("large-v2"));
        assert!(retry.retry("large-v3"));
        retry.submit("large-v3", true, |_| Ok(false)).unwrap();
        retry.submit("large-v3", false, |_| panic!("busy")).unwrap();
        let mut sent = 0;
        retry
            .submit("large-v3", true, |_| {
                sent += 1;
                Ok(true)
            })
            .unwrap();
        retry
            .submit("large-v3", true, |_| panic!("duplicate"))
            .unwrap();
        assert_eq!(sent, 1);
        assert!(retry.retry("large-v3"));
        retry
            .submit("large-v3", true, |_| panic!("already in flight"))
            .unwrap();
        retry.event(
            &SessionEvent::ModelPreparationFailed {
                model: "large-v3".into(),
                message: "failure".into(),
            },
            "large-v3",
        );
        assert!(retry.retry("large-v3"));
        retry
            .submit("large-v2", true, |_| panic!("stale model"))
            .unwrap();
        retry
            .submit("large-v3", true, |_| panic!("stale intent"))
            .unwrap();
    }

    #[test]
    fn explicit_request_and_cancellation_never_schedule_an_automatic_retry() {
        let mut retry = DownloadRetry::default();
        retry.event(
            &SessionEvent::ModelPreparationFailed {
                model: "m".into(),
                message: "failure".into(),
            },
            "m",
        );
        retry.event(
            &SessionEvent::ModelPreparationFailed {
                model: "old".into(),
                message: "stale".into(),
            },
            "m",
        );
        retry
            .submit("m", true, |_| panic!("automatic retry"))
            .unwrap();
        retry.request("m");
        retry.cancel();
        retry
            .submit("m", true, |_| panic!("cancelled intent"))
            .unwrap();
        assert!(retry.retry("m"));
        retry.event(
            &SessionEvent::ModelReady {
                model: "other".into(),
            },
            "m",
        );
        retry
            .submit("m", true, |model| {
                assert_eq!(model, "m");
                Ok(true)
            })
            .unwrap();
        retry.event(&SessionEvent::ModelReady { model: "m".into() }, "m");
        assert!(!retry.retry("m"));
    }
}
