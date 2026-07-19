mod common;

use common::{lets_bin, with_temp_kdl};

#[test]
fn precondition_pass_allows_run() {
    let (_dir, path) = with_temp_kdl(
        r#"
        build {
            precondition "true"
            run "echo BUILT"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "build"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("BUILT"));
}

#[test]
fn precondition_failure_blocks_with_message() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            precondition "test -f missing-credentials" message="Add credentials first"
            run "echo DEPLOYED"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("DEPLOYED"),
        "guarded run executed:\n{stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Add credentials first"), "got:\n{stderr}");
}

#[test]
fn precondition_blocks_before_deps_run() {
    let (_dir, path) = with_temp_kdl(
        r#"
        prep "echo PREP-RAN"
        deploy {
            precondition "false"
            deps "prep"
            run "echo DEPLOYED"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("PREP-RAN"),
        "deps must not run when the precondition fails:\n{stdout}"
    );
}

#[test]
fn status_met_skips_task() {
    let (_dir, path) = with_temp_kdl(
        r#"
        setup {
            status "true"
            run "echo SETUP-RAN"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "setup"])
        .output()
        .unwrap();

    assert!(output.status.success(), "skip must be a success");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("SETUP-RAN"),
        "up-to-date task ran:\n{stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("up to date"), "got:\n{stderr}");
}

#[test]
fn status_unmet_runs_task() {
    let (_dir, path) = with_temp_kdl(
        r#"
        setup {
            status "true" "false"
            run "echo SETUP-RAN"
        }
        "#,
    );

    // ALL status checks must pass to skip; one failing means run.
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "setup"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("SETUP-RAN"));
}

#[test]
fn force_overrides_status() {
    let (_dir, path) = with_temp_kdl(
        r#"
        setup {
            status "true"
            run "echo SETUP-RAN"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--force", "setup"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("SETUP-RAN"));
}

#[test]
fn up_to_date_dep_skips_but_main_runs() {
    let (_dir, path) = with_temp_kdl(
        r#"
        setup {
            status "true"
            run "echo SETUP-RAN"
        }
        build {
            deps "setup"
            run "echo BUILT"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "build"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("SETUP-RAN"), "{stdout}");
    assert!(stdout.contains("BUILT"), "{stdout}");
}

#[test]
fn dry_run_previews_gates_without_evaluating() {
    let (dir, path) = with_temp_kdl(
        r#"
        build {
            precondition "sh -c 'touch pre-probe; true'"
            status "sh -c 'touch status-probe; true'"
            run "echo BUILT"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--dry-run", "build"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[dry-run] precondition:"), "{stdout}");
    assert!(stdout.contains("[dry-run] status:"), "{stdout}");
    assert!(stdout.contains("[dry-run] echo BUILT"), "{stdout}");
    // The gate commands themselves must not have executed.
    assert!(!dir.path().join("pre-probe").exists());
    assert!(!dir.path().join("status-probe").exists());
}

#[test]
fn gate_commands_use_task_env() {
    let (_dir, path) = with_temp_kdl(
        r#"
        guarded {
            env GATE="open"
            precondition "test \"$GATE\" = open"
            run "echo PASSED"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "guarded"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("PASSED"));
}
