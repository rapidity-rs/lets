//! Error types for lets.
//!
//! All errors use [`miette::Diagnostic`] for rich terminal output with
//! source spans, colored labels, and help text.

use std::path::PathBuf;

use miette::SourceSpan;

/// All error types produced by lets.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum Error {
    #[error("no lets.kdl found (searched from {start_dir} to filesystem root)")]
    ConfigNotFound { start_dir: PathBuf },

    #[error("{message}")]
    #[diagnostic()]
    Parse {
        message: String,
        #[source_code]
        src: miette::NamedSource<String>,
        #[label("{message}")]
        span: SourceSpan,
    },

    #[error("{message}")]
    #[diagnostic()]
    ParseNoSpan { message: String },

    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("dependency cycle detected: {cycle}")]
    CycleDetected { cycle: String },

    #[error("command failed with exit code {code}")]
    CommandFailed { code: i32 },

    #[error("command was terminated by a signal")]
    CommandSignaled,

    #[error("{0}")]
    Other(String),
}

/// Convenience alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;
