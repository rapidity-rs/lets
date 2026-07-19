mod common;

use common::{lets_bin, with_temp_kdl};

fn run(path: &std::path::Path, cmd: &[&str]) -> std::process::Output {
    let mut args = vec!["--file", path.to_str().unwrap()];
    args.extend_from_slice(cmd);
    lets_bin().args(&args).output().unwrap()
}

#[test]
fn global_var_interpolates() {
    let (_dir, path) = with_temp_kdl(
        r#"
        vars {
            registry "ghcr.io/acme"
        }
        push "echo pushing to {registry}"
        "#,
    );

    let output = run(&path, &["push"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("pushing to ghcr.io/acme"));
}

#[test]
fn var_can_reference_earlier_var() {
    let (_dir, path) = with_temp_kdl(
        r#"
        vars {
            registry "ghcr.io/acme"
            image "{registry}/app"
        }
        push "echo pushing {image}:latest"
        "#,
    );

    let output = run(&path, &["push"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("pushing ghcr.io/acme/app:latest"));
}

#[test]
fn command_var_overrides_global() {
    let (_dir, path) = with_temp_kdl(
        r#"
        vars {
            env-name "prod"
        }
        deploy {
            vars {
                env-name "staging"
            }
            run "echo deploying to {env-name}"
        }
        "#,
    );

    let output = run(&path, &["deploy"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("deploying to staging"));
}

#[test]
fn group_vars_inherited_by_children() {
    let (_dir, path) = with_temp_kdl(
        r#"
        db {
            vars {
                dsn "postgres://localhost/dev"
            }
            migrate "echo migrating {dsn}"
        }
        "#,
    );

    let output = run(&path, &["db", "migrate"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("migrating postgres://localhost/dev"));
}

#[test]
fn arg_shadows_var() {
    let (_dir, path) = with_temp_kdl(
        r#"
        vars {
            target "default-target"
        }
        build {
            arg target default="from-arg"
            run "echo building {target}"
        }
        "#,
    );

    // Declared args/flags win over config vars.
    let output = run(&path, &["build"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("building from-arg"));

    let output = run(&path, &["build", "cli-value"]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("building cli-value"));
}

#[test]
fn vars_work_in_dep_tasks() {
    let (_dir, path) = with_temp_kdl(
        r#"
        vars {
            name "world"
        }
        greet "echo hello {name}"
        main {
            deps "greet"
            run "echo done"
        }
        "#,
    );

    let output = run(&path, &["main"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("hello world"));
}

#[test]
fn undeclared_placeholder_renders_empty() {
    // Regression: unknown {names} used to reach clap and panic.
    let (_dir, path) = with_temp_kdl(
        r#"
        build {
            flag verbose "-v"
            run "echo start{missing}end"
        }
        "#,
    );

    let output = run(&path, &["build"]);
    assert!(
        output.status.success(),
        "unknown placeholder must not crash"
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("startend"));
}
