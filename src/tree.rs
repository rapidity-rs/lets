//! Internal representation of a lets CLI, built from a parsed KDL file.
//!
//! The central type is [`CommandTree`], which holds the top-level description,
//! global config, and a tree of [`CommandNode`]s. Each node is decomposed into
//! sub-structs for clarity:
//!
//! - [`RunConfig`] — what to run (commands, platform variants)
//! - [`Orchestration`] — deps, steps, before/after hooks
//! - [`EnvConfig`] — environment variables and .env file
//! - [`ExecConfig`] — shell, dir, timeout, retry, silent
//! - [`Interactive`] — confirm, prompt, choose

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

/// Supported platforms for platform guards and platform-specific run commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Platform {
    Macos,
    Linux,
    Windows,
}

impl Platform {
    /// Returns the platform matching the current OS.
    pub fn current() -> Option<Self> {
        match std::env::consts::OS {
            "macos" => Some(Self::Macos),
            "linux" => Some(Self::Linux),
            "windows" => Some(Self::Windows),
            _ => None,
        }
    }
}

impl FromStr for Platform {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "macos" => Ok(Self::Macos),
            "linux" => Ok(Self::Linux),
            "windows" => Ok(Self::Windows),
            other => Err(format!(
                "unknown platform '{other}' (expected macos, linux, or windows)"
            )),
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Macos => write!(f, "macos"),
            Self::Linux => write!(f, "linux"),
            Self::Windows => write!(f, "windows"),
        }
    }
}

/// Internal representation of a lets CLI, built from a parsed KDL file.
#[derive(Debug, Clone)]
pub struct CommandTree {
    /// Top-level description shown in `--help`.
    pub description: Option<String>,
    /// Global configuration.
    pub config: Config,
    /// Top-level commands.
    pub commands: Vec<CommandNode>,
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Whether to sort commands alphabetically in help and list output.
    pub sorted: bool,
    /// Default shell for all commands (can be overridden per-command).
    pub shell: Option<String>,
}

/// Configuration for what command to run and on which platforms.
#[derive(Debug, Clone, Default)]
pub struct RunConfig {
    /// Shell commands to execute sequentially.
    pub commands: Vec<String>,
    /// Platform-specific run commands.
    pub platform_run: BTreeMap<Platform, String>,
    /// Restrict this command to specific platforms.
    pub platform: Vec<Platform>,
}

impl RunConfig {
    /// Returns true if any run command is configured (generic or platform-specific).
    pub fn has_command(&self) -> bool {
        !self.commands.is_empty() || !self.platform_run.is_empty()
    }

    /// Resolve the commands to execute. Platform-specific commands take priority;
    /// if none match, falls back to the generic commands list.
    pub fn resolve(&self) -> &[String] {
        if let Some(platform) = Platform::current()
            && let Some(cmd) = self.platform_run.get(&platform)
        {
            // Return platform command as a single-element slice.
            // Lifetime is tied to &self, so this is safe.
            return std::slice::from_ref(cmd);
        }
        &self.commands
    }
}

/// Task orchestration: deps, steps, hooks.
#[derive(Debug, Clone, Default)]
pub struct Orchestration {
    /// Tasks to run in parallel before this command. Each entry is (path, span).
    pub deps: Vec<(Vec<String>, miette::SourceSpan)>,
    /// Tasks to run sequentially before this command. Each entry is (path, span).
    pub steps: Vec<(Vec<String>, miette::SourceSpan)>,
    /// Shell command to run before the main `run`.
    pub before: Option<String>,
    /// Shell command to run after the main `run`.
    pub after: Option<String>,
}

/// Environment variable configuration.
#[derive(Debug, Clone, Default)]
pub struct EnvConfig {
    /// Explicit environment variables.
    pub vars: Vec<(String, String)>,
    /// Path to a .env file to load.
    pub file: Option<PathBuf>,
}

/// Execution settings: shell, dir, timeout, retry, silent.
#[derive(Debug, Clone, Default)]
pub struct ExecConfig {
    /// Working directory for this command.
    pub dir: Option<PathBuf>,
    /// Shell to use instead of the default `sh`.
    pub shell: Option<String>,
    /// Timeout duration.
    pub timeout: Option<Duration>,
    /// Number of retry attempts.
    pub retry_count: Option<u32>,
    /// Delay between retries.
    pub retry_delay: Option<Duration>,
    /// Suppress stdout unless command fails.
    pub silent: bool,
}

/// Interactive prompts and confirmations.
#[derive(Debug, Clone, Default)]
pub struct Interactive {
    /// Confirmation prompt shown before execution.
    pub confirm: Option<String>,
    /// Interactive prompts that bind user input to variables.
    pub prompts: Vec<PromptDef>,
    /// Interactive choice selections that bind to variables.
    pub chooses: Vec<ChooseDef>,
}

#[derive(Debug, Clone)]
pub struct CommandNode {
    /// Command name (used as the subcommand identifier).
    pub name: String,
    /// Source span of this node in the KDL file (for error reporting).
    #[allow(dead_code)]
    pub span: miette::SourceSpan,
    /// Help text shown in `--help`.
    pub description: Option<String>,
    /// Extended help text shown in `lets <cmd> --help`.
    pub long_description: Option<String>,
    /// Usage examples shown at the bottom of `--help`.
    pub examples: Option<String>,
    /// Hide this command from help output and listings.
    pub hide: bool,
    /// Deprecation message. Some("") = deprecated, Some(msg) = deprecated with message.
    pub deprecated: Option<String>,
    /// Positional arguments.
    pub args: Vec<ArgDef>,
    /// Flags (boolean options).
    pub flags: Vec<FlagDef>,
    /// Command aliases (e.g. `alias "t"` makes `lets t` work).
    pub aliases: Vec<String>,
    /// What to run.
    pub run: RunConfig,
    /// Task orchestration.
    pub orch: Orchestration,
    /// Environment variables.
    pub env: EnvConfig,
    /// Execution settings.
    pub exec: ExecConfig,
    /// Interactive features.
    pub interactive: Interactive,
    /// Subcommands (if this is a group).
    pub children: Vec<CommandNode>,
}

#[derive(Debug, Clone)]
pub struct PromptDef {
    /// Variable name (used in interpolation).
    pub name: String,
    /// Message shown to the user.
    pub message: String,
    /// Default value (used when --yes is passed).
    pub default: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChooseDef {
    /// Variable name (used in interpolation).
    pub name: String,
    /// Choices to present.
    pub choices: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ArgDef {
    /// Argument name (used in interpolation and as the clap ID).
    pub name: String,
    /// Help text shown in `--help`.
    pub help: Option<String>,
    /// Default value (makes the arg optional).
    pub default: Option<String>,
    /// Allowed values. Empty means any string is accepted.
    pub choices: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FlagDef {
    /// Flag name (used as `--name` and in interpolation).
    pub name: String,
    /// Single-character short alias (e.g. `-d`).
    pub short: Option<char>,
    /// Help text shown in `--help`.
    pub help: Option<String>,
    /// Value type. None = boolean flag, Some = flag that takes a value.
    pub value_type: Option<FlagType>,
    /// Default value (only for valued flags).
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlagType {
    String,
    Int,
    Float,
}

impl CommandTree {
    /// Resolve a task path like `["db", "migrate"]` to a `&CommandNode`.
    pub fn resolve_path(&self, path: &[String]) -> Option<&CommandNode> {
        let mut commands = &self.commands;
        let mut node = None;
        for segment in path {
            let found = commands.iter().find(|c| c.name == *segment)?;
            node = Some(found);
            commands = &found.children;
        }
        node
    }
}

impl CommandNode {
    /// Returns true if this node has something to execute (directly or via orchestration).
    pub fn is_runnable(&self) -> bool {
        self.run.has_command() || !self.orch.deps.is_empty() || !self.orch.steps.is_empty()
    }

    /// Returns true if this node has subcommands.
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }
}
