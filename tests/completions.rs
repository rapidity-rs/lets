mod common;

use common::{lets_bin, with_temp_kdl};
use std::fs;

#[test]
fn list_command() {
    let (_dir, path) = with_temp_kdl(
        r#"
        description "Test project"
        build "cargo build"
        db {
            description "Database commands"
            migrate "echo migrate"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--list"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Test project"));
    assert!(stdout.contains("build"));
    assert!(stdout.contains("db"));
    assert!(stdout.contains("migrate"));
}

#[test]
fn check_valid_config() {
    let (_dir, path) = with_temp_kdl(
        r#"
        build "cargo build"
        test "cargo test"
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "self", "check"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("valid"));
    assert!(stdout.contains("2 commands"));
}

#[test]
fn completions_generates_output() {
    let (_dir, path) = with_temp_kdl(r#"build "cargo build""#);

    let output = lets_bin()
        .args([
            "--file",
            path.to_str().unwrap(),
            "self",
            "completions",
            "bash",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("lets"));
    assert!(stdout.contains("complete") || stdout.contains("compadd") || stdout.contains("_lets"));
}

#[test]
fn completions_zsh() {
    let (_dir, path) = with_temp_kdl(r#"build "cargo build""#);

    let output = lets_bin()
        .args([
            "--file",
            path.to_str().unwrap(),
            "self",
            "completions",
            "zsh",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("_lets"));
}

#[test]
fn init_creates_kdl() {
    let dir = tempfile::tempdir().unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lets"))
        .args(["self", "init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Created lets.kdl"));

    let content = fs::read_to_string(dir.path().join("lets.kdl")).unwrap();
    assert!(content.contains("description"));
}

#[test]
fn init_refuses_if_exists() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("lets.kdl"), "existing").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lets"))
        .args(["self", "init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already exists"));
}

#[test]
fn init_detects_cargo_project() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lets"))
        .args(["self", "init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = fs::read_to_string(dir.path().join("lets.kdl")).unwrap();
    assert!(content.contains("cargo build"));
    assert!(content.contains("cargo test"));
}
