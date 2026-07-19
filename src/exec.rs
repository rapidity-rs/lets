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
use std::sync::atomic::{AtomicUsize, Ordering};

use clap::ArgMatches;
use parking_lot::{Condvar, Mutex};

use crate::error::{Error, Result};
use crate::interpolate::{self, Placeholder};
use crate::output::{OutputMode, TaskSink};
use crate::shell::{ExecContext, exec_shell};
use crate::tree::{CommandNode, CommandTree, FlagType};

/// Resolve the matched subcommand from clap back to our tree and execute it.
pub fn run(tree: &CommandTree, matches: &ArgMatches) -> Result<()> {
    let Some((node, node_matches, root_key)) = resolve(tree, matches) else {
        return Ok(());
    };

    warn_deprecated(node);

    let yes = matches.get_flag("yes");
    let dry_run = matches.get_flag("dry-run");
    let ctx = ExecContext::from_node(node, &tree.config, dry_run)?;

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
    };
    let mut visited = HashSet::new();
    orch.collect_interaction(node, &root_key, yes, &mut visited)?;

    let interactive_vars = orch
        .interactions
        .get(&root_key)
        .cloned()
        .unwrap_or_default();

    // 1. Parallel deps (run-once).
    orch.run_deps(node)?;

    // 2. Sequential steps (run-once).
    orch.run_steps(node)?;

    // 3. Hooks and main commands. The root command is never prefixed or
    // grouped (it may be interactive); silent still buffers until failure.
    let sink = TaskSink::for_root(node.exec.silent);
    let result = (|| -> Result<()> {
        if let Some(before) = &node.orch.before {
            exec_shell(before, &ctx, &sink)?;
        }

        for command in node.run.resolve() {
            let interpolated = interpolate_cmd(command, node, node_matches, &interactive_vars);
            exec_shell(&interpolated, &ctx, &sink)?;
        }

        if let Some(after) = &node.orch.after {
            exec_shell(after, &ctx, &sink)?;
        }
        Ok(())
    })();
    sink.finish(result.is_err());
    result
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
            let (target, _args) = self
                .tree
                .resolve_ref(task_ref)
                .ok_or_else(|| Error::Other(format!("task '{}' not found", task_ref.display())))?;
            self.collect_interaction(target, &task_ref.tokens, yes, visited)?;
        }

        let vars = run_interactive(node, yes)?;

        // Confirmations are skipped in dry-run (nothing will execute) and
        // auto-accepted by --yes. Without a TTY, dialoguer fails — a command
        // guarded by `confirm` never silently runs.
        if let Some(confirm_msg) = &node.interactive.confirm
            && !self.dry_run
            && !yes
        {
            let rendered = interpolate_simple(confirm_msg, &vars);
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

    /// Run a referenced task at most once per invocation.
    fn run_task(&self, key: &[String], node: &CommandNode, args: &[String]) -> Result<()> {
        match self.registry.claim(key) {
            Claim::Done => Ok(()),
            Claim::Failed => Err(Error::Other(format!("task '{}' failed", key.join(" ")))),
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

    /// Run all deps in parallel using scoped threads. Fails on first error.
    fn run_deps(&self, node: &CommandNode) -> Result<()> {
        if node.orch.deps.is_empty() {
            return Ok(());
        }

        std::thread::scope(|s| {
            let handles: Vec<_> = node
                .orch
                .deps
                .iter()
                .map(|task_ref| {
                    s.spawn(|| {
                        let (dep_node, args) =
                            self.tree.resolve_ref(task_ref).ok_or_else(|| {
                                Error::Other(format!("dep '{}' not found", task_ref.display()))
                            })?;
                        self.run_task(&task_ref.tokens, dep_node, args)
                    })
                })
                .collect();

            for handle in handles {
                handle
                    .join()
                    .map_err(|_| Error::Other("dependency thread panicked".to_string()))??;
            }

            Ok(())
        })
    }

    /// Run all steps sequentially. Fails on first error.
    fn run_steps(&self, node: &CommandNode) -> Result<()> {
        for task_ref in &node.orch.steps {
            let (step_node, args) = self
                .tree
                .resolve_ref(task_ref)
                .ok_or_else(|| Error::Other(format!("step '{}' not found", task_ref.display())))?;
            self.run_task(&task_ref.tokens, step_node, args)?;
        }
        Ok(())
    }

    /// Execute a command node invoked via deps/steps.
    /// With reference arguments, they are parsed by the target's own clap
    /// definition; otherwise interpolation falls back to declared defaults.
    fn exec_node(&self, node: &CommandNode, key: &[String], args: &[String]) -> Result<()> {
        warn_deprecated(node);

        let ctx = ExecContext::from_node(node, &self.tree.config, self.dry_run)?;

        // Nested deps and steps run first; they present through their own sinks.
        self.run_deps(node)?;
        self.run_steps(node)?;

        // This task's own output (hooks + run commands) goes through one sink
        // so grouped mode flushes it as a single block.
        let label = key.join(" ");
        let color_seq = self.color_seq.fetch_add(1, Ordering::Relaxed);
        let sink = TaskSink::for_task(self.mode, &label, node.exec.silent, color_seq);
        let result = self.exec_node_commands(node, key, args, &ctx, &sink);
        sink.finish(result.is_err());
        result
    }

    /// Hooks and run commands of a single task, routed through its sink.
    fn exec_node_commands(
        &self,
        node: &CommandNode,
        key: &[String],
        args: &[String],
        ctx: &ExecContext,
        sink: &TaskSink,
    ) -> Result<()> {
        // Before hook.
        if let Some(before) = &node.orch.before {
            exec_shell(before, ctx, sink)?;
        }

        // Main commands.
        let empty = HashMap::new();
        let vars = self.interactions.get(key).unwrap_or(&empty);
        if args.is_empty() {
            for command in node.run.resolve() {
                let interpolated = interpolate_with_defaults(command, node, vars);
                exec_shell(&interpolated, ctx, sink)?;
            }
        } else {
            // Validated at load time; parse again here to get real ArgMatches.
            let argv = std::iter::once(node.name.clone()).chain(args.iter().cloned());
            let matches = crate::cli::build_subcommand(node, false)
                .try_get_matches_from(argv)
                .map_err(|e| {
                    Error::Other(format!(
                        "invalid arguments for task '{}': {e}",
                        key.join(" ")
                    ))
                })?;
            for command in node.run.resolve() {
                let interpolated = interpolate_cmd(command, node, &matches, vars);
                exec_shell(&interpolated, ctx, sink)?;
            }
        }

        // After hook.
        if let Some(after) = &node.orch.after {
            exec_shell(after, ctx, sink)?;
        }

        Ok(())
    }
}

/// Interpolate a run string from interactive bindings and declared defaults.
/// Used when a command is invoked via deps/steps (no ArgMatches available).
fn interpolate_with_defaults(
    command: &str,
    node: &CommandNode,
    vars: &HashMap<String, String>,
) -> String {
    interpolate::render(command, |p| match p {
        Placeholder::Passthrough | Placeholder::Conditional(_, _) => None,
        Placeholder::EnvVar(var_name) => {
            if let Some((_, v)) = node.env.vars.iter().find(|(k, _)| k == var_name) {
                Some(v.clone())
            } else {
                std::env::var(var_name).ok()
            }
        }
        Placeholder::Variable(name) => {
            if let Some(value) = vars.get(name) {
                return Some(value.clone());
            }
            if let Some(arg) = node.args.iter().find(|a| a.name == name) {
                return arg.default.clone();
            }
            if let Some(flag) = node.flags.iter().find(|f| f.name == name) {
                return flag.default.clone();
            }
            None
        }
    })
}

/// Process interactive prompts and choices, returning variable bindings.
fn run_interactive(node: &CommandNode, yes: bool) -> Result<HashMap<String, String>> {
    let mut vars = HashMap::new();

    // Process choose nodes first (so confirm can reference them).
    for choose in &node.interactive.chooses {
        let value = if yes {
            choose.choices.first().cloned().unwrap_or_default()
        } else {
            let selection = dialoguer::Select::new()
                .with_prompt(&choose.name)
                .items(&choose.choices)
                .default(0)
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

/// Simple interpolation of `{name}` from a variable map (for confirm messages).
fn interpolate_simple(template: &str, vars: &HashMap<String, String>) -> String {
    interpolate::render(template, |p| match p {
        Placeholder::Variable(name) => vars.get(name).cloned(),
        _ => None,
    })
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
fn resolve<'a>(
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

/// Replace placeholders in the command string with values from ArgMatches + interactive vars.
fn interpolate_cmd(
    command: &str,
    node: &CommandNode,
    matches: &ArgMatches,
    extra_vars: &HashMap<String, String>,
) -> String {
    interpolate::render(command, |p| match p {
        Placeholder::Passthrough => matches.get_many::<String>("--").map(|trailing| {
            let joined: Vec<&str> = trailing.map(|s| s.as_str()).collect();
            joined.join(" ")
        }),
        Placeholder::EnvVar(var_name) => {
            if let Some((_, v)) = node.env.vars.iter().find(|(k, _)| k == var_name) {
                Some(v.clone())
            } else {
                std::env::var(var_name).ok()
            }
        }
        Placeholder::Conditional(flag_name, text) => {
            if matches.get_flag(flag_name) {
                Some(text.to_string())
            } else {
                None
            }
        }
        Placeholder::Variable(name) => {
            if let Some(value) = extra_vars.get(name) {
                return Some(value.clone());
            }
            get_value(node, matches, name)
        }
    })
}

/// Extract a value as a string, using the node's flag definitions to determine the type.
fn get_value(node: &CommandNode, matches: &ArgMatches, name: &str) -> Option<String> {
    if let Some(flag) = node.flags.iter().find(|f| f.name == name) {
        return match flag.value_type {
            Some(FlagType::Int) => matches.get_one::<i64>(name).map(|v| v.to_string()),
            Some(FlagType::Float) => matches.get_one::<f64>(name).map(|v| v.to_string()),
            Some(FlagType::String) => matches.get_one::<String>(name).cloned(),
            None => None,
        };
    }

    matches.get_one::<String>(name).cloned()
}
