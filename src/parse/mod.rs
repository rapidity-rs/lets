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
    RunConfig, VarValue,
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
        Error::Parse(Box::new(crate::error::SourceDiagnostic::new(
            message.into(),
            miette::NamedSource::new(self.name.clone(), self.source.clone()),
            span,
        )))
    }

    /// Like [`SourceCtx::error`], but marks the span with a short phrase
    /// instead of repeating the whole message under the snippet.
    pub(crate) fn error_labeled(
        &self,
        message: impl Into<String>,
        label: impl Into<String>,
        span: miette::SourceSpan,
    ) -> Error {
        Error::Parse(Box::new(crate::error::SourceDiagnostic {
            message: message.into(),
            src: miette::NamedSource::new(self.name.clone(), self.source.clone()),
            labels: vec![miette::LabeledSpan::new_with_span(Some(label.into()), span)],
            help: None,
        }))
    }

    /// Lift a `kdl` parse failure into our own diagnostic.
    ///
    /// `KdlError`'s `Display` is the constant "Failed to parse KDL document";
    /// everything useful lives in its sub-diagnostics, and its own source is
    /// unnamed, so reporting it directly would lose both the message and the
    /// `lets.kdl:3:5` location. The first sub-diagnostic becomes the headline
    /// and any others are additional markers on the same snippet.
    fn syntax_error(&self, err: &kdl::KdlError) -> Error {
        let src = miette::NamedSource::new(self.name.clone(), self.source.clone());
        let Some(first) = err.diagnostics.first() else {
            return Error::Parse(Box::new(crate::error::SourceDiagnostic {
                message: "invalid KDL syntax".to_string(),
                src,
                labels: Vec::new(),
                help: None,
            }));
        };

        let labels = err
            .diagnostics
            .iter()
            .map(|d| {
                let text = d.label.clone().or_else(|| d.message.clone());
                miette::LabeledSpan::new_with_span(text, d.span)
            })
            .collect();

        Error::Parse(Box::new(crate::error::SourceDiagnostic {
            message: first
                .message
                .clone()
                .unwrap_or_else(|| "invalid KDL syntax".to_string()),
            src,
            labels,
            help: first.help.clone(),
        }))
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

    let doc: KdlDocument = source
        .parse()
        .map_err(|e: kdl::KdlError| ctx.syntax_error(&e))?;

    // Version gate first: a config written for a newer lets may use syntax
    // this binary rejects, and "upgrade lets" beats a parse error.
    check_min_version(&doc)?;

    let mut tree = CommandTree {
        description: None,
        config: Config::default(),
        commands: Vec::new(),
        includes: Vec::new(),
        vars: Vec::new(),
    };

    let base_dir = path.parent().unwrap_or(Path::new("."));

    let mut seen_singletons: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for node in doc.nodes() {
        let name = node.name().value();
        if matches!(name, "description" | "config") && !seen_singletons.insert(name) {
            return Err(ctx.error(
                format!("duplicate top-level '{name}': only one is allowed"),
                node.span(),
            ));
        }
        match name {
            "description" => {
                tree.description = first_string_arg(node);
            }
            "config" => {
                tree.config = parse_config(node)?;
            }
            "include" => {
                if let Some(include_path_str) = first_string_arg(node) {
                    let include_path = base_dir.join(&include_path_str);
                    let included = parse_file(&include_path)?;
                    tree.commands.extend(included.commands);
                    tree.includes.push(include_path);
                    tree.includes.extend(included.includes);
                }
            }
            "vars" => {
                tree.vars.extend(parse_vars_block(node)?);
            }
            "cmd" => {
                tree.commands.push(parse_explicit_command(node)?);
            }
            _ => {
                tree.commands.push(parse_command(node)?);
            }
        }
    }

    resolve_vars(&mut tree)?;
    crate::validate::validate(&tree, &ctx)?;
    Ok(tree)
}

/// Enforce `config { min-version "X.Y.Z" }` before anything else is parsed.
fn check_min_version(doc: &KdlDocument) -> Result<()> {
    let required = doc
        .nodes()
        .iter()
        .find(|n| n.name().value() == "config")
        .and_then(|config| config.children())
        .and_then(|children| {
            children
                .nodes()
                .iter()
                .find(|n| n.name().value() == "min-version")
        })
        .and_then(first_string_arg);
    let Some(required) = required else {
        return Ok(());
    };

    let current = env!("CARGO_PKG_VERSION");
    if version_parts(&required) > version_parts(current) {
        return Err(Error::ParseNoSpan {
            message: format!(
                "this project requires lets >= {required}, but you have {current} \
                 (upgrade with `cargo install lets-cli` or your package manager)"
            ),
        });
    }
    Ok(())
}

/// Dotted version → numeric parts for lexicographic comparison.
/// Missing segments are 0; non-numeric segments are ignored.
fn version_parts(v: &str) -> [u64; 3] {
    let mut parts = [0u64; 3];
    for (i, seg) in v.split('.').take(3).enumerate() {
        parts[i] = seg
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap_or(0);
    }
    parts
}

/// Parse a `vars` block: children are `name "value"` pairs (static) or
/// `name cmd="shell command"` (dynamic, evaluated lazily at run time).
fn parse_vars_block(node: &KdlNode) -> Result<Vec<(String, VarValue)>> {
    let Some(children) = node.children() else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for child in children.nodes() {
        let name = child.name().value().to_string();
        let value = first_string_arg(child);
        let cmd = named_string(child, "cmd");
        let var = match (value, cmd) {
            (Some(_), Some(_)) => {
                return Err(Error::ParseNoSpan {
                    message: format!(
                        "var '{name}' has both a value and cmd=; use one or the other"
                    ),
                });
            }
            (Some(value), None) => VarValue::Static(value),
            (None, Some(cmd)) => VarValue::command(cmd),
            (None, None) => {
                return Err(Error::ParseNoSpan {
                    message: format!("var '{name}' needs a value (\"…\") or a command (cmd=\"…\")"),
                });
            }
        };
        out.push((name, var));
    }
    Ok(out)
}

/// Resolve `{name}` references inside var values and merge scopes onto each
/// node: globals, then each ancestor group's vars, then the node's own —
/// later entries win at lookup time. Dynamic vars have their `cmd` string
/// rendered here (against the static scope where they were declared), so
/// run-time evaluation is scope-independent and safely cacheable.
fn resolve_vars(tree: &mut CommandTree) -> Result<()> {
    let mut globals: Vec<(String, VarValue)> = Vec::new();
    for (name, value) in std::mem::take(&mut tree.vars) {
        let rendered = render_var(&name, value, &globals)?;
        globals.push((name, rendered));
    }
    for cmd in &mut tree.commands {
        resolve_node_vars(cmd, &globals)?;
    }
    tree.vars = globals;
    Ok(())
}

fn resolve_node_vars(node: &mut CommandNode, inherited: &[(String, VarValue)]) -> Result<()> {
    let mut merged: Vec<(String, VarValue)> = inherited.to_vec();
    for (name, value) in std::mem::take(&mut node.vars) {
        let rendered = render_var(&name, value, &merged)?;
        merged.push((name, rendered));
    }
    node.vars = merged;
    let scope = node.vars.clone();
    for child in &mut node.children {
        resolve_node_vars(child, &scope)?;
    }
    Ok(())
}

/// Render the static text of a var (or a dynamic var's command string)
/// against earlier vars and the environment (`{$VAR}`, empty when unset).
/// Referencing a dynamic var from another var is a load error: the value
/// would depend on evaluation order.
fn render_var(name: &str, value: VarValue, scope: &[(String, VarValue)]) -> Result<VarValue> {
    use crate::interpolate::{Placeholder, Resolution};
    let render_text = |text: &str| {
        crate::interpolate::render(text, |p| match p {
            Placeholder::Variable(n) => match scope.iter().rev().find(|(k, _)| k == n) {
                Some((_, VarValue::Static(v))) => Resolution::Value(v.clone()),
                Some((_, VarValue::Command { .. })) => Resolution::Error(format!(
                    "var '{name}' references dynamic var '{n}'; dynamic values resolve \
                     at run time — reference {{{n}}} directly where it's used, or make \
                     '{name}' dynamic too"
                )),
                None => Resolution::Unknown,
            },
            Placeholder::EnvVar(n) => std::env::var(n)
                .ok()
                .map_or(Resolution::Skip, Resolution::Value),
            _ => Resolution::Unknown,
        })
        .map_err(|e| Error::ParseNoSpan {
            message: format!("in var '{name}': {e}"),
        })
    };
    match value {
        VarValue::Static(text) => Ok(VarValue::Static(render_text(&text)?)),
        VarValue::Command { cmd, cache } => Ok(VarValue::Command {
            cmd: render_text(&cmd)?,
            cache,
        }),
    }
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
        run_policy: crate::tree::RunPolicy::default(),
        vars: Vec::new(),
        sources: Vec::new(),
        generates: Vec::new(),
        preconditions: Vec::new(),
        status: Vec::new(),
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
    // Nodes that hold a single value: a repeat would silently discard the
    // earlier one, so it's rejected. List-like nodes (deps, env, run, …)
    // extend instead.
    const SCALAR_NODES: &[&str] = &[
        "description",
        "long-description",
        "examples",
        "run-policy",
        "hide",
        "deprecated",
        "before",
        "after",
        "env-file",
        "dir",
        "shell",
        "confirm",
        "timeout",
        "retry",
        "silent",
        "run-macos",
        "run-linux",
        "run-windows",
    ];

    // Block: `task-name { ... }`
    if let Some(children) = node.children() {
        let mut seen_scalars: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for child in children.nodes() {
            let child_name = child.name().value();
            // `quiet` is an alias for `silent`; they share one slot.
            let scalar_key = if child_name == "quiet" {
                "silent"
            } else {
                child_name
            };
            if SCALAR_NODES.contains(&scalar_key) && !seen_scalars.insert(scalar_key) {
                return Err(Error::ParseNoSpan {
                    message: format!(
                        "duplicate '{child_name}' in '{}': only one is allowed, \
                         and a repeat would silently replace the first",
                        cmd.name
                    ),
                });
            }
            match child_name {
                "description" => {
                    cmd.description = first_string_arg(child);
                }
                "long-description" => {
                    cmd.long_description = first_string_arg(child);
                }
                "sources" => {
                    cmd.sources.extend(parse_string_list(child));
                }
                "generates" => {
                    cmd.generates.extend(parse_string_list(child));
                }
                "vars" => {
                    cmd.vars.extend(parse_vars_block(child)?);
                }
                "run-policy" => {
                    let value = first_string_arg(child).unwrap_or_default();
                    cmd.run_policy = match value.as_str() {
                        "once" => crate::tree::RunPolicy::Once,
                        "always" => crate::tree::RunPolicy::Always,
                        other => {
                            return Err(Error::ParseNoSpan {
                                message: format!(
                                    "invalid run-policy '{other}' (expected once or always)"
                                ),
                            });
                        }
                    };
                }
                "precondition" => {
                    let cmd_str = first_string_arg(child).ok_or_else(|| Error::ParseNoSpan {
                        message: "precondition requires a shell command argument".to_string(),
                    })?;
                    cmd.preconditions.push(crate::tree::Precondition {
                        cmd: cmd_str,
                        message: named_string(child, "message"),
                    });
                }
                "status" => {
                    cmd.status.extend(parse_string_list(child));
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
                    cmd.orch.deps.extend(parse_task_refs(child));
                }
                "steps" => {
                    cmd.orch.steps.extend(parse_task_refs(child));
                }
                "before" => {
                    cmd.orch.before = first_string_arg(child);
                }
                "after" => {
                    cmd.orch.after = first_string_arg(child);
                }
                "defer" => {
                    if let Some(cmd_str) = first_string_arg(child) {
                        cmd.orch.defers.push(cmd_str);
                    }
                }
                "env" => {
                    cmd.env.vars.extend(parse_env(child));
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
                    cmd.run.platform.extend(parse_platform_list(child)?);
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
                    cmd.aliases.extend(parse_string_list(child));
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
                        return Err(Error::ParseNoSpan {
                            message: format!(
                                "unknown node '{other}' in '{}': did you mean '{suggestion}'? \
                                 (rename it, or write `cmd {other}` to define a subcommand \
                                 with this name)",
                                cmd.name
                            ),
                        });
                    }
                    cmd.children.push(parse_command(child)?);
                }
            }
        }
    }

    Ok(cmd)
}
