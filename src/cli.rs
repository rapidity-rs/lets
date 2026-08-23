//! Clap CLI builder.
//!
//! Converts a [`CommandTree`] into a [`clap::Command`]
//! using the builder API (not derive macros, since the CLI structure is only
//! known at runtime).
//!
//! Also builds the `lets self` subcommand for internal management commands
//! (init, setup, check, completions).

use clap::Command;
use clap::builder::styling::{AnsiColor, Styles};
use clap_complete::Shell;

use crate::style;
use crate::tree::{ArgDef, CommandNode, CommandTree, FlagDef, FlagType};

/// clap's own help and error styling, drawn from the shared palette so
/// generated help matches everything lets prints itself.
const STYLES: Styles = Styles::styled()
    .header(style::HEADING)
    .usage(style::HEADING)
    .literal(style::NAME)
    .placeholder(AnsiColor::Cyan.on_default())
    .valid(style::SUCCESS)
    .invalid(AnsiColor::Yellow.on_default())
    .error(AnsiColor::Red.on_default().bold());

/// Leak a String to get a `&'static str`.
/// This is fine — we build the CLI once and it lives for the process lifetime.
fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

/// Build the `lets self` subcommand with all internal commands.
pub fn build_self_command() -> Command {
    Command::new("self")
        .about("Manage lets itself")
        .styles(STYLES)
        .disable_help_subcommand(true)
        .subcommand_required(true)
        .subcommand(Command::new("init").about("Generate a starter lets.kdl for this project"))
        .subcommand(
            Command::new("setup")
                .about("Print shell setup command for completions")
                .arg(
                    clap::Arg::new("shell")
                        .help("Shell to configure (zsh, bash, fish). Auto-detected if omitted.")
                        .required(false),
                ),
        )
        .subcommand(
            Command::new("check")
                .about("Validate the lets.kdl file without running anything")
                .arg(
                    clap::Arg::new("file")
                        .long("file")
                        .short('f')
                        .help("Path to lets.kdl file"),
                ),
        )
        .subcommand(
            Command::new("completions")
                .about("Generate shell completions")
                .arg(
                    clap::Arg::new("file")
                        .long("file")
                        .short('f')
                        .help("Path to lets.kdl file"),
                )
                .arg(
                    clap::Arg::new("shell")
                        .required(true)
                        .value_parser(clap::value_parser!(Shell)),
                ),
        )
}

/// Build a `clap::Command` from our internal command tree.
pub fn build_cli(tree: &CommandTree) -> Command {
    let mut app = Command::new("lets")
        .version(env!("CARGO_PKG_VERSION"))
        .styles(STYLES)
        // clap does its own per-stream detection under Auto, matching ours.
        .color(match style::choice() {
            style::ColorChoice::Auto => clap::ColorChoice::Auto,
            style::ColorChoice::Always => clap::ColorChoice::Always,
            style::ColorChoice::Never => clap::ColorChoice::Never,
        })
        .arg(
            clap::Arg::new("file")
                .long("file")
                .short('f')
                .help("Path to lets.kdl file")
                .global(true),
        )
        .arg(
            clap::Arg::new("yes")
                .long("yes")
                .short('y')
                .help("Skip all confirmation prompts")
                .action(clap::ArgAction::SetTrue)
                .global(true),
        )
        .arg(
            clap::Arg::new("dry-run")
                .long("dry-run")
                .help("Show what would be executed without running it")
                .action(clap::ArgAction::SetTrue)
                .global(true),
        )
        .arg(
            clap::Arg::new("output")
                .long("output")
                .help("Output mode for tasks run via deps/steps")
                .value_parser(clap::builder::PossibleValuesParser::new([
                    "interleaved",
                    "group",
                    "prefixed",
                ]))
                .global(true),
        )
        .arg(
            clap::Arg::new("watch")
                .long("watch")
                .help("Re-run the command when files in its sources change")
                .action(clap::ArgAction::SetTrue)
                .global(true),
        )
        .arg(
            clap::Arg::new("force")
                .long("force")
                .help("Run tasks even if their status checks report up to date")
                .action(clap::ArgAction::SetTrue)
                .global(true),
        )
        .arg(
            clap::Arg::new("jobs")
                .long("jobs")
                .help("Maximum number of tasks running concurrently")
                .value_parser(clap::value_parser!(u64).range(1..))
                .global(true),
        )
        .arg(
            clap::Arg::new("color")
                .long("color")
                .value_name("when")
                .help("When to use colored output")
                .value_parser(clap::builder::PossibleValuesParser::new([
                    "auto", "always", "never",
                ]))
                .global(true),
        )
        .arg(
            clap::Arg::new("list")
                .long("list")
                .help("List all available commands")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            clap::Arg::new("json")
                .long("json")
                .help("With --list: print the command tree as JSON")
                .action(clap::ArgAction::SetTrue)
                .requires("list"),
        )
        .arg(
            clap::Arg::new("verbose")
                .long("verbose")
                .help("Echo each command before it runs")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            clap::Arg::new("keep-going")
                .long("keep-going")
                .help("Continue independent tasks after a failure and report all failures")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            clap::Arg::new("summary")
                .long("summary")
                .help("Print a per-task status and timing table after the run")
                .action(clap::ArgAction::SetTrue),
        )
        .subcommand(build_self_command())
        .subcommand_required(false)
        .disable_help_subcommand(true);

    if let Some(desc) = &tree.description {
        app = app.about(leak(desc));
    }

    let commands = maybe_sorted(tree.commands.iter(), tree.config.sorted);
    for cmd in commands {
        app = app.subcommand(build_subcommand(cmd, tree.config.sorted));
    }

    app
}

fn maybe_sorted<'a>(
    iter: impl Iterator<Item = &'a CommandNode>,
    sorted: bool,
) -> Vec<&'a CommandNode> {
    let mut items: Vec<_> = iter.collect();
    if sorted {
        items.sort_by(|a, b| a.name.cmp(&b.name));
    }
    items
}

pub(crate) fn build_subcommand(node: &CommandNode, sorted: bool) -> Command {
    let mut cmd = Command::new(leak(&node.name)).disable_help_subcommand(true);

    if !node.aliases.is_empty() {
        let aliases: Vec<&'static str> = node.aliases.iter().map(|s| leak(s)).collect();
        cmd = cmd.aliases(aliases);
    }

    // Build the about string with optional alias/deprecated suffixes.
    if let Some(desc) = &node.description {
        let mut about = desc.clone();
        let mut suffixes = Vec::new();

        if !node.aliases.is_empty() {
            let alias_list = node.aliases.join(", ");
            let label = if node.aliases.len() == 1 {
                "alias"
            } else {
                "aliases"
            };
            suffixes.push(format!("{label}: {alias_list}"));
        }

        if let Some(msg) = &node.deprecated {
            if msg.is_empty() {
                suffixes.push("deprecated".to_string());
            } else {
                suffixes.push(format!("deprecated: {msg}"));
            }
        }

        if !suffixes.is_empty() {
            // clap parses these escapes and re-renders them under its own
            // colour setting, so they are emitted unconditionally.
            about = format!(
                "{about} {}",
                style::render(style::DIM, format!("({})", suffixes.join(", ")))
            );
        }

        cmd = cmd.about(leak(&about));
    }

    if let Some(long_desc) = &node.long_description {
        cmd = cmd.long_about(leak(long_desc));
    }

    if let Some(examples) = &node.examples {
        // Auto-indent each line so users can write examples without manual padding.
        let indented: String = examples
            .lines()
            .map(|line| {
                if line.trim().is_empty() {
                    String::new()
                } else {
                    format!("  {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let formatted = format!("{}\n{indented}", style::render(style::HEADING, "Examples:"));
        cmd = cmd.after_help(leak(&formatted));
    }

    if node.hide {
        cmd = cmd.hide(true);
    }

    for arg_def in &node.args {
        cmd = cmd.arg(build_arg(arg_def));
    }

    for flag_def in &node.flags {
        cmd = cmd.arg(build_flag(flag_def));
    }

    // If any template contains {--}, register a trailing var arg to capture
    // passthrough args (hooks and gates interpolate it too).
    let has_passthrough = node.uses_passthrough();
    if has_passthrough {
        cmd = cmd.arg(
            clap::Arg::new("--")
                .trailing_var_arg(true)
                .allow_hyphen_values(true)
                .num_args(..)
                .required(false)
                .hide(true),
        );
    }

    if node.has_children() {
        let children = maybe_sorted(node.children.iter(), sorted);
        for child in children {
            cmd = cmd.subcommand(build_subcommand(child, sorted));
        }
        if !node.is_runnable() {
            cmd = cmd.subcommand_required(true);
        }
    }

    cmd
}

fn typed_parser(arg: clap::Arg, value_type: Option<&FlagType>) -> clap::Arg {
    match value_type {
        Some(FlagType::Int) => arg.value_parser(clap::value_parser!(i64)),
        Some(FlagType::Float) => arg.value_parser(clap::value_parser!(f64)),
        Some(FlagType::String) | None => arg,
    }
}

fn build_arg(def: &ArgDef) -> clap::Arg {
    let mut arg = clap::Arg::new(leak(&def.name));

    if let Some(help) = &def.help {
        arg = arg.help(leak(help));
    }

    if def.rest {
        // Variadic: everything left on the command line. `required=#true`
        // demands at least one value.
        arg = arg.num_args(if def.required { 1.. } else { 0.. });
        arg = arg.required(def.required);
    } else if let Some(default) = &def.default {
        arg = arg.default_value(leak(default));
        arg = arg.required(false);
    } else {
        arg = arg.required(def.required);
    }

    if !def.choices.is_empty() {
        let values: Vec<&'static str> = def.choices.iter().map(|s| leak(s)).collect();
        arg = arg.value_parser(clap::builder::PossibleValuesParser::new(values));
    } else {
        arg = typed_parser(arg, def.value_type.as_ref());
    }

    if let Some(env) = &def.env {
        arg = arg.env(leak(env));
    }

    arg
}

fn build_flag(def: &FlagDef) -> clap::Arg {
    let mut flag = clap::Arg::new(leak(&def.name)).long(leak(&def.name));

    if def.value_type.is_some() {
        // Valued flag: takes an argument.
        flag = flag.num_args(1).required(false);

        if !def.choices.is_empty() {
            let values: Vec<&'static str> = def.choices.iter().map(|s| leak(s)).collect();
            flag = flag.value_parser(clap::builder::PossibleValuesParser::new(values));
        } else {
            flag = typed_parser(flag, def.value_type.as_ref());
        }

        if let Some(default) = &def.default {
            flag = flag.default_value(leak(default));
        }
        if let Some(env) = &def.env {
            flag = flag.env(leak(env));
        }
    } else {
        // Boolean flag.
        flag = flag.action(clap::ArgAction::SetTrue);
    }

    if let Some(ch) = def.short {
        flag = flag.short(ch);
    }

    if let Some(help) = &def.help {
        flag = flag.help(leak(help));
    }

    flag
}
