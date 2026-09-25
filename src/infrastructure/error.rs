#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseError {
    pub message: String,
}

impl DatabaseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
