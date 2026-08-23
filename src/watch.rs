//! Watch mode: re-run a command when its `sources` change.
//!
//! The supervisor re-executes the current binary (argv minus `--watch`) as a
//! child in its own process group and restarts it whenever a debounced
//! filesystem event matches the union of `sources` globs collected from the
//! target command's transitive task graph. Re-execing (rather than looping
//! in-process) gives each iteration a fresh run-once registry and picks up
//! config edits for free — the config file itself is always watched.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use clap::ArgMatches;
use globset::{Glob, GlobSet, GlobSetBuilder};
use notify_debouncer_full::{DebounceEventResult, new_debouncer, notify::RecursiveMode};

use crate::error::{Error, Result};
use crate::tree::{CommandNode, CommandTree};

/// Debounce window for filesystem events (coalesces editor save bursts).
const DEBOUNCE: Duration = Duration::from_millis(250);
/// Grace period between SIGTERM and SIGKILL when restarting the child.
const KILL_GRACE: Duration = Duration::from_secs(2);

enum Msg {
    /// Paths from a debounced filesystem event batch.
    Fs(Vec<PathBuf>),
    /// The child of the given generation exited on its own.
    ChildExit(u64, ExitStatus),
    /// Ctrl-C / SIGTERM received.
    Interrupt,
}

/// Run the watch supervisor for the command matched in `matches`.
pub fn run(tree: &CommandTree, matches: &ArgMatches, config_path: &Path) -> Result<()> {
    let Some((node, _, root_key)) = crate::exec::resolve(tree, matches) else {
        return Ok(());
    };

    // Union of sources across the transitive task graph.
    let mut patterns = Vec::new();
    collect_sources(tree, node, &mut patterns);
    if patterns.is_empty() {
        return Err(Error::Other(format!(
            "'{}' has no sources to watch (add a `sources \"glob\"` node to it \
             or one of its deps/steps)",
            root_key.join(" ")
        )));
    }

    // Re-prompting on every rerun is hostile; require non-interactive tasks.
    if !matches.get_flag("yes") && graph_has_interaction(tree, node) {
        return Err(Error::Other(
            "interactive prompts are not supported with --watch; \
             pass --yes (prompts use their defaults) or remove them"
                .to_string(),
        ));
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in &patterns {
        // Globs are validated at config load time.
        builder.add(Glob::new(pattern).map_err(|e| Error::Other(e.to_string()))?);
    }
    let globs = builder.build().map_err(|e| Error::Other(e.to_string()))?;

    let root = config_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    // Watchers report canonical paths (e.g. /private/var vs /var on macOS);
    // canonicalize so strip_prefix works when matching relative globs.
    let root = root.canonicalize().unwrap_or(root);

    // Config edits always trigger a restart (the child re-parses on start):
    // the root file plus everything pulled in via `include`, recursively.
    let config_files: Vec<PathBuf> = std::iter::once(config_path.to_path_buf())
        .chain(tree.includes.iter().cloned())
        .map(|p| p.canonicalize().unwrap_or(p))
        .collect();

    let (tx, rx) = mpsc::channel::<Msg>();

    // Filesystem watcher. Only mutations count: Linux inotify also reports
    // reads (Access events), and the re-exec'd child reads lets.kdl on every
    // start — reacting to reads would restart-loop forever.
    let fs_tx = tx.clone();
    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        if let Ok(events) = result {
            let paths: Vec<PathBuf> = events
                .into_iter()
                .filter(|e| e.kind.is_create() || e.kind.is_modify() || e.kind.is_remove())
                .flat_map(|e| e.paths.clone())
                .collect();
            if !paths.is_empty() {
                let _ = fs_tx.send(Msg::Fs(paths));
            }
        }
    })
    .map_err(|e| Error::Other(format!("failed to create file watcher: {e}")))?;
    debouncer
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| Error::Other(format!("failed to watch '{}': {e}", root.display())))?;
    // Included files can live outside the watch root; watch those directly.
    for config_file in &config_files {
        if !config_file.starts_with(&root) {
            debouncer
                .watch(config_file, RecursiveMode::NonRecursive)
                .map_err(|e| {
                    Error::Other(format!("failed to watch '{}': {e}", config_file.display()))
                })?;
        }
    }

    // Ctrl-C / SIGTERM.
    let int_tx = tx.clone();
    ctrlc::set_handler(move || {
        let _ = int_tx.send(Msg::Interrupt);
    })
    .map_err(|e| Error::Other(format!("failed to install signal handler: {e}")))?;

    let child_args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--watch")
        .collect();

    status(&format!(
        "watching {} pattern(s) under {}",
        patterns.len(),
        root.display()
    ));

    let mut generation: u64 = 0;
    loop {
        generation += 1;
        let mut child = spawn_child(&child_args)?;
        let pid = child.id();

        // Waiter thread reaps the child and reports its exit.
        let exit_tx = tx.clone();
        let generation_now = generation;
        std::thread::spawn(move || {
            if let Ok(exit_status) = child.wait() {
                let _ = exit_tx.send(Msg::ChildExit(generation_now, exit_status));
            }
        });

        let mut running = true;
        let restart = 'inner: loop {
            match rx.recv() {
                Ok(Msg::Fs(paths)) => {
                    let n = matching(&paths, &root, &globs, &config_files);
                    if n == 0 {
                        continue;
                    }
                    status(&format!("{n} change(s) detected, restarting"));
                    if running {
                        kill_child_group(pid, &rx, generation);
                    }
                    break 'inner true;
                }
                Ok(Msg::ChildExit(generated, exit_status)) if generated == generation => {
                    running = false;
                    let desc = match exit_status.code() {
                        Some(0) => "command finished".to_string(),
                        Some(code) => format!("command exited with {code}"),
                        None => "command terminated by signal".to_string(),
                    };
                    status(&format!("{desc}; waiting for changes"));
                }
                Ok(Msg::ChildExit(..)) => {} // stale generation, ignore
                Ok(Msg::Interrupt) | Err(_) => {
                    if running {
                        kill_child_group(pid, &rx, generation);
                    }
                    break 'inner false;
                }
            }
        };

        if !restart {
            return Ok(());
        }
    }
}

/// Spawn one iteration: this binary without `--watch`, in its own process
/// group so a restart can kill the entire task tree.
fn spawn_child(args: &[String]) -> Result<Child> {
    let exe = std::env::current_exe()
        .map_err(|e| Error::Other(format!("failed to locate lets binary: {e}")))?;
    let mut cmd = Command::new(exe);
    cmd.args(args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
        .map_err(|e| Error::Other(format!("failed to spawn command: {e}")))
}

/// SIGTERM the child's process group, wait up to the grace period for it to
/// exit (its waiter thread reports on the shared channel), then SIGKILL.
fn kill_child_group(pid: u32, rx: &mpsc::Receiver<Msg>, generation: u64) {
    signal_group(pid, false);
    let deadline = Instant::now() + KILL_GRACE;
    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            signal_group(pid, true);
            return;
        };
        match rx.recv_timeout(remaining) {
            Ok(Msg::ChildExit(generated, _)) if generated == generation => return,
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                signal_group(pid, true);
                return;
            }
        }
    }
}

fn signal_group(pid: u32, force: bool) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{self, Signal};
        use nix::unistd::Pid;
        let sig = if force {
            Signal::SIGKILL
        } else {
            Signal::SIGTERM
        };
        let _ = signal::killpg(Pid::from_raw(pid as i32), sig);
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, force);
    }
}

/// Count event paths matching the sources globs (or any config file).
fn matching(paths: &[PathBuf], root: &Path, globs: &GlobSet, config_files: &[PathBuf]) -> usize {
    paths
        .iter()
        .filter(|p| {
            let relative = p.strip_prefix(root).unwrap_or(p);
            if globs.is_match(relative) {
                return true;
            }
            // Config edits always trigger: the child re-parses on restart.
            config_files.iter().any(|c| c == *p)
        })
        .count()
}

/// Gather `sources` from a node and its transitive deps/steps.
fn collect_sources(tree: &CommandTree, node: &CommandNode, out: &mut Vec<String>) {
    for pattern in &node.sources {
        if !out.contains(pattern) {
            out.push(pattern.clone());
        }
    }
    for task_ref in node.orch.deps.iter().chain(&node.orch.steps) {
        if let Some((target, _)) = tree.resolve_ref(task_ref) {
            collect_sources(tree, target, out);
        }
    }
}

/// Whether any node in the transitive graph has prompts, chooses, or confirms.
fn graph_has_interaction(tree: &CommandTree, node: &CommandNode) -> bool {
    if node.interactive.confirm.is_some()
        || !node.interactive.prompts.is_empty()
        || !node.interactive.chooses.is_empty()
    {
        return true;
    }
    node.orch
        .deps
        .iter()
        .chain(&node.orch.steps)
        .any(|task_ref| {
            tree.resolve_ref(task_ref)
                .is_some_and(|(target, _)| graph_has_interaction(tree, target))
        })
}

/// Print a dim status line to stderr (never mixed into task output).
fn status(msg: &str) {
    eprintln!(
        "{}",
        crate::style::err(crate::style::DIM, format!("[watch] {msg}"))
    );
}
