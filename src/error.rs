use std::process::ExitStatus;

#[derive(Debug, thiserror::Error)]
pub enum BlessError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("MongoDB error: {0}")]
    Mongo(#[from] mongodb::error::Error),

    #[error("Logger initialization failed: {0}")]
    Logger(#[from] log::SetLoggerError),

    #[error("Command failed with {status}")]
    CommandFailed { status: ExitStatus },

    #[error("{0}")]
    Config(String),
}
