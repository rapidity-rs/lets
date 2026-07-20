//! Tooling surface: --list --json, min-version gate, and the bare-invocation
//! fallback when no terminal is attached.

mod common;

use common::{lets_bin, with_temp_kdl};

fn run_file(path: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut argv = vec!["--file", path.to_str().unwrap()];
    argv.extend_from_slice(args);
    lets_bin().args(&argv).output().unwrap()
}

#[test]
fn list_json_is_valid_and_complete() {
    let (_dir, path) = with_temp_kdl(
        r#"
        description "My project"
        build {
            description "Build it"
            alias "b"
            arg target "debug" "release" default="debug"
            flag threads "-t" type="int" default="4"
            flag release
            run "cargo build {target} -j {threads}"
        }
        helper {
            hide
            run "echo helper"
        }
        db {
            description "Database"
            migrate "diesel migration run"
        }
        "#,
    );

    let output = run_file(&path, &["--list", "--json"]);
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");

    assert_eq!(json["description"], "My project");
    let commands = json["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 3);

    let build = &commands[0];
    assert_eq!(build["name"], "build");
    assert_eq!(build["aliases"][0], "b");
    assert_eq!(build["hidden"], false);
    assert_eq!(build["runnable"], true);
    assert_eq!(build["args"][0]["name"], "target");
    assert_eq!(build["args"][0]["choices"][1], "release");
    assert_eq!(build["args"][0]["default"], "debug");
    assert_eq!(build["flags"][0]["name"], "threads");
    assert_eq!(build["flags"][0]["type"], "int");
    assert_eq!(build["flags"][0]["short"], "t");
    assert_eq!(build["flags"][1]["type"], "bool");

    let helper = &commands[1];
    assert_eq!(helper["hidden"], true);

    let db = &commands[2];
    assert_eq!(db["runnable"], false);
    assert_eq!(db["commands"][0]["name"], "migrate");
}

#[test]
fn json_escapes_special_characters() {
    let (_dir, path) =
        with_temp_kdl("greet \"echo hi\" description=\"say \\\"hi\\\" \\\\ wave\"\n");

    let output = run_file(&path, &["--list", "--json"]);
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON despite quotes/backslashes");
    assert_eq!(json["commands"][0]["description"], "say \"hi\" \\ wave");
}

#[test]
fn json_requires_list() {
    let (_dir, path) = with_temp_kdl("hello \"echo hi\"\n");
    let output = run_file(&path, &["--json", "hello"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--list"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn min_version_gate_blocks_old_binary() {
    let (_dir, path) = with_temp_kdl(
        r#"
        config {
            min-version "99.0.0"
        }
        hello "echo hi"
        "#,
    );

    let output = run_file(&path, &["hello"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("requires lets >= 99.0.0"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains(env!("CARGO_PKG_VERSION")),
        "stderr: {stderr}"
    );
}

#[test]
fn min_version_gate_passes_current_binary() {
    let (_dir, path) = with_temp_kdl(
        r#"
        config {
            min-version "0.1.0"
        }
        hello "echo hi"
        "#,
    );

    let output = run_file(&path, &["hello"]);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hi");
}

#[test]
fn min_version_gate_precedes_parse_strictness() {
    // A config using hypothetical future syntax (unknown config option)
    // must report the version requirement, not the syntax error.
    let (_dir, path) = with_temp_kdl(
        r#"
        config {
            min-version "99.0.0"
            hyper-drive "engaged"
        }
        hello "echo hi"
        "#,
    );

    let output = run_file(&path, &["hello"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("requires lets >= 99.0.0"),
        "stderr: {stderr}"
    );
    assert!(
        !stderr.contains("unknown config option"),
        "stderr: {stderr}"
    );
}

#[test]
fn bare_invocation_without_tty_prints_help() {
    let (_dir, path) = with_temp_kdl("hello \"echo hi\" description=\"Say hello\"\n");

    // stdin/stdout are pipes here, so no picker: help text is printed.
    let output = run_file(&path, &[]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:"), "stdout: {stdout}");
    assert!(stdout.contains("hello"), "stdout: {stdout}");
}
