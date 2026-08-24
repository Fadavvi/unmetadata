use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("format error in {path}: {message}")]
    Format { path: String, message: String },
    #[error("unsupported format: {0}")]
    Unsupported(String),
}

impl Error {
    pub fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }

    pub fn format(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Format { path: path.into(), message: message.into() }
    }
}
