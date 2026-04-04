mod common;

use common::{lets_bin, with_temp_kdl};
use std::fs;

#[test]
fn dry_run_shows_command() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            run "scripts/deploy.sh"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--dry-run", "deploy"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[dry-run] scripts/deploy.sh"));
}

#[test]
fn dry_run_with_steps_and_hooks() {
    let (_dir, path) = with_temp_kdl(
        r#"
        lint "echo lint"
        deploy {
            steps "lint"
            before "echo before"
            after "echo after"
            run "scripts/deploy.sh"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--dry-run", "deploy"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[dry-run] echo lint"));
    assert!(stdout.contains("[dry-run] echo before"));
    assert!(stdout.contains("[dry-run] scripts/deploy.sh"));
    assert!(stdout.contains("[dry-run] echo after"));
}

#[test]
fn alias_works() {
    let (_dir, path) = with_temp_kdl(
        r#"
        test {
            alias "t"
            run "echo TESTING"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "t"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("TESTING"));
}

#[test]
fn retry_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let kdl_path = dir.path().join("lets.kdl");
    let counter_path = dir.path().join("counter");

    // Write a script that fails twice then succeeds.
    let script_path = dir.path().join("flaky.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
count=$(cat "{counter}" 2>/dev/null || echo 0)
count=$((count + 1))
echo $count > "{counter}"
if [ $count -lt 3 ]; then exit 1; fi
echo SUCCESS
"#,
            counter = counter_path.display()
        ),
    )
    .unwrap();

    fs::write(
        &kdl_path,
        format!(
            r#"
            flaky {{
                retry count=3 delay="100ms"
                run "sh {script}"
            }}
            "#,
            script = script_path.display()
        ),
    )
    .unwrap();

    let output = lets_bin()
        .args(["--file", kdl_path.to_str().unwrap(), "flaky"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("SUCCESS"));
}

#[test]
fn timeout_kills_long_command() {
    let (_dir, path) = with_temp_kdl(
        r#"
        slow {
            timeout "1s"
            run "sleep 60"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "slow"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("timed out"));
}

#[test]
fn silent_suppresses_output_on_success() {
    let (_dir, path) = with_temp_kdl(
        r#"
        quiet {
            silent
            run "echo SHOULD_BE_HIDDEN"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "quiet"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("SHOULD_BE_HIDDEN"));
}

#[test]
fn silent_shows_output_on_failure() {
    let (_dir, path) = with_temp_kdl(
        r#"
        quiet-fail {
            silent
            run "echo FAILURE_OUTPUT && exit 1"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "quiet-fail"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FAILURE_OUTPUT"));
}

#[test]
fn include_merges_commands() {
    let dir = tempfile::tempdir().unwrap();

    // Create an included file.
    fs::write(
        dir.path().join("db.kdl"),
        r#"
        db-migrate "echo MIGRATE"
        db-reset "echo RESET"
        "#,
    )
    .unwrap();

    // Create the main file with an include.
    let kdl_path = dir.path().join("lets.kdl");
    fs::write(
        &kdl_path,
        r#"
        description "Main project"
        include "db.kdl"
        build "echo BUILD"
        "#,
    )
    .unwrap();

    // Verify included commands are available.
    let output = lets_bin()
        .args(["--file", kdl_path.to_str().unwrap(), "db-migrate"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MIGRATE"));

    // Verify main commands still work.
    let output = lets_bin()
        .args(["--file", kdl_path.to_str().unwrap(), "build"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BUILD"));
}

#[test]
fn include_shows_in_list() {
    let dir = tempfile::tempdir().unwrap();

    fs::write(dir.path().join("extra.kdl"), r#"extra "echo EXTRA""#).unwrap();

    let kdl_path = dir.path().join("lets.kdl");
    fs::write(
        &kdl_path,
        r#"
        include "extra.kdl"
        main "echo MAIN"
        "#,
    )
    .unwrap();

    let output = lets_bin()
        .args(["--file", kdl_path.to_str().unwrap(), "--list"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("extra"));
    assert!(stdout.contains("main"));
}

#[test]
fn explicit_cmd_subcommand_with_reserved_name() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            run "echo DEPLOY"
            cmd run {
                run "echo RUN-SERVICE"
            }
        }
        "#,
    );

    // The deploy command itself
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DEPLOY"));

    // The "run" subcommand under deploy
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy", "run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("RUN-SERVICE"));
}

#[test]
fn explicit_cmd_top_level_reserved_name() {
    let (_dir, path) = with_temp_kdl(
        r#"
        cmd config "echo CONFIG-CMD"
        build "echo BUILD"
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "config"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CONFIG-CMD"));
}

#[test]
fn hidden_command_not_in_help() {
    let (_dir, path) = with_temp_kdl(
        r#"
        visible "echo visible"
        helper {
            hide
            run "echo helper"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--help"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("visible"),
        "visible command should appear: {stdout}"
    );
    assert!(
        !stdout.contains("helper"),
        "hidden command should not appear: {stdout}"
    );
}

#[test]
fn hidden_command_still_executes() {
    let (_dir, path) = with_temp_kdl(
        r#"
        helper {
            hide
            run "echo HIDDEN-OUTPUT"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "helper"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("HIDDEN-OUTPUT"));
}

#[test]
fn hidden_command_runs_via_deps() {
    let (_dir, path) = with_temp_kdl(
        r#"
        setup {
            hide
            run "echo SETUP-DONE"
        }
        build {
            deps "setup"
            run "echo BUILD-DONE"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "build"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("SETUP-DONE"));
    assert!(stdout.contains("BUILD-DONE"));
}

#[test]
fn hidden_command_not_in_list() {
    let (_dir, path) = with_temp_kdl(
        r#"
        visible "echo visible"
        helper {
            hide
            run "echo helper"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--list"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("visible"));
    assert!(!stdout.contains("helper"));
}

#[test]
fn long_description_in_help() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            description "Deploy the app"
            long-description "Deploy the app to production. Runs migrations first."
            run "echo deploy"
        }
        "#,
    );

    // Parent help shows short description.
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Deploy the app"));

    // Command help shows long description.
    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Runs migrations first"));
}

#[test]
fn examples_in_help() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            description "Deploy the app"
            examples "lets deploy staging"
            run "echo deploy"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy", "--help"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Examples:"),
        "should have Examples header: {stdout}"
    );
    assert!(
        stdout.contains("lets deploy staging"),
        "should have example text: {stdout}"
    );
}

#[test]
fn deprecated_shows_in_help() {
    let (_dir, path) = with_temp_kdl(
        r#"
        old-cmd {
            deprecated "Use new-cmd instead"
            description "Old command"
            run "echo old"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "--help"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("deprecated"),
        "should show deprecated marker: {stdout}"
    );
    assert!(
        stdout.contains("Use new-cmd instead"),
        "should show message: {stdout}"
    );
}

#[test]
fn deprecated_warns_on_execution() {
    let (_dir, path) = with_temp_kdl(
        r#"
        old-cmd {
            deprecated "Use new-cmd instead"
            run "echo OLD-OUTPUT"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "old-cmd"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("OLD-OUTPUT"),
        "command should still execute"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("deprecated"),
        "should warn on stderr: {stderr}"
    );
    assert!(
        stderr.contains("Use new-cmd instead"),
        "should include message: {stderr}"
    );
}

#[test]
fn typo_warning_on_misspelled_keyword() {
    let (_dir, path) = with_temp_kdl(
        r#"
        deploy {
            descrption "Deploy the app"
            run "echo deploy"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "deploy"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("did you mean 'description'"),
        "should warn about typo: {stderr}"
    );
}

#[test]
fn no_typo_warning_for_legitimate_subcommand() {
    let (_dir, path) = with_temp_kdl(
        r#"
        db {
            description "Database commands"
            migrate "echo migrate"
            reset "echo reset"
        }
        "#,
    );

    let output = lets_bin()
        .args(["--file", path.to_str().unwrap(), "db", "migrate"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("did you mean"),
        "should not warn for legitimate subcommands: {stderr}"
    );
}
