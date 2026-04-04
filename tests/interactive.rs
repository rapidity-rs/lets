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
fn choose_with_yes_uses_first() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            choose environment "dev" "staging" "prod"
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
fn choose_and_confirm_with_yes() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            choose environment "dev" "staging" "prod"
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
