use std::error::Error;

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

#[derive(Debug, Default)]
pub struct ErrorReport {
    /// Additional context for the errors, such as JSON file contents
    context: Option<String>,

    /// Errors regarding a specific context, such as multiple detected error in a JSON file.
    errors: Vec<Box<dyn Error>>,
}

impl ErrorReport {
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    #[inline]
    pub fn add_error<E: Error + 'static>(&mut self, error: E) {
        self.errors.push(Box::new(error));
    }

    #[inline]
    pub fn set_context(&mut self, context: String) {
        self.context = Some(context)
    }

    #[inline]
    pub fn context(&self) -> Option<&str> {
        self.context.as_deref()
    }

    #[inline]
    pub fn errors(&self) -> &[Box<dyn Error>] {
        self.errors.as_slice()
    }
}
