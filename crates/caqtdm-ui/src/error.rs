use std::io;

#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error("{path}: {source}")]
    Io { path: String, source: io::Error },
    #[error("xml: {0}")]
    Xml(String),
    #[error("{0}")]
    Message(String),
}

impl UiError {
    pub fn io(path: String, source: io::Error) -> Self {
        Self::Io { path, source }
    }

    pub fn msg(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}
