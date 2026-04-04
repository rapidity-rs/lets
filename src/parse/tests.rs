use super::typo::check_typo;
use super::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::tree::{FlagType, Platform};

fn parse(input: &str) -> CommandTree {
    parse_source(input, &PathBuf::from("test.kdl")).unwrap()
}

#[test]
fn one_liner() {
    let tree = parse(r#"dev "cargo watch -x run""#);
    assert_eq!(tree.commands.len(), 1);
    assert_eq!(tree.commands[0].name, "dev");
    assert_eq!(
        tree.commands[0].run.commands.first().map(|s| s.as_str()),
        Some("cargo watch -x run")
    );
}

#[test]
fn top_level_description() {
    let tree = parse(r#"description "My project""#);
    assert_eq!(tree.description.as_deref(), Some("My project"));
    assert!(tree.commands.is_empty());
}

#[test]
fn block_with_run() {
    let tree = parse(
        r#"
        deploy {
            description "Deploy the app"
            run "scripts/deploy.sh"
        }
        "#,
    );
    assert_eq!(tree.commands.len(), 1);
    let cmd = &tree.commands[0];
    assert_eq!(cmd.name, "deploy");
    assert_eq!(cmd.description.as_deref(), Some("Deploy the app"));
    assert_eq!(
        cmd.run.commands.first().map(|s| s.as_str()),
        Some("scripts/deploy.sh")
    );
}

#[test]
fn nested_subcommands() {
    let tree = parse(
        r#"
        db {
            description "Database commands"
            migrate "diesel migration run"
            reset "diesel database reset"
        }
        "#,
    );
    assert_eq!(tree.commands.len(), 1);
    let db = &tree.commands[0];
    assert_eq!(db.name, "db");
    assert_eq!(db.children.len(), 2);
    assert_eq!(db.children[0].name, "migrate");
    assert_eq!(db.children[1].name, "reset");
}

#[test]
fn mixed_commands() {
    let tree = parse(
        r#"
        description "My project tasks"
        dev "cargo watch -x run"
        test "cargo test"
        db {
            migrate "diesel migration run"
            reset "diesel database reset"
        }
        "#,
    );
    assert_eq!(tree.description.as_deref(), Some("My project tasks"));
    assert_eq!(tree.commands.len(), 3);
    assert_eq!(tree.commands[2].children.len(), 2);
}

#[test]
fn arg_with_choices() {
    let tree = parse(
        r#"
        deploy {
            arg environment "dev" "staging" "prod"
            run "deploy.sh {environment}"
        }
        "#,
    );
    let cmd = &tree.commands[0];
    assert_eq!(cmd.args.len(), 1);
    let arg = &cmd.args[0];
    assert_eq!(arg.name, "environment");
    assert_eq!(arg.choices, vec!["dev", "staging", "prod"]);
    assert!(arg.default.is_none());
}

#[test]
fn arg_with_help_and_default() {
    let tree = parse(
        r#"
        greet {
            arg name help="Who to greet" default="world"
            run "echo hello {name}"
        }
        "#,
    );
    let arg = &tree.commands[0].args[0];
    assert_eq!(arg.name, "name");
    assert_eq!(arg.help.as_deref(), Some("Who to greet"));
    assert_eq!(arg.default.as_deref(), Some("world"));
}

#[test]
fn arg_simple_required() {
    let tree = parse(
        r#"
        greet {
            arg name
            run "echo hello {name}"
        }
        "#,
    );
    let arg = &tree.commands[0].args[0];
    assert_eq!(arg.name, "name");
    assert!(arg.help.is_none());
    assert!(arg.default.is_none());
    assert!(arg.choices.is_empty());
}

#[test]
fn multiple_args() {
    let tree = parse(
        r#"
        copy {
            arg source
            arg dest
            run "cp {source} {dest}"
        }
        "#,
    );
    let cmd = &tree.commands[0];
    assert_eq!(cmd.args.len(), 2);
    assert_eq!(cmd.args[0].name, "source");
    assert_eq!(cmd.args[1].name, "dest");
}

#[test]
fn flag_with_short_and_help() {
    let tree = parse(
        r#"
        deploy {
            flag dry-run "-d" help="Show what would happen"
            run "deploy.sh"
        }
        "#,
    );
    let flag = &tree.commands[0].flags[0];
    assert_eq!(flag.name, "dry-run");
    assert_eq!(flag.short, Some('d'));
    assert_eq!(flag.help.as_deref(), Some("Show what would happen"));
}

#[test]
fn flag_simple() {
    let tree = parse(
        r#"
        build {
            flag verbose
            run "cargo build"
        }
        "#,
    );
    let flag = &tree.commands[0].flags[0];
    assert_eq!(flag.name, "verbose");
    assert!(flag.short.is_none());
    assert!(flag.help.is_none());
}

#[test]
fn multiple_flags() {
    let tree = parse(
        r#"
        build {
            flag verbose "-v"
            flag release "-r"
            run "cargo build"
        }
        "#,
    );
    let cmd = &tree.commands[0];
    assert_eq!(cmd.flags.len(), 2);
    assert_eq!(cmd.flags[0].name, "verbose");
    assert_eq!(cmd.flags[0].short, Some('v'));
    assert_eq!(cmd.flags[1].name, "release");
    assert_eq!(cmd.flags[1].short, Some('r'));
}

#[test]
fn flag_typed_int_with_default() {
    let tree = parse(
        r#"
        deploy {
            flag replicas "-r" type="int" default="3"
            run "deploy --replicas {replicas}"
        }
        "#,
    );
    let flag = &tree.commands[0].flags[0];
    assert_eq!(flag.name, "replicas");
    assert_eq!(flag.short, Some('r'));
    assert_eq!(flag.value_type, Some(FlagType::Int));
    assert_eq!(flag.default.as_deref(), Some("3"));
}

#[test]
fn flag_typed_string() {
    let tree = parse(
        r#"
        run-cmd {
            flag output "-o" type="string" default="json"
            run "cmd --output {output}"
        }
        "#,
    );
    let flag = &tree.commands[0].flags[0];
    assert_eq!(flag.name, "output");
    assert_eq!(flag.value_type, Some(FlagType::String));
    assert_eq!(flag.default.as_deref(), Some("json"));
}

#[test]
fn flag_typed_float() {
    let tree = parse(
        r#"
        scale {
            flag factor type="float" default="1.5"
            run "scale --factor {factor}"
        }
        "#,
    );
    let flag = &tree.commands[0].flags[0];
    assert_eq!(flag.name, "factor");
    assert_eq!(flag.value_type, Some(FlagType::Float));
    assert_eq!(flag.default.as_deref(), Some("1.5"));
}

#[test]
fn steps_parsing() {
    let tree = parse(
        r#"
        lint "cargo clippy"
        test "cargo test"
        build "cargo build --release"
        ci {
            steps "lint" "test" "build"
        }
        "#,
    );
    let ci = &tree.commands[3];
    assert_eq!(ci.name, "ci");
    assert_eq!(
        paths(&ci.orch.steps),
        vec![vec!["lint"], vec!["test"], vec!["build"]]
    );
    assert!(ci.run.commands.is_empty());
}

#[test]
fn deps_parsing() {
    let tree = parse(
        r#"
        test "cargo test"
        build "cargo build --release"
        release {
            deps "test" "build"
            run "gh release create"
        }
        "#,
    );
    let release = &tree.commands[2];
    assert_eq!(paths(&release.orch.deps), vec![vec!["test"], vec!["build"]]);
    assert_eq!(
        release.run.commands.first().map(|s| s.as_str()),
        Some("gh release create")
    );
}

#[test]
fn deps_nested_path() {
    let tree = parse(
        r#"
        db {
            migrate "diesel migration run"
        }
        deploy {
            deps "db migrate"
            run "scripts/deploy.sh"
        }
        "#,
    );
    let deploy = &tree.commands[1];
    assert_eq!(paths(&deploy.orch.deps), vec![vec!["db", "migrate"]]);
}

#[test]
fn before_after_hooks() {
    let tree = parse(
        r#"
        deploy {
            before "echo starting"
            after "echo done"
            run "scripts/deploy.sh"
        }
        "#,
    );
    let deploy = &tree.commands[0];
    assert_eq!(deploy.orch.before.as_deref(), Some("echo starting"));
    assert_eq!(deploy.orch.after.as_deref(), Some("echo done"));
}

#[test]
fn is_runnable_with_steps_only() {
    let tree = parse(
        r#"
        lint "cargo clippy"
        test "cargo test"
        ci {
            steps "lint" "test"
        }
        "#,
    );
    let ci = &tree.commands[2];
    assert!(ci.is_runnable());
    assert!(ci.run.commands.is_empty());
}

#[test]
fn resolve_path_flat() {
    let tree = parse(r#"test "cargo test""#);
    let node = tree.resolve_path(&["test".to_string()]).unwrap();
    assert_eq!(node.name, "test");
}

#[test]
fn resolve_path_nested() {
    let tree = parse(
        r#"
        db {
            migrate "diesel migration run"
        }
        "#,
    );
    let node = tree
        .resolve_path(&["db".to_string(), "migrate".to_string()])
        .unwrap();
    assert_eq!(node.name, "migrate");
}

#[test]
fn resolve_path_missing() {
    let tree = parse(r#"test "cargo test""#);
    assert!(tree.resolve_path(&["nope".to_string()]).is_none());
}

fn paths(refs: &[(Vec<String>, miette::SourceSpan)]) -> Vec<Vec<String>> {
    refs.iter().map(|(p, _)| p.clone()).collect()
}

#[test]
fn env_parsing() {
    let tree = parse(
        r#"
        serve {
            env PORT="3000" RUST_LOG="debug"
            run "cargo run"
        }
        "#,
    );
    let cmd = &tree.commands[0];
    assert_eq!(
        cmd.env.vars,
        vec![
            ("PORT".to_string(), "3000".to_string()),
            ("RUST_LOG".to_string(), "debug".to_string()),
        ]
    );
}

#[test]
fn env_file_parsing() {
    let tree = parse(
        r#"
        serve {
            env-file ".env.local"
            run "cargo run"
        }
        "#,
    );
    assert_eq!(
        tree.commands[0].env.file.as_deref(),
        Some(Path::new(".env.local"))
    );
}

#[test]
fn dir_parsing() {
    let tree = parse(
        r#"
        build {
            dir "frontend"
            run "npm run build"
        }
        "#,
    );
    assert_eq!(
        tree.commands[0].exec.dir.as_deref(),
        Some(Path::new("frontend"))
    );
}

#[test]
fn shell_parsing() {
    let tree = parse(
        r#"
        test {
            shell "bash"
            run "echo hello"
        }
        "#,
    );
    assert_eq!(tree.commands[0].exec.shell.as_deref(), Some("bash"));
}

#[test]
fn platform_parsing() {
    let tree = parse(
        r#"
        install {
            platform "macos" "linux"
            run "echo install"
        }
        "#,
    );
    assert_eq!(
        tree.commands[0].run.platform,
        vec![Platform::Macos, Platform::Linux]
    );
}

#[test]
fn platform_run_variants() {
    let tree = parse(
        r#"
        install {
            run-macos "brew install foo"
            run-linux "apt install foo"
            run-windows "choco install foo"
        }
        "#,
    );
    let cmd = &tree.commands[0];
    assert_eq!(
        cmd.run
            .platform_run
            .get(&Platform::Macos)
            .map(|s| s.as_str()),
        Some("brew install foo")
    );
    assert_eq!(
        cmd.run
            .platform_run
            .get(&Platform::Linux)
            .map(|s| s.as_str()),
        Some("apt install foo")
    );
    assert_eq!(
        cmd.run
            .platform_run
            .get(&Platform::Windows)
            .map(|s| s.as_str()),
        Some("choco install foo")
    );
    assert!(cmd.run.commands.is_empty());
}

#[test]
fn confirm_parsing() {
    let tree = parse(
        r#"
        deploy {
            confirm "Are you sure?"
            run "scripts/deploy.sh"
        }
        "#,
    );
    assert_eq!(
        tree.commands[0].interactive.confirm.as_deref(),
        Some("Are you sure?")
    );
}

#[test]
fn prompt_parsing() {
    let tree = parse(
        r#"
        greet {
            prompt name "What is your name?" default="world"
            run "echo hello {name}"
        }
        "#,
    );
    let prompt = &tree.commands[0].interactive.prompts[0];
    assert_eq!(prompt.name, "name");
    assert_eq!(prompt.message, "What is your name?");
    assert_eq!(prompt.default.as_deref(), Some("world"));
}

#[test]
fn prompt_without_message() {
    let tree = parse(
        r#"
        greet {
            prompt name
            run "echo hello {name}"
        }
        "#,
    );
    let prompt = &tree.commands[0].interactive.prompts[0];
    assert_eq!(prompt.name, "name");
    assert_eq!(prompt.message, "name: ");
}

#[test]
fn choose_parsing() {
    let tree = parse(
        r#"
        deploy {
            choose environment "dev" "staging" "prod"
            run "deploy.sh {environment}"
        }
        "#,
    );
    let choose = &tree.commands[0].interactive.chooses[0];
    assert_eq!(choose.name, "environment");
    assert_eq!(choose.choices, vec!["dev", "staging", "prod"]);
}

#[test]
fn alias_parsing() {
    let tree = parse(
        r#"
        test {
            alias "t" "tst"
            run "cargo test"
        }
        "#,
    );
    assert_eq!(tree.commands[0].aliases, vec!["t", "tst"]);
}

#[test]
fn timeout_parsing() {
    let tree = parse(
        r#"
        slow {
            timeout "30s"
            run "sleep 60"
        }
        "#,
    );
    assert_eq!(tree.commands[0].exec.timeout, Some(Duration::from_secs(30)));
}

#[test]
fn retry_parsing() {
    let tree = parse(
        r#"
        flaky {
            retry count=3 delay="1s"
            run "curl http://example.com"
        }
        "#,
    );
    let cmd = &tree.commands[0];
    assert_eq!(cmd.exec.retry_count, Some(3));
    assert_eq!(cmd.exec.retry_delay, Some(Duration::from_secs(1)));
}

#[test]
fn silent_parsing() {
    let tree = parse(
        r#"
        quiet-cmd {
            silent
            run "echo shh"
        }
        "#,
    );
    assert!(tree.commands[0].exec.silent);
}

#[test]
fn quiet_alias_parsing() {
    let tree = parse(
        r#"
        quiet-cmd {
            quiet
            run "echo shh"
        }
        "#,
    );
    assert!(tree.commands[0].exec.silent);
}

#[test]
fn explicit_cmd_top_level() {
    let tree = parse(
        r#"
        cmd config {
            description "Manage configuration"
            run "echo config"
        }
        "#,
    );
    assert_eq!(tree.commands.len(), 1);
    assert_eq!(tree.commands[0].name, "config");
    assert_eq!(
        tree.commands[0].run.commands.first().map(|s| s.as_str()),
        Some("echo config")
    );
}

#[test]
fn explicit_cmd_inline() {
    let tree = parse(r#"cmd include "echo include""#);
    assert_eq!(tree.commands[0].name, "include");
    assert_eq!(
        tree.commands[0].run.commands.first().map(|s| s.as_str()),
        Some("echo include")
    );
}

#[test]
fn explicit_cmd_as_subcommand() {
    let tree = parse(
        r#"
        deploy {
            description "Deploy commands"
            run "scripts/deploy.sh"
            cmd run {
                description "Run the service"
                run "scripts/start.sh"
            }
        }
        "#,
    );
    let deploy = &tree.commands[0];
    assert_eq!(
        deploy.run.commands.first().map(|s| s.as_str()),
        Some("scripts/deploy.sh")
    );
    assert_eq!(deploy.children.len(), 1);
    assert_eq!(deploy.children[0].name, "run");
    assert_eq!(
        deploy.children[0].run.commands.first().map(|s| s.as_str()),
        Some("scripts/start.sh")
    );
}

#[test]
fn explicit_cmd_reserved_names_as_subcommands() {
    let tree = parse(
        r#"
        tools {
            cmd alias {
                run "echo managing aliases"
            }
            cmd flag {
                run "echo managing flags"
            }
        }
        "#,
    );
    let tools = &tree.commands[0];
    assert_eq!(tools.children.len(), 2);
    assert_eq!(tools.children[0].name, "alias");
    assert_eq!(tools.children[1].name, "flag");
}

#[test]
fn hide_parsing() {
    let tree = parse(
        r#"
        setup-db {
            hide
            run "diesel database setup"
        }
        "#,
    );
    assert!(tree.commands[0].hide);
}

#[test]
fn long_description_parsing() {
    let tree = parse(
        r#"
        deploy {
            description "Deploy the app"
            long-description "Deploy the app to production. Runs migrations first."
            run "scripts/deploy.sh"
        }
        "#,
    );
    let cmd = &tree.commands[0];
    assert_eq!(cmd.description.as_deref(), Some("Deploy the app"));
    assert_eq!(
        cmd.long_description.as_deref(),
        Some("Deploy the app to production. Runs migrations first.")
    );
}

#[test]
fn examples_parsing() {
    let tree = parse(
        r#"
        deploy {
            description "Deploy the app"
            examples "lets deploy staging"
            run "scripts/deploy.sh"
        }
        "#,
    );
    assert_eq!(
        tree.commands[0].examples.as_deref(),
        Some("lets deploy staging")
    );
}

#[test]
fn deprecated_bare() {
    let tree = parse(
        r#"
        old-cmd {
            deprecated
            run "echo old"
        }
        "#,
    );
    assert_eq!(tree.commands[0].deprecated.as_deref(), Some(""));
}

#[test]
fn deprecated_with_message() {
    let tree = parse(
        r#"
        old-cmd {
            deprecated "Use new-cmd instead"
            run "echo old"
        }
        "#,
    );
    assert_eq!(
        tree.commands[0].deprecated.as_deref(),
        Some("Use new-cmd instead")
    );
}

#[test]
fn typo_detection() {
    assert_eq!(check_typo("descrption"), Some("description"));
    assert_eq!(check_typo("descripion"), Some("description"));
    assert_eq!(check_typo("slient"), Some("silent"));
    assert_eq!(check_typo("rum"), Some("run"));
    assert_eq!(check_typo("aliass"), Some("alias"));
    assert_eq!(check_typo("env-fle"), Some("env-file"));
    // Legitimate subcommand names should not trigger.
    assert_eq!(check_typo("migrate"), None);
    assert_eq!(check_typo("build"), None);
    assert_eq!(check_typo("deploy"), None);
}

#[test]
fn one_liner_with_description_property() {
    let tree = parse(r#"greet "echo hello" description="Say hello""#);
    let cmd = &tree.commands[0];
    assert_eq!(cmd.name, "greet");
    assert_eq!(
        cmd.run.commands.first().map(|s| s.as_str()),
        Some("echo hello")
    );
    assert_eq!(cmd.description.as_deref(), Some("Say hello"));
}

#[test]
fn block_description_overrides_inline() {
    let tree = parse(
        r#"
        greet "echo hello" description="inline desc" {
            description "block desc"
        }
        "#,
    );
    assert_eq!(tree.commands[0].description.as_deref(), Some("block desc"));
}

#[test]
fn multiple_run_commands() {
    let tree = parse(
        r#"
        deploy {
            run "echo step1"
            run "echo step2"
            run "echo step3"
        }
        "#,
    );
    let cmd = &tree.commands[0];
    assert_eq!(cmd.run.commands.len(), 3);
    assert_eq!(cmd.run.commands[0], "echo step1");
    assert_eq!(cmd.run.commands[1], "echo step2");
    assert_eq!(cmd.run.commands[2], "echo step3");
}

#[test]
fn inline_cmd_plus_block_run() {
    let tree = parse(
        r#"
        deploy "echo inline" {
            run "echo extra"
        }
        "#,
    );
    let cmd = &tree.commands[0];
    assert_eq!(cmd.run.commands.len(), 2);
    assert_eq!(cmd.run.commands[0], "echo inline");
    assert_eq!(cmd.run.commands[1], "echo extra");
}
