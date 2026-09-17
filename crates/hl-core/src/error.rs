pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid SteamID: {0}")]
    InvalidSteamId(String),

    #[error("invalid TF2 path: {0}")]
    InvalidTfPath(String),

    #[error("unknown TF2 class: {0}")]
    UnknownClass(String),

    #[error("config key not set: {0}")]
    MissingConfig(&'static str),
}

impl Error {
    /// Stable machine-readable tag, so the UI can react to a specific failure
    /// without matching on message text.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::InvalidSteamId(_) => "invalid_steamid",
            Error::InvalidTfPath(_) => "invalid_tf_path",
            Error::UnknownClass(_) => "unknown_class",
            Error::MissingConfig(_) => "missing_config",
        }
    }
}
