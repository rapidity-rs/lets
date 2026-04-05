mod common;

use common::{lets_bin, with_temp_kdl};

#[test]
fn help_shows_description_and_commands() {
    let (_dir, path) = with_temp_kdl(
        r#"
        description "Test project"
        build "echo building"
        test "echo testing"
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--help"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Test project"));
    assert!(stdout.contains("build"));
    assert!(stdout.contains("test"));
}

#[test]
fn runs_one_liner_command() {
    let (_dir, path) = with_temp_kdl(r#"greet "echo hello-lets""#);

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "greet"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello-lets"));
}

#[test]
fn runs_nested_subcommand() {
    let (_dir, path) = with_temp_kdl(
        r#"
        db {
            description "Database commands"
            ping "echo db-pong"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "db", "ping"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("db-pong"));
}

#[test]
fn subcommand_help_shows_children() {
    let (_dir, path) = with_temp_kdl(
        r#"
        db {
            description "Database commands"
            migrate "echo migrate"
            reset "echo reset"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "db", "--help"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Database commands"));
    assert!(stdout.contains("migrate"));
    assert!(stdout.contains("reset"));
}

#[test]
fn command_failure_propagates_exit_code() {
    let (_dir, path) = with_temp_kdl(r#"fail "exit 42""#);

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "fail"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn bare_invocation_shows_help() {
    let (_dir, path) = with_temp_kdl(
        r#"
        description "My project"
        build "echo build"
        db {
            description "Database"
            migrate "echo migrate"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("My project"));
    assert!(stdout.contains("build"));
    assert!(stdout.contains("db"));
}

#[test]
fn missing_config_shows_help_with_hint() {
    let dir = tempfile::tempdir().unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lets"))
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Bare invocation with no config shows help (exit 0) with a hint on stderr.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("self"), "should show self command in help");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No lets.kdl found"));
    assert!(stderr.contains("lets self init"));
}
