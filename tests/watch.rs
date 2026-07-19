mod common;

use std::fs;
use std::io::Read;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use common::{lets_bin, with_temp_kdl};

/// Poll `check` until it returns true or the deadline passes.
fn wait_until(timeout: Duration, mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn kill_tree(child: &mut Child) {
    // The supervisor's own children are in separate process groups; killing
    // the supervisor is enough here because test tasks finish instantly.
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn watch_reruns_on_source_change() {
    let (dir, path) = with_temp_kdl(
        r#"
        build {
            sources "src/**"
            run "sh -c 'echo RUN >> runs.log'"
        }
        "#,
    );
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "one").unwrap();
    let log = root.join("runs.log");

    let mut watcher = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--watch", "build"])
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let runs = |log: &std::path::Path| {
        fs::read_to_string(log)
            .map(|s| s.matches("RUN").count())
            .unwrap_or(0)
    };

    // First run happens immediately.
    assert!(
        wait_until(Duration::from_secs(10), || runs(&log) >= 1),
        "first run never happened"
    );

    // A matching change triggers a rerun.
    fs::write(root.join("src/main.rs"), "two").unwrap();
    let ok = wait_until(Duration::from_secs(10), || runs(&log) >= 2);
    kill_tree(&mut watcher);
    assert!(ok, "no rerun after source change");
}

#[test]
fn watch_ignores_non_matching_change() {
    let (dir, path) = with_temp_kdl(
        r#"
        build {
            sources "src/**"
            run "sh -c 'echo RUN >> runs.log'"
        }
        "#,
    );
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    let log = root.join("runs.log");

    let mut watcher = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--watch", "build"])
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let runs = |log: &std::path::Path| {
        fs::read_to_string(log)
            .map(|s| s.matches("RUN").count())
            .unwrap_or(0)
    };

    assert!(
        wait_until(Duration::from_secs(10), || runs(&log) >= 1),
        "first run never happened"
    );

    // Absorb any straggler events from the tempdir setup writes (the
    // watcher may see the just-created lets.kdl and legitimately restart
    // once), then take a baseline.
    std::thread::sleep(Duration::from_secs(1));
    let baseline = runs(&log);

    // Unrelated file: no rerun within a generous window.
    fs::write(root.join("unrelated.txt"), "noise").unwrap();
    std::thread::sleep(Duration::from_secs(2));
    let count = runs(&log);
    kill_tree(&mut watcher);
    assert_eq!(count, baseline, "unrelated change must not trigger a rerun");
}

#[test]
fn watch_without_sources_errors() {
    let (_dir, path) = with_temp_kdl(
        r#"
        build "echo hi"
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--watch", "build"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no sources to watch"), "got:\n{stderr}");
}

#[test]
fn watch_rejects_interactive_without_yes() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            sources "src/**"
            confirm "Really?"
            run "echo hi"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--watch", "deploy"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not supported with --watch"),
        "got:\n{stderr}"
    );
}

#[test]
fn watch_collects_sources_from_deps() {
    // Sources live on the dep, not the invoked command: still watchable.
    let (dir, path) = with_temp_kdl(
        r#"
        compile {
            sources "src/**"
            run "sh -c 'echo COMPILED >> runs.log'"
        }
        run-it {
            deps "compile"
            run "echo ok"
        }
        "#,
    );
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();

    let mut watcher = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--watch", "run-it"])
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let log = root.join("runs.log");
    let ok = wait_until(Duration::from_secs(10), || log.exists());
    kill_tree(&mut watcher);
    let mut stdout = String::new();
    if let Some(mut out) = watcher.stdout.take() {
        out.read_to_string(&mut stdout).ok();
    }
    assert!(ok, "dep with sources never ran; stdout:\n{stdout}");
}

#[test]
fn invalid_sources_glob_rejected_at_load() {
    let (_dir, path) = with_temp_kdl(
        r#"
        build {
            sources "src/[bad"
            run "echo hi"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "build"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid sources glob"), "got:\n{stderr}");
}

#[test]
fn editing_included_config_triggers_restart() {
    let (dir, path) = with_temp_kdl(
        r#"
        include "extra.kdl"
        build {
            sources "src/**"
            run "sh -c 'echo RUN >> runs.log'"
        }
        "#,
    );
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("extra.kdl"), "helper \"echo hi\"\n").unwrap();
    let log = root.join("runs.log");
    let stderr_file = fs::File::create(root.join("watch-stderr.log")).unwrap();

    let mut watcher = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--watch", "build"])
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(stderr_file)
        .spawn()
        .unwrap();

    let runs = |log: &std::path::Path| {
        fs::read_to_string(log)
            .map(|s| s.matches("RUN").count())
            .unwrap_or(0)
    };

    assert!(
        wait_until(Duration::from_secs(10), || runs(&log) >= 1),
        "first run never happened"
    );
    // Absorb setup-event stragglers.
    std::thread::sleep(Duration::from_secs(1));

    // Editing an included config file must restart the supervisor loop.
    // (The task itself may then skip as up to date — its sources didn't
    // change — so assert on the restart, not on task output.)
    let restarts = |root: &std::path::Path| {
        fs::read_to_string(root.join("watch-stderr.log"))
            .map(|s| s.matches("restarting").count())
            .unwrap_or(0)
    };
    let before = restarts(root);
    fs::write(root.join("extra.kdl"), "helper \"echo changed\"\n").unwrap();
    let ok = wait_until(Duration::from_secs(10), || restarts(root) > before);
    kill_tree(&mut watcher);
    assert!(ok, "no restart after include change");
}
