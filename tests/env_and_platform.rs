mod common;

use common::{lets_bin, with_temp_kdl};

#[test]
fn env_vars_set() {
    let (_dir, path) = with_temp_kdl(
        r#"
        serve {
            env PORT="3000" RUST_LOG="debug"
            run "echo $PORT $RUST_LOG"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "serve"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("3000 debug"));
}

#[test]
fn env_var_interpolation() {
    let (_dir, path) = with_temp_kdl(
        r#"
        serve {
            env PORT="4000"
            run "echo port={$PORT}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "serve"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("port=4000"));
}

#[test]
fn env_file_loading() {
    let dir = tempfile::tempdir().unwrap();
    let kdl_path = dir.path().join("lets.kdl");
    let env_path = dir.path().join(".env.local");

    std::fs::write(&env_path, "# comment\nDB_HOST=localhost\nDB_PORT=5432\n").unwrap();

    std::fs::write(
        &kdl_path,
        r#"
        serve {
            env-file ".env.local"
            run "echo $DB_HOST:$DB_PORT"
        }
        "#,
    )
    .unwrap();

    let output = lets_bin()
        .args(["--file", kdl_path.to_str().unwrap(), "serve"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("localhost:5432"));
}

#[test]
fn env_overrides_env_file() {
    let dir = tempfile::tempdir().unwrap();
    let kdl_path = dir.path().join("lets.kdl");
    let env_path = dir.path().join(".env");

    std::fs::write(&env_path, "PORT=3000\n").unwrap();

    std::fs::write(
        &kdl_path,
        r#"
        serve {
            env-file ".env"
            env PORT="9999"
            run "echo $PORT"
        }
        "#,
    )
    .unwrap();

    let output = lets_bin()
        .args(["--file", kdl_path.to_str().unwrap(), "serve"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("9999"));
}

#[test]
fn working_directory() {
    let dir = tempfile::tempdir().unwrap();
    let subdir = dir.path().join("subdir");
    std::fs::create_dir(&subdir).unwrap();
    let kdl_path = dir.path().join("lets.kdl");

    std::fs::write(
        &kdl_path,
        r#"
        pwd-test {
            dir "subdir"
            run "pwd"
        }
        "#,
    )
    .unwrap();

    let output = lets_bin()
        .args(["--file", kdl_path.to_str().unwrap(), "pwd-test"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("subdir"));
}

#[test]
fn shell_override() {
    let (_dir, path) = with_temp_kdl(
        r#"
        test-shell {
            shell "bash"
            run "echo running-in-bash"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "test-shell"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("running-in-bash"));
}

#[test]
fn platform_specific_run() {
    // This test uses run-macos/run-linux depending on the current OS.
    let (_dir, path) = with_temp_kdl(
        r#"
        install {
            run-macos "echo MACOS"
            run-linux "echo LINUX"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "install"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    if cfg!(target_os = "macos") {
        assert!(stdout.contains("MACOS"));
    } else if cfg!(target_os = "linux") {
        assert!(stdout.contains("LINUX"));
    }
}

#[test]
fn platform_run_falls_back_to_generic() {
    let (_dir, path) = with_temp_kdl(
        r#"
        install {
            run-windows "echo WINDOWS"
            run "echo GENERIC"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "install"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // On non-windows, should fall back to the generic `run`.
    if !cfg!(target_os = "windows") {
        assert!(stdout.contains("GENERIC"));
    }
}

#[test]
fn env_file_edge_cases() {
    let dir = tempfile::tempdir().unwrap();
    let kdl_path = dir.path().join("lets.kdl");
    let env_path = dir.path().join(".env.edge");

    // Edge cases: export prefix, double-quoted values with spaces,
    // single-quoted values, blank lines, inline comments.
    std::fs::write(
        &env_path,
        r#"
# Full-line comment
export EXPORTED_VAR=from_export

DOUBLE_QUOTED="hello world"
SINGLE_QUOTED='single val'

PLAIN=simple
"#,
    )
    .unwrap();

    std::fs::write(
        &kdl_path,
        r#"
        show {
            env-file ".env.edge"
            run "echo $EXPORTED_VAR / $DOUBLE_QUOTED / $SINGLE_QUOTED / $PLAIN"
        }
        "#,
    )
    .unwrap();

    let output = lets_bin()
        .args(["--file", kdl_path.to_str().unwrap(), "show"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("from_export"),
        "export prefix not handled: {stdout}"
    );
    assert!(
        stdout.contains("hello world"),
        "double-quoted spaces not handled: {stdout}"
    );
    assert!(
        stdout.contains("single val"),
        "single-quoted values not handled: {stdout}"
    );
    assert!(
        stdout.contains("simple"),
        "plain values not handled: {stdout}"
    );
}

#[test]
fn config_env_applies_to_every_task() {
    let (_dir, path) = with_temp_kdl(
        r#"
        config {
            env GREETING="hello" SCOPE="global"
        }
        show "echo $GREETING $SCOPE"
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "show"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("hello global"));
}

#[test]
fn task_env_overrides_config_env() {
    let (_dir, path) = with_temp_kdl(
        r#"
        config {
            env SCOPE="global"
        }
        show {
            env SCOPE="task"
            run "echo scope=$SCOPE"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "show"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("scope=task"));
}

#[test]
fn config_env_file_loads_for_every_task() {
    let (dir, path) = with_temp_kdl(
        r#"
        config {
            env-file ".env.shared"
            env OVERRIDE="from-config"
        }
        show "echo $SHARED $OVERRIDE"
        "#,
    );
    std::fs::write(
        dir.path().join(".env.shared"),
        "SHARED=file-value\nOVERRIDE=from-file\n",
    )
    .unwrap();

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "show"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // env-file loads, and explicit config env overrides it.
    assert!(
        stdout.contains("file-value from-config"),
        "stdout: {stdout}"
    );
}

#[test]
fn unknown_config_option_is_load_error() {
    let (_dir, path) = with_temp_kdl(
        r#"
        config {
            shel "zsh"
        }
        show "echo hi"
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "show"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown config option 'shel'"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
