use std::fmt;

/// Failure from a control-system plugin. The message is safe to show in the
/// viewer log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DsError {
    pub message: String,
}

impl DsError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for DsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DsError {}
