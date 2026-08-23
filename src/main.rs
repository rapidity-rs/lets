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
//! Loading a config:
//!
//! - [`discover`] — finds `lets.kdl` by walking up from cwd
//! - [`parse`] — KDL file → [`tree::CommandTree`]
//! - [`tree`] — internal representation of commands, args, flags
//! - [`validate`] — ref resolution and cycle detection
//! - [`error`] — every diagnostic the tool can report
//!
//! Running a command:
//!
//! - [`cli`] — converts tree into [`clap::Command`]
//! - [`exec`] — orchestration: deps, steps, hooks, interpolation
//! - [`shell`] — process spawning, timeout, retry, signal handling
//! - [`interpolate`] — unified `{…}` placeholder rendering
//! - [`fingerprint`] — content hashes behind `sources` up-to-date checks
//! - [`watch`] — re-runs a command when its `sources` change
//!
//! Presenting it:
//!
//! - [`style`] — one colour decision, one palette, shared text measurement
//! - [`output`] — per-task output routing for parallel runs
//! - [`listing`] — the `--list` tree and its JSON form
//! - [`picker`] — the interactive command picker for a bare `lets`
//! - [`plan`] — dry-run narration
//! - [`self_commands`] — the `lets self` family

mod cli;
mod discover;
mod error;
mod exec;
mod fingerprint;
mod interpolate;
mod listing;
mod output;
mod parse;
mod picker;
mod plan;
mod self_commands;
mod shell;
mod style;
mod tree;
mod validate;
mod watch;

use std::path::PathBuf;
use std::process;

fn main() {
    // Colour is resolved before anything can print, including diagnostics.
    style::init(resolve_color_choice());
    install_diagnostic_handler();
    let result = run();
    if exec::interrupted() {
        process::exit(130);
    }
    if let Err(e) = result {
        // A failing task keeps its own exit code so `lets test` substitutes
        // for the command it wraps, but it still says what failed.
        let code = e.exit_code();
        let report: miette::Report = e.into();
        eprintln!("{report:?}");
        process::exit(code);
    }
}

/// Render aggregate failures as one nested tree rather than a run of
/// separate `Error:` blocks, so several failed tasks read as one report.
fn install_diagnostic_handler() {
    let color = style::enabled(style::Stream::Stderr);
    let _ = miette::set_hook(Box::new(move |_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .show_related_errors_as_nested()
                .color(color)
                .build(),
        )
    }));
}

/// Read `--color` straight from argv. Diagnostics can fire while the config
/// is still being loaded, long before clap has parsed anything, so the
/// colour decision can't wait for `ArgMatches`. An invalid value is left
/// alone here; clap reports it properly a moment later.
fn resolve_color_choice() -> style::ColorChoice {
    let args: Vec<String> = std::env::args().collect();
    for (i, arg) in args.iter().enumerate() {
        let value = if arg == "--color" {
            args.get(i + 1).map(String::as_str)
        } else {
            arg.strip_prefix("--color=")
        };
        if let Some(value) = value {
            return value.parse().unwrap_or_default();
        }
    }
    style::ColorChoice::default()
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
            includes: Vec::new(),
            vars: Vec::new(),
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
            return self_commands::cmd_init();
        }
    }

    // Handle `lets self setup <shell>` before config discovery.
    if is_self_setup() {
        return self_commands::handle_self_setup();
    }

    let (mut tree, config_path) = match resolve_config_path() {
        Ok(path) => {
            let tree = parse::parse_file(&path)?;
            (tree, Some(path))
        }
        Err(e @ error::Error::ConfigNotFound { .. }) => {
            // Anything that names a command needs a config. Letting clap
            // parse first would report `unrecognized subcommand 'build'`
            // and bury the real problem: there is no lets.kdl here.
            if !is_help_only_invocation() {
                return Err(e);
            }
            // Bare `lets` still shows help, so a first-time user sees what
            // the tool offers alongside the hint.
            let empty = tree::CommandTree {
                description: None,
                config: tree::Config::default(),
                commands: Vec::new(),
                includes: Vec::new(),
                vars: Vec::new(),
            };
            (empty, None)
        }
        Err(e) => return Err(e),
    };
    let config_found = config_path.is_some();
    let mut clap_cmd = cli::build_cli(&tree);

    let matches = clap_cmd.clone().get_matches();

    // --output overrides the config's output mode.
    if let Some(mode) = matches.get_one::<String>("output") {
        tree.config.output = mode.parse().map_err(error::Error::Other)?;
    }

    // --jobs overrides the config's concurrency cap.
    if let Some(jobs) = matches.get_one::<u64>("jobs") {
        tree.config.jobs = Some(*jobs as usize);
    }

    // --verbose forces command echo on.
    if matches.get_flag("verbose") {
        tree.config.echo = true;
    }

    // Built-in flags.
    if matches.get_flag("list") {
        if matches.get_flag("json") {
            listing::print_command_list_json(&tree);
        } else {
            listing::print_command_list(&tree, false);
        }
        return Ok(());
    }

    // Handle `lets self <subcommand>`.
    if let Some(("self", self_matches)) = matches.subcommand() {
        return handle_self(&tree, &mut clap_cmd, self_matches);
    }

    // The project root anchors child cwd, `dir`/`env-file` resolution,
    // sources, and fingerprints. Absolute so none of it depends on where
    // the user invoked from (`--file relative.kdl` included).
    let project_root = config_path
        .as_deref()
        .and_then(std::path::Path::parent)
        .filter(|p| !p.as_os_str().is_empty())
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let project_root = std::path::absolute(&project_root).unwrap_or(project_root);

    if matches.subcommand().is_none() {
        // On a terminal, offer a picker over runnable commands; elsewhere
        // (pipes, CI) keep printing help.
        if config_found && picker::available() {
            match picker::pick(&tree) {
                picker::Outcome::Run(path) => {
                    let argv = std::env::args().skip(1).chain(path);
                    let matches = clap_cmd
                        .clone()
                        .get_matches_from(std::iter::once("lets".to_string()).chain(argv));
                    return exec::run(&tree, &matches, &project_root);
                }
                // Backing out of the picker is an answer, not a request for
                // help: return to the prompt rather than a wall of text.
                picker::Outcome::Cancelled => return Ok(()),
                picker::Outcome::Nothing => {}
            }
        }
        clap_cmd.print_help().ok();
        println!();
        if !config_found {
            eprintln!();
            eprintln!("No lets.kdl found. Run `lets self init` to get started.");
        }
        return Ok(());
    }

    // --watch supervises re-runs; the child re-execs without the flag.
    if matches.get_flag("watch")
        && let Some(path) = &config_path
    {
        return watch::run(&tree, &matches, path);
    }

    exec::run(&tree, &matches, &project_root)
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
                listing::count_commands(tree)
            );
            // The parsed tree, hidden commands included: an unintended
            // subcommand (e.g. a misplaced keyword) shows up here.
            println!();
            listing::print_command_list(tree, true);
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

/// Whether the invocation can be served without a config: a bare `lets`,
/// or one asking only for help or the version.
fn is_help_only_invocation() -> bool {
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().is_none() {
        return true;
    }
    args.any(|a| matches!(a.as_str(), "-h" | "--help" | "-V" | "--version"))
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

fn resolve_config_path() -> error::Result<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    for i in 1..args.len() {
        let arg = &args[i];
        // Match every spelling clap accepts: --file P, --file=P, -f P, -fP.
        let file_arg = if arg == "--file" || arg == "-f" {
            args.get(i + 1).cloned()
        } else if let Some(rest) = arg.strip_prefix("--file=") {
            Some(rest.to_string())
        } else if let Some(rest) = arg.strip_prefix("-f")
            && !arg.starts_with("--")
            && !rest.is_empty()
        {
            Some(rest.to_string())
        } else {
            None
        };
        if let Some(file_arg) = file_arg {
            let path = PathBuf::from(file_arg);
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
