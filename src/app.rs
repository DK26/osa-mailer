use crate::errors::ErrorReport;

pub struct AppState {
    error_reports: Option<Vec<ErrorReport>>,
}

impl AppState {
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

    #[test]
    fn somekind() {
        assert_eq!(1, 1)
    }
}
