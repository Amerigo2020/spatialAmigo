//! Error type shared by all modules.

use thiserror::Error;

/// Everything that can go wrong while loading scenes, parsing text or evaluating queries.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum Error {
    /// The JSON could not be deserialized.
    #[error("invalid JSON: {0}")]
    Json(String),
    /// The scene is syntactically valid JSON but violates the format rules.
    #[error("invalid scene: {0}")]
    InvalidScene(String),
    /// The query is structurally invalid (e.g. `between` with one anchor).
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    /// The template parser could not understand the sentence.
    #[error("could not parse \"{text}\": {reason}")]
    Parse {
        /// Input text.
        text: String,
        /// Human readable reason.
        reason: String,
    },
    /// The query references a label or id that does not exist in the scene.
    #[error("unknown object reference: {0}")]
    UnknownReference(String),
    /// An egocentric relation was requested but the scene has no viewpoint.
    #[error("egocentric relation \"{0}\" needs a scene viewpoint")]
    MissingViewpoint(String),
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e.to_string())
    }
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, Error>;
