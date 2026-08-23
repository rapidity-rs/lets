//! Command tree validation.
//!
//! Runs after parsing to catch structural errors:
//! - All `deps`/`steps` references resolve to existing commands
//! - Arguments supplied in a reference (or defaults) satisfy the target's CLI
//! - No dependency cycles exist (direct or indirect, via DFS)
//! - Every `{…}` placeholder resolves against statically known names

use std::collections::HashSet;

use crate::error::{Error, Result};
use crate::interpolate::{self, Placeholder, RenderError, Resolution};
use crate::parse::SourceCtx;
use crate::tree::{CommandNode, CommandTree};

/// Validate the command tree: names are usable, refs resolve, no cycles
/// exist, source globs compile, and placeholders resolve.
pub fn validate(tree: &CommandTree, ctx: &SourceCtx) -> Result<()> {
    validate_names(&tree.commands, ctx, true)?;
    validate_inputs(&tree.commands, ctx)?;
    validate_config_env(tree)?;
    validate_refs(tree, &tree.commands, ctx)?;
    validate_no_cycles(tree, &tree.commands, ctx)?;
    validate_sources(&tree.commands, ctx)?;
    validate_placeholders(&tree.commands, ctx)?;
    Ok(())
}

/// Config-level env values may reference global vars and `{$VAR}` only.
fn validate_config_env(tree: &CommandTree) -> Result<()> {
    for (key, value) in &tree.config.env {
        interpolate::render(value, |p| match p {
            Placeholder::EnvVar(_) => Resolution::Skip,
            Placeholder::Variable(name) if tree.vars.iter().any(|(k, _)| k == name) => {
                Resolution::Skip
            }
            _ => Resolution::Unknown,
        })
        .map(drop)
        .map_err(|e| Error::ParseNoSpan {
            message: format!("in config env value '{key}': {e}"),
        })?;
    }
    Ok(())
}

/// Structural rules for arg/flag declarations: rest args are last and
/// unique, type/choices don't conflict, env fallbacks only where a value
/// exists to fall back to, and rest args don't fight `{--}` for the same
/// trailing tokens.
fn validate_inputs(commands: &[CommandNode], ctx: &SourceCtx) -> Result<()> {
    for cmd in commands {
        for (i, arg) in cmd.args.iter().enumerate() {
            if !arg.choices.is_empty() && arg.value_type.is_some() {
                return Err(ctx.error(
                    format!(
                        "arg '{}' on '{}': choices and type= are mutually exclusive \
                         (choices are always strings)",
                        arg.name, cmd.name
                    ),
                    cmd.span,
                ));
            }
            if arg.rest {
                if arg.value_type.is_some() {
                    return Err(ctx.error(
                        format!(
                            "rest arg '{}' on '{}' cannot have type= \
                             (values are strings)",
                            arg.name, cmd.name
                        ),
                        cmd.span,
                    ));
                }
                if i != cmd.args.len() - 1 {
                    return Err(ctx.error(
                        format!(
                            "rest arg '{}' on '{}' must be the last declared arg",
                            arg.name, cmd.name
                        ),
                        cmd.span,
                    ));
                }
                if cmd.uses_passthrough() {
                    return Err(ctx.error(
                        format!(
                            "'{}' declares rest arg '{}' and uses {{--}}: both capture \
                             trailing tokens; pick one",
                            cmd.name, arg.name
                        ),
                        cmd.span,
                    ));
                }
            }
        }
        for flag in &cmd.flags {
            if !flag.choices.is_empty() && flag.value_type != Some(crate::tree::FlagType::String) {
                return Err(ctx.error(
                    format!(
                        "flag '{}' on '{}': choices and type= are mutually exclusive \
                         (choices are always strings)",
                        flag.name, cmd.name
                    ),
                    cmd.span,
                ));
            }
            if flag.env.is_some() && flag.value_type.is_none() {
                return Err(ctx.error(
                    format!(
                        "boolean flag '{}' on '{}' cannot have env= \
                         (only args and valued flags)",
                        flag.name, cmd.name
                    ),
                    cmd.span,
                ));
            }
        }
        validate_inputs(&cmd.children, ctx)?;
    }
    Ok(())
}

/// Built-in global flags every command inherits; user flags may not reuse
/// their names or shorts (clap would panic on the duplicate, or silently
/// shadow the built-in).
const RESERVED_FLAG_NAMES: &[&str] = &[
    "file", "yes", "dry-run", "output", "watch", "force", "jobs", "help",
];
const RESERVED_FLAG_SHORTS: &[char] = &['f', 'y', 'j', 'h'];

/// Check command names, aliases, and arg/flag ids for collisions that would
/// break the generated CLI: duplicate siblings, the reserved `self` name,
/// duplicate arg/flag ids on one command, and built-in global flags.
fn validate_names(commands: &[CommandNode], ctx: &SourceCtx, top_level: bool) -> Result<()> {
    let mut labels: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for cmd in commands {
        for label in
            std::iter::once(cmd.name.as_str()).chain(cmd.aliases.iter().map(String::as_str))
        {
            if top_level && label == "self" {
                return Err(ctx.error(
                    "'self' is reserved for lets built-in commands (lets self init/check/…)",
                    cmd.span,
                ));
            }
            if let Some(owner) = labels.insert(label, cmd.name.as_str()) {
                let what = if label == cmd.name {
                    "command name"
                } else {
                    "alias"
                };
                return Err(ctx.error(
                    format!("{what} '{label}' on '{}' collides with '{owner}'", cmd.name),
                    cmd.span,
                ));
            }
        }

        let mut ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut shorts: std::collections::HashSet<char> = std::collections::HashSet::new();
        for arg in &cmd.args {
            if arg.name == "help" {
                return Err(ctx.error(
                    format!("arg name 'help' on '{}' is reserved", cmd.name),
                    cmd.span,
                ));
            }
            if !ids.insert(&arg.name) {
                return Err(ctx.error(
                    format!("duplicate arg/flag name '{}' on '{}'", arg.name, cmd.name),
                    cmd.span,
                ));
            }
        }
        for flag in &cmd.flags {
            if RESERVED_FLAG_NAMES.contains(&flag.name.as_str()) {
                return Err(ctx.error(
                    format!(
                        "flag name '{}' on '{}' is reserved by the built-in global \
                         --{} flag",
                        flag.name, cmd.name, flag.name
                    ),
                    cmd.span,
                ));
            }
            if !ids.insert(&flag.name) {
                return Err(ctx.error(
                    format!("duplicate arg/flag name '{}' on '{}'", flag.name, cmd.name),
                    cmd.span,
                ));
            }
            if let Some(short) = flag.short {
                if RESERVED_FLAG_SHORTS.contains(&short) {
                    return Err(ctx.error(
                        format!(
                            "short flag '-{short}' on '{}' is reserved by a built-in \
                             global flag",
                            cmd.name
                        ),
                        cmd.span,
                    ));
                }
                if !shorts.insert(short) {
                    return Err(ctx.error(
                        format!("duplicate short flag '-{short}' on '{}'", cmd.name),
                        cmd.span,
                    ));
                }
            }
        }

        validate_names(&cmd.children, ctx, false)?;
    }
    Ok(())
}

/// Which placeholder kinds a template may use, besides `{$VAR}` (always
/// allowed — environment lookups are inherently runtime).
enum TemplateKind {
    /// Run commands, hooks, defers, gates: full scope.
    Shell,
    /// Confirm messages: interactive bindings and vars.
    Confirm,
    /// Env values: vars only.
    EnvValue,
}

/// Check every placeholder in shell-bound templates, confirm messages, and
/// env values against the names statically known for the node. Unknown
/// placeholders fail at load — never silently at run time.
fn validate_placeholders(commands: &[CommandNode], ctx: &SourceCtx) -> Result<()> {
    for cmd in commands {
        for template in cmd.shell_templates() {
            check_template(cmd, template, TemplateKind::Shell, ctx)?;
        }
        if let Some(confirm) = &cmd.interactive.confirm {
            check_template(cmd, confirm, TemplateKind::Confirm, ctx)?;
        }
        for (key, value) in &cmd.env.vars {
            check_template(cmd, value, TemplateKind::EnvValue, ctx).map_err(|e| match e {
                Error::Parse(mut d) => {
                    d.message = format!("in env value '{key}': {}", d.message);
                    Error::Parse(d)
                }
                other => other,
            })?;
        }
        validate_placeholders(&cmd.children, ctx)?;
    }
    Ok(())
}

fn check_template(
    cmd: &CommandNode,
    template: &str,
    kind: TemplateKind,
    ctx: &SourceCtx,
) -> Result<()> {
    let interactive: HashSet<&str> = cmd
        .interactive
        .prompts
        .iter()
        .map(|p| p.name.as_str())
        .chain(cmd.interactive.chooses.iter().map(|c| c.name.as_str()))
        .collect();
    let vars: HashSet<&str> = cmd.vars.iter().map(|(k, _)| k.as_str()).collect();

    // A plain `{name}` needs a value guaranteed to exist: required args,
    // declared defaults, env fallbacks, or list-like rest args (empty when
    // absent). Optional values without any of those must go through
    // `{?name:text}` or `$LETS_ARG_*` instead.
    let variable_known = |name: &str| match kind {
        TemplateKind::Shell => {
            interactive.contains(name)
                || vars.contains(name)
                || cmd.args.iter().any(|a| {
                    a.name == name
                        && (a.rest || a.required || a.default.is_some() || a.env.is_some())
                })
                || cmd.flags.iter().any(|f| {
                    f.name == name
                        && f.value_type.is_some()
                        && (f.default.is_some() || f.env.is_some())
                })
        }
        TemplateKind::Confirm => interactive.contains(name) || vars.contains(name),
        TemplateKind::EnvValue => vars.contains(name),
    };

    let result = interpolate::render(template, |p| match p {
        Placeholder::EnvVar(_) => Resolution::Skip,
        Placeholder::Passthrough => match kind {
            TemplateKind::Shell => Resolution::Skip,
            _ => Resolution::Unknown,
        },
        Placeholder::Conditional(name, _) => {
            if matches!(kind, TemplateKind::Shell) && cmd.presence_testable(name) {
                Resolution::Skip
            } else {
                Resolution::Unknown
            }
        }
        Placeholder::Variable(name) => {
            if variable_known(name) {
                Resolution::Skip
            } else {
                Resolution::Unknown
            }
        }
    });

    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            // Declared-but-not-guaranteed names get tailored hints.
            let message = match &e {
                RenderError::Unknown { placeholder }
                    if cmd
                        .flags
                        .iter()
                        .any(|f| f.name == *placeholder && f.value_type.is_none()) =>
                {
                    format!(
                        "boolean flag '{placeholder}' cannot be interpolated directly; \
                         use {{?{placeholder}:text}}"
                    )
                }
                RenderError::Unknown { placeholder }
                    if cmd.args.iter().any(|a| a.name == *placeholder)
                        || cmd.flags.iter().any(|f| f.name == *placeholder) =>
                {
                    format!(
                        "'{placeholder}' may be absent at run time; add default= or env=, \
                         or use {{?{placeholder}:text}} / $LETS_ARG_* instead"
                    )
                }
                _ => e.to_string(),
            };
            Err(ctx.error(
                format!("in '{}': {message} in \"{template}\"", cmd.name),
                cmd.span,
            ))
        }
    }
}

/// Check that every `sources`/`generates` pattern is a valid glob.
fn validate_sources(commands: &[CommandNode], ctx: &SourceCtx) -> Result<()> {
    for cmd in commands {
        for (kind, patterns) in [("sources", &cmd.sources), ("generates", &cmd.generates)] {
            for pattern in patterns {
                if let Err(e) = globset::Glob::new(pattern) {
                    return Err(
                        ctx.error(format!("invalid {kind} glob '{pattern}': {e}"), cmd.span)
                    );
                }
            }
        }
        validate_sources(&cmd.children, ctx)?;
    }
    Ok(())
}

/// Check that all dep/step references resolve to existing commands and that
/// supplied arguments (plus declared defaults) satisfy the target's CLI.
fn validate_refs(tree: &CommandTree, commands: &[CommandNode], ctx: &SourceCtx) -> Result<()> {
    for cmd in commands {
        for refs in [&cmd.orch.deps, &cmd.orch.steps] {
            for task_ref in refs {
                let display_path = task_ref.display();
                let Some((target, args)) = tree.resolve_ref(task_ref) else {
                    return Err(ctx.error(format!("unknown task '{display_path}'"), task_ref.span));
                };

                // Parse the reference's arguments against the target's real
                // clap definition so bad references fail at load time.
                let clap_cmd = crate::cli::build_subcommand(target, false);
                let argv = std::iter::once(target.name.clone()).chain(args.iter().cloned());
                let matches = match clap_cmd.try_get_matches_from(argv) {
                    Ok(matches) => matches,
                    Err(e) => {
                        return Err(ctx.error(
                            format!(
                                "invalid task reference '{display_path}': {}",
                                clap_error_summary(&e)
                            ),
                            task_ref.span,
                        ));
                    }
                };

                // Valued flags without defaults must be supplied explicitly:
                // they would otherwise interpolate as empty strings.
                for flag in &target.flags {
                    if flag.value_type.is_some()
                        && flag.default.is_none()
                        && !matches.contains_id(&flag.name)
                    {
                        return Err(ctx.error(
                            format!(
                                "'{display_path}' has required valued flags without defaults \
                                 (supply a value in the reference or add default=)"
                            ),
                            task_ref.span,
                        ));
                    }
                }
            }
        }
        validate_refs(tree, &cmd.children, ctx)?;
    }
    Ok(())
}

/// Condense a rendered clap error to its message lines (drops the Usage block).
fn clap_error_summary(err: &clap::Error) -> String {
    let rendered = err.render().to_string();
    let mut parts: Vec<&str> = Vec::new();
    for line in rendered.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Usage:") {
            break;
        }
        parts.push(trimmed.strip_prefix("error:").map_or(trimmed, str::trim));
    }
    parts.join(" ")
}

fn validate_no_cycles(tree: &CommandTree, commands: &[CommandNode], ctx: &SourceCtx) -> Result<()> {
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut stack = Vec::new();

    for cmd in commands {
        detect_cycle(
            tree,
            cmd,
            std::slice::from_ref(&cmd.name),
            &mut stack,
            &mut visiting,
            &mut visited,
            ctx,
        )?;
    }
    Ok(())
}

/// Depth-first search over dependency edges. `stack` is the chain of tasks
/// currently being followed, so a cycle can be reported as the route that
/// forms it rather than as a single name.
fn detect_cycle(
    tree: &CommandTree,
    node: &CommandNode,
    key: &[String],
    stack: &mut Vec<Vec<String>>,
    visiting: &mut HashSet<Vec<String>>,
    visited: &mut HashSet<Vec<String>>,
    ctx: &SourceCtx,
) -> Result<()> {
    let owned = key.to_vec();
    if visited.contains(&owned) {
        return Ok(());
    }
    visiting.insert(owned.clone());
    stack.push(owned.clone());

    for task_ref in node.orch.deps.iter().chain(&node.orch.steps) {
        if let Some((target, args)) = tree.resolve_ref(task_ref) {
            let path = task_ref.tokens[..task_ref.tokens.len() - args.len()].to_vec();
            // Catch the cycle on the edge that closes it: that edge's span
            // is the line the user has to change.
            if visiting.contains(&path) {
                return Err(cycle_error(stack, &path, task_ref.span, ctx));
            }
            detect_cycle(tree, target, &path, stack, visiting, visited, ctx)?;
        }
    }

    stack.pop();
    visiting.remove(&owned);
    visited.insert(owned);

    // Subcommands are tasks in their own right, not steps of their parent,
    // so they are searched outside the parent's dependency chain.
    for child in &node.children {
        let mut child_key = key.to_vec();
        child_key.push(child.name.clone());
        detect_cycle(tree, child, &child_key, stack, visiting, visited, ctx)?;
    }
    Ok(())
}

/// Render the cycle as the route around it: `a → b → a`.
fn cycle_error(
    stack: &[Vec<String>],
    target: &[String],
    span: miette::SourceSpan,
    ctx: &SourceCtx,
) -> Error {
    let start = stack
        .iter()
        .position(|key| key.as_slice() == target)
        .unwrap_or(0);
    let route: Vec<String> = stack[start..]
        .iter()
        .chain(std::iter::once(&target.to_vec()))
        .map(|key| key.join(" "))
        .collect();
    ctx.error_labeled(
        format!("dependency cycle: {}", route.join(" → ")),
        "closes the cycle",
        span,
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::parse::parse_source;
    use crate::tree::CommandTree;

    fn parse(input: &str) -> CommandTree {
        parse_source(input, &PathBuf::from("test.kdl")).unwrap()
    }

    fn parse_err(input: &str) -> String {
        parse_source(input, &PathBuf::from("test.kdl"))
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn unknown_dep_ref() {
        let err = parse_err(
            r#"
            ci {
                deps "lint" "test"
            }
            "#,
        );
        assert!(err.contains("unknown task 'lint'"), "got: {err}");
    }

    #[test]
    fn unknown_step_ref() {
        let err = parse_err(
            r#"
            ci {
                steps "nope"
            }
            "#,
        );
        assert!(err.contains("unknown task 'nope'"), "got: {err}");
    }

    #[test]
    fn dep_with_required_args() {
        let err = parse_err(
            r#"
            greet {
                arg name
                run "echo {name}"
            }
            ci {
                deps "greet"
            }
            "#,
        );
        assert!(err.contains("required arguments"), "got: {err}");
    }

    #[test]
    fn dep_with_optional_args_ok() {
        // Should succeed — args with defaults are fine in deps/steps targets.
        let tree = parse(
            r#"
            greet {
                arg name default="world"
                run "echo hello {name}"
            }
            ci {
                deps "greet"
            }
            "#,
        );
        assert_eq!(tree.commands[1].orch.deps.len(), 1);
    }

    #[test]
    fn direct_cycle() {
        let err = parse_err(
            r#"
            a {
                deps "b"
                run "echo a"
            }
            b {
                deps "a"
                run "echo b"
            }
            "#,
        );
        assert!(err.contains("a → b → a"), "got: {err}");
    }

    #[test]
    fn self_cycle() {
        let err = parse_err(
            r#"
            a {
                deps "a"
                run "echo a"
            }
            "#,
        );
        assert!(err.contains("a → a"), "got: {err}");
    }

    #[test]
    fn indirect_cycle() {
        let err = parse_err(
            r#"
            a {
                deps "b"
                run "echo a"
            }
            b {
                steps "c"
                run "echo b"
            }
            c {
                deps "a"
                run "echo c"
            }
            "#,
        );
        // The whole route, not just the task the search happened to reach.
        assert!(err.contains("a → b → c → a"), "got: {err}");
    }

    #[test]
    fn nested_command_cycle_names_full_paths() {
        let err = parse_err(
            r#"
            db {
                migrate { deps "db reset" }
                reset { deps "db migrate" }
            }
            "#,
        );
        assert!(
            err.contains("db migrate → db reset → db migrate"),
            "got: {err}"
        );
    }

    #[test]
    fn sibling_subcommands_are_not_a_cycle() {
        // Containment is not a dependency edge: a group's children sharing
        // a parent must not read as a route back to it.
        let tree = parse(
            r#"
            db {
                migrate "echo m"
                reset "echo r"
            }
            all { deps "db migrate" "db reset" }
            "#,
        );
        assert_eq!(tree.commands.len(), 2);
    }

    #[test]
    fn dep_with_boolean_flag_ok() {
        // Boolean flags default to false — {?flag:text} produces empty string.
        let tree = parse(
            r#"
            build {
                flag release "-r"
                run "cargo build {?release:--release}"
            }
            ci {
                deps "build"
            }
            "#,
        );
        assert_eq!(tree.commands[1].orch.deps.len(), 1);
    }

    #[test]
    fn dep_with_passthrough_ok() {
        // {--} produces empty string when no trailing args.
        let tree = parse(
            r#"
            test {
                run "cargo test {--}"
            }
            ci {
                deps "test"
            }
            "#,
        );
        assert_eq!(tree.commands[1].orch.deps.len(), 1);
    }

    #[test]
    fn dep_with_env_interpolation_ok() {
        let tree = parse(
            r#"
            serve {
                env PORT="3000"
                run "echo {$PORT}"
            }
            ci {
                deps "serve"
            }
            "#,
        );
        assert_eq!(tree.commands[1].orch.deps.len(), 1);
    }

    #[test]
    fn dep_with_required_valued_flag() {
        let err = parse_err(
            r#"
            deploy {
                flag replicas "-r" type="int"
                run "deploy --replicas {replicas}"
            }
            ci {
                deps "deploy"
            }
            "#,
        );
        assert!(err.contains("required valued flags"), "got: {err}");
    }

    #[test]
    fn dep_supplying_required_arg_ok() {
        let tree = parse(
            r#"
            greet {
                arg name
                run "echo {name}"
            }
            ci {
                deps "greet world"
            }
            "#,
        );
        assert_eq!(tree.commands[1].orch.deps.len(), 1);
    }

    #[test]
    fn dep_supplying_valued_flag_ok() {
        let tree = parse(
            r#"
            deploy {
                flag replicas "-r" type="int" default="1"
                run "deploy --replicas {replicas}"
            }
            ci {
                deps "deploy --replicas 3"
            }
            "#,
        );
        assert_eq!(tree.commands[1].orch.deps.len(), 1);
    }

    #[test]
    fn dep_with_unknown_flag_rejected() {
        let err = parse_err(
            r#"
            build "echo hi"
            ci {
                deps "build --nope"
            }
            "#,
        );
        assert!(err.contains("invalid task reference"), "got: {err}");
    }

    #[test]
    fn dep_with_invalid_choice_rejected() {
        let err = parse_err(
            r#"
            deploy {
                arg environment "dev" "prod"
                run "echo {environment}"
            }
            ci {
                deps "deploy banana"
            }
            "#,
        );
        assert!(err.contains("invalid task reference"), "got: {err}");
    }
}
