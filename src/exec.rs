//! Command orchestration.
//!
//! Resolves the matched subcommand from clap back to the command tree and
//! executes the full lifecycle: deps (parallel), steps (sequential),
//! interactive prompts, confirmation, before/after hooks, and run commands.
//!
//! Delegates actual process spawning to [`crate::shell`] and placeholder
//! interpolation to [`crate::interpolate`].

use parking_lot::{Condvar, Mutex};
use std::collections::HashMap;

use clap::ArgMatches;

use crate::error::{Error, Result};
use crate::interpolate::{self, Placeholder};
use crate::shell::{ExecContext, exec_shell};
use crate::tree::{CommandNode, CommandTree, FlagType};

/// Resolve the matched subcommand from clap back to our tree and execute it.
pub fn run(tree: &CommandTree, matches: &ArgMatches) -> Result<()> {
    let Some((node, node_matches)) = resolve(tree, matches) else {
        return Ok(());
    };

    // Warn if the command is deprecated.
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

    let yes = matches.get_flag("yes");
    let dry_run = matches.get_flag("dry-run");
    let ctx = ExecContext::from_node(node, &tree.config, dry_run)?;
    let registry = Registry::new();

    // Collect interactive variable bindings.
    let interactive_vars = run_interactive(node, yes)?;

    // 1. Parallel deps.
    run_deps(node, tree, &registry, dry_run)?;

    // 2. Sequential steps.
    run_steps(node, tree, &registry, dry_run)?;

    // 3. Confirm (after interactive vars are collected, so interpolation works in the message).
    if let Some(confirm_msg) = &node.interactive.confirm
        && !dry_run
    {
        let rendered = interpolate_simple(confirm_msg, &interactive_vars);
        if !yes {
            let confirmed = dialoguer::Confirm::new()
                .with_prompt(&rendered)
                .default(false)
                .interact()
                .map_err(|e| Error::Other(format!("prompt failed: {e}")))?;
            if !confirmed {
                return Err(Error::Other("aborted by user".to_string()));
            }
        }
    }

    // 4. Before hook.
    if let Some(before) = &node.orch.before {
        exec_shell(before, &ctx)?;
    }

    // 5. Main commands (with interpolation from clap matches + interactive vars).
    for command in node.run.resolve() {
        let interpolated = interpolate_cmd(command, node, node_matches, &interactive_vars);
        exec_shell(&interpolated, &ctx)?;
    }

    // 6. After hook.
    if let Some(after) = &node.orch.after {
        exec_shell(after, &ctx)?;
    }

    Ok(())
}

/// Execute a command node directly, without ArgMatches (for deps/steps invocation).
/// Interpolates the run string using default values for args and flags.
fn exec_node_direct(
    node: &CommandNode,
    tree: &CommandTree,
    registry: &Registry,
    dry_run: bool,
) -> Result<()> {
    // Warn if the command is deprecated.
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

    let ctx = ExecContext::from_node(node, &tree.config, dry_run)?;

    // Parallel deps.
    run_deps(node, tree, registry, dry_run)?;

    // Sequential steps.
    run_steps(node, tree, registry, dry_run)?;

    // Before hook.
    if let Some(before) = &node.orch.before {
        exec_shell(before, &ctx)?;
    }

    // Main commands — interpolate with defaults.
    for command in node.run.resolve() {
        let interpolated = interpolate_with_defaults(command, node);
        exec_shell(&interpolated, &ctx)?;
    }

    // After hook.
    if let Some(after) = &node.orch.after {
        exec_shell(after, &ctx)?;
    }

    Ok(())
}

/// Interpolate a run string using only the node's default values.
/// Used when a command is invoked via deps/steps (no ArgMatches available).
fn interpolate_with_defaults(command: &str, node: &CommandNode) -> String {
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

/// Run a referenced task at most once per invocation.
fn run_task(
    key: &[String],
    node: &CommandNode,
    tree: &CommandTree,
    registry: &Registry,
    dry_run: bool,
) -> Result<()> {
    match registry.claim(key) {
        Claim::Done => Ok(()),
        Claim::Failed => Err(Error::Other(format!("task '{}' failed", key.join(" ")))),
        Claim::Run => {
            let mut guard = ClaimGuard {
                registry,
                key,
                ok: false,
            };
            let result = exec_node_direct(node, tree, registry, dry_run);
            guard.ok = result.is_ok();
            result
        }
    }
}

/// Run all deps in parallel using scoped threads. Fails on first error.
fn run_deps(
    node: &CommandNode,
    tree: &CommandTree,
    registry: &Registry,
    dry_run: bool,
) -> Result<()> {
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
                    let (dep_node, _args) = tree.resolve_ref(task_ref).ok_or_else(|| {
                        Error::Other(format!("dep '{}' not found", task_ref.display()))
                    })?;
                    run_task(&task_ref.tokens, dep_node, tree, registry, dry_run)
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
fn run_steps(
    node: &CommandNode,
    tree: &CommandTree,
    registry: &Registry,
    dry_run: bool,
) -> Result<()> {
    for task_ref in &node.orch.steps {
        let (step_node, _args) = tree
            .resolve_ref(task_ref)
            .ok_or_else(|| Error::Other(format!("step '{}' not found", task_ref.display())))?;
        run_task(&task_ref.tokens, step_node, tree, registry, dry_run)?;
    }
    Ok(())
}

/// Walk the ArgMatches subcommand chain to find the deepest matched CommandNode.
fn resolve<'a>(
    tree: &'a CommandTree,
    matches: &'a ArgMatches,
) -> Option<(&'a CommandNode, &'a ArgMatches)> {
    let (name, sub_matches) = matches.subcommand()?;
    let node = tree.commands.iter().find(|c| c.name == name)?;
    resolve_node(node, sub_matches)
}

fn resolve_node<'a>(
    node: &'a CommandNode,
    matches: &'a ArgMatches,
) -> Option<(&'a CommandNode, &'a ArgMatches)> {
    if let Some((child_name, child_matches)) = matches.subcommand()
        && let Some(child) = node.children.iter().find(|c| c.name == child_name)
    {
        return resolve_node(child, child_matches);
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
