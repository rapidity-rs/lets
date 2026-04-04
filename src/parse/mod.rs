//! KDL parser for `lets.kdl` config files.
//!
//! Reads a KDL file and converts it into a [`CommandTree`].
//! Handles all node types: commands (one-liner and block), args, flags, deps,
//! steps, hooks, env, platform variants, interactive prompts, and more.
//!
//! Includes typo detection for misspelled keywords and delegates validation
//! to [`crate::validate`].

mod fields;
mod helpers;
mod typo;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use kdl::{KdlDocument, KdlNode};

use crate::error::{Error, Result};
use crate::tree::{
    CommandNode, CommandTree, Config, EnvConfig, ExecConfig, Interactive, Orchestration, Platform,
    RunConfig,
};

use fields::{parse_arg, parse_choose, parse_flag, parse_prompt};
use helpers::{
    first_string_arg, named_int, named_string, parse_config, parse_duration, parse_env,
    parse_platform_list, parse_string_list, parse_task_refs,
};
use typo::check_typo;

/// Source context carried through parsing for rich error messages.
#[derive(Clone)]
pub(crate) struct SourceCtx {
    name: String,
    source: String,
}

impl SourceCtx {
    pub(crate) fn error(&self, message: impl Into<String>, span: miette::SourceSpan) -> Error {
        Error::Parse {
            message: message.into(),
            src: miette::NamedSource::new(self.name.clone(), self.source.clone()),
            span,
        }
    }

    fn error_no_span(&self, message: impl Into<String>) -> Error {
        Error::ParseNoSpan {
            message: message.into(),
        }
    }
}

/// Parse a `lets.kdl` file into a `CommandTree`.
pub fn parse_file(path: &Path) -> Result<CommandTree> {
    let source = std::fs::read_to_string(path).map_err(|e| Error::ReadFile {
        path: path.to_path_buf(),
        source: e,
    })?;
    parse_source(&source, path)
}

pub(crate) fn parse_source(source: &str, path: &Path) -> Result<CommandTree> {
    let ctx = SourceCtx {
        name: path.display().to_string(),
        source: source.to_string(),
    };

    let doc: KdlDocument = source.parse().map_err(|e: kdl::KdlError| {
        // KDL errors already contain span info in their Display output.
        ctx.error_no_span(e.to_string())
    })?;

    let mut tree = CommandTree {
        description: None,
        config: Config::default(),
        commands: Vec::new(),
    };

    let base_dir = path.parent().unwrap_or(Path::new("."));

    for node in doc.nodes() {
        let name = node.name().value();
        match name {
            "description" => {
                tree.description = first_string_arg(node);
            }
            "config" => {
                tree.config = parse_config(node);
            }
            "include" => {
                if let Some(include_path_str) = first_string_arg(node) {
                    let include_path = base_dir.join(&include_path_str);
                    let included = parse_file(&include_path)?;
                    tree.commands.extend(included.commands);
                }
            }
            "cmd" => {
                tree.commands.push(parse_explicit_command(node)?);
            }
            _ => {
                tree.commands.push(parse_command(node)?);
            }
        }
    }

    crate::validate::validate(&tree, &ctx)?;
    Ok(tree)
}

/// Parse a `cmd` node: `cmd name "inline command"` or `cmd name { ... }`.
/// The first positional arg is the command name, the second (if present) is an inline run string.
fn parse_explicit_command(node: &KdlNode) -> Result<CommandNode> {
    let positional: Vec<String> = node
        .entries()
        .iter()
        .filter(|e| e.name().is_none())
        .filter_map(|e| e.value().as_string().map(|s| s.to_string()))
        .collect();

    let name = positional
        .first()
        .ok_or_else(|| Error::ParseNoSpan {
            message: "cmd node requires a name as the first argument".to_string(),
        })?
        .clone();

    // Build a synthetic node-like parse: reuse parse_command_body.
    let inline_cmd = positional.get(1).cloned();
    parse_command_body(name, inline_cmd, node)
}

fn parse_command(node: &KdlNode) -> Result<CommandNode> {
    let name = node.name().value().to_string();
    let inline_cmd = first_string_arg(node);
    parse_command_body(name, inline_cmd, node)
}

fn parse_command_body(
    name: String,
    inline_cmd: Option<String>,
    node: &KdlNode,
) -> Result<CommandNode> {
    // Support description= as a named property on the node itself (for one-liners).
    let inline_desc = named_string(node, "description");

    let mut cmd = CommandNode {
        name,
        span: node.span(),
        description: inline_desc,
        long_description: None,
        examples: None,
        hide: false,
        deprecated: None,
        args: Vec::new(),
        flags: Vec::new(),
        aliases: Vec::new(),
        run: RunConfig {
            commands: inline_cmd.into_iter().collect(),
            ..Default::default()
        },
        orch: Orchestration::default(),
        env: EnvConfig::default(),
        exec: ExecConfig::default(),
        interactive: Interactive::default(),
        children: Vec::new(),
    };

    // Block: `task-name { ... }`
    if let Some(children) = node.children() {
        for child in children.nodes() {
            let child_name = child.name().value();
            match child_name {
                "description" => {
                    cmd.description = first_string_arg(child);
                }
                "long-description" => {
                    cmd.long_description = first_string_arg(child);
                }
                "examples" => {
                    cmd.examples = first_string_arg(child);
                }
                "hide" => {
                    cmd.hide = true;
                }
                "deprecated" => {
                    cmd.deprecated = Some(first_string_arg(child).unwrap_or_default());
                }
                "run" => {
                    if let Some(s) = first_string_arg(child) {
                        cmd.run.commands.push(s);
                    }
                }
                "arg" => {
                    cmd.args.push(parse_arg(child)?);
                }
                "flag" => {
                    cmd.flags.push(parse_flag(child)?);
                }
                "deps" => {
                    cmd.orch.deps = parse_task_refs(child);
                }
                "steps" => {
                    cmd.orch.steps = parse_task_refs(child);
                }
                "before" => {
                    cmd.orch.before = first_string_arg(child);
                }
                "after" => {
                    cmd.orch.after = first_string_arg(child);
                }
                "env" => {
                    cmd.env.vars = parse_env(child);
                }
                "env-file" => {
                    cmd.env.file = first_string_arg(child).map(PathBuf::from);
                }
                "dir" => {
                    cmd.exec.dir = first_string_arg(child).map(PathBuf::from);
                }
                "shell" => {
                    cmd.exec.shell = first_string_arg(child);
                }
                "platform" => {
                    cmd.run.platform = parse_platform_list(child)?;
                }
                "run-macos" => {
                    if let Some(v) = first_string_arg(child) {
                        cmd.run.platform_run.insert(Platform::Macos, v);
                    }
                }
                "run-linux" => {
                    if let Some(v) = first_string_arg(child) {
                        cmd.run.platform_run.insert(Platform::Linux, v);
                    }
                }
                "run-windows" => {
                    if let Some(v) = first_string_arg(child) {
                        cmd.run.platform_run.insert(Platform::Windows, v);
                    }
                }
                "confirm" => {
                    cmd.interactive.confirm = first_string_arg(child);
                }
                "prompt" => {
                    cmd.interactive.prompts.push(parse_prompt(child)?);
                }
                "choose" => {
                    cmd.interactive.chooses.push(parse_choose(child)?);
                }
                "alias" => {
                    cmd.aliases = parse_string_list(child);
                }
                "timeout" => {
                    if let Some(s) = first_string_arg(child) {
                        cmd.exec.timeout = Some(
                            parse_duration(&s)
                                .map_err(|msg| Error::ParseNoSpan { message: msg })?,
                        );
                    }
                }
                "retry" => {
                    cmd.exec.retry_count = named_int(child, "count").map(|v| v as u32);
                    if let Some(s) = named_string(child, "delay") {
                        cmd.exec.retry_delay = Some(
                            parse_duration(&s)
                                .map_err(|msg| Error::ParseNoSpan { message: msg })?,
                        );
                    }
                }
                "silent" | "quiet" => {
                    cmd.exec.silent = true;
                }
                "cmd" => {
                    cmd.children.push(parse_explicit_command(child)?);
                }
                other => {
                    if let Some(suggestion) = check_typo(other) {
                        eprintln!(
                            "\x1b[33mwarning:\x1b[0m unknown node '{other}' in '{}' \
                             (did you mean '{suggestion}'?). Treating as subcommand.",
                            cmd.name
                        );
                    }
                    cmd.children.push(parse_command(child)?);
                }
            }
        }
    }

    Ok(cmd)
}
