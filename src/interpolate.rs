//! Unified placeholder interpolation.
//!
//! Scans for `{…}` placeholders and calls a resolver closure for each one.
//! `{{` and `}}` are escapes for literal braces, so shell constructs like
//! `awk '{{print $1}}'` or `${{HOME}}` survive interpolation.
//!
//! The resolver returns a [`Resolution`]: substitute a value, substitute
//! nothing, or declare the placeholder unresolvable — which fails the whole
//! render. Silently dropping an unknown placeholder is never an option; it
//! corrupts the command in ways the user only notices much later.

use std::fmt;

/// Parsed placeholder types found inside `{…}`.
pub enum Placeholder<'a> {
    /// `{--}` — passthrough args
    Passthrough,
    /// `{$VAR}` — environment variable
    EnvVar(&'a str),
    /// `{?flag:text}` — conditional: include text if flag is set
    Conditional(&'a str, &'a str),
    /// `{name}` — variable (arg, flag, or interactive var)
    Variable(&'a str),
}

/// Outcome of resolving one placeholder.
pub enum Resolution {
    /// Substitute this text.
    Value(String),
    /// Substitute nothing (false conditional, unset environment variable,
    /// absent passthrough args).
    Skip,
    /// The placeholder cannot be resolved in this context; rendering fails.
    Unknown,
}

/// Why rendering a template failed.
#[derive(Debug, PartialEq, Eq)]
pub enum RenderError {
    /// A placeholder no resolver recognized.
    Unknown { placeholder: String },
    /// A `{` without a matching `}`.
    Unterminated,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::Unknown { placeholder } => write!(
                f,
                "unresolved placeholder '{{{placeholder}}}' \
                 (write '{{{{' and '}}}}' for literal braces)"
            ),
            RenderError::Unterminated => write!(
                f,
                "unterminated '{{' placeholder \
                 (write '{{{{' and '}}}}' for literal braces)"
            ),
        }
    }
}

impl std::error::Error for RenderError {}

fn parse_placeholder(placeholder: &str) -> Placeholder<'_> {
    if placeholder == "--" {
        Placeholder::Passthrough
    } else if let Some(var_name) = placeholder.strip_prefix('$') {
        Placeholder::EnvVar(var_name)
    } else if let Some(rest) = placeholder.strip_prefix('?') {
        if let Some((flag_name, text)) = rest.split_once(':') {
            Placeholder::Conditional(flag_name, text)
        } else {
            // Malformed conditional — surfaces as an unknown variable.
            Placeholder::Variable(placeholder)
        }
    } else {
        Placeholder::Variable(placeholder)
    }
}

/// Render a template string by scanning for `{…}` placeholders and resolving
/// each via the provided closure. `{{`/`}}` emit literal braces.
pub fn render(
    template: &str,
    mut resolve: impl FnMut(Placeholder<'_>) -> Resolution,
) -> Result<String, RenderError> {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                result.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                result.push('}');
            }
            '{' => {
                let mut placeholder = String::new();
                let mut terminated = false;
                for inner in chars.by_ref() {
                    if inner == '}' {
                        terminated = true;
                        break;
                    }
                    placeholder.push(inner);
                }
                if !terminated {
                    return Err(RenderError::Unterminated);
                }

                match resolve(parse_placeholder(&placeholder)) {
                    Resolution::Value(value) => result.push_str(&value),
                    Resolution::Skip => {}
                    Resolution::Unknown => return Err(RenderError::Unknown { placeholder }),
                }
            }
            _ => result.push(ch),
        }
    }

    Ok(result)
}

/// Quote a string for safe use as a single word in `sh -c`.
///
/// Bare-safe strings pass through; everything else is single-quoted with
/// embedded quotes rewritten as `'\''`.
pub fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./:=@%+,".contains(c))
    {
        return s.to_string();
    }
    let mut quoted = String::with_capacity(s.len() + 2);
    quoted.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn resolve_from(map: &HashMap<String, String>) -> impl FnMut(Placeholder<'_>) -> Resolution {
        move |p| match p {
            Placeholder::Variable(name) => match map.get(name) {
                Some(v) => Resolution::Value(v.clone()),
                None => Resolution::Unknown,
            },
            Placeholder::EnvVar(name) => match std::env::var(name) {
                Ok(v) => Resolution::Value(v),
                Err(_) => Resolution::Skip,
            },
            Placeholder::Conditional(name, text) => {
                if map.get(name).is_some_and(|v| v == "true") {
                    Resolution::Value(text.to_string())
                } else {
                    Resolution::Skip
                }
            }
            Placeholder::Passthrough => Resolution::Skip,
        }
    }

    #[test]
    fn simple_variable() {
        let map = vars(&[("name", "world")]);
        let out = render("hello {name}", resolve_from(&map)).unwrap();
        assert_eq!(out, "hello world");
    }

    #[test]
    fn env_var() {
        // SAFETY: test-local variable name, no concurrent reader relies on it.
        unsafe { std::env::set_var("LETS_TEST_INTERP", "42") };
        let map = vars(&[]);
        let out = render("v={$LETS_TEST_INTERP}", resolve_from(&map)).unwrap();
        assert_eq!(out, "v=42");
    }

    #[test]
    fn unset_env_var_renders_empty() {
        let map = vars(&[]);
        let out = render("v={$LETS_TEST_UNSET_XYZ}", resolve_from(&map)).unwrap();
        assert_eq!(out, "v=");
    }

    #[test]
    fn conditional_true() {
        let map = vars(&[("release", "true")]);
        let out = render("cargo build {?release:--release}", resolve_from(&map)).unwrap();
        assert_eq!(out, "cargo build --release");
    }

    #[test]
    fn conditional_false() {
        let map = vars(&[]);
        let out = render("cargo build {?release:--release}", resolve_from(&map)).unwrap();
        assert_eq!(out, "cargo build ");
    }

    #[test]
    fn no_placeholders() {
        let map = vars(&[]);
        let out = render("plain command", resolve_from(&map)).unwrap();
        assert_eq!(out, "plain command");
    }

    #[test]
    fn unresolved_variable_errors() {
        let map = vars(&[]);
        let err = render("hello {name}", resolve_from(&map)).unwrap_err();
        assert_eq!(
            err,
            RenderError::Unknown {
                placeholder: "name".to_string()
            }
        );
    }

    #[test]
    fn escaped_braces() {
        let map = vars(&[]);
        let out = render("awk '{{print $1}}'", resolve_from(&map)).unwrap();
        assert_eq!(out, "awk '{print $1}'");
    }

    #[test]
    fn escaped_braces_with_variable() {
        let map = vars(&[("file", "data.csv")]);
        let out = render("awk '{{print $2}}' {file}", resolve_from(&map)).unwrap();
        assert_eq!(out, "awk '{print $2}' data.csv");
    }

    #[test]
    fn double_escape_makes_go_template() {
        let map = vars(&[]);
        let out = render("--format '{{{{.State}}}}'", resolve_from(&map)).unwrap();
        assert_eq!(out, "--format '{{.State}}'");
    }

    #[test]
    fn lone_closing_brace_passes_through() {
        let map = vars(&[]);
        let out = render("fi; }", resolve_from(&map)).unwrap();
        assert_eq!(out, "fi; }");
    }

    #[test]
    fn unterminated_placeholder_errors() {
        let map = vars(&[]);
        let err = render("echo {oops", resolve_from(&map)).unwrap_err();
        assert_eq!(err, RenderError::Unterminated);
    }

    #[test]
    fn quote_passes_safe_words() {
        assert_eq!(shell_quote("--nocapture"), "--nocapture");
        assert_eq!(shell_quote("a/b.c:d=e"), "a/b.c:d=e");
    }

    #[test]
    fn quote_wraps_spaces_and_specials() {
        assert_eq!(shell_quote("foo bar"), "'foo bar'");
        assert_eq!(shell_quote("a;rm -rf"), "'a;rm -rf'");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }
}
