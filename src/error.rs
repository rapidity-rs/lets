//! Error types for lets.
//!
//! All errors use [`miette::Diagnostic`] for rich terminal output with
//! source spans, colored labels, and help text.

use std::path::PathBuf;

use miette::SourceSpan;

/// All error types produced by lets.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum Error {
    #[error("no lets.kdl found in {} or any parent directory", start_dir.display())]
    #[diagnostic(help("run `lets self init` to create one"))]
    ConfigNotFound { start_dir: PathBuf },

    /// A config-file problem with source to point at. Boxed because it
    /// carries the file's text, and every `Result` in the crate would
    /// otherwise pay for it.
    #[error(transparent)]
    #[diagnostic(transparent)]
    Parse(Box<SourceDiagnostic>),

    #[error("{message}")]
    #[diagnostic()]
    ParseNoSpan { message: String },

    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

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

    /// A prompt with no way to ask. Reported before anything runs, naming
    /// the prompt and how to get past it, rather than surfacing dialoguer's
    /// bare "not a terminal".
    #[error("task '{task}' needs an answer for {prompt}, but there is no terminal to ask on")]
    #[diagnostic(help("{remedy}"))]
    NotInteractive {
        task: String,
        prompt: String,
        remedy: String,
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

/// A diagnostic anchored in the config file: a message, the source to
/// render a snippet from, the spans to mark, and an optional fix hint.
///
/// Covers both our own errors and grammar violations lifted out of `kdl`,
/// whose sub-diagnostics carry exactly this shape.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("{message}")]
pub struct SourceDiagnostic {
    pub message: String,
    #[source_code]
    pub src: miette::NamedSource<String>,
    #[label(collection)]
    pub labels: Vec<miette::LabeledSpan>,
    #[help]
    pub help: Option<String>,
}

impl SourceDiagnostic {
    /// A single span labelled with the message itself.
    pub fn new(message: String, src: miette::NamedSource<String>, span: SourceSpan) -> Self {
        SourceDiagnostic {
            labels: vec![miette::LabeledSpan::new_with_span(
                Some(message.clone()),
                span,
            )],
            message,
            src,
            help: None,
        }
    }
}

/// Convenience alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;
