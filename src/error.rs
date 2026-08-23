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

    #[error("task '{task}' failed with exit code {code}")]
    #[diagnostic(help("while running `{command}`"))]
    CommandFailed {
        task: String,
        command: String,
        code: i32,
    },

    #[error("task '{task}' was terminated by a signal")]
    #[diagnostic(help("while running `{command}`"))]
    CommandSignaled { task: String, command: String },

    /// A task referenced more than once in one invocation, whose first
    /// attempt already failed. Reported distinctly so a shared dependency
    /// doesn't look like a second, independent failure.
    #[error("task '{task}' failed earlier in this run")]
    DependencyFailed { task: String },

    /// Several tasks failed, or one failed and left others unrun.
    #[error("{}", plural(*count))]
    TasksFailed {
        count: usize,
        #[related]
        failures: Vec<Error>,
        #[help]
        skipped: Option<String>,
    },

    #[error("{0}")]
    Other(String),
}

fn plural(count: usize) -> String {
    if count == 1 {
        "1 task failed".to_string()
    } else {
        format!("{count} tasks failed")
    }
}

impl Error {
    /// Exit status for this error. A failing task propagates its own exit
    /// code so `lets test` is as usable in a script as the command it wraps.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::CommandFailed { code, .. } => *code,
            Error::TasksFailed { failures, .. } => failures.first().map_or(1, Error::exit_code),
            _ => 1,
        }
    }

    /// Whether this error already names the task it belongs to, so callers
    /// don't prefix it a second time.
    pub fn names_task(&self) -> bool {
        matches!(
            self,
            Error::CommandFailed { .. }
                | Error::CommandSignaled { .. }
                | Error::DependencyFailed { .. }
                | Error::TasksFailed { .. }
        )
    }
}

/// Convenience alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;
