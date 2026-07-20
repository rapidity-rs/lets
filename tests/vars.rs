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
fn undeclared_placeholder_fails_at_load() {
    // Unknown {names} are config bugs: they must fail loudly at load time,
    // never render as empty strings (and never reach clap and panic).
    let (_dir, path) = with_temp_kdl(
        r#"
        build {
            flag verbose "-v"
            run "echo start{missing}end"
        }
        "#,
    );

    let output = run(&path, &["build"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unresolved placeholder"),
        "stderr was: {stderr}"
    );
    assert!(stderr.contains("missing"), "stderr was: {stderr}");
}

#[test]
fn escaped_braces_render_literally() {
    let (_dir, path) = with_temp_kdl(
        r#"
        columns {
            run "echo 'x y' | awk '{{print $1}}'"
        }
        "#,
    );

    let output = run(&path, &["columns"]);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "x");
}

#[test]
fn hooks_and_gates_interpolate() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            arg target default="staging"
            vars {
                registry "ghcr.io/acme"
            }
            precondition "test -n '{registry}'"
            before "echo before={target}"
            defer "echo defer={target}"
            run "echo run={registry}"
            after "echo after={target}"
        }
        "#,
    );

    let output = run(&path, &["deploy", "prod"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("before=prod"), "stdout was: {stdout}");
    assert!(stdout.contains("run=ghcr.io/acme"), "stdout was: {stdout}");
    assert!(stdout.contains("after=prod"), "stdout was: {stdout}");
    assert!(stdout.contains("defer=prod"), "stdout was: {stdout}");
}

#[test]
fn env_values_interpolate_vars() {
    let (_dir, path) = with_temp_kdl(
        r#"
        vars {
            region "eu-west-1"
        }
        show {
            env DEPLOY_REGION="{region}"
            run "printenv DEPLOY_REGION"
        }
        "#,
    );

    let output = run(&path, &["show"]);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "eu-west-1");
}

#[test]
fn args_and_flags_exported_as_env() {
    let (_dir, path) = with_temp_kdl(
        r#"
        build {
            arg target default="debug"
            flag release "-r"
            flag threads "-t" type="int" default="4"
            run "echo arg=$LETS_ARG_TARGET flag=${{LETS_FLAG_RELEASE:-0}} threads=$LETS_FLAG_THREADS"
        }
        "#,
    );

    let output = run(&path, &["build", "fast", "--release", "-t", "8"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("arg=fast flag=1 threads=8"),
        "stdout was: {stdout}"
    );

    let output = run(&path, &["build"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("arg=debug flag=0 threads=4"),
        "stdout was: {stdout}"
    );
}

#[test]
fn passthrough_preserves_quoting() {
    let (_dir, path) = with_temp_kdl(
        r#"
        show {
            run "printf '[%s]\n' {--}"
        }
        "#,
    );

    let output = run(&path, &["show", "--", "foo bar", "baz"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "[foo bar]\n[baz]", "stdout was: {stdout}");
}

#[test]
fn boolean_flag_interpolation_is_load_error() {
    let (_dir, path) = with_temp_kdl(
        r#"
        build {
            flag release "-r"
            run "cargo build {release}"
        }
        "#,
    );

    let output = run(&path, &["build"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("boolean flag 'release'"),
        "stderr was: {stderr}"
    );
}

#[test]
fn unknown_var_reference_is_load_error() {
    let (_dir, path) = with_temp_kdl(
        r#"
        vars {
            image "{registry}/app"
        }
        push "docker push {image}"
        "#,
    );

    let output = run(&path, &["push"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("in var 'image'"), "stderr was: {stderr}");
}
