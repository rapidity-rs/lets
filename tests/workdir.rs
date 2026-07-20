//! Working-directory semantics: commands run from the config-file directory
//! regardless of where lets was invoked, `dir` and `env-file` resolve
//! against it, and the location env vars are exported.

mod common;

use std::fs;

use common::{lets_bin, with_temp_kdl};

#[test]
fn commands_run_from_config_dir() {
    let (dir, _path) = with_temp_kdl("whereami \"pwd\"\n");
    let subdir = dir.path().join("sub");
    fs::create_dir(&subdir).unwrap();

    // Invoke from a subdirectory; discovery walks up, execution stays rooted.
    let output = lets_bin()
        .arg("whereami")
        .current_dir(&subdir)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let reported = fs::canonicalize(stdout.trim()).unwrap();
    let expected = fs::canonicalize(dir.path()).unwrap();
    assert_eq!(reported, expected, "stdout was: {stdout}");
}

#[test]
fn dir_resolves_relative_to_config() {
    let (dir, _path) = with_temp_kdl(
        r#"
        inner {
            dir "nested"
            run "pwd"
        }
        "#,
    );
    fs::create_dir(dir.path().join("nested")).unwrap();
    let elsewhere = dir.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();

    let output = lets_bin()
        .arg("inner")
        .current_dir(&elsewhere)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let reported = fs::canonicalize(stdout.trim()).unwrap();
    let expected = fs::canonicalize(dir.path().join("nested")).unwrap();
    assert_eq!(reported, expected, "stdout was: {stdout}");
}

#[test]
fn env_file_resolves_relative_to_config() {
    let (dir, _path) = with_temp_kdl(
        r#"
        show {
            env-file ".env.test"
            run "printenv FROM_ENV_FILE"
        }
        "#,
    );
    fs::write(dir.path().join(".env.test"), "FROM_ENV_FILE=loaded\n").unwrap();
    let subdir = dir.path().join("sub");
    fs::create_dir(&subdir).unwrap();

    let output = lets_bin()
        .arg("show")
        .current_dir(&subdir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "loaded");
}

#[test]
fn location_env_vars_exported() {
    let (dir, _path) =
        with_temp_kdl("where \"echo root=$LETS_PROJECT_ROOT from=$LETS_INVOCATION_DIR\"\n");
    let subdir = dir.path().join("sub");
    fs::create_dir(&subdir).unwrap();

    let output = lets_bin()
        .arg("where")
        .current_dir(&subdir)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.trim();
    let (root_part, from_part) = line.split_once(" from=").expect("both vars present");
    let root = root_part.strip_prefix("root=").unwrap();
    assert_eq!(
        fs::canonicalize(root).unwrap(),
        fs::canonicalize(dir.path()).unwrap(),
        "stdout was: {stdout}"
    );
    assert_eq!(
        fs::canonicalize(from_part).unwrap(),
        fs::canonicalize(&subdir).unwrap(),
        "stdout was: {stdout}"
    );
}

#[test]
fn dir_task_unaffected_by_file_flag_from_elsewhere() {
    // --file with a config in another directory: execution still roots at
    // the config's directory, not the caller's.
    let (dir, path) = with_temp_kdl("whereami \"pwd\"\n");
    let elsewhere = tempfile::tempdir().unwrap();

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "whereami"])
        .current_dir(elsewhere.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let reported = fs::canonicalize(stdout.trim()).unwrap();
    let expected = fs::canonicalize(dir.path()).unwrap();
    assert_eq!(reported, expected, "stdout was: {stdout}");
}

#[test]
fn file_flag_accepts_equals_and_bundled_forms() {
    let (_dir, path) = with_temp_kdl("hello \"echo hi\"\n");
    let elsewhere = tempfile::tempdir().unwrap();

    for args in [
        vec![format!("--file={}", path.display()), "hello".to_string()],
        vec![format!("-f{}", path.display()), "hello".to_string()],
        vec![
            "--file".to_string(),
            path.display().to_string(),
            "hello".to_string(),
        ],
    ] {
        let output = lets_bin()
            .args(&args)
            .current_dir(elsewhere.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "args {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hi");
    }
}
