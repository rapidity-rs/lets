//! Command tree validation.
//!
//! Runs after parsing to catch structural errors:
//! - All `deps`/`steps` references resolve to existing commands
//! - Referenced commands have no required arguments (since deps/steps can't supply them)
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

/// Check that all dep/step references resolve to existing commands with no required args.
fn validate_refs(tree: &CommandTree, commands: &[CommandNode], ctx: &SourceCtx) -> Result<()> {
    for cmd in commands {
        for refs in [&cmd.orch.deps, &cmd.orch.steps] {
            for (task_path, ref_span) in refs {
                let display_path = task_path.join(" ");
                let target = tree.resolve_path(task_path).ok_or_else(|| {
                    ctx.error(format!("unknown task '{display_path}'"), *ref_span)
                })?;

                let has_required_args = target.args.iter().any(|a| a.default.is_none());
                if has_required_args {
                    return Err(ctx.error(
                        format!(
                            "'{display_path}' has required arguments \
                             (deps/steps cannot supply arguments)"
                        ),
                        *ref_span,
                    ));
                }

                let has_required_flags = target
                    .flags
                    .iter()
                    .any(|f| f.value_type.is_some() && f.default.is_none());
                if has_required_flags {
                    return Err(ctx.error(
                        format!(
                            "'{display_path}' has required valued flags without defaults \
                             (deps/steps cannot supply flag values)"
                        ),
                        *ref_span,
                    ));
                }
            }
        }
        validate_refs(tree, &cmd.children, ctx)?;
    }
    Ok(())
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
        for (task_path, _span) in refs {
            if let Some(target) = tree.resolve_path(task_path) {
                detect_cycle(tree, target, task_path, visiting, visited)?;
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
}
