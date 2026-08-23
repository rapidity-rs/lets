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
