use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClipError {
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    BackendUnavailable(String),
    #[error("{0}")]
    Clipboard(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl ClipError {
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
    }

    pub fn backend_unavailable(message: impl Into<String>) -> Self {
        Self::BackendUnavailable(message.into())
    }

    pub fn clipboard(message: impl Into<String>) -> Self {
        Self::Clipboard(message.into())
    }
}
