#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("command `{0}` is not available right now: {1}")]
    Disabled(String, String),
    #[error("invalid parameters for `{cmd}`: {msg}")]
    BadParams { cmd: String, msg: String },
    #[error("no active document")]
    NoDocument,
    #[error("{0}")]
    Other(String),
    /// An edit would give an object infinite or undefined coordinates: nothing was changed, and a command typed at
    /// the command line ends instead of asking again.
    #[error("{0}")]
    TooLarge(String),
    /// A command panicked; the guard kept the drawing as it was (a bug: please report it).
    #[error("internal error in `{0}` (the drawing was kept as it was): {1}")]
    Internal(String, String),
}

impl From<cadcraft_doc::DocError> for EngineError {
    fn from(e: cadcraft_doc::DocError) -> Self {
        match e {
            // One message for a result too large to represent, whichever check finds it.
            cadcraft_doc::DocError::NonFinite(h) => EngineError::TooLarge(crate::finite::too_large(h)),
            e => EngineError::Other(e.to_string()),
        }
    }
}

impl From<cadcraft_render::ExtentsRefusal> for EngineError {
    fn from(refusal: cadcraft_render::ExtentsRefusal) -> Self {
        EngineError::Other(refusal.to_string())
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;
