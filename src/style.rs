//! Terminal styling: one decision, one palette.
//!
//! Colour is decided once at startup — from `--color`, `NO_COLOR`,
//! `CLICOLOR_FORCE`, `TERM`, and whether each stream is a terminal — and
//! every styled string in the crate goes through [`out`] or [`err`]. Stdout
//! and stderr are decided independently, because task output and
//! diagnostics are routinely redirected apart from each other.
//!
//! Strings handed to clap are the exception: clap parses the escapes into
//! its own representation and re-renders them under its colour setting, so
//! those use [`render`] and let clap decide.

use std::fmt::Display;
use std::io::IsTerminal;
use std::str::FromStr;
use std::sync::OnceLock;

use clap::builder::styling::{AnsiColor, Style};

/// When to emit colour, as requested on the command line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorChoice {
    /// Colour when the stream is a terminal that wants it.
    #[default]
    Auto,
    Always,
    Never,
}

impl FromStr for ColorChoice {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "auto" => Ok(ColorChoice::Auto),
            "always" => Ok(ColorChoice::Always),
            "never" => Ok(ColorChoice::Never),
            other => Err(format!(
                "invalid color mode '{other}' (expected auto, always, or never)"
            )),
        }
    }
}

/// Which output stream a styled string is bound for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// Resolved per-stream decision, computed once at startup.
struct Decision {
    stdout: bool,
    stderr: bool,
}

static DECISION: OnceLock<Decision> = OnceLock::new();
static CHOICE: OnceLock<ColorChoice> = OnceLock::new();

/// Resolve colour for the rest of the run. Called once, before anything
/// can print — including the diagnostic handler, which needs the same
/// answer for stderr.
pub fn init(choice: ColorChoice) {
    let _ = CHOICE.set(choice);
    let _ = DECISION.set(Decision {
        stdout: resolve(choice, Stream::Stdout),
        stderr: resolve(choice, Stream::Stderr),
    });
}

fn resolve(choice: ColorChoice, stream: Stream) -> bool {
    match choice {
        ColorChoice::Never => false,
        ColorChoice::Always => true,
        // An explicit flag beats the environment; a request to suppress
        // colour beats a request to force it.
        ColorChoice::Auto => {
            if env_enabled("NO_COLOR") {
                return false;
            }
            if env_enabled("CLICOLOR_FORCE") {
                return true;
            }
            if std::env::var("TERM").as_deref() == Ok("dumb") {
                return false;
            }
            match stream {
                Stream::Stdout => std::io::stdout().is_terminal(),
                Stream::Stderr => std::io::stderr().is_terminal(),
            }
        }
    }
}

/// A conventional on/off environment variable: set, non-empty, not "0".
fn env_enabled(name: &str) -> bool {
    std::env::var_os(name)
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

/// The requested colour mode, for handing to libraries that do their own
/// per-stream detection (clap).
pub fn choice() -> ColorChoice {
    CHOICE.get().copied().unwrap_or_default()
}

/// Whether `stream` takes colour.
pub fn enabled(stream: Stream) -> bool {
    let decision = DECISION.get_or_init(|| Decision {
        stdout: resolve(ColorChoice::Auto, Stream::Stdout),
        stderr: resolve(ColorChoice::Auto, Stream::Stderr),
    });
    match stream {
        Stream::Stdout => decision.stdout,
        Stream::Stderr => decision.stderr,
    }
}

/// Style `text` for stdout, or leave it plain.
pub fn out(style: Style, text: impl Display) -> String {
    styled(style, text, enabled(Stream::Stdout))
}

/// Style `text` for stderr, or leave it plain.
pub fn err(style: Style, text: impl Display) -> String {
    styled(style, text, enabled(Stream::Stderr))
}

/// Style `text` unconditionally. Only for strings handed to clap, which
/// parses the escapes and re-renders them under its own colour setting.
pub fn render(style: Style, text: impl Display) -> String {
    styled(style, text, true)
}

fn styled(style: Style, text: impl Display, enabled: bool) -> String {
    if enabled {
        format!("{}{text}{}", style.render(), style.render_reset())
    } else {
        text.to_string()
    }
}

/// De-emphasized text: tree connectors, descriptions, echoed commands,
/// anything the eye should skip on the way to the content.
pub const DIM: Style = Style::new().dimmed();

/// A command or task name.
pub const NAME: Style = AnsiColor::Cyan.on_default().bold();

/// A command group: something that holds subcommands but can't be run on
/// its own. Distinct from [`NAME`] so runnable commands stand out.
pub const GROUP: Style = Style::new().bold();

/// A section heading, matching clap's own.
pub const HEADING: Style = AnsiColor::Green.on_default().bold();

/// A task that succeeded.
pub const SUCCESS: Style = AnsiColor::Green.on_default();

/// A task that failed.
pub const FAILURE: Style = AnsiColor::Red.on_default();

/// Colours cycled across task labels so parallel output stays tellable
/// apart. Distinct hues rather than a gradient: adjacent tasks should not
/// look similar.
const LABELS: [AnsiColor; 6] = [
    AnsiColor::Cyan,
    AnsiColor::Magenta,
    AnsiColor::Green,
    AnsiColor::Yellow,
    AnsiColor::Blue,
    AnsiColor::BrightCyan,
];

/// The label colour for the `n`th task to start.
pub fn label(n: usize) -> Style {
    LABELS[n % LABELS.len()].on_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_choice_overrides_detection() {
        assert!(!resolve(ColorChoice::Never, Stream::Stdout));
        assert!(resolve(ColorChoice::Always, Stream::Stdout));
    }

    #[test]
    fn styled_is_a_no_op_when_disabled() {
        assert_eq!(styled(NAME, "build", false), "build");
        assert!(styled(NAME, "build", true).contains('\u{1b}'));
    }

    #[test]
    fn color_choice_parses_its_three_modes() {
        assert_eq!("auto".parse(), Ok(ColorChoice::Auto));
        assert_eq!("always".parse(), Ok(ColorChoice::Always));
        assert_eq!("never".parse(), Ok(ColorChoice::Never));
        assert!("sometimes".parse::<ColorChoice>().is_err());
    }
}
