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
}
