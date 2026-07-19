mod common;

use common::{lets_bin, with_temp_kdl};

#[test]
fn defer_runs_after_success() {
    let (_dir, path) = with_temp_kdl(
        r#"
        task {
            defer "echo CLEANED"
            run "echo WORKED"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "task"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let work = stdout.find("WORKED").expect("run output missing");
    let clean = stdout.find("CLEANED").expect("defer output missing");
    assert!(work < clean, "defer must run after the body:\n{stdout}");
}

#[test]
fn defer_runs_after_failure() {
    let (_dir, path) = with_temp_kdl(
        r#"
        task {
            defer "echo CLEANED"
            run "echo STARTED"
            run "false"
            run "echo NEVER"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "task"])
        .output()
        .unwrap();

    assert!(!output.status.success(), "task failure must propagate");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("CLEANED"),
        "defer skipped on failure:\n{stdout}"
    );
    assert!(!stdout.contains("NEVER"));
}

#[test]
fn multiple_defers_run_lifo() {
    let (_dir, path) = with_temp_kdl(
        r#"
        task {
            defer "echo FIRST-DECLARED"
            defer "echo SECOND-DECLARED"
            run "echo BODY"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "task"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let second = stdout.find("SECOND-DECLARED").unwrap();
    let first = stdout.find("FIRST-DECLARED").unwrap();
    assert!(second < first, "defers must run LIFO:\n{stdout}");
}

#[test]
fn defer_failure_warns_but_preserves_success() {
    let (_dir, path) = with_temp_kdl(
        r#"
        task {
            defer "false"
            run "echo WORKED"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "task"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "a failing defer must not fail the task"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("defer"), "expected a warning:\n{stderr}");
}

#[test]
fn dep_defer_runs_when_dep_fails() {
    let (_dir, path) = with_temp_kdl(
        r#"
        service {
            defer "echo SERVICE-CLEANED"
            run "echo SERVICE-UP"
            run "false"
        }
        main {
            deps "service"
            run "echo NEVER"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "main"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("SERVICE-CLEANED"), "{stdout}");
    assert!(!stdout.contains("NEVER"));
}

#[test]
fn defer_skipped_when_task_up_to_date() {
    let (_dir, path) = with_temp_kdl(
        r#"
        task {
            status "true"
            defer "echo CLEANED"
            run "echo BODY"
        }
        "#,
    );

    // The body never ran, so there is nothing to clean up.
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "task"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("CLEANED"), "{stdout}");
}

/// Ctrl-C during a run: children die, defers still execute, exit code 130.
#[cfg(unix)]
#[test]
fn sigint_runs_defers_and_exits_130() {
    use std::os::unix::process::CommandExt;
    use std::time::{Duration, Instant};

    let (dir, path) = with_temp_kdl(
        r#"
        serve {
            defer "sh -c 'echo CLEANED >> cleanup.log'"
            run "sh -c 'echo READY > ready.marker; sleep 30'"
        }
        "#,
    );
    let root = dir.path();
    let stderr_file = std::fs::File::create(root.join("stderr.log")).unwrap();

    let mut cmd = lets_bin();
    cmd.args(["--file", path.to_str().unwrap(), "serve"])
        .current_dir(root)
        .stdout(std::process::Stdio::null())
        .stderr(stderr_file);
    // Own process group so we can signal lets + its children like a terminal.
    cmd.process_group(0);
    let mut child = cmd.spawn().unwrap();

    // Wait for the long-running command to start.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.join("ready.marker").exists() {
        assert!(Instant::now() < deadline, "serve never started");
        std::thread::sleep(Duration::from_millis(50));
    }
    // Give lets a beat to finish installing its signal handler.
    std::thread::sleep(Duration::from_millis(200));

    // Emulate Ctrl-C: SIGINT to the whole foreground process group.
    nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    )
    .expect("killpg failed");

    let exit = child.wait().unwrap();
    let stderr = std::fs::read_to_string(root.join("stderr.log")).unwrap_or_default();
    assert_eq!(
        exit.code(),
        Some(130),
        "expected exit 130, got {exit:?}; stderr:\n{stderr}"
    );
    let log = std::fs::read_to_string(root.join("cleanup.log")).unwrap_or_default();
    assert!(
        log.contains("CLEANED"),
        "defer did not run on SIGINT; stderr:\n{stderr}"
    );
}
