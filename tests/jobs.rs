mod common;

use common::{lets_bin, with_temp_kdl};

/// Two parallel deps whose interleaved-sleep output proves concurrency.
const RACY: &str = r#"
    a "sh -c 'echo A1; sleep 0.15; echo A2'"
    b "sh -c 'echo B1; sleep 0.15; echo B2'"
    all {
        deps "a" "b"
        run "echo DONE"
    }
"#;

#[test]
fn jobs_one_serializes_parallel_deps() {
    let config = format!("config {{ jobs 1 }}\n{RACY}");
    let (_dir, path) = with_temp_kdl(&config);

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "all"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // With a single permit each task runs to completion before the next
    // starts: blocks stay contiguous even in interleaved output mode.
    assert!(
        stdout.contains("A1\nA2") && stdout.contains("B1\nB2"),
        "tasks interleaved despite jobs 1:\n{stdout}"
    );
}

#[test]
fn jobs_flag_overrides_config() {
    let config = format!("config {{ jobs 8 }}\n{RACY}");
    let (_dir, path) = with_temp_kdl(&config);

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--jobs", "1", "all"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("A1\nA2") && stdout.contains("B1\nB2"),
        "--jobs 1 not applied:\n{stdout}"
    );
}

#[test]
fn unlimited_still_runs_concurrently() {
    let (_dir, path) = with_temp_kdl(RACY);

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "all"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Both first lines appear before either second line: concurrent.
    let a2 = stdout.find("A2").unwrap();
    let b1 = stdout.find("B1").unwrap();
    assert!(
        b1 < a2,
        "expected concurrent execution without a cap:\n{stdout}"
    );
}

#[test]
fn nested_deps_do_not_deadlock_with_jobs_one() {
    // Depth 3 with jobs 1: permits must never be held across recursion.
    let (_dir, path) = with_temp_kdl(
        r#"
        config { jobs 1 }
        leaf "echo LEAF"
        mid {
            deps "leaf"
            run "echo MID"
        }
        top {
            deps "mid"
            run "echo TOP"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "top"])
        .output()
        .unwrap();

    assert!(output.status.success(), "deadlocked or failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for word in ["LEAF", "MID", "TOP"] {
        assert!(stdout.contains(word), "{stdout}");
    }
}

#[test]
fn invalid_jobs_rejected_at_load() {
    let (_dir, path) = with_temp_kdl(
        r#"
        config { jobs 0 }
        a "echo hi"
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "a"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("jobs"), "got:\n{stderr}");
}
