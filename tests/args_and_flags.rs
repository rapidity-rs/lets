mod common;

use common::{lets_bin, with_temp_kdl};

#[test]
fn arg_interpolation() {
    let (_dir, path) = with_temp_kdl(
        r#"
        greet {
            arg name
            run "echo hello {name}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "greet", "world"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello world"));
}

#[test]
fn arg_with_default() {
    let (_dir, path) = with_temp_kdl(
        r#"
        greet {
            arg name default="world"
            run "echo hello {name}"
        }
        "#,
    );

    // Without providing the arg — should use default
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "greet"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello world"));

    // With explicit value
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "greet", "taylor"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello taylor"));
}

#[test]
fn arg_choices_valid() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            arg environment "dev" "staging" "prod"
            run "echo deploying to {environment}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy", "staging"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("deploying to staging"));
}

#[test]
fn arg_choices_invalid() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            arg environment "dev" "staging" "prod"
            run "echo deploying to {environment}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy", "banana"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid value"));
}

#[test]
fn arg_missing_required() {
    let (_dir, path) = with_temp_kdl(
        r#"
        greet {
            arg name
            run "echo hello {name}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "greet"])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn multiple_args_interpolation() {
    let (_dir, path) = with_temp_kdl(
        r#"
        copy {
            arg source
            arg dest
            run "echo copying {source} to {dest}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "copy", "a.txt", "b.txt"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("copying a.txt to b.txt"));
}

#[test]
fn flag_conditional_interpolation() {
    let (_dir, path) = with_temp_kdl(
        r#"
        build {
            flag release "-r" help="Build in release mode"
            run "echo cargo build {?release:--release}"
        }
        "#,
    );

    // With flag
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "build", "--release"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cargo build --release"));

    // Without flag
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "build"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should just be "cargo build " (with trailing space from the placeholder becoming empty)
    assert!(stdout.contains("cargo build"));
    assert!(!stdout.contains("--release"));
}

#[test]
fn flag_short_alias() {
    let (_dir, path) = with_temp_kdl(
        r#"
        build {
            flag verbose "-v"
            run "echo building {?verbose:--verbose}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "build", "-v"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("building --verbose"));
}

#[test]
fn args_and_flags_together() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            arg environment "dev" "staging" "prod"
            flag preview "-d" help="Preview only"
            run "echo deploy {environment} {?preview:--preview}"
        }
        "#,
    );

    let output = lets_bin()
        .args([
            "--file",
            path.to_str().unwrap(),
            "deploy",
            "staging",
            "--preview",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("deploy staging --preview"));
}

#[test]
fn valued_flag_with_default() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            flag replicas "-r" type="int" default="3"
            run "echo replicas={replicas}"
        }
        "#,
    );

    // Without flag — uses default
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("replicas=3"));

    // With explicit value
    let output = lets_bin()
        .args([
            "--file",
            path.to_str().unwrap(),
            "deploy",
            "--replicas",
            "5",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("replicas=5"));
}

#[test]
fn valued_flag_short_alias() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            flag replicas "-r" type="int" default="3"
            run "echo replicas={replicas}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy", "-r", "10"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("replicas=10"));
}

#[test]
fn valued_flag_type_validation() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            flag replicas type="int" default="3"
            run "echo replicas={replicas}"
        }
        "#,
    );

    let output = lets_bin()
        .args([
            "--file",
            path.to_str().unwrap(),
            "deploy",
            "--replicas",
            "abc",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid value"));
}

#[test]
fn valued_flag_string_type() {
    let (_dir, path) = with_temp_kdl(
        r#"
        query {
            flag format "-o" type="string" default="json"
            run "echo format={format}"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "query", "--format", "csv"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("format=csv"));
}

#[test]
fn args_and_valued_flags_together() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            arg environment "dev" "staging" "prod"
            flag replicas "-r" type="int" default="3"
            flag preview "-d"
            run "echo deploy {environment} --replicas {replicas} {?preview:--preview}"
        }
        "#,
    );

    let output = lets_bin()
        .args([
            "--file",
            path.to_str().unwrap(),
            "deploy",
            "staging",
            "--replicas",
            "5",
            "--preview",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("deploy staging --replicas 5 --preview"));
}

fn run_file(path: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut argv = vec!["--file", path.to_str().unwrap()];
    argv.extend_from_slice(args);
    lets_bin().args(&argv).output().unwrap()
}

#[test]
fn rest_arg_captures_remaining_values_quoted() {
    let (_dir, path) = with_temp_kdl(
        r#"
        pack {
            arg files rest=#true
            run "printf '[%s]\n' {files}"
        }
        "#,
    );

    let output = run_file(&path, &["pack", "a", "b c", "d"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "[a]\n[b c]\n[d]", "stdout was: {stdout}");

    // Absent rest arg renders empty (printf still emits one empty slot).
    let output = run_file(&path, &["pack"]);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "[]");
}

#[test]
fn typed_arg_validates_and_interpolates() {
    let (_dir, path) = with_temp_kdl(
        r#"
        wait {
            arg seconds type="int"
            run "echo waiting {seconds}"
        }
        "#,
    );

    let output = run_file(&path, &["wait", "5"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("waiting 5"));

    let output = run_file(&path, &["wait", "soon"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid value"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn optional_arg_presence_conditional_and_env_export() {
    let (_dir, path) = with_temp_kdl(
        r#"
        greet {
            arg name required=#false
            run "echo who=${{LETS_ARG_NAME:-nobody}} {?name:--personal}"
        }
        "#,
    );

    let output = run_file(&path, &["greet", "taylor"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("who=taylor --personal"), "stdout: {stdout}");

    let output = run_file(&path, &["greet"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("who=nobody"), "stdout: {stdout}");
    assert!(!stdout.contains("--personal"), "stdout: {stdout}");
}

#[test]
fn plain_interpolation_of_optional_arg_is_load_error() {
    let (_dir, path) = with_temp_kdl(
        r#"
        greet {
            arg name required=#false
            run "echo hello {name}"
        }
        "#,
    );

    let output = run_file(&path, &["greet"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("may be absent"), "stderr: {stderr}");
}

#[test]
fn flag_choices_validate() {
    let (_dir, path) = with_temp_kdl(
        r#"
        export {
            flag format "-o" "json" "yaml" default="json"
            run "echo format={format}"
        }
        "#,
    );

    let output = run_file(&path, &["export", "-o", "yaml"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("format=yaml"));

    let output = run_file(&path, &["export", "-o", "xml"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid value"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn env_fallback_supplies_flag_value() {
    let (_dir, path) = with_temp_kdl(
        r#"
        serve {
            flag port type="int" env="LETS_TEST_PORT" default="3000"
            run "echo port={port}"
        }
        "#,
    );

    // Explicit flag wins.
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "serve", "--port", "9999"])
        .env("LETS_TEST_PORT", "4444")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("port=9999"));

    // Env fallback beats the default.
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "serve"])
        .env("LETS_TEST_PORT", "4444")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("port=4444"));

    // Neither -> default.
    let output = run_file(&path, &["serve"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("port=3000"));
}

#[test]
fn rest_arg_and_passthrough_conflict_is_load_error() {
    let (_dir, path) = with_temp_kdl(
        r#"
        pack {
            arg files rest=#true
            run "tar cf out.tar {files} {--}"
        }
        "#,
    );

    let output = run_file(&path, &["pack"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("pick one"), "stderr: {stderr}");
}

#[test]
fn rest_arg_via_deps_reference() {
    let (_dir, path) = with_temp_kdl(
        r#"
        pack {
            arg files rest=#true
            run "printf '[%s]\n' {files}"
        }
        release {
            deps "pack one two"
        }
        "#,
    );

    let output = run_file(&path, &["release"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[one]\n[two]"), "stdout: {stdout}");
}

/// Built-in flags apply to a run, so they have to work where a user
/// naturally types them — after the command name, not only before it.
#[test]
fn built_in_flags_work_after_the_command_name() {
    let (_dir, path) = with_temp_kdl(
        r#"
        lint "echo linting"
        ci {
            steps "lint"
            run "echo done"
        }
        "#,
    );

    for flag in [
        "--verbose",
        "--keep-going",
        "--summary",
        "--dry-run",
        "--force",
    ] {
        let before = lets_bin()
            .args(["--file", path.to_str().unwrap(), flag, "ci"])
            .output()
            .unwrap();
        let after = lets_bin()
            .args(["--file", path.to_str().unwrap(), "ci", flag])
            .output()
            .unwrap();

        assert!(before.status.success(), "{flag} before the command failed");
        assert!(
            after.status.success(),
            "{flag} after the command failed: {}",
            String::from_utf8_lossy(&after.stderr)
        );
    }
}

/// The flag has to take effect from either position, not merely parse.
#[test]
fn a_trailing_built_in_flag_takes_effect() {
    let (_dir, path) = with_temp_kdl("ci \"echo done\"\n");

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "ci", "--summary"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("total:"), "stderr: {stderr}");
}
