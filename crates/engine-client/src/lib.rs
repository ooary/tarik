//! Engine process lifecycle and newline-delimited request/response transport.

pub mod process;

pub use process::EngineProcess;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("engine process failed to start: {0}")]
    Spawn(String),
    #[error("engine channel closed before response")]
    ChannelClosed,
    #[error("engine protocol error: {0}")]
    Protocol(String),
    #[error("engine returned a structured error: {code}: {message}")]
    Engine { code: String, message: String },
}
