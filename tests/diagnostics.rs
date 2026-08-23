//! Error output: what a failing config or run actually tells the user.

mod common;

use common::{lets_bin, with_temp_kdl};

fn stderr_for(content: &str, args: &[&str]) -> String {
    let (_dir, path) = with_temp_kdl(content);
    let mut argv = vec!["--file", path.to_str().unwrap()];
    argv.extend_from_slice(args);
    let output = lets_bin().args(&argv).output().unwrap();
    assert!(!output.status.success(), "expected a failure");
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// `kdl` reports grammar violations as sub-diagnostics whose outer `Display`
/// is a constant string. Reporting that string alone left the most common
/// first-time error with no location at all.
#[test]
fn kdl_syntax_error_shows_message_location_and_snippet() {
    let stderr = stderr_for(
        "description \"unclosed\nbuild \"cargo build\"\n",
        &["--list"],
    );

    assert!(
        !stderr.contains("Failed to parse KDL document"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("quoted string"), "stderr: {stderr}");
    // Filename and line, from the snippet header.
    assert!(stderr.contains("lets.kdl:1:"), "stderr: {stderr}");
    // The offending source line itself.
    assert!(
        stderr.contains("description \"unclosed"),
        "stderr: {stderr}"
    );
}

#[test]
fn kdl_syntax_error_keeps_the_fix_hint() {
    let stderr = stderr_for("description \"unclosed\nbuild \"x\"\n", &["--list"]);

    // kdl suggests triple-quoting for multi-line strings; that hint is the
    // most useful part of the diagnostic and must survive the re-wrap.
    assert!(stderr.contains("multi-line"), "stderr: {stderr}");
}

#[test]
fn unclosed_block_points_at_the_block() {
    let stderr = stderr_for("build {\n    run \"cargo build\"\n", &["--list"]);

    assert!(stderr.contains("closing"), "stderr: {stderr}");
    assert!(stderr.contains("lets.kdl:1:"), "stderr: {stderr}");
}

#[test]
fn trailing_brace_points_at_the_brace() {
    let stderr = stderr_for("build \"x\"\n}\n", &["--list"]);

    assert!(stderr.contains("lets.kdl:2:1"), "stderr: {stderr}");
}

/// With no lets.kdl, clap used to report `unrecognized subcommand 'build'` —
/// technically true, but it hides the actual problem.
#[test]
fn missing_config_is_reported_instead_of_an_unknown_subcommand() {
    let dir = tempfile::tempdir().unwrap();

    for args in [
        vec!["build"],
        vec!["--list"],
        vec!["self", "check"],
        vec!["db", "migrate"],
    ] {
        let output = lets_bin()
            .current_dir(dir.path())
            .args(&args)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(!output.status.success(), "{args:?} should fail");
        assert!(
            stderr.contains("no lets.kdl found"),
            "{args:?} stderr: {stderr}"
        );
        assert!(
            stderr.contains("lets self init"),
            "{args:?} stderr: {stderr}"
        );
        assert!(
            !stderr.contains("unrecognized subcommand"),
            "{args:?} stderr: {stderr}"
        );
    }
}

/// Asking for help or the version needs no config, and `lets self init` has
/// to work in exactly the directory that has none.
#[test]
fn help_version_and_init_still_work_without_a_config() {
    let dir = tempfile::tempdir().unwrap();

    for args in [vec!["--help"], vec!["--version"], vec!["self", "init"]] {
        let output = lets_bin()
            .current_dir(dir.path())
            .args(&args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// Without a terminal, dialoguer failed with a bare "IO error: not a
/// terminal" — no task, no prompt, no way forward.
#[test]
fn prompts_without_a_terminal_name_the_task_and_the_way_out() {
    let cases = [
        (
            r#"deploy {
                choose environment "dev" "prod" default="dev"
                run "echo {environment}"
            }"#,
            "deploy",
            "choose environment",
            "\"dev\"",
        ),
        (
            r#"greet {
                prompt name "Who?" default="world"
                run "echo {name}"
            }"#,
            "greet",
            "prompt name",
            "\"world\"",
        ),
        (
            r#"clean {
                confirm "Sure?"
                run "echo cleaning"
            }"#,
            "clean",
            "confirm",
            "--yes",
        ),
    ];

    for (config, task, prompt, remedy) in cases {
        let stderr = stderr_for(config, &[task]);
        assert!(stderr.contains(task), "stderr: {stderr}");
        assert!(stderr.contains(prompt), "stderr: {stderr}");
        assert!(stderr.contains("--yes"), "stderr: {stderr}");
        assert!(stderr.contains(remedy), "stderr: {stderr}");
        assert!(!stderr.contains("IO error"), "stderr: {stderr}");
    }
}

/// A choose with no default can't be answered by --yes either; say so
/// rather than reporting only that there is no terminal.
#[test]
fn choose_without_a_default_asks_for_one() {
    let stderr = stderr_for(
        r#"deploy {
            choose environment "dev" "prod"
            run "echo {environment}"
        }"#,
        &["deploy"],
    );
    assert!(stderr.contains("default="), "stderr: {stderr}");
}

/// The task that needs the answer is named, not the one invoked.
#[test]
fn a_dependency_needing_input_names_itself() {
    let stderr = stderr_for(
        r#"
        deploy {
            confirm "Sure?"
            run "echo deploying"
        }
        release {
            deps "deploy"
            run "echo tagging"
        }
        "#,
        &["release"],
    );
    assert!(stderr.contains("task 'deploy'"), "stderr: {stderr}");
}
