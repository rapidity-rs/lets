mod common;

use common::{lets_bin, with_temp_kdl};

#[test]
fn passthrough_args() {
    let (_dir, path) = with_temp_kdl(
        r#"
        test {
            run "echo cargo test {--}"
        }
        "#,
    );

    let output = lets_bin()
        .args([
            "--file",
            path.to_str().unwrap(),
            "test",
            "--",
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cargo test --nocapture --test-threads=1"));
}

#[test]
fn passthrough_empty() {
    let (_dir, path) = with_temp_kdl(
        r#"
        test {
            run "echo cargo test {--}"
        }
        "#,
    );

    // No -- args provided — {--} becomes empty
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "test"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cargo test"));
}

#[test]
fn passthrough_with_args_and_flags() {
    let (_dir, path) = with_temp_kdl(
        r#"
        test {
            arg suite "unit" "integration"
            flag verbose "-v"
            run "echo test {suite} {?verbose:--verbose} {--}"
        }
        "#,
    );

    let output = lets_bin()
        .args([
            "--file",
            path.to_str().unwrap(),
            "test",
            "unit",
            "--verbose",
            "--",
            "--nocapture",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test unit --verbose --nocapture"));
}

#[test]
fn passthrough_with_hyphenated_args() {
    let (_dir, path) = with_temp_kdl(
        r#"
        run-cmd {
            run "echo cmd {--}"
        }
        "#,
    );

    // Args after -- can include flags like --foo and -b
    let output = lets_bin()
        .args([
            "--file",
            path.to_str().unwrap(),
            "run-cmd",
            "--",
            "--foo",
            "-b",
            "value",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cmd --foo -b value"));
}
