//! Command tree validation.
//!
//! Runs after parsing to catch structural errors:
//! - All `deps`/`steps` references resolve to existing commands
//! - Arguments supplied in a reference (or defaults) satisfy the target's CLI
//! - No dependency cycles exist (direct or indirect, via DFS)

use std::collections::HashSet;

use crate::error::{Error, Result};
use crate::parse::SourceCtx;
use crate::tree::{CommandNode, CommandTree};

/// Validate the command tree: check refs resolve and no cycles exist.
pub fn validate(tree: &CommandTree, ctx: &SourceCtx) -> Result<()> {
    validate_refs(tree, &tree.commands, ctx)?;
    validate_no_cycles(tree, &tree.commands)?;
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

fn validate_no_cycles(tree: &CommandTree, commands: &[CommandNode]) -> Result<()> {
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();

    for cmd in commands {
        detect_cycle(
            tree,
            cmd,
            std::slice::from_ref(&cmd.name),
            &mut visiting,
            &mut visited,
        )?;
    }
    Ok(())
}

fn detect_cycle(
    tree: &CommandTree,
    node: &CommandNode,
    node_path: &[String],
    visiting: &mut HashSet<Vec<String>>,
    visited: &mut HashSet<Vec<String>>,
) -> Result<()> {
    let key = node_path.to_vec();

    if visited.contains(&key) {
        return Ok(());
    }
    if !visiting.insert(key.clone()) {
        return Err(Error::CycleDetected {
            cycle: node_path.join(" → "),
        });
    }

    for refs in [&node.orch.deps, &node.orch.steps] {
        for task_ref in refs {
            if let Some((target, args)) = tree.resolve_ref(task_ref) {
                let path = &task_ref.tokens[..task_ref.tokens.len() - args.len()];
                detect_cycle(tree, target, path, visiting, visited)?;
            }
        }
    }

    // Also recurse into child commands to check their deps/steps.
    for child in &node.children {
        let mut child_path = node_path.to_vec();
        child_path.push(child.name.clone());
        detect_cycle(tree, child, &child_path, visiting, visited)?;
    }

    visiting.remove(&key);
    visited.insert(key);
    Ok(())
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
        assert!(err.contains("cycle"), "got: {err}");
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
        assert!(err.contains("cycle"), "got: {err}");
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
        assert!(err.contains("cycle"), "got: {err}");
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
                flag replicas "-r" type="int"
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
