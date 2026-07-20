//! Per-task output routing.
//!
//! When tasks run in parallel their output would interleave arbitrarily.
//! [`OutputMode`] selects a presentation strategy, and [`TaskSink`] is the
//! per-task consumer that shell execution writes through. New presentation
//! backends (a TUI, terminal-multiplexer panes) only need a new sink variant.

use std::io::{BufRead, BufReader, IsTerminal, PipeReader, Read, Write};
use std::str::FromStr;

use parking_lot::Mutex;

/// How output from tasks executed via deps/steps is presented.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputMode {
    /// Children inherit the terminal; lines from parallel tasks interleave.
    #[default]
    Interleaved,
    /// Buffer each task's output; print it as one labeled block when the task
    /// finishes.
    Group,
    /// Stream lines live, each prefixed with its task's label.
    Prefixed,
}

impl FromStr for OutputMode {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "interleaved" => Ok(OutputMode::Interleaved),
            "group" => Ok(OutputMode::Group),
            "prefixed" => Ok(OutputMode::Prefixed),
            other => Err(format!(
                "invalid output mode '{other}' (expected interleaved, group, or prefixed)"
            )),
        }
    }
}

/// ANSI color codes cycled across task labels in prefixed mode.
const LABEL_COLORS: [u8; 6] = [36, 35, 32, 33, 34, 96];

/// Colorize a task label when stdout is a terminal.
fn colorize(label: &str, color_seq: usize) -> String {
    if std::io::stdout().is_terminal() {
        let color = LABEL_COLORS[color_seq % LABEL_COLORS.len()];
        format!("\x1b[{color}m{label}\x1b[0m")
    } else {
        label.to_string()
    }
}

/// Where a task's process output goes.
pub enum TaskSink {
    /// Child inherits the parent's stdio.
    Inherit,
    /// Merged stdout+stderr streamed line-by-line with a label prefix.
    Prefix { prefix: String },
    /// Merged stdout+stderr collected and flushed as one block when the task
    /// settles. With `only_on_failure` the block is dropped on success
    /// (implements `silent`).
    Buffer {
        header: Option<String>,
        /// Plain (uncolored) task label, for CI fold markers.
        label: Option<String>,
        only_on_failure: bool,
        buf: Mutex<Vec<u8>>,
    },
}

impl TaskSink {
    /// Sink for a task label under the given mode. `silent` buffers output
    /// and shows it only when the task fails, regardless of mode.
    pub fn for_task(mode: OutputMode, label: &str, silent: bool, color_seq: usize) -> TaskSink {
        if silent {
            return TaskSink::Buffer {
                header: Some(format!("[{}]", colorize(label, color_seq))),
                label: Some(label.to_string()),
                only_on_failure: true,
                buf: Mutex::new(Vec::new()),
            };
        }
        match mode {
            OutputMode::Interleaved => TaskSink::Inherit,
            OutputMode::Group => TaskSink::Buffer {
                header: Some(format!("[{}]", colorize(label, color_seq))),
                label: Some(label.to_string()),
                only_on_failure: false,
                buf: Mutex::new(Vec::new()),
            },
            OutputMode::Prefixed => TaskSink::Prefix {
                prefix: format!("[{}] ", colorize(label, color_seq)),
            },
        }
    }

    /// Sink for the root command: never prefixed or grouped (it may be
    /// interactive), but `silent` still buffers and dumps on failure.
    pub fn for_root(silent: bool) -> TaskSink {
        if silent {
            TaskSink::Buffer {
                header: None,
                label: None,
                only_on_failure: true,
                buf: Mutex::new(Vec::new()),
            }
        } else {
            TaskSink::Inherit
        }
    }

    /// Write one lets-generated line (e.g. a command echo) through the sink,
    /// so it lands inside the task's block or prefix like process output.
    pub fn emit(&self, line: &str) {
        match self {
            TaskSink::Inherit => println!("{line}"),
            TaskSink::Prefix { prefix } => {
                let mut out = std::io::stdout().lock();
                let _ = out.write_all(prefix.as_bytes());
                let _ = out.write_all(line.as_bytes());
                let _ = out.write_all(b"\n");
            }
            TaskSink::Buffer { buf, .. } => {
                let mut buf = buf.lock();
                buf.extend_from_slice(line.as_bytes());
                buf.push(b'\n');
            }
        }
    }

    /// Whether child processes need their stdio piped through this sink.
    pub fn captures(&self) -> bool {
        !matches!(self, TaskSink::Inherit)
    }

    /// Drain a merged stdout+stderr pipe until EOF.
    pub fn consume(&self, reader: PipeReader) {
        match self {
            TaskSink::Inherit => {}
            TaskSink::Prefix { prefix } => {
                let mut reader = BufReader::new(reader);
                let mut line = Vec::new();
                loop {
                    line.clear();
                    match reader.read_until(b'\n', &mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            // One write per line so parallel tasks never tear
                            // mid-line.
                            let mut out = std::io::stdout().lock();
                            let _ = out.write_all(prefix.as_bytes());
                            let _ = out.write_all(&line);
                            if !line.ends_with(b"\n") {
                                let _ = out.write_all(b"\n");
                            }
                        }
                    }
                }
            }
            TaskSink::Buffer { buf, .. } => {
                let mut reader = reader;
                let mut chunk = Vec::new();
                if reader.read_to_end(&mut chunk).is_ok() {
                    buf.lock().extend_from_slice(&chunk);
                }
            }
        }
    }

    /// Called once when the task settles. Flushes buffered output.
    pub fn finish(&self, failed: bool) {
        if let TaskSink::Buffer {
            header,
            label,
            only_on_failure,
            buf,
        } = self
        {
            let buf = buf.lock();
            if buf.is_empty() || (*only_on_failure && !failed) {
                return;
            }
            // Fold labeled blocks on GitHub Actions so long task output
            // collapses in the log view.
            let fold = label
                .as_deref()
                .filter(|_| std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true"));
            // One locked write so the block stays contiguous across threads.
            let mut out = std::io::stdout().lock();
            if let Some(label) = fold {
                let _ = writeln!(out, "::group::{label}");
            }
            if let Some(header) = header {
                let _ = out.write_all(header.as_bytes());
                let _ = out.write_all(b"\n");
            }
            let _ = out.write_all(&buf);
            if !buf.ends_with(b"\n") {
                let _ = out.write_all(b"\n");
            }
            if fold.is_some() {
                let _ = writeln!(out, "::endgroup::");
            }
        }
    }
}
