//! Command orchestration.
//!
//! Resolves the matched subcommand from clap back to the command tree and
//! executes the full lifecycle: interactive prompts and confirmations
//! (collected up front, across the whole task graph), deps (parallel,
//! run-once), steps (sequential, run-once), before/after hooks, and run
//! commands.
//!
//! Delegates actual process spawning to [`crate::shell`] and placeholder
//! interpolation to [`crate::interpolate`].

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use clap::ArgMatches;
use parking_lot::{Condvar, Mutex};

use crate::error::{Error, Result};
use crate::fingerprint;
use crate::interpolate::{self, Placeholder, Resolution};
use crate::output::{OutputMode, TaskSink};
use crate::shell::{self, ExecContext, exec_shell};
use crate::tree::{CommandNode, CommandTree, FlagType, RunPolicy};

/// Resolve the matched subcommand from clap back to our tree and execute it.
pub fn run(tree: &CommandTree, matches: &ArgMatches, project_root: &Path) -> Result<()> {
    let Some((node, node_matches, root_key)) = resolve(tree, matches) else {
        return Ok(());
    };

    warn_deprecated(node);

    let yes = matches.get_flag("yes");
    let dry_run = matches.get_flag("dry-run");
    let force = matches.get_flag("force");
    let keep_going = matches.get_flag("keep-going");
    let summary = matches.get_flag("summary");
    let root_label = root_key.join(" ");
    let mut ctx = ExecContext::from_node(node, &root_label, &tree.config, project_root, dry_run)?;
    ctx.env.extend(export_env(node, Some(node_matches)));

    install_signal_handler();

    // Collect every prompt/choose/confirm in the transitive task graph up
    // front, serially, before any (parallel) execution starts. A declined
    // confirmation aborts the run before anything has executed.
    let mut orch = Orchestrator {
        tree,
        registry: Registry::new(),
        interactions: HashMap::new(),
        mode: tree.config.output,
        color_seq: AtomicUsize::new(0),
        dry_run,
        force,
        keep_going,
        project_root: project_root.to_path_buf(),
        permits: Semaphore::new(tree.config.jobs),
        summary: Mutex::new(Vec::new()),
    };
    let mut visited = HashSet::new();
    orch.collect_interaction(node, &root_key, yes, &mut visited)?;

    let interactive_vars = orch
        .interactions
        .get(&root_key)
        .cloned()
        .unwrap_or_default();

    // Every template renders up front: a task whose hooks, gates, or
    // cleanup can't render must fail before anything executes.
    let rendered = Renderer {
        node,
        matches: Some(node_matches),
        vars: &interactive_vars,
        env: &ctx.env,
        config_shell: tree.config.shell.as_deref(),
        root: project_root,
    }
    .render_task()?;

    let total_start = Instant::now();
    let result = (|| -> Result<()> {
        // 1. Preconditions gate the whole task, before any work starts.
        check_preconditions(&rendered.preconditions, &node.name, &ctx, dry_run)?;

        // 2. Parallel deps (run-once).
        orch.run_deps(node)?;

        // 3. Sequential steps (run-once).
        orch.run_steps(node)?;

        // 4. Status checks: deps may have satisfied them, so they run after.
        if is_up_to_date(&rendered.status, &ctx, dry_run, force)? {
            report_up_to_date(&root_label);
            orch.record(&root_label, Outcome::UpToDate, Duration::ZERO);
            return Ok(());
        }

        // 5. Sources fingerprint: skip when inputs are unchanged since the
        // last successful run.
        let freshness = check_freshness(project_root, &root_key, node, dry_run, force)?;
        if matches!(freshness, fingerprint::Freshness::Current) {
            report_up_to_date(&root_label);
            orch.record(&root_label, Outcome::UpToDate, Duration::ZERO);
            return Ok(());
        }

        // 6. Hooks and main commands. The root command is never prefixed or
        // grouped (it may be interactive); silent still buffers until failure.
        let body_start = Instant::now();
        let sink = TaskSink::for_root(node.exec.silent);
        let result = exec_task_commands(&rendered, &ctx, &sink);
        // Cleanup runs whenever the task reached its body — success, failure,
        // or interrupt.
        run_defers(&rendered.defers, &ctx, &sink);
        sink.finish(result.is_err());
        record_freshness(project_root, &root_key, freshness, result.is_ok());
        let outcome = if result.is_ok() {
            Outcome::Success
        } else {
            Outcome::Failed
        };
        orch.record(&root_label, outcome, body_start.elapsed());
        result
    })();

    if summary {
        orch.print_summary(total_start.elapsed());
    }
    result
}

/// Evaluate the sources fingerprint unless bypassed by --force or dry-run
/// (which prints a preview line instead).
fn check_freshness(
    project_root: &Path,
    key: &[String],
    node: &CommandNode,
    dry_run: bool,
    force: bool,
) -> Result<fingerprint::Freshness> {
    if node.sources.is_empty() || force {
        return Ok(fingerprint::Freshness::NoInputs);
    }
    if dry_run {
        println!(
            "[dry-run] fingerprint: {} source pattern(s)",
            node.sources.len()
        );
        return Ok(fingerprint::Freshness::NoInputs);
    }
    fingerprint::check(project_root, key, node)
}

/// Persist the fingerprint after a successful run. Cache trouble is a
/// warning, never a failure — fingerprinting is an optimization.
fn record_freshness(
    project_root: &Path,
    key: &[String],
    freshness: fingerprint::Freshness,
    succeeded: bool,
) {
    if succeeded
        && let fingerprint::Freshness::Stale(digest) = freshness
        && let Err(e) = fingerprint::record(project_root, key, &digest)
    {
        eprintln!("\x1b[33mwarning:\x1b[0m could not record fingerprint: {e}");
    }
}

/// Print a deprecation warning if the node is marked deprecated.
fn warn_deprecated(node: &CommandNode) {
    if let Some(msg) = &node.deprecated {
        if msg.is_empty() {
            eprintln!("\x1b[33mwarning:\x1b[0m '{}' is deprecated", node.name);
        } else {
            eprintln!(
                "\x1b[33mwarning:\x1b[0m '{}' is deprecated. {msg}",
                node.name
            );
        }
    }
}

/// Set once the process receives SIGINT/SIGTERM; the run unwinds with
/// command failures, defers still execute, and main exits 130.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Whether this run was interrupted by a signal.
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// First SIGINT/SIGTERM: flag the run and forward SIGTERM to children in
/// their own process groups (the terminal only reaches our own group).
/// Second signal: exit immediately.
fn install_signal_handler() {
    let result = ctrlc::set_handler(|| {
        if INTERRUPTED.swap(true, Ordering::SeqCst) {
            std::process::exit(130);
        }
        shell::terminate_process_groups();
    });
    if let Err(e) = result {
        eprintln!("\x1b[33mwarning:\x1b[0m could not install signal handler: {e}");
    }
}

/// Run a task's rendered defer commands in reverse declaration order (LIFO).
/// Defer failures warn but never change the task's result.
fn run_defers(defers: &[String], ctx: &ExecContext, sink: &TaskSink) {
    for defer in defers.iter().rev() {
        if let Err(e) = exec_shell(defer, ctx, sink) {
            // The defer text is already in the message; report only how it
            // ended, not the task-shaped error wrapping it.
            let detail = match &e {
                Error::CommandFailed { code, .. } => format!("exit code {code}"),
                Error::CommandSignaled { .. } => "terminated by a signal".to_string(),
                other => other.to_string(),
            };
            eprintln!("\x1b[33mwarning:\x1b[0m defer `{defer}` failed ({detail})");
        }
    }
}

/// Fail fast when any precondition exits non-zero. Dry-run only previews.
fn check_preconditions(
    preconditions: &[(String, Option<String>)],
    task: &str,
    ctx: &ExecContext,
    dry_run: bool,
) -> Result<()> {
    for (cmd, message) in preconditions {
        if dry_run {
            println!("[dry-run] precondition: {cmd}");
            continue;
        }
        if !shell::check_shell(cmd, ctx)? {
            let detail = message.clone().unwrap_or_else(|| format!("`{cmd}` failed"));
            return Err(Error::Other(format!(
                "precondition not met for '{task}': {detail}"
            )));
        }
    }
    Ok(())
}

/// Canonical identity for a task reference: the command path plus parsed
/// argument values in declaration order. `build -j 8 --release` and
/// `build --release -j 8` are one task, and `build` equals `build -j 4`
/// when 4 is the declared default.
fn canonical_key(target: &CommandNode, tokens: &[String], args: &[String]) -> Vec<String> {
    let path = &tokens[..tokens.len() - args.len()];
    if target.args.is_empty() && target.flags.is_empty() {
        return path.to_vec();
    }

    let argv = std::iter::once(target.name.clone()).chain(args.iter().cloned());
    let Ok(matches) = crate::cli::build_subcommand(target, false).try_get_matches_from(argv) else {
        // Load-time validation makes this unreachable; fall back to raw tokens.
        return tokens.to_vec();
    };

    let mut key = path.to_vec();
    for arg in &target.args {
        if let Some(value) = get_value(target, &matches, &arg.name) {
            key.push(format!("{}={value}", arg.name));
        }
    }
    for flag in &target.flags {
        if flag.value_type.is_none() {
            if matches.get_flag(&flag.name) {
                key.push(format!("{}=true", flag.name));
            }
        } else if let Some(value) = get_value(target, &matches, &flag.name) {
            key.push(format!("{}={value}", flag.name));
        }
    }
    key
}

/// A task with status checks is up to date when ALL of them exit zero.
/// Dry-run previews the checks and never skips; --force disables skipping.
fn is_up_to_date(status: &[String], ctx: &ExecContext, dry_run: bool, force: bool) -> Result<bool> {
    if status.is_empty() {
        return Ok(false);
    }
    if dry_run {
        for check in status {
            println!("[dry-run] status: {check}");
        }
        return Ok(false);
    }
    if force {
        return Ok(false);
    }
    for check in status {
        if !shell::check_shell(check, ctx)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Dim stderr notice for a skipped task (never mixed into task output).
fn report_up_to_date(label: &str) {
    eprintln!("\x1b[2m'{label}' is up to date\x1b[0m");
}

/// Shared state for one invocation's task graph execution.
struct Orchestrator<'a> {
    tree: &'a CommandTree,
    registry: Registry,
    /// Interactive variable bindings collected up front, keyed by task key.
    interactions: HashMap<Vec<String>, HashMap<String, String>>,
    /// Output presentation for tasks run via deps/steps.
    mode: OutputMode,
    /// Monotonic counter assigning label colors.
    color_seq: AtomicUsize,
    dry_run: bool,
    /// Ignore status checks (--force): tasks run even when up to date.
    force: bool,
    /// Continue past step failures and report them all (--keep-going).
    keep_going: bool,
    /// Directory containing the config file; sources/generates and the
    /// fingerprint cache are relative to it.
    project_root: PathBuf,
    /// Caps concurrently executing task bodies (config jobs / --jobs).
    permits: Semaphore,
    /// Rows for the --summary table, in completion order.
    summary: Mutex<Vec<SummaryRow>>,
}

/// How a task settled, for the --summary table.
#[derive(Clone, Copy)]
enum Outcome {
    Success,
    Failed,
    UpToDate,
}

/// One row of the --summary table.
struct SummaryRow {
    label: String,
    outcome: Outcome,
    duration: Duration,
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs >= 60.0 {
        format!("{}m{:02}s", d.as_secs() / 60, d.as_secs() % 60)
    } else if secs >= 1.0 {
        format!("{secs:.1}s")
    } else {
        format!("{}ms", d.as_millis())
    }
}

/// Collapse per-task results into one report. A lone failure with nothing
/// left unrun passes through untouched — it already names its task, and an
/// aggregate wrapper would only add a layer. Anything else aggregates, so
/// the user sees every failure and everything skipped in one place.
fn settle(results: Vec<(String, Result<()>)>, skipped: &[String]) -> Result<()> {
    let mut failures: Vec<Error> = results
        .into_iter()
        .filter_map(|(label, r)| r.err().map(|e| attribute(&label, e)))
        .collect();

    // A shared dependency that failed once is reported once: drop the
    // "failed earlier" markers its other referrers produced.
    let reported: HashSet<String> = failures
        .iter()
        .filter_map(|e| match e {
            Error::CommandFailed { task, .. } | Error::CommandSignaled { task, .. } => {
                Some(task.clone())
            }
            _ => None,
        })
        .collect();
    failures.retain(|e| !matches!(e, Error::DependencyFailed { task } if reported.contains(task)));

    if failures.is_empty() {
        return Ok(());
    }
    if failures.len() == 1 && skipped.is_empty() {
        return Err(failures.remove(0));
    }
    Err(Error::TasksFailed {
        count: failures.len(),
        failures,
        skipped: skipped_note(skipped),
    })
}

/// Name the task on errors that don't already identify themselves (env-file
/// trouble, interpolation failures), so every failure in an aggregate
/// report says where it came from.
fn attribute(label: &str, err: Error) -> Error {
    if err.names_task() {
        err
    } else {
        Error::Other(format!("task '{label}': {err}"))
    }
}

/// Steps after a fail-fast failure never ran; say so rather than leaving
/// the user to infer it from missing output.
fn skipped_note(skipped: &[String]) -> Option<String> {
    if skipped.is_empty() {
        return None;
    }
    Some(format!("did not run: {}", skipped.join(", ")))
}

/// Minimal counting semaphore. Permits wrap only a task's command phase —
/// never the deps/steps recursion — so a parent waiting on children can
/// never deadlock the pool.
struct Semaphore {
    /// None = unlimited.
    state: Option<(Mutex<usize>, Condvar)>,
}

impl Semaphore {
    fn new(limit: Option<usize>) -> Self {
        Semaphore {
            state: limit.map(|n| (Mutex::new(n), Condvar::new())),
        }
    }

    fn acquire(&self) -> SemaphoreGuard<'_> {
        if let Some((available, cv)) = &self.state {
            let mut available = available.lock();
            while *available == 0 {
                cv.wait(&mut available);
            }
            *available -= 1;
        }
        SemaphoreGuard { semaphore: self }
    }
}

struct SemaphoreGuard<'a> {
    semaphore: &'a Semaphore,
}

impl Drop for SemaphoreGuard<'_> {
    fn drop(&mut self) {
        if let Some((available, cv)) = &self.semaphore.state {
            *available.lock() += 1;
            cv.notify_one();
        }
    }
}

impl Orchestrator<'_> {
    /// Depth-first walk of the transitive task graph (deps, then steps, then
    /// the node itself) running every prompt/choose/confirm serially.
    ///
    /// Interaction must happen before execution: deps run on parallel threads
    /// that cannot share a terminal, and a user declining a confirmation
    /// should abort the run before any work has started.
    fn collect_interaction(
        &mut self,
        node: &CommandNode,
        key: &[String],
        yes: bool,
        visited: &mut HashSet<Vec<String>>,
    ) -> Result<()> {
        if !visited.insert(key.to_vec()) {
            return Ok(());
        }

        for task_ref in node.orch.deps.iter().chain(&node.orch.steps) {
            let (target, args) = self
                .tree
                .resolve_ref(task_ref)
                .ok_or_else(|| Error::Other(format!("task '{}' not found", task_ref.display())))?;
            let child_key = canonical_key(target, &task_ref.tokens, args);
            self.collect_interaction(target, &child_key, yes, visited)?;
        }

        let vars = run_interactive(node, yes)?;

        // Confirmations are skipped in dry-run (nothing will execute) and
        // auto-accepted by --yes. Without a TTY, dialoguer fails — a command
        // guarded by `confirm` never silently runs.
        if let Some(confirm_msg) = &node.interactive.confirm
            && !self.dry_run
            && !yes
        {
            let rendered = interpolate_simple(
                confirm_msg,
                &vars,
                node,
                self.tree.config.shell.as_deref(),
                &self.project_root,
            )?;
            let confirmed = dialoguer::Confirm::new()
                .with_prompt(&rendered)
                .default(false)
                .interact()
                .map_err(|e| Error::Other(format!("prompt failed: {e}")))?;
            if !confirmed {
                return Err(Error::Other("aborted by user".to_string()));
            }
        }

        if !vars.is_empty() {
            self.interactions.insert(key.to_vec(), vars);
        }
        Ok(())
    }

    /// Run a referenced task — at most once per invocation unless its
    /// run-policy is "always".
    fn run_task(&self, key: &[String], node: &CommandNode, args: &[String]) -> Result<()> {
        if node.run_policy == RunPolicy::Always {
            return self.exec_node(node, key, args);
        }
        match self.registry.claim(key) {
            Claim::Done => Ok(()),
            Claim::Failed => Err(Error::DependencyFailed {
                task: key.join(" "),
            }),
            Claim::Run => {
                let mut guard = ClaimGuard {
                    registry: &self.registry,
                    key,
                    ok: false,
                };
                let result = self.exec_node(node, key, args);
                guard.ok = result.is_ok();
                result
            }
        }
    }

    /// Run all deps in parallel using scoped threads. Every dep runs to
    /// completion; a single failure passes through unchanged, several
    /// aggregate into one report.
    fn run_deps(&self, node: &CommandNode) -> Result<()> {
        if node.orch.deps.is_empty() {
            return Ok(());
        }

        let results: Vec<(String, Result<()>)> = std::thread::scope(|s| {
            let handles: Vec<_> = node
                .orch
                .deps
                .iter()
                .map(|task_ref| {
                    let handle = s.spawn(|| {
                        let (dep_node, args) =
                            self.tree.resolve_ref(task_ref).ok_or_else(|| {
                                Error::Other(format!("dep '{}' not found", task_ref.display()))
                            })?;
                        let key = canonical_key(dep_node, &task_ref.tokens, args);
                        self.run_task(&key, dep_node, args)
                    });
                    (task_ref.display(), handle)
                })
                .collect();

            handles
                .into_iter()
                .map(|(label, handle)| {
                    let result = handle.join().unwrap_or_else(|_| {
                        Err(Error::Other("dependency thread panicked".to_string()))
                    });
                    (label, result)
                })
                .collect()
        });

        settle(results, &[])
    }

    /// Run all steps sequentially. Fails on the first error unless
    /// --keep-going, which runs the remaining steps and reports every
    /// failure at once.
    fn run_steps(&self, node: &CommandNode) -> Result<()> {
        let mut results = Vec::new();
        let mut skipped = Vec::new();
        for (i, task_ref) in node.orch.steps.iter().enumerate() {
            let (step_node, args) = self
                .tree
                .resolve_ref(task_ref)
                .ok_or_else(|| Error::Other(format!("step '{}' not found", task_ref.display())))?;
            let key = canonical_key(step_node, &task_ref.tokens, args);
            let result = self.run_task(&key, step_node, args);
            let failed = result.is_err();
            results.push((task_ref.display(), result));
            if failed && !self.keep_going {
                skipped = node.orch.steps[i + 1..]
                    .iter()
                    .map(|r| r.display())
                    .collect();
                break;
            }
        }
        settle(results, &skipped)
    }

    /// Append one row to the --summary table.
    fn record(&self, label: &str, outcome: Outcome, duration: Duration) {
        self.summary.lock().push(SummaryRow {
            label: label.to_string(),
            outcome,
            duration,
        });
    }

    /// Print the per-task status and timing table (--summary) to stderr.
    fn print_summary(&self, total: Duration) {
        let rows = self.summary.lock();
        if rows.is_empty() {
            return;
        }
        let width = rows.iter().map(|r| r.label.len()).max().unwrap_or(0);
        eprintln!();
        for row in rows.iter() {
            let (mark, note) = match row.outcome {
                Outcome::Success => ("\x1b[32m✓\x1b[0m", format_duration(row.duration)),
                Outcome::Failed => ("\x1b[31m✗\x1b[0m", format_duration(row.duration)),
                Outcome::UpToDate => ("\x1b[2m-\x1b[0m", "up to date".to_string()),
            };
            eprintln!("  {mark} {label:<width$}  {note}", label = row.label);
        }
        eprintln!("  total: {}", format_duration(total));
    }

    /// Execute a command node invoked via deps/steps.
    /// With reference arguments, they are parsed by the target's own clap
    /// definition; otherwise interpolation falls back to declared defaults.
    fn exec_node(&self, node: &CommandNode, key: &[String], args: &[String]) -> Result<()> {
        warn_deprecated(node);

        // Validated at load time; parse again here to get real ArgMatches.
        let matches = if args.is_empty() {
            None
        } else {
            let argv = std::iter::once(node.name.clone()).chain(args.iter().cloned());
            let parsed = crate::cli::build_subcommand(node, false)
                .try_get_matches_from(argv)
                .map_err(|e| {
                    Error::Other(format!(
                        "invalid arguments for task '{}': {e}",
                        key.join(" ")
                    ))
                })?;
            Some(parsed)
        };

        let label = key.join(" ");
        let mut ctx = ExecContext::from_node(
            node,
            &label,
            &self.tree.config,
            &self.project_root,
            self.dry_run,
        )?;
        ctx.env.extend(export_env(node, matches.as_ref()));

        // Every template renders up front, before any of the task's work.
        let empty = HashMap::new();
        let vars = self.interactions.get(key).unwrap_or(&empty);
        let rendered = Renderer {
            node,
            matches: matches.as_ref(),
            vars,
            env: &ctx.env,
            config_shell: self.tree.config.shell.as_deref(),
            root: &self.project_root,
        }
        .render_task()?;

        // Preconditions gate the task before any of its work starts.
        check_preconditions(&rendered.preconditions, &node.name, &ctx, self.dry_run)?;

        // Nested deps and steps run first; they present through their own sinks.
        self.run_deps(node)?;
        self.run_steps(node)?;

        // Status checks may have been satisfied by deps, so they run after.
        if is_up_to_date(&rendered.status, &ctx, self.dry_run, self.force)? {
            report_up_to_date(&label);
            self.record(&label, Outcome::UpToDate, Duration::ZERO);
            return Ok(());
        }

        // Sources fingerprint: skip when inputs are unchanged.
        let freshness = check_freshness(&self.project_root, key, node, self.dry_run, self.force)?;
        if matches!(freshness, fingerprint::Freshness::Current) {
            report_up_to_date(&label);
            self.record(&label, Outcome::UpToDate, Duration::ZERO);
            return Ok(());
        }

        // This task's own output (hooks + run commands) goes through one sink
        // so grouped mode flushes it as a single block. The permit caps how
        // many task bodies execute at once.
        let body_start = Instant::now();
        let color_seq = self.color_seq.fetch_add(1, Ordering::Relaxed);
        let sink = TaskSink::for_task(self.mode, &label, node.exec.silent, color_seq);
        let permit = self.permits.acquire();
        let result = exec_task_commands(&rendered, &ctx, &sink);
        drop(permit);
        // Cleanup runs whenever the task reached its body — success,
        // failure, or interrupt.
        run_defers(&rendered.defers, &ctx, &sink);
        sink.finish(result.is_err());
        record_freshness(&self.project_root, key, freshness, result.is_ok());
        let outcome = if result.is_ok() {
            Outcome::Success
        } else {
            Outcome::Failed
        };
        self.record(&label, outcome, body_start.elapsed());
        result
    }
}

/// Hooks and run commands of a single rendered task, routed through its sink.
fn exec_task_commands(rendered: &RenderedTask, ctx: &ExecContext, sink: &TaskSink) -> Result<()> {
    if let Some(before) = &rendered.before {
        exec_shell(before, ctx, sink)?;
    }
    for command in &rendered.run {
        exec_shell(command, ctx, sink)?;
    }
    if let Some(after) = &rendered.after {
        exec_shell(after, ctx, sink)?;
    }
    Ok(())
}

/// Fully interpolated shell strings for one task invocation.
struct RenderedTask {
    run: Vec<String>,
    before: Option<String>,
    after: Option<String>,
    defers: Vec<String>,
    preconditions: Vec<(String, Option<String>)>,
    status: Vec<String>,
}

/// Placeholder resolution for one task invocation.
///
/// With `matches` (command line, or a reference carrying arguments), args
/// and flags resolve through clap — defaults included. Without, declared
/// defaults apply and boolean conditionals resolve to their unset state.
/// Unresolvable placeholders are errors, never silent empty strings.
struct Renderer<'a> {
    node: &'a CommandNode,
    matches: Option<&'a ArgMatches>,
    /// Interactive bindings (prompt/choose results).
    vars: &'a HashMap<String, String>,
    /// The task's resolved environment (env-file + env), for `{$VAR}`.
    env: &'a [(String, String)],
    /// Config shell + project root, for lazy dynamic-var evaluation.
    config_shell: Option<&'a str>,
    root: &'a Path,
}

impl Renderer<'_> {
    /// Render every shell-bound template of the node up front, before any
    /// execution: a task whose cleanup or gates can't render must fail
    /// before it starts, not midway through.
    fn render_task(&self) -> Result<RenderedTask> {
        Ok(RenderedTask {
            run: self
                .node
                .run
                .resolve()
                .iter()
                .map(|c| self.render(c))
                .collect::<Result<_>>()?,
            before: self
                .node
                .orch
                .before
                .as_deref()
                .map(|s| self.render(s))
                .transpose()?,
            after: self
                .node
                .orch
                .after
                .as_deref()
                .map(|s| self.render(s))
                .transpose()?,
            defers: self
                .node
                .orch
                .defers
                .iter()
                .map(|s| self.render(s))
                .collect::<Result<_>>()?,
            preconditions: self
                .node
                .preconditions
                .iter()
                .map(|p| Ok((self.render(&p.cmd)?, p.message.clone())))
                .collect::<Result<_>>()?,
            status: self
                .node
                .status
                .iter()
                .map(|s| self.render(s))
                .collect::<Result<_>>()?,
        })
    }

    fn render(&self, template: &str) -> Result<String> {
        interpolate::render(template, |p| self.resolve(p))
            .map_err(|e| Error::Other(format!("in task '{}': {e}", self.node.name)))
    }

    fn resolve(&self, p: Placeholder<'_>) -> Resolution {
        match p {
            // Passthrough args are quoted per token: `lets test -- "foo bar"`
            // must reach the child as one argument.
            Placeholder::Passthrough => {
                let Some(matches) = self.matches else {
                    return Resolution::Skip;
                };
                match matches.get_many::<String>("--") {
                    Some(trailing) => {
                        let quoted: Vec<String> =
                            trailing.map(|s| interpolate::shell_quote(s)).collect();
                        Resolution::Value(quoted.join(" "))
                    }
                    None => Resolution::Skip,
                }
            }
            Placeholder::EnvVar(name) => self
                .env
                .iter()
                .rev()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
                .or_else(|| std::env::var(name).ok())
                .map_or(Resolution::Skip, Resolution::Value),
            // Presence test: boolean flag set, optional/rest arg provided,
            // or no-default valued flag provided (incl. via env fallback).
            Placeholder::Conditional(name, text) => {
                if !self.node.presence_testable(name) {
                    return Resolution::Unknown;
                }
                let is_bool_flag = self
                    .node
                    .flags
                    .iter()
                    .any(|f| f.name == name && f.value_type.is_none());
                let set = match self.matches {
                    Some(m) if is_bool_flag => m.get_flag(name),
                    Some(m) => m.contains_id(name),
                    None => false,
                };
                if set {
                    Resolution::Value(text.to_string())
                } else {
                    Resolution::Skip
                }
            }
            Placeholder::Variable(name) => {
                if let Some(value) = self.vars.get(name) {
                    return Resolution::Value(value.clone());
                }
                let from_cli = match self.matches {
                    Some(matches) => get_value(self.node, matches, name),
                    None => {
                        if let Some(arg) = self.node.args.iter().find(|a| a.name == name) {
                            // Rest args are list-like: absent means empty.
                            if arg.rest {
                                Some(arg.default.clone().unwrap_or_default())
                            } else {
                                arg.default.clone()
                            }
                        } else {
                            self.node
                                .flags
                                .iter()
                                .find(|f| f.name == name && f.value_type.is_some())
                                .and_then(|f| f.default.clone())
                        }
                    }
                };
                if let Some(value) = from_cli {
                    return Resolution::Value(value);
                }
                match self.node.lookup_var(name) {
                    Some(def) => {
                        match shell::resolve_var(name, def, self.config_shell, self.root) {
                            Ok(v) => Resolution::Value(v),
                            Err(msg) => Resolution::Error(msg),
                        }
                    }
                    None => Resolution::Unknown,
                }
            }
        }
    }
}

/// Environment exports mirroring the task's CLI values: `LETS_ARG_<NAME>`
/// and `LETS_FLAG_<NAME>` (uppercased, `-` → `_`). Boolean flags export "1"
/// only when set, so `${LETS_FLAG_X:+...}` idioms work; values are exact,
/// letting scripts quote them safely instead of splicing placeholders.
fn export_env(node: &CommandNode, matches: Option<&ArgMatches>) -> Vec<(String, String)> {
    fn mangle(prefix: &str, name: &str) -> String {
        format!("{prefix}{}", name.to_uppercase().replace('-', "_"))
    }

    let mut out = Vec::new();
    for arg in &node.args {
        let value = match matches {
            Some(m) => {
                if arg.rest {
                    // Raw space-join for env consumption (no shell quoting).
                    m.get_many::<String>(&arg.name)
                        .map(|vals| vals.cloned().collect::<Vec<_>>().join(" "))
                } else {
                    typed_one(m, &arg.name, arg.value_type.as_ref())
                }
            }
            None => arg.default.clone(),
        };
        if let Some(v) = value {
            out.push((mangle("LETS_ARG_", &arg.name), v));
        }
    }
    for flag in &node.flags {
        if flag.value_type.is_none() {
            if matches.is_some_and(|m| m.get_flag(&flag.name)) {
                out.push((mangle("LETS_FLAG_", &flag.name), "1".to_string()));
            }
        } else {
            let value = match matches {
                Some(m) => get_value(node, m, &flag.name),
                None => flag.default.clone(),
            };
            if let Some(v) = value {
                out.push((mangle("LETS_FLAG_", &flag.name), v));
            }
        }
    }
    out
}

/// Process interactive prompts and choices, returning variable bindings.
fn run_interactive(node: &CommandNode, yes: bool) -> Result<HashMap<String, String>> {
    let mut vars = HashMap::new();

    // Process choose nodes first (so confirm can reference them).
    for choose in &node.interactive.chooses {
        let value = if yes {
            // Never guess a choice non-interactively: require an explicit
            // default rather than silently picking the first option.
            choose.default.clone().ok_or_else(|| {
                Error::Other(format!(
                    "choose '{}' has no default; cannot run with --yes \
                     (add default=\"...\" to the choose node)",
                    choose.name
                ))
            })?
        } else {
            let cursor = choose
                .default
                .as_ref()
                .and_then(|d| choose.choices.iter().position(|c| c == d))
                .unwrap_or(0);
            let selection = dialoguer::Select::new()
                .with_prompt(&choose.name)
                .items(&choose.choices)
                .default(cursor)
                .interact()
                .map_err(|e| Error::Other(format!("choose failed: {e}")))?;
            choose.choices[selection].clone()
        };
        vars.insert(choose.name.clone(), value);
    }

    // Process prompt nodes.
    for prompt in &node.interactive.prompts {
        let value = if yes {
            prompt.default.clone().unwrap_or_default()
        } else {
            let mut p = dialoguer::Input::<String>::new().with_prompt(&prompt.message);
            if let Some(default) = &prompt.default {
                p = p.default(default.clone());
            }
            p.interact_text()
                .map_err(|e| Error::Other(format!("prompt failed: {e}")))?
        };
        vars.insert(prompt.name.clone(), value);
    }

    Ok(vars)
}

/// Interpolation for confirm messages: interactive bindings first, then the
/// node's config vars, then `{$VAR}` from the process environment.
fn interpolate_simple(
    template: &str,
    vars: &HashMap<String, String>,
    node: &CommandNode,
    config_shell: Option<&str>,
    root: &Path,
) -> Result<String> {
    interpolate::render(template, |p| match p {
        Placeholder::Variable(name) => {
            if let Some(value) = vars.get(name) {
                return Resolution::Value(value.clone());
            }
            match node.lookup_var(name) {
                Some(def) => match shell::resolve_var(name, def, config_shell, root) {
                    Ok(v) => Resolution::Value(v),
                    Err(msg) => Resolution::Error(msg),
                },
                None => Resolution::Unknown,
            }
        }
        Placeholder::EnvVar(name) => {
            std::env::var(name).map_or(Resolution::Skip, Resolution::Value)
        }
        _ => Resolution::Unknown,
    })
    .map_err(|e| Error::Other(format!("in confirm of '{}': {e}", node.name)))
}

/// Execution state of a task tracked by the [`Registry`].
#[derive(Clone, Copy, PartialEq)]
enum TaskState {
    Running,
    Done,
    Failed,
}

/// Outcome of claiming a task for execution.
enum Claim {
    /// Caller owns execution and must report completion.
    Run,
    /// Task already completed successfully.
    Done,
    /// Task already ran and failed.
    Failed,
}

/// Tracks tasks that have started or finished during this invocation so each
/// unique task runs at most once, no matter how many places reference it.
/// Concurrent claims of an in-flight task block until it settles.
struct Registry {
    states: Mutex<HashMap<Vec<String>, TaskState>>,
    cv: Condvar,
}

impl Registry {
    fn new() -> Self {
        Registry {
            states: Mutex::new(HashMap::new()),
            cv: Condvar::new(),
        }
    }

    /// Claim a task for execution, blocking while another thread runs it.
    fn claim(&self, key: &[String]) -> Claim {
        let mut states = self.states.lock();
        loop {
            match states.get(key) {
                None => {
                    states.insert(key.to_vec(), TaskState::Running);
                    return Claim::Run;
                }
                Some(TaskState::Running) => {
                    self.cv.wait(&mut states);
                }
                Some(TaskState::Done) => return Claim::Done,
                Some(TaskState::Failed) => return Claim::Failed,
            }
        }
    }

    fn complete(&self, key: &[String], ok: bool) {
        let state = if ok {
            TaskState::Done
        } else {
            TaskState::Failed
        };
        self.states.lock().insert(key.to_vec(), state);
        self.cv.notify_all();
    }
}

/// Marks the claimed task as settled on drop, so waiters can never deadlock
/// even if execution panics. Defaults to failure until told otherwise.
struct ClaimGuard<'a> {
    registry: &'a Registry,
    key: &'a [String],
    ok: bool,
}

impl Drop for ClaimGuard<'_> {
    fn drop(&mut self) {
        self.registry.complete(self.key, self.ok);
    }
}

/// Walk the ArgMatches subcommand chain to find the deepest matched
/// CommandNode, along with its full command path.
pub(crate) fn resolve<'a>(
    tree: &'a CommandTree,
    matches: &'a ArgMatches,
) -> Option<(&'a CommandNode, &'a ArgMatches, Vec<String>)> {
    let (name, sub_matches) = matches.subcommand()?;
    let node = tree.commands.iter().find(|c| c.name == name)?;
    let mut path = vec![name.to_string()];
    resolve_node(node, sub_matches, &mut path).map(|(n, m)| (n, m, path))
}

fn resolve_node<'a>(
    node: &'a CommandNode,
    matches: &'a ArgMatches,
    path: &mut Vec<String>,
) -> Option<(&'a CommandNode, &'a ArgMatches)> {
    if let Some((child_name, child_matches)) = matches.subcommand()
        && let Some(child) = node.children.iter().find(|c| c.name == child_name)
    {
        path.push(child_name.to_string());
        return resolve_node(child, child_matches, path);
    }

    if !node.is_runnable() {
        return None;
    }
    Some((node, matches))
}

/// Single typed arg/flag value rendered as a string. `ty` None = string.
fn typed_one(matches: &ArgMatches, name: &str, ty: Option<&FlagType>) -> Option<String> {
    match ty {
        Some(FlagType::Int) => matches.get_one::<i64>(name).map(|v| v.to_string()),
        Some(FlagType::Float) => matches.get_one::<f64>(name).map(|v| v.to_string()),
        Some(FlagType::String) | None => matches.get_one::<String>(name).cloned(),
    }
}

/// Extract a declared arg/flag value as a string for interpolation. Names
/// that aren't declared on the node return None (asking clap about unknown
/// ids panics). Rest args join their values shell-quoted; an absent rest
/// arg is an empty string, like `{--}`.
fn get_value(node: &CommandNode, matches: &ArgMatches, name: &str) -> Option<String> {
    if let Some(arg) = node.args.iter().find(|a| a.name == name) {
        if arg.rest {
            let joined = matches
                .get_many::<String>(name)
                .map(|vals| {
                    vals.map(|s| interpolate::shell_quote(s))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            return Some(joined);
        }
        return typed_one(matches, name, arg.value_type.as_ref());
    }
    if let Some(flag) = node.flags.iter().find(|f| f.name == name) {
        // Boolean flags have no direct value ({?name:text} tests them).
        let ty = flag.value_type.as_ref()?;
        return typed_one(matches, name, Some(ty));
    }
    None
}
