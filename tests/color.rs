//! Colour handling: nothing styled reaches a pipe, a log, or a terminal
//! that asked for plain text.

mod common;

use common::{lets_bin, with_temp_kdl};

const ESC: char = '\u{1b}';

/// Every surface that prints something lets generated itself. Task output
/// is whatever the child emits and is not ours to strip.
const CONFIG: &str = r#"
description "Colour surfaces"

lint "echo linting"
test "echo testing"

ci {
    description "Everything at once"
    deps "lint" "test"
    run "echo done"
}

skipme {
    description "Reports up to date"
    status "true"
    run "echo never"
}

boom {
    description "Fails"
    run "sh -c 'exit 3'"
}

old {
    deprecated "use 'ci'"
    run "echo ran"
}
"#;

const INVOCATIONS: &[&[&str]] = &[
    &["--list"],
    &["--help"],
    &["ci"],
    &["--summary", "ci"],
    &["--verbose", "ci"],
    &["--output", "prefixed", "ci"],
    &["--output", "group", "ci"],
    &["skipme"],
    &["boom"],
    &["old"],
    &["nosuchtask"],
    &["self", "check"],
];

fn run(args: &[&str], env: &[(&str, &str)]) -> (String, String) {
    let (_dir, path) = with_temp_kdl(CONFIG);
    let mut argv = vec!["--file", path.to_str().unwrap()];
    argv.extend_from_slice(args);
    let mut cmd = lets_bin();
    cmd.args(&argv);
    // Inherited CI settings would otherwise decide this for us.
    cmd.env_remove("CLICOLOR_FORCE").env_remove("NO_COLOR");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let output = cmd.output().unwrap();
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Test output is piped, so nothing should be styled — this is the case
/// that used to write escape codes straight into logs and files.
#[test]
fn redirected_output_is_never_styled() {
    for args in INVOCATIONS {
        let (stdout, stderr) = run(args, &[]);
        assert!(!stdout.contains(ESC), "{args:?} stdout: {stdout:?}");
        assert!(!stderr.contains(ESC), "{args:?} stderr: {stderr:?}");
    }
}

/// `--color=always` is the escape hatch for piping into a pager or a log
/// viewer that renders ANSI.
#[test]
fn color_always_styles_a_pipe() {
    let (stdout, _) = run(&["--color", "always", "--list"], &[]);
    assert!(stdout.contains(ESC), "stdout: {stdout:?}");

    let (_, stderr) = run(&["--color", "always", "boom"], &[]);
    assert!(stderr.contains(ESC), "stderr: {stderr:?}");
}

/// NO_COLOR must win over a forced-on environment, and `--color=never`
/// must win over everything.
#[test]
fn suppression_beats_forcing() {
    let (stdout, _) = run(&["--list"], &[("CLICOLOR_FORCE", "1")]);
    assert!(stdout.contains(ESC), "CLICOLOR_FORCE ignored: {stdout:?}");

    let (stdout, _) = run(&["--list"], &[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")]);
    assert!(!stdout.contains(ESC), "NO_COLOR ignored: {stdout:?}");

    let (stdout, _) = run(
        &["--color", "never", "--list"],
        &[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")],
    );
    assert!(!stdout.contains(ESC), "--color=never ignored: {stdout:?}");

    let (stdout, _) = run(&["--color", "always", "--list"], &[("NO_COLOR", "1")]);
    assert!(
        stdout.contains(ESC),
        "an explicit --color=always should win: {stdout:?}"
    );
}

/// An empty or "0" value is the conventional way to say "unset".
#[test]
fn empty_and_zero_env_values_do_not_suppress() {
    for value in ["", "0"] {
        let (stdout, _) = run(&["--color", "always", "--list"], &[("NO_COLOR", value)]);
        assert!(stdout.contains(ESC), "NO_COLOR={value:?}: {stdout:?}");
    }
}

/// `--color` reaches clap's own help and error rendering, not just ours.
#[test]
fn clap_output_follows_the_same_choice() {
    let (stdout, _) = run(&["--color", "always", "--help"], &[]);
    assert!(stdout.contains(ESC), "help: {stdout:?}");

    let (_, stderr) = run(&["--color", "always", "nosuchtask"], &[]);
    assert!(stderr.contains(ESC), "clap error: {stderr:?}");
}

/// Machine-readable output is never a styling surface, whatever is asked.
#[test]
fn json_listing_is_always_plain() {
    let (stdout, _) = run(&["--color", "always", "--list", "--json"], &[]);
    assert!(!stdout.contains(ESC), "stdout: {stdout:?}");
}

/// Grouped, prefixed, and silent tasks are read through a pipe, so a child
/// sees no terminal and drops its own colour — even when the output is on
/// its way to one. lets asks it to keep colour instead, but only when lets'
/// own stdout is styled.
#[test]
fn captured_children_are_told_to_keep_their_color() {
    let config = r#"
        show "sh -c 'echo CF=$CLICOLOR_FORCE FC=$FORCE_COLOR'"
        wrap { deps "show" }
        override {
            env CLICOLOR_FORCE="0"
            run "sh -c 'echo CF=$CLICOLOR_FORCE'"
        }
    "#;

    let (_dir, path) = with_temp_kdl(config);
    let run = |args: &[&str]| {
        let mut argv = vec!["--file", path.to_str().unwrap()];
        argv.extend_from_slice(args);
        let output = lets_bin()
            .args(&argv)
            .env_remove("CLICOLOR_FORCE")
            .env_remove("FORCE_COLOR")
            .env_remove("NO_COLOR")
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    // Test output is piped, so lets is not styling anything and must not
    // push colour onto children either.
    let piped = run(&["--output", "prefixed", "wrap"]);
    assert!(piped.contains("CF= FC="), "stdout: {piped:?}");

    // With colour forced on, a captured child is asked to keep its own.
    let forced = run(&["--color", "always", "--output", "prefixed", "wrap"]);
    assert!(forced.contains("CF=1 FC=1"), "stdout: {forced:?}");

    // A task that sets the variable itself still wins.
    let overridden = run(&["--color", "always", "--output", "prefixed", "override"]);
    assert!(overridden.contains("CF=0"), "stdout: {overridden:?}");
}
