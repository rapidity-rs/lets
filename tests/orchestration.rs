mod common;

use common::{lets_bin, with_temp_kdl};

#[test]
fn steps_run_sequentially() {
    let (_dir, path) = with_temp_kdl(
        r#"
        lint "echo LINT"
        test "echo TEST"
        build "echo BUILD"
        ci {
            steps "lint" "test" "build"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "ci"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Verify all steps ran and in order.
    let lint_pos = stdout.find("LINT").unwrap();
    let test_pos = stdout.find("TEST").unwrap();
    let build_pos = stdout.find("BUILD").unwrap();
    assert!(lint_pos < test_pos);
    assert!(test_pos < build_pos);
}

#[test]
fn steps_with_own_run() {
    let (_dir, path) = with_temp_kdl(
        r#"
        lint "echo LINT"
        release {
            steps "lint"
            run "echo RELEASE"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "release"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lint_pos = stdout.find("LINT").unwrap();
    let release_pos = stdout.find("RELEASE").unwrap();
    assert!(lint_pos < release_pos);
}

#[test]
fn before_after_hooks() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            before "echo BEFORE"
            after "echo AFTER"
            run "echo MAIN"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let before_pos = stdout.find("BEFORE").unwrap();
    let main_pos = stdout.find("MAIN").unwrap();
    let after_pos = stdout.find("AFTER").unwrap();
    assert!(before_pos < main_pos);
    assert!(main_pos < after_pos);
}

#[test]
fn steps_failure_stops_execution() {
    let (_dir, path) = with_temp_kdl(
        r#"
        fail "exit 1"
        after "echo SHOULD_NOT_RUN"
        ci {
            steps "fail" "after"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "ci"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("SHOULD_NOT_RUN"));
}

#[test]
fn nested_steps_ref() {
    let (_dir, path) = with_temp_kdl(
        r#"
        db {
            migrate "echo MIGRATE"
        }
        deploy {
            steps "db migrate"
            run "echo DEPLOY"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let migrate_pos = stdout.find("MIGRATE").unwrap();
    let deploy_pos = stdout.find("DEPLOY").unwrap();
    assert!(migrate_pos < deploy_pos);
}

#[test]
fn deps_run_before_main() {
    let (_dir, path) = with_temp_kdl(
        r#"
        test "echo TEST"
        lint "echo LINT"
        release {
            deps "test" "lint"
            run "echo RELEASE"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "release"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Both deps should have run.
    assert!(stdout.contains("TEST"));
    assert!(stdout.contains("LINT"));
    // Main command runs after deps.
    let release_pos = stdout.find("RELEASE").unwrap();
    let test_pos = stdout.find("TEST").unwrap();
    let lint_pos = stdout.find("LINT").unwrap();
    assert!(test_pos < release_pos);
    assert!(lint_pos < release_pos);
}

#[test]
fn deps_failure_stops_main() {
    let (_dir, path) = with_temp_kdl(
        r#"
        fail "exit 1"
        release {
            deps "fail"
            run "echo SHOULD_NOT_RUN"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "release"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("SHOULD_NOT_RUN"));
}

#[test]
fn deps_and_steps_combined() {
    let (_dir, path) = with_temp_kdl(
        r#"
        lint "echo LINT"
        test "echo TEST"
        build "echo BUILD"
        deploy {
            deps "lint" "test"
            steps "build"
            run "echo DEPLOY"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // All should have run.
    assert!(stdout.contains("LINT"));
    assert!(stdout.contains("TEST"));
    assert!(stdout.contains("BUILD"));
    assert!(stdout.contains("DEPLOY"));
    // Deps and steps both before main.
    let deploy_pos = stdout.find("DEPLOY").unwrap();
    let build_pos = stdout.find("BUILD").unwrap();
    assert!(build_pos < deploy_pos);
}

#[test]
fn deps_steps_and_hooks_combined() {
    let (_dir, path) = with_temp_kdl(
        r#"
        lint "echo LINT"
        deploy {
            deps "lint"
            before "echo BEFORE"
            after "echo AFTER"
            run "echo MAIN"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Order: deps → before → main → after
    let lint_pos = stdout.find("LINT").unwrap();
    let before_pos = stdout.find("BEFORE").unwrap();
    let main_pos = stdout.find("MAIN").unwrap();
    let after_pos = stdout.find("AFTER").unwrap();
    assert!(lint_pos < before_pos);
    assert!(before_pos < main_pos);
    assert!(main_pos < after_pos);
}

#[test]
fn steps_target_with_flag_uses_default() {
    let (_dir, path) = with_temp_kdl(
        r#"
        build {
            flag release "-r" help="Release mode"
            run "echo cargo build {?release:--release}"
        }
        ci {
            steps "build"
        }
        "#,
    );

    // "build" invoked as a step — boolean flag defaults to false, {?release:--release} is empty.
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "ci"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cargo build"));
    assert!(!stdout.contains("--release"));
}

#[test]
fn steps_target_with_arg_default() {
    let (_dir, path) = with_temp_kdl(
        r#"
        greet {
            arg name default="world"
            run "echo hello {name}"
        }
        ci {
            steps "greet"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "ci"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello world"));
}

#[test]
fn steps_target_with_passthrough() {
    let (_dir, path) = with_temp_kdl(
        r#"
        test {
            run "echo cargo test {--}"
        }
        ci {
            steps "test"
        }
        "#,
    );

    // {--} resolves to empty when invoked as a step.
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "ci"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cargo test"));
}

#[test]
fn multiple_run_commands_execute_sequentially() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            run "echo STEP1"
            run "echo STEP2"
            run "echo STEP3"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("STEP1"));
    assert!(stdout.contains("STEP2"));
    assert!(stdout.contains("STEP3"));

    // Verify order: STEP1 appears before STEP2 before STEP3.
    let pos1 = stdout.find("STEP1").unwrap();
    let pos2 = stdout.find("STEP2").unwrap();
    let pos3 = stdout.find("STEP3").unwrap();
    assert!(pos1 < pos2 && pos2 < pos3, "steps should be in order");
}

#[test]
fn multiple_run_stops_on_failure() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            run "echo BEFORE"
            run "false"
            run "echo AFTER"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BEFORE"), "first command should run");
    assert!(
        !stdout.contains("AFTER"),
        "third command should not run after failure"
    );
}
