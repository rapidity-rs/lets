mod common;

use common::{lets_bin, with_temp_kdl};

/// Two parallel deps whose multi-line output would interleave without
/// buffering: each line sleeps so both tasks are in flight simultaneously.
const RACY_DEPS: &str = r#"
    a "sh -c 'echo A1; sleep 0.05; echo A2; sleep 0.05; echo A3'"
    b "sh -c 'echo B1; sleep 0.05; echo B2; sleep 0.05; echo B3'"
    all {
        deps "a" "b"
        run "echo DONE"
    }
"#;

#[test]
fn group_mode_keeps_task_output_contiguous() {
    let config = format!("config {{ output \"group\" }}\n{RACY_DEPS}");
    let (_dir, path) = with_temp_kdl(&config);

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "all"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Each task's block must be contiguous, preceded by its label header.
    assert!(
        stdout.contains("[a]\nA1\nA2\nA3"),
        "block a torn:\n{stdout}"
    );
    assert!(
        stdout.contains("[b]\nB1\nB2\nB3"),
        "block b torn:\n{stdout}"
    );
    assert!(stdout.contains("DONE"));
}

#[test]
fn prefixed_mode_labels_every_line() {
    let config = format!("config {{ output \"prefixed\" }}\n{RACY_DEPS}");
    let (_dir, path) = with_temp_kdl(&config);

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "all"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in ["[a] A1", "[a] A2", "[a] A3", "[b] B1", "[b] B2", "[b] B3"] {
        assert!(stdout.contains(line), "missing '{line}':\n{stdout}");
    }
    // The root command itself stays unprefixed.
    assert!(stdout.contains("\nDONE") || stdout.starts_with("DONE"));
}

#[test]
fn interleaved_is_default_and_unlabeled() {
    let (_dir, path) = with_temp_kdl(RACY_DEPS);

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "all"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("A1"));
    assert!(stdout.contains("B1"));
    assert!(!stdout.contains("[a]"), "no labels expected:\n{stdout}");
}

#[test]
fn output_flag_overrides_config() {
    let config = format!("config {{ output \"interleaved\" }}\n{RACY_DEPS}");
    let (_dir, path) = with_temp_kdl(&config);

    let output = lets_bin()
        .args([
            "--file",
            path.to_str().unwrap(),
            "--output",
            "prefixed",
            "all",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[a] A1"), "flag not applied:\n{stdout}");
}

#[test]
fn group_mode_flushes_failed_task_output() {
    let (_dir, path) = with_temp_kdl(
        r#"
        config { output "group" }
        broken "sh -c 'echo BEFORE-FAIL; exit 3'"
        all {
            deps "broken"
            run "echo NEVER"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "all"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[broken]\nBEFORE-FAIL"),
        "failed task output lost:\n{stdout}"
    );
    assert!(!stdout.contains("NEVER"));
}

#[test]
fn silent_dep_stays_quiet_in_group_mode() {
    let (_dir, path) = with_temp_kdl(
        r#"
        config { output "group" }
        quiet {
            silent
            run "echo SHOULD-NOT-APPEAR"
        }
        all {
            deps "quiet"
            run "echo DONE"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "all"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("SHOULD-NOT-APPEAR"), "{stdout}");
    assert!(stdout.contains("DONE"));
}

#[test]
fn silent_dep_dumps_labeled_output_on_failure() {
    let (_dir, path) = with_temp_kdl(
        r#"
        quiet {
            silent
            run "sh -c 'echo HIDDEN-UNTIL-FAILURE; exit 1'"
        }
        all {
            deps "quiet"
            run "echo NEVER"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "all"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("HIDDEN-UNTIL-FAILURE"),
        "silent failure output lost:\n{stdout}"
    );
}

#[test]
fn invalid_output_mode_rejected_at_load() {
    let (_dir, path) = with_temp_kdl(
        r#"
        config { output "fancy" }
        a "echo hi"
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "a"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid output mode"), "got:\n{stderr}");
}

#[test]
fn prefixed_mode_merges_stderr() {
    let (_dir, path) = with_temp_kdl(
        r#"
        config { output "prefixed" }
        warns "sh -c 'echo to-stdout; echo to-stderr >&2'"
        all {
            deps "warns"
            run "echo DONE"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "all"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[warns] to-stdout"), "{stdout}");
    assert!(stdout.contains("[warns] to-stderr"), "{stdout}");
}
