mod common;

use common::{lets_bin, with_temp_kdl};

#[test]
fn confirm_with_yes_flag() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            confirm "Are you sure?"
            run "echo DEPLOYED"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--yes", "deploy"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DEPLOYED"));
}

#[test]
fn confirm_without_yes_flag_non_tty() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            confirm "Are you sure?"
            run "echo DEPLOYED"
        }
        "#,
    );

    // Without --yes and without a TTY (piped stdin), dialoguer should error/abort.
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("DEPLOYED"));
}

#[test]
fn prompt_with_yes_uses_default() {
    let (_dir, path) = with_temp_kdl(
        r#"
        greet {
            prompt name "What is your name?" default="world"
            run "echo hello {name}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--yes", "greet"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello world"));
}

#[test]
fn choose_with_yes_uses_default() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            choose environment "dev" "staging" "prod" default="staging"
            run "echo deploying to {environment}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--yes", "deploy"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("deploying to staging"));
}

#[test]
fn choose_with_yes_and_no_default_errors() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            choose environment "dev" "staging" "prod"
            run "echo deploying to {environment}"
        }
        "#,
    );

    // Guessing an environment non-interactively would be dangerous;
    // --yes without a default must fail, not pick the first choice.
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--yes", "deploy"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("deploying to"), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no default"), "{stderr}");
}

#[test]
fn choose_with_invalid_default_rejected_at_load() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            choose environment "dev" "prod" default="banana"
            run "echo deploying to {environment}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not one of the choices"), "{stderr}");
}

#[test]
fn choose_and_confirm_with_yes() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            choose environment "dev" "staging" "prod" default="dev"
            confirm "Deploy to {environment}?"
            run "echo deploying to {environment}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--yes", "deploy"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("deploying to dev"));
}

#[test]
fn multiple_prompts_with_yes() {
    let (_dir, path) = with_temp_kdl(
        r#"
        setup {
            prompt user "Username?" default="admin"
            prompt host "Hostname?" default="localhost"
            run "echo {user}@{host}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--yes", "setup"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("admin@localhost"));
}

#[test]
fn dep_confirm_blocks_without_yes_non_tty() {
    let (_dir, path) = with_temp_kdl(
        r#"
        danger {
            confirm "Really?"
            run "echo DANGER-RAN"
        }
        caller {
            deps "danger"
            run "echo MAIN-RAN"
        }
        "#,
    );

    // A confirm-guarded task must never run silently when pulled in as a dep.
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "caller"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("DANGER-RAN"), "guarded dep ran:\n{stdout}");
    assert!(!stdout.contains("MAIN-RAN"), "main ran:\n{stdout}");
}

#[test]
fn dep_confirm_with_yes_runs() {
    let (_dir, path) = with_temp_kdl(
        r#"
        danger {
            confirm "Really?"
            run "echo DANGER-RAN"
        }
        caller {
            deps "danger"
            run "echo MAIN-RAN"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--yes", "caller"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DANGER-RAN"));
    assert!(stdout.contains("MAIN-RAN"));
}

#[test]
fn step_prompt_default_interpolates_with_yes() {
    let (_dir, path) = with_temp_kdl(
        r#"
        greet {
            prompt name "Who?" default="world"
            run "echo HELLO-{name}"
        }
        wrapper {
            steps "greet"
            run "echo WRAPPED"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--yes", "wrapper"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("HELLO-world"),
        "prompt default not used:\n{stdout}"
    );
    assert!(stdout.contains("WRAPPED"));
}
