//! One startup offer and model-scoped consent retained under command backpressure.
#[derive(Default)]
pub(crate) struct DownloadConsent {
    startup: Option<String>,
    approved: Option<String>,
    missing: Option<String>,
}

impl DownloadConsent {
    pub fn unavailable(&mut self, model: String) {
        self.missing = Some(model.clone());
        self.startup = Some(model);
    }

    pub fn ready(&mut self, model: &str) {
        if self.missing.as_deref() == Some(model) {
            self.missing = None;
            self.startup = None;
        }
    }

    pub fn cancelled(&mut self, model: String) {
        self.missing = Some(model);
        self.startup = None;
        self.approved = None;
    }

    pub fn retry(&mut self, model: &str) -> bool {
        if self.missing.as_deref() == Some(model) {
            self.startup = Some(model.into());
            true
        } else {
            false
        }
    }

    pub fn take_offer(&mut self, model: &str, available: bool) -> bool {
        available && self.startup.take().is_some_and(|pending| pending == model)
    }

    pub fn approve(&mut self, model: &str) {
        self.startup = None;
        self.approved = Some(model.into());
    }

    pub fn pending(&mut self, model: &str) -> bool {
        if self
            .approved
            .as_deref()
            .is_some_and(|pending| pending != model)
        {
            self.approved = None;
        }
        self.approved.is_some()
    }

    pub fn submitted(&mut self) {
        self.approved = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_initial_verification_allows_recording_retry_without_an_automatic_prompt() {
        let mut consent = DownloadConsent::default();
        consent.cancelled("large-v3".into());
        assert!(!consent.take_offer("large-v3", true));
        assert!(consent.retry("large-v3"));
        assert!(consent.take_offer("large-v3", true));
    }

    #[test]
    fn recording_retry_reoffers_declined_model_but_not_ready_or_stale_model() {
        let mut consent = DownloadConsent::default();
        consent.unavailable("large-v3".into());
        assert!(consent.take_offer("large-v3", true));
        assert!(!consent.retry("large-v2"));
        assert!(consent.retry("large-v3"));
        assert!(consent.take_offer("large-v3", true));
        consent.ready("large-v3");
        assert!(!consent.retry("large-v3"));
    }

    #[test]
    fn startup_offer_is_deferred_until_available_and_not_repeated_after_decline() {
        let mut consent = DownloadConsent::default();
        consent.unavailable("large-v3".into());
        assert!(!consent.take_offer("large-v3", false));
        assert!(consent.take_offer("large-v3", true));
        assert!(!consent.pending("large-v3"));
        assert!(!consent.take_offer("large-v3", true));
    }

    #[test]
    fn approval_survives_backpressure_but_not_a_model_change() {
        let mut consent = DownloadConsent::default();
        consent.approve("large-v3");
        assert!(consent.pending("large-v3"));
        assert!(consent.pending("large-v3"));
        assert!(!consent.pending("large-v2"));
        assert!(!consent.pending("large-v3"));
        consent.approve("large-v3");
        consent.submitted();
        assert!(!consent.pending("large-v3"));
    }

    #[test]
    fn stale_startup_selection_is_not_offered() {
        let mut consent = DownloadConsent::default();
        consent.unavailable("large-v3".into());
        assert!(!consent.take_offer("large-v2", true));
        assert!(!consent.take_offer("large-v3", true));
    }
}
