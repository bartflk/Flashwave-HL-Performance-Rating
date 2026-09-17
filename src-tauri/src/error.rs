//! Errors as the frontend sees them.
//!
//! Every command returns `Result<T, CmdError>`, which serialises to
//! `{ kind, message }` so the UI never has to parse an error string.

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CmdError {
    pub kind: String,
    pub message: String,
}

impl CmdError {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        CmdError { kind: kind.into(), message: message.into() }
    }
}

impl From<hl_core::Error> for CmdError {
    fn from(e: hl_core::Error) -> Self {
        CmdError::new(e.kind(), e.to_string())
    }
}

impl From<anyhow::Error> for CmdError {
    fn from(e: anyhow::Error) -> Self {
        // Include the cause chain: "opening database `...`: unable to open
        // database file" is actionable, "internal error" is not.
        let message = e
            .chain()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(": ");
        tracing::error!(error = %message, "command failed");
        CmdError::new("internal", message)
    }
}

pub type CmdResult<T> = std::result::Result<T, CmdError>;
