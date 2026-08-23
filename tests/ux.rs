//! Execution UX: command echo, --keep-going aggregation, --summary table,
//! and CI fold markers in group mode.

mod common;

use common::{lets_bin, with_temp_kdl};

fn run_file(path: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut argv = vec!["--file", path.to_str().unwrap()];
    argv.extend_from_slice(args);
    lets_bin().args(&argv).output().unwrap()
}

#[test]
fn verbose_echoes_commands() {
    let (_dir, path) = with_temp_kdl("hello \"echo hi\"\n");

    let output = run_file(&path, &["--verbose", "hello"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Non-TTY: plain `$ cmd` line before the output.
    assert!(stdout.contains("$ echo hi"), "stdout: {stdout}");
    assert!(stdout.contains("hi"), "stdout: {stdout}");

    // Without --verbose or config echo, no echo line.
    let output = run_file(&path, &["hello"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("$ echo hi"), "stdout: {stdout}");
}

#[test]
fn config_echo_enables_echo() {
    let (_dir, path) = with_temp_kdl(
        r#"
        config {
            echo
        }
        hello "echo hi"
        "#,
    );

    let output = run_file(&path, &["hello"]);
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("$ echo hi"),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn echo_routes_through_group_sink() {
    // In group mode the echo line must land inside the task's block,
    // after its [label] header — not interleaved before it.
    let (_dir, path) = with_temp_kdl(
        r#"
        config {
            output "group"
            echo
        }
        child "echo from-child"
        all {
            deps "child"
            run "echo root"
        }
        "#,
    );

    let output = run_file(&path, &["all"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let header = stdout.find("[child]").expect("group header");
    let echo = stdout.find("$ echo from-child").expect("echo line");
    assert!(echo > header, "echo must be inside the block: {stdout}");
}

#[test]
fn keep_going_reports_all_step_failures() {
    let (_dir, path) = with_temp_kdl(
        r#"
        one "sh -c 'echo one-ran; exit 1'"
        two "echo two-ran"
        three "sh -c 'echo three-ran; exit 1'"
        ci {
            steps "one" "two" "three"
        }
        "#,
    );

    // Fail-fast: step two and three never run.
    let output = run_file(&path, &["ci"]);
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("one-ran"), "stdout: {stdout}");
    assert!(!stdout.contains("two-ran"), "stdout: {stdout}");

    // Keep-going: everything runs, both failures reported.
    let output = run_file(&path, &["--keep-going", "ci"]);
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("two-ran"), "stdout: {stdout}");
    assert!(stdout.contains("three-ran"), "stdout: {stdout}");
    assert!(stderr.contains("2 tasks failed"), "stderr: {stderr}");
    assert!(stderr.contains("task 'one' failed"), "stderr: {stderr}");
    assert!(stderr.contains("task 'three' failed"), "stderr: {stderr}");
}

#[test]
fn single_failure_names_the_task_and_command() {
    let (_dir, path) = with_temp_kdl("boom \"sh -c 'exit 3'\"\n");

    let output = run_file(&path, &["boom"]);
    // The failing task's exit code carries through, so `lets` substitutes
    // for the command it wraps.
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("task 'boom' failed"), "stderr: {stderr}");
    assert!(stderr.contains("exit code 3"), "stderr: {stderr}");
    assert!(stderr.contains("sh -c 'exit 3'"), "stderr: {stderr}");
}

#[test]
fn nested_task_failure_names_the_full_path() {
    let (_dir, path) = with_temp_kdl(
        r#"
        db {
            migrate "exit 1"
        }
        "#,
    );

    let output = run_file(&path, &["db", "migrate"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("task 'db migrate' failed"),
        "stderr: {stderr}"
    );
}

#[test]
fn fail_fast_reports_the_steps_that_never_ran() {
    let (_dir, path) = with_temp_kdl(
        r#"
        one "echo one-ran"
        two "exit 1"
        three "echo three-ran"
        four "echo four-ran"
        ci {
            steps "one" "two" "three" "four"
        }
        "#,
    );

    let output = run_file(&path, &["ci"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("task 'two' failed"), "stderr: {stderr}");
    assert!(stderr.contains("did not run"), "stderr: {stderr}");
    assert!(stderr.contains("three"), "stderr: {stderr}");
    assert!(stderr.contains("four"), "stderr: {stderr}");
}

#[test]
fn keep_going_reports_a_lone_failure() {
    // A single failure used to exit silently: --keep-going promises to
    // report every failure, including when there is only one.
    let (_dir, path) = with_temp_kdl(
        r#"
        ok "echo fine"
        bad "exit 1"
        ci {
            steps "ok" "bad"
        }
        "#,
    );

    let output = run_file(&path, &["--keep-going", "ci"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("task 'bad' failed"), "stderr: {stderr}");
}

#[test]
fn shared_failed_dependency_is_reported_once() {
    // `bad` is a dep of both `a` and `b`; the second reference finds the
    // memoized failure and must not read as an independent one.
    let (_dir, path) = with_temp_kdl(
        r#"
        bad "exit 1"
        a { deps "bad" }
        b { deps "bad" }
        all {
            deps "a" "b"
        }
        "#,
    );

    let output = run_file(&path, &["all"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("task 'bad' failed with exit code 1").count(),
        1,
        "stderr: {stderr}"
    );
}

#[test]
fn parallel_dep_failures_aggregate() {
    let (_dir, path) = with_temp_kdl(
        r#"
        bad-a "exit 1"
        bad-b "exit 2"
        all {
            deps "bad-a" "bad-b"
            run "echo never"
        }
        "#,
    );

    let output = run_file(&path, &["all"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("2 tasks failed"), "stderr: {stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("never"), "stdout: {stdout}");
}

#[test]
fn summary_lists_tasks_with_status() {
    let (_dir, path) = with_temp_kdl(
        r#"
        lint "echo linting"
        test "echo testing"
        ci {
            deps "lint" "test"
            run "echo done"
        }
        "#,
    );

    let output = run_file(&path, &["--summary", "ci"]);
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("lint"), "stderr: {stderr}");
    assert!(stderr.contains("test"), "stderr: {stderr}");
    assert!(stderr.contains("ci"), "stderr: {stderr}");
    // The footer says the total is wall clock, which under parallel deps
    // is less than the sum of the rows above it.
    assert!(stderr.contains("3 tasks in"), "stderr: {stderr}");
    assert!(stderr.contains("elapsed"), "stderr: {stderr}");

    // Without the flag, no table.
    let output = run_file(&path, &["ci"]);
    assert!(!String::from_utf8_lossy(&output.stderr).contains("elapsed"));
}

#[test]
fn summary_marks_failures_and_up_to_date() {
    let (_dir, path) = with_temp_kdl(
        r#"
        fresh {
            status "true"
            run "echo never"
        }
        bad "exit 1"
        all {
            deps "fresh" "bad"
            run "echo body"
        }
        "#,
    );

    let output = run_file(&path, &["--summary", "all"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("up to date"), "stderr: {stderr}");
    assert!(stderr.contains("bad"), "stderr: {stderr}");
}

#[test]
fn group_mode_emits_ci_fold_markers_on_github_actions() {
    let (_dir, path) = with_temp_kdl(
        r#"
        config {
            output "group"
        }
        child "echo inside"
        all {
            deps "child"
            run "echo root"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "all"])
        .env("GITHUB_ACTIONS", "true")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("::group::child"), "stdout: {stdout}");
    assert!(stdout.contains("::endgroup::"), "stdout: {stdout}");

    // Not on GHA: no markers.
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "all"])
        .env_remove("GITHUB_ACTIONS")
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("::group::"));
}

/// A dry run is a plan, not a flat list of shell strings: each command is
/// grouped under the task it belongs to and tagged with its phase.
#[test]
fn dry_run_groups_commands_by_task_and_phase() {
    let (_dir, path) = with_temp_kdl(
        r#"
        lint "echo linting"
        db {
            migrate "echo migrating"
        }
        deploy {
            precondition "test -f .env"
            deps "lint"
            steps "db migrate"
            defer "echo cleanup"
            before "echo starting"
            run "echo shipping"
            after "echo done"
        }
        "#,
    );

    let output = run_file(&path, &["--dry-run", "deploy"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Every task announces itself, nested ones by their full path.
    for task in ["lint", "db migrate", "deploy"] {
        assert!(
            stdout.lines().any(|l| l == task),
            "no header for {task:?}: {stdout}"
        );
    }
    for phase in [
        "precondition test -f .env",
        "run          echo linting",
        "run          echo migrating",
        "before       echo starting",
        "run          echo shipping",
        "after        echo done",
        "defer        echo cleanup",
    ] {
        assert!(stdout.contains(phase), "missing {phase:?}: {stdout}");
    }
}

/// Deps execute in parallel, but a preview you can't read twice the same
/// way is not a preview. Dry runs walk them in declaration order.
#[test]
fn dry_run_order_is_stable() {
    let (_dir, path) = with_temp_kdl(
        r#"
        alpha "echo alpha"
        beta "echo beta"
        gamma "echo gamma"
        all { deps "alpha" "beta" "gamma" }
        "#,
    );

    let first = run_file(&path, &["--dry-run", "all"]);
    let stdout = String::from_utf8_lossy(&first.stdout).to_string();

    let order: Vec<usize> = ["alpha", "beta", "gamma"]
        .iter()
        .map(|name| stdout.find(&format!("echo {name}")).unwrap())
        .collect();
    assert!(order[0] < order[1] && order[1] < order[2], "{stdout}");

    for _ in 0..5 {
        let again = run_file(&path, &["--dry-run", "all"]);
        assert_eq!(String::from_utf8_lossy(&again.stdout), stdout);
    }
}

/// Echoed commands from parallel tasks share one unlabelled stream in the
/// default output mode, so they have to say which task they came from.
#[test]
fn verbose_labels_echoes_from_dependencies() {
    let (_dir, path) = with_temp_kdl(
        r#"
        lint "echo linting"
        ci {
            deps "lint"
            run "echo done"
        }
        "#,
    );

    let stdout = String::from_utf8_lossy(&run_file(&path, &["--verbose", "ci"]).stdout).to_string();
    assert!(stdout.contains("[lint] $ echo linting"), "stdout: {stdout}");
    // The invoked task needs no label; there is nothing to tell it from.
    assert!(stdout.contains("\n$ echo done"), "stdout: {stdout}");
}

#[test]
fn verbose_on_a_lone_task_adds_no_label() {
    let (_dir, path) = with_temp_kdl("lint \"echo linting\"\n");

    let stdout =
        String::from_utf8_lossy(&run_file(&path, &["--verbose", "lint"]).stdout).to_string();
    assert!(stdout.starts_with("$ echo linting"), "stdout: {stdout}");
}

/// Task names are padded by character count, and durations right-align so
/// they can be compared down the column.
#[test]
fn summary_columns_line_up() {
    let (_dir, path) = with_temp_kdl(
        r#"
        x "echo x"
        a-considerably-longer-name "echo y"
        skipped {
            status "true"
            run "echo never"
        }
        all { deps "x" "a-considerably-longer-name" "skipped" }
        "#,
    );

    let stderr =
        String::from_utf8_lossy(&run_file(&path, &["--summary", "all"]).stderr).to_string();
    // Table rows only: the up-to-date notice printed during the run has
    // the same words but is not part of the table.
    let rows: Vec<&str> = stderr
        .lines()
        .filter(|l| l.starts_with("  ✓ ") || l.starts_with("  ✗ ") || l.starts_with("  - "))
        .collect();
    assert!(rows.len() >= 3, "stderr: {stderr}");

    // Every row ends its note at the same column.
    let widths: Vec<usize> = rows.iter().map(|l| l.chars().count()).collect();
    assert!(
        widths.windows(2).all(|w| w[0] == w[1]),
        "ragged rows {widths:?}: {stderr}"
    );
}
