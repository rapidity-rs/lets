//! lets — a declarative CLI builder.
//!
//! Reads a `lets.kdl` config file and dynamically constructs a full
//! [clap](https://docs.rs/clap) CLI at runtime: subcommands, typed arguments,
//! flags, help text, and shell completions — all from a config file.
//!
//! # Architecture
//!
//! ```text
//! lets.kdl → parse → tree → cli (clap) → exec → shell
//! ```
//!
//! - [`parse`] — KDL file → [`tree::CommandTree`]
//! - [`tree`] — internal representation of commands, args, flags
//! - [`validate`] — ref resolution and cycle detection
//! - [`cli`] — converts tree into [`clap::Command`]
//! - [`exec`] — orchestration: deps, steps, hooks, interpolation
//! - [`shell`] — process spawning, timeout, retry, signal handling
//! - [`interpolate`] — unified `{…}` placeholder rendering
//! - [`discover`] — finds `lets.kdl` by walking up from cwd

mod cli;
mod commands;
mod discover;
mod error;
mod exec;
mod interpolate;
mod parse;
mod shell;
mod tree;
mod validate;

use std::path::PathBuf;
use std::process;

fn main() {
    if let Err(e) = run() {
        match &e {
            error::Error::CommandFailed { code } => process::exit(*code),
            _ => {
                let report: miette::Report = e.into();
                eprintln!("{report:?}");
                process::exit(1);
            }
        }
    }
}

fn run() -> error::Result<()> {
    // Handle dynamic shell completions via LETS_COMPLETE env var.
    // This must run before anything else — CompleteEnv will exit the process
    // after outputting completions.
    clap_complete::CompleteEnv::with_factory(|| {
        // Best-effort: build the full CLI if a config exists, otherwise just the self command.
        if let Ok(path) = resolve_config_path()
            && let Ok(tree) = parse::parse_file(&path)
        {
            return cli::build_cli(&tree);
        }
        cli::build_cli(&tree::CommandTree {
            description: None,
            config: tree::Config::default(),
            commands: Vec::new(),
        })
    })
    .var("LETS_COMPLETE")
    .complete();

    // Handle `lets self init` before config discovery (no lets.kdl needed).
    // We check raw args to avoid requiring a config file for init.
    if is_self_init() {
        let cmd = cli::build_self_command();
        let matches = cmd.get_matches_from(std::env::args().skip(1));
        if let Some(("init", _)) = matches.subcommand() {
            return commands::cmd_init();
        }
    }

    // Handle `lets self setup <shell>` before config discovery.
    if is_self_setup() {
        return commands::handle_self_setup();
    }

    let config_path = match resolve_config_path() {
        Ok(path) => path,
        Err(error::Error::ConfigNotFound { .. }) => {
            return handle_no_config();
        }
        Err(e) => return Err(e),
    };
    let tree = parse::parse_file(&config_path)?;
    let mut clap_cmd = cli::build_cli(&tree);

    let matches = clap_cmd.clone().get_matches();

    // Built-in flags.
    if matches.get_flag("list") {
        commands::print_command_list(&tree);
        return Ok(());
    }

    // Handle `lets self <subcommand>`.
    if let Some(("self", self_matches)) = matches.subcommand() {
        return handle_self(&tree, &mut clap_cmd, self_matches);
    }

    if matches.subcommand().is_none() {
        clap_cmd.print_help().ok();
        println!();
        return Ok(());
    }

    exec::run(&tree, &matches)
}

/// Handle `lets self <subcommand>` after config is loaded.
fn handle_self(
    tree: &tree::CommandTree,
    clap_cmd: &mut clap::Command,
    matches: &clap::ArgMatches,
) -> error::Result<()> {
    match matches.subcommand() {
        Some(("check", _)) => {
            println!(
                "lets.kdl is valid ({} commands)",
                commands::count_commands(tree)
            );
            Ok(())
        }
        Some(("completions", sub_matches)) => {
            let shell = sub_matches
                .get_one::<clap_complete::Shell>("shell")
                .copied()
                .unwrap();
            clap_complete::generate(shell, clap_cmd, "lets", &mut std::io::stdout());
            Ok(())
        }
        _ => {
            // Should not happen — clap enforces subcommand_required.
            Ok(())
        }
    }
}

/// Check if the user is running `lets self init` by scanning raw args.
/// This avoids needing a config file to exist before we can parse the CLI.
fn is_self_init() -> bool {
    let args: Vec<String> = std::env::args().collect();
    // Look for "self" followed by "init", skipping flags like --file.
    let positional: Vec<&str> = args[1..]
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(|s| s.as_str())
        .collect();
    positional.first() == Some(&"self") && positional.get(1) == Some(&"init")
}

/// Check if the user is running `lets self setup`.
fn is_self_setup() -> bool {
    let args: Vec<String> = std::env::args().collect();
    let positional: Vec<&str> = args[1..]
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(|s| s.as_str())
        .collect();
    positional.first() == Some(&"self") && positional.get(1) == Some(&"setup")
}

/// Handle the case where no lets.kdl is found.
/// Shows a friendly message instead of a raw error.
fn handle_no_config() -> error::Result<()> {
    eprintln!("No lets.kdl found in this directory or any parent.");
    eprintln!();
    eprintln!("To get started, run:");
    eprintln!();
    eprintln!("  lets self init");
    eprintln!();
    process::exit(1);
}

fn resolve_config_path() -> error::Result<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    for i in 0..args.len() {
        if (args[i] == "--file" || args[i] == "-f") && i + 1 < args.len() {
            let path = PathBuf::from(&args[i + 1]);
            if path.is_file() {
                return Ok(path);
            }
            return Err(error::Error::ReadFile {
                path: path.clone(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
            });
        }
    }

    let cwd = std::env::current_dir().map_err(|e| error::Error::Other(format!("{e}")))?;
    discover::find_config(&cwd)
}
