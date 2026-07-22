//! The crate's typed error. Library code returns this; the app binary collapses it into an
//! `anyhow` context chain at the boundary.

/// Every fallible operation in `lumagen-core` funnels through this one enum so callers can
/// match on the category. `#[from]` impls let `?` convert upstream errors automatically.
/// Variants whose payload is the `source()` keep their Display to THIS layer only — the
/// source carries the lower-level detail, and restating it in the parent message prints it
/// twice in a chain-walking renderer.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Filesystem or network I/O failed.
    #[error("I/O error")]
    Io(#[from] std::io::Error),

    /// An image failed to decode (import, provider result, library scan).
    #[error("image decode error: {0}")]
    Decode(String),

    /// An image failed to encode (export, document save).
    #[error("image encode error: {0}")]
    Encode(String),

    /// A generation provider (fal.ai / OpenRouter) failed.
    #[error("provider error")]
    Provider(#[from] crate::providers::ProviderError),

    /// An OS-vault (keyring) operation failed.
    #[error("keyring error: {0}")]
    Keyring(String),

    /// Serialization/deserialization of a document, manifest, or prompt pack failed.
    #[error("serialization error: {0}")]
    Serialize(String),

    /// The operation isn't valid for the current state (e.g. export before any map exists).
    #[error("invalid state: {0}")]
    InvalidState(String),
}

/// Convenience alias for core results.
pub type Result<T> = std::result::Result<T, Error>;

impl From<image::ImageError> for Error {
    fn from(e: image::ImageError) -> Self {
        Error::Decode(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialize(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `#[from]` marks the payload as `source()`; the parent Display must not restate it or
    /// a chain-walking renderer prints the same message twice.
    #[test]
    fn io_display_names_its_layer_and_keeps_the_source() {
        let err = Error::from(std::io::Error::new(std::io::ErrorKind::NotFound, "frame gone"));
        assert_eq!(err.to_string(), "I/O error");
        let source = std::error::Error::source(&err).expect("io source preserved");
        assert!(source.to_string().contains("frame gone"), "source carries the detail, got {source}");
    }

    #[test]
    fn provider_display_does_not_duplicate_the_source() {
        let err = Error::from(crate::providers::ProviderError::Empty);
        assert_eq!(err.to_string(), "provider error");
        let source = std::error::Error::source(&err).expect("provider source preserved");
        assert_eq!(source.to_string(), "no image in response");
    }
}
