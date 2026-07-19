//! Every directory under examples/ is a runnable lets project. This harness
//! keeps them honest: each config must validate, build a CLI, and dry-run its
//! listed commands. The docs render these files verbatim, so a failure here
//! means the documentation is showing something the tool no longer supports.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::lets_bin;

/// Smoke invocations per example: each is dry-run with `--yes`.
/// Every example directory MUST have an entry (enforced below).
const SMOKE: &[(&str, &[&[&str]])] = &[
    ("basics", &[&["build"], &["t"], &["db", "migrate"]]),
    (
        "args-and-flags",
        &[
            &["deploy", "staging"],
            &["greet"],
            &["build", "--release"],
            &["serve", "-p", "8080"],
            &["test", "--", "--nocapture"],
        ],
    ),
    ("orchestration", &[&["ci"], &["release"]]),
    ("output-modes", &[&["all"]]),
    ("interactive", &[&["deploy"], &["greet"], &["release"]]),
    (
        "environment",
        &[&["serve"], &["show-env"], &["install"], &["inspect"]],
    ),
    ("watch", &[&["dev"], &["test"]]),
    (
        "advanced",
        &[
            &["stack"],
            &["bundle"],
            &["init"],
            &["guarded"],
            &["health-check"],
            &["deploy"],
            &["old-deploy"],
            &["ship"],
        ],
    ),
    ("monorepo", &[&["ci"], &["frontend", "build"]]),
];

fn examples_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn run_in(dir: &Path, args: &[&str]) -> std::process::Output {
    lets_bin().current_dir(dir).args(args).output().unwrap()
}

/// Every example dir has a smoke entry and every entry has a dir.
#[test]
fn smoke_table_covers_every_example() {
    let on_disk: BTreeSet<String> = std::fs::read_dir(examples_root())
        .unwrap()
        .filter_map(|e| {
            let e = e.unwrap();
            let has_config = e.path().join("lets.kdl").is_file();
            has_config.then(|| e.file_name().to_string_lossy().into_owned())
        })
        .collect();
    let in_table: BTreeSet<String> = SMOKE.iter().map(|(name, _)| name.to_string()).collect();

    assert_eq!(
        on_disk, in_table,
        "examples/ directories and the SMOKE table must match; \
         add new examples to tests/examples.rs"
    );
}

/// Each example parses, validates, and builds a CLI.
#[test]
fn every_example_validates() {
    for (name, _) in SMOKE {
        let dir = examples_root().join(name);

        let check = run_in(&dir, &["self", "check"]);
        assert!(
            check.status.success(),
            "{name}: self check failed:\n{}",
            String::from_utf8_lossy(&check.stderr)
        );

        let list = run_in(&dir, &["--list"]);
        assert!(
            list.status.success(),
            "{name}: --list failed:\n{}",
            String::from_utf8_lossy(&list.stderr)
        );
    }
}

/// Each documented invocation resolves and interpolates under dry-run.
#[test]
fn every_example_smoke_runs() {
    for (name, invocations) in SMOKE {
        let dir = examples_root().join(name);
        for invocation in *invocations {
            let mut args = vec!["--dry-run", "--yes"];
            args.extend_from_slice(invocation);
            let output = run_in(&dir, &args);
            assert!(
                output.status.success(),
                "{name}: `lets {}` failed:\nstdout:\n{}\nstderr:\n{}",
                invocation.join(" "),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
