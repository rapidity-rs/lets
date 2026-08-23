//! Dry-run plan rendering.
//!
//! A dry run is a narration, not an execution: it groups what each task
//! would do under that task's name and names the phase every command
//! belongs to, so a pipeline reads as a plan instead of a flat list of
//! shell strings with no attribution.
//!
//! The task header is emitted lazily by [`step`] rather than at task entry,
//! so a task that turns out to do nothing prints nothing.

use parking_lot::Mutex;

use crate::style::{self, DIM, NAME};

/// Which part of a task's lifecycle a line belongs to. Rendered in a fixed
/// column so the commands themselves line up.
#[derive(Clone, Copy)]
pub enum Phase {
    Precondition,
    Status,
    Sources,
    Before,
    Run,
    After,
    Defer,
}

impl Phase {
    fn label(self) -> &'static str {
        match self {
            Phase::Precondition => "precondition",
            Phase::Status => "status",
            Phase::Sources => "sources",
            Phase::Before => "before",
            Phase::Run => "run",
            Phase::After => "after",
            Phase::Defer => "defer",
        }
    }
}

/// Width of the phase column: the longest label plus a separating space.
const PHASE_WIDTH: usize = 13;

/// The task whose header was printed last, so each task is announced once.
/// Dry runs are serialized (see `Orchestrator::run_deps`), so this is a
/// narration order, not a race.
static CURRENT: Mutex<Option<String>> = Mutex::new(None);

/// Narrate one thing `task` would do.
pub fn step(task: &str, phase: Phase, detail: &str) {
    let mut current = CURRENT.lock();
    if current.as_deref() != Some(task) {
        // Blank line between tasks, but not before the first.
        if current.is_some() {
            println!();
        }
        println!("{}", style::out(NAME, task));
        *current = Some(task.to_string());
    }
    println!(
        "  {}{detail}",
        style::out(DIM, format!("{:<PHASE_WIDTH$}", phase.label()))
    );
}

/// Note that a task would be skipped, with the reason in place of a command.
pub fn skipped(task: &str, phase: Phase, reason: &str) {
    step(task, phase, &style::out(DIM, reason));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_labels_fit_their_column() {
        for phase in [
            Phase::Precondition,
            Phase::Status,
            Phase::Sources,
            Phase::Before,
            Phase::Run,
            Phase::After,
            Phase::Defer,
        ] {
            assert!(
                phase.label().len() < PHASE_WIDTH,
                "'{}' needs a wider column",
                phase.label()
            );
        }
    }
}
