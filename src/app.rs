use crate::errors::ErrorReport;

/// Application state accumulated during a run: the error reports of every
/// failed step (per e-mail, per entry), ready to be logged or relayed to
/// the entry's `notify_error` recipients.
#[derive(Debug, Default)]
pub struct AppState {
    error_reports: Option<Vec<ErrorReport>>,
}

impl AppState {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_error_report(&mut self, error_report: ErrorReport) {
        match self.error_reports {
            Some(ref mut errors) => errors.push(error_report),
            None => self.error_reports = Some(vec![error_report]),
        }
    }

    pub fn error_reports(&self) -> Option<&[ErrorReport]> {
        self.error_reports.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::EntryError;

    #[test]
    fn new_state_has_no_reports() {
        let state = AppState::new();
        assert!(state.error_reports().is_none());
    }

    #[test]
    fn collects_reports_in_order() {
        let mut state = AppState::new();

        state.add_error_report(
            ErrorReport::new()
                .set_context("mail-one".to_string())
                .add_error(EntryError::MissingField("to")),
        );
        state.add_error_report(
            ErrorReport::new()
                .set_context("mail-two".to_string())
                .add_error(EntryError::MissingEmailSection),
        );

        let reports = state.error_reports().expect("reports were added");
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].context(), Some("mail-one"));
        assert_eq!(reports[1].context(), Some("mail-two"));
        assert_eq!(reports[0].errors().len(), 1);
        // ErrorEvent.0 is the timestamp, .1 the boxed error
        assert_eq!(
            reports[0].errors()[0].1.to_string(),
            EntryError::MissingField("to").to_string()
        );
    }
}
