mod common;

use std::fs;

use common::{lets_bin, with_temp_kdl};

const KDL: &str = r#"
    build {
        sources "src/**"
        run "sh -c 'echo RUN >> runs.log'"
    }
"#;

fn runs(root: &std::path::Path) -> usize {
    fs::read_to_string(root.join("runs.log"))
        .map(|s| s.matches("RUN").count())
        .unwrap_or(0)
}

fn run_build(
    root: &std::path::Path,
    path: &std::path::Path,
    extra: &[&str],
) -> std::process::Output {
    let mut args = vec!["--file", path.to_str().unwrap()];
    args.extend_from_slice(extra);
    args.push("build");
    lets_bin().current_dir(root).args(&args).output().unwrap()
}

#[test]
fn second_run_skips_when_sources_unchanged() {
    let (dir, path) = with_temp_kdl(KDL);
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "one").unwrap();

    let first = run_build(root, &path, &[]);
    assert!(first.status.success());
    assert_eq!(runs(root), 1);

    let second = run_build(root, &path, &[]);
    assert!(second.status.success(), "skip must be a success");
    assert_eq!(runs(root), 1, "unchanged sources must skip the task");
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(stderr.contains("up to date"), "got:\n{stderr}");
}

#[test]
fn changed_source_invalidates() {
    let (dir, path) = with_temp_kdl(KDL);
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "one").unwrap();

    assert!(run_build(root, &path, &[]).status.success());
    fs::write(root.join("src/main.rs"), "two").unwrap();
    assert!(run_build(root, &path, &[]).status.success());
    assert_eq!(runs(root), 2, "changed source must re-run");
}

#[test]
fn force_bypasses_fingerprint() {
    let (dir, path) = with_temp_kdl(KDL);
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "one").unwrap();

    assert!(run_build(root, &path, &[]).status.success());
    assert!(run_build(root, &path, &["--force"]).status.success());
    assert_eq!(runs(root), 2, "--force must re-run");
}

#[test]
fn changed_command_invalidates() {
    let (dir, path) = with_temp_kdl(KDL);
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "one").unwrap();

    assert!(run_build(root, &path, &[]).status.success());

    // Same sources, different command: the fingerprint must change.
    fs::write(
        &path,
        r#"
        build {
            sources "src/**"
            run "sh -c 'echo RUN >> runs.log; echo changed'"
        }
        "#,
    )
    .unwrap();
    assert!(run_build(root, &path, &[]).status.success());
    assert_eq!(runs(root), 2, "config change must re-run");
}

#[test]
fn missing_generates_forces_run() {
    let (dir, path) = with_temp_kdl(
        r#"
        build {
            sources "src/**"
            generates "dist/**"
            run "sh -c 'echo RUN >> runs.log'"
        }
        "#,
    );
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "one").unwrap();

    // The task never creates dist/, so it is never up to date.
    assert!(run_build(root, &path, &[]).status.success());
    assert!(run_build(root, &path, &[]).status.success());
    assert_eq!(runs(root), 2, "missing generates must force a run");
}

#[test]
fn present_generates_allow_skip() {
    let (dir, path) = with_temp_kdl(
        r#"
        build {
            sources "src/**"
            generates "dist/**"
            run "sh -c 'mkdir -p dist; echo out > dist/bundle; echo RUN >> runs.log'"
        }
        "#,
    );
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "one").unwrap();

    assert!(run_build(root, &path, &[]).status.success());
    assert!(run_build(root, &path, &[]).status.success());
    assert_eq!(
        runs(root),
        1,
        "existing outputs + unchanged sources must skip"
    );
}

#[test]
fn failed_run_records_nothing() {
    let (dir, path) = with_temp_kdl(
        r#"
        build {
            sources "src/**"
            run "sh -c 'echo RUN >> runs.log; exit 1'"
        }
        "#,
    );
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "one").unwrap();

    assert!(!run_build(root, &path, &[]).status.success());
    assert!(!run_build(root, &path, &[]).status.success());
    assert_eq!(runs(root), 2, "failed runs must never mark up to date");
}

#[test]
fn cache_directory_is_self_gitignored() {
    let (dir, path) = with_temp_kdl(KDL);
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "one").unwrap();

    assert!(run_build(root, &path, &[]).status.success());
    let marker = fs::read_to_string(root.join(".lets/.gitignore")).unwrap();
    assert_eq!(marker.trim(), "*");
}

#[test]
fn fingerprinted_dep_skips_but_main_runs() {
    let (dir, path) = with_temp_kdl(
        r#"
        compile {
            sources "src/**"
            run "sh -c 'echo COMPILE >> runs.log'"
        }
        package {
            deps "compile"
            run "sh -c 'echo PACKAGE >> runs.log'"
        }
        "#,
    );
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "one").unwrap();

    let go = || {
        lets_bin()
            .current_dir(root)
            .args(["--file", path.to_str().unwrap(), "package"])
            .output()
            .unwrap()
    };
    assert!(go().status.success());
    assert!(go().status.success());

    let log = fs::read_to_string(root.join("runs.log")).unwrap();
    assert_eq!(log.matches("COMPILE").count(), 1, "dep must skip:\n{log}");
    assert_eq!(log.matches("PACKAGE").count(), 2, "main must run:\n{log}");
}

#[test]
fn dry_run_previews_without_recording() {
    let (dir, path) = with_temp_kdl(KDL);
    let root = dir.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "one").unwrap();

    let output = run_build(root, &path, &["--dry-run"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The plan names the sources check as a phase of the task.
    assert!(stdout.contains("sources"), "{stdout}");
    assert!(stdout.contains("pattern(s)"), "{stdout}");
    assert!(!root.join(".lets").exists(), "dry-run must not write cache");

    // A real run afterwards is not fooled by the dry run.
    assert!(run_build(root, &path, &[]).status.success());
    assert_eq!(runs(root), 1);
}
