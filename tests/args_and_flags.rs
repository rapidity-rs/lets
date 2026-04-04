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
            flag dry-run "-d" help="Dry run"
            run "echo deploy {environment} {?dry-run:--dry-run}"
        }
        "#,
    );

    let output = lets_bin()
        .args([
            "--file",
            path.to_str().unwrap(),
            "deploy",
            "staging",
            "--dry-run",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("deploy staging --dry-run"));
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
            flag dry-run "-d"
            run "echo deploy {environment} --replicas {replicas} {?dry-run:--dry-run}"
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
            "--dry-run",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("deploy staging --replicas 5 --dry-run"));
}
