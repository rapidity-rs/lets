//! Command orchestration.
//!
//! Resolves the matched subcommand from clap back to the command tree and
//! executes the full lifecycle: deps (parallel), steps (sequential),
//! interactive prompts, confirmation, before/after hooks, and run commands.
//!
//! Delegates actual process spawning to [`crate::shell`] and placeholder
//! interpolation to [`crate::interpolate`].

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

    // Collect interactive variable bindings.
    let interactive_vars = run_interactive(node, yes)?;

    // 1. Parallel deps.
    run_deps(node, tree, dry_run)?;

    // 2. Sequential steps.
    for (path, _) in &node.orch.steps {
        let display = path.join(" ");
        let step_node = tree
            .resolve_path(path)
            .ok_or_else(|| Error::Other(format!("step '{display}' not found")))?;
        exec_node_direct(step_node, tree, dry_run)?;
    }

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
fn exec_node_direct(node: &CommandNode, tree: &CommandTree, dry_run: bool) -> Result<()> {
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
    run_deps(node, tree, dry_run)?;

    // Sequential steps.
    for (path, _) in &node.orch.steps {
        let display = path.join(" ");
        let step_node = tree
            .resolve_path(path)
            .ok_or_else(|| Error::Other(format!("step '{display}' not found")))?;
        exec_node_direct(step_node, tree, dry_run)?;
    }

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

/// Run all deps in parallel using scoped threads. Fails on first error.
fn run_deps(node: &CommandNode, tree: &CommandTree, dry_run: bool) -> Result<()> {
    if node.orch.deps.is_empty() {
        return Ok(());
    }

    std::thread::scope(|s| {
        let handles: Vec<_> = node
            .orch
            .deps
            .iter()
            .map(|(path, _)| {
                s.spawn(|| {
                    let display = path.join(" ");
                    let dep_node = tree
                        .resolve_path(path)
                        .ok_or_else(|| Error::Other(format!("dep '{display}' not found")))?;
                    exec_node_direct(dep_node, tree, dry_run)
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
