//! Error types for lets.
//!
//! All errors use [`miette::Diagnostic`] for rich terminal output with
//! source spans, colored labels, and help text.

use std::path::{Path, PathBuf};

use miette::SourceSpan;

/// All error types produced by lets.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum Error {
    #[error(
        "no lets.kdl found in {} or any parent directory",
        describe_dir(start_dir)
    )]
    #[diagnostic(help("run `lets self init` to create one"))]
    ConfigNotFound { start_dir: PathBuf },

    /// A config-file problem with source to point at. Boxed because it
    /// carries the file's text, and every `Result` in the crate would
    /// otherwise pay for it.
    #[error(transparent)]
    #[diagnostic(transparent)]
    Parse(Box<SourceDiagnostic>),

    /// A config error with nothing to point at. Only two places still
    /// produce one — config-level `env` values and var rendering — because
    /// neither vars nor config env carry spans in the tree. Everything
    /// else should use [`Error::at`].
    #[error("{message}")]
    #[diagnostic()]
    ParseNoSpan { message: String },

    /// A config error that knows where it happened but not which file.
    /// Parsers deep in the tree have the offending `KdlNode` but not the
    /// document text; the source is attached on the way out, where it is.
    #[error("{message}")]
    ParseAt {
        message: String,
        span: SourceSpan,
        help: Option<String>,
    },

    #[error("failed to read {}: {source}", display_path(path))]
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
    /// A config error pointing at the node that caused it.
    pub fn at(message: impl Into<String>, span: SourceSpan) -> Error {
        Error::ParseAt {
            message: message.into(),
            span,
            help: None,
        }
    }

    /// As [`Error::at`], plus a suggested fix.
    pub fn at_with_help(
        message: impl Into<String>,
        span: SourceSpan,
        help: impl Into<String>,
    ) -> Error {
        Error::ParseAt {
            message: message.into(),
            span,
            help: Some(help.into()),
        }
    }

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

/// A non-fatal notice. Rendered through the same handler as errors so
/// warnings and failures read as one system, and so colour detection —
/// `NO_COLOR`, redirected output — is decided in one place.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("{message}")]
#[diagnostic(severity(Warning))]
pub struct Warning {
    message: String,
    #[help]
    help: Option<String>,
}

impl Warning {
    pub fn new(message: impl Into<String>) -> Self {
        Warning {
            message: message.into(),
            help: None,
        }
    }

    pub fn with_help(message: impl Into<String>, help: impl Into<String>) -> Self {
        Warning {
            message: message.into(),
            help: Some(help.into()),
        }
    }

    /// Write to stderr, where it can't be mistaken for task output.
    pub fn emit(self) {
        let report: miette::Report = self.into();
        eprint!("{report:?}");
    }
}

/// Render a path the way the user would type it: relative to the working
/// directory when that is shorter, absolute when it isn't. Config paths are
/// resolved to absolutes internally, and a long one wraps across most of a
/// diagnostic's header.
pub fn display_path(path: &Path) -> String {
    let absolute = path.display().to_string();
    let Ok(cwd) = std::env::current_dir() else {
        return absolute;
    };
    let Ok(full) = std::path::absolute(path) else {
        return absolute;
    };
    match relative_to(&full, &cwd) {
        Some(rel) if rel.as_os_str().len() < absolute.len() => rel.display().to_string(),
        _ => absolute,
    }
}

/// Name a directory as the user thinks of it, so the common case reads as
/// "the current directory" rather than a path they'd have to compare.
fn describe_dir(dir: &Path) -> String {
    match std::env::current_dir() {
        Ok(cwd) if cwd == dir => "the current directory".to_string(),
        _ => display_path(dir),
    }
}

/// `path` expressed relative to `base`, walking up with `..` as needed.
fn relative_to(path: &Path, base: &Path) -> Option<PathBuf> {
    let mut ancestor = base;
    let mut ups = PathBuf::new();
    loop {
        if let Ok(rest) = path.strip_prefix(ancestor) {
            return Some(ups.join(rest));
        }
        ancestor = ancestor.parent()?;
        ups.push("..");
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
    /// One marked span, unannotated: the message already sits directly above
    /// the snippet, and repeating it under the underline both doubles the
    /// text and pushes long messages past the terminal width.
    pub fn new(message: String, src: miette::NamedSource<String>, span: SourceSpan) -> Self {
        SourceDiagnostic {
            message,
            src,
            labels: vec![miette::LabeledSpan::new_with_span(None, span)],
            help: None,
        }
    }
}

/// Convenience alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;
