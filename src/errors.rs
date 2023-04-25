#![allow(dead_code)]

use std::error::Error;

use chrono::{DateTime, Utc};

#[derive(thiserror::Error, Debug)]
pub enum EntryError {
    #[error("Entry does not contain `email` section")]
    MissingEmailSection,

    #[error("The `email` section is missing the `{0}` field")]
    MissingField(&'static str),

    #[error("The field `{0}` is containing a wrong type")]
    WrongFieldType(&'static str),

    #[error("Wrong item type in array `{0}`")]
    WrongArrayItem(&'static str),

    #[error("Failed to parse the entry `{id}`:\n{content})\n{error}")]
    ParsingFailure {
        id: String,
        content: String,
        error: serde_json::Error,
    },
}

#[derive(Debug)]
pub struct ErrorEvent(DateTime<Utc>, Box<dyn Error + Send + Sync + 'static>);

impl<T: Error + Send + Sync + 'static> From<T> for ErrorEvent {
    fn from(error: T) -> Self {
        ErrorEvent(chrono::offset::Utc::now(), Box::new(error))
    }
}

/// There could be a special case where an error type is not implementing the `std::error::Error` type.
/// For these cases, you'll have to use this `ErrorWrapper<E>` and maybe implement your own `From<ErrorWrapper<E>>` for
/// the `ErrorEvent` type. Currently this is used to wrap the `anyhow::Error` type.
pub struct ErrorWrapper<E>(E);

impl From<ErrorWrapper<anyhow::Error>> for ErrorEvent {
    fn from(error: ErrorWrapper<anyhow::Error>) -> Self {
        ErrorEvent(chrono::offset::Utc::now(), error.0.into())
    }
}

#[derive(Debug, Default)]
pub struct ErrorReport {
    /// Additional context for the errors, such as JSON file contents
    context: Option<String>,

    /// Errors regarding a specific context, such as multiple detected error in a JSON file.
    errors: Vec<ErrorEvent>,
}

impl ErrorReport {
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    #[inline]
    pub fn add_error<E: Into<ErrorEvent>>(mut self, error: E) -> Self {
        self.errors.push(error.into());
        self
    }

    #[inline]
    pub fn set_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }

    #[inline]
    pub fn context(&self) -> Option<&str> {
        self.context.as_deref()
    }

    #[inline]
    pub fn errors(&self) -> &[ErrorEvent] {
        self.errors.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn test_error_report() {
        let error_report = ErrorReport::new()
            .set_context("test_file.json".to_string())
            .add_error(EntryError::MissingField("unique_by"))
            .add_error(EntryError::MissingEmailSection)
            .add_error(ErrorWrapper(anyhow!("anyhow error")));

        let mut errors_iter = error_report.errors().iter();

        let ErrorEvent(_timestamp, error) = errors_iter.next().unwrap();

        assert_eq!(
            error.to_string(),
            EntryError::MissingField("unique_by").to_string()
        );

        let ErrorEvent(_timestamp, error) = errors_iter.next().unwrap();

        assert_eq!(
            error.to_string(),
            EntryError::MissingEmailSection.to_string()
        );

        let ErrorEvent(_timestamp, error) = errors_iter.next().unwrap();

        assert_eq!(error.to_string(), "anyhow error".to_string());

        assert_eq!(error_report.context(), Some("test_file.json"));
    }
}
