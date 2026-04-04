//! Unified placeholder interpolation.
//!
//! Scans for `{…}` placeholders and calls a resolver closure for each one.
//! The resolver returns `Some(replacement)` to substitute, or `None` to leave empty.

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

/// Render a template string by scanning for `{…}` placeholders and resolving each
/// via the provided closure.
pub fn render(
    template: &str,
    mut resolve: impl FnMut(Placeholder<'_>) -> Option<String>,
) -> String {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut placeholder = String::new();
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
                placeholder.push(inner);
            }

            let parsed = if placeholder == "--" {
                Placeholder::Passthrough
            } else if let Some(var_name) = placeholder.strip_prefix('$') {
                Placeholder::EnvVar(var_name)
            } else if let Some(rest) = placeholder.strip_prefix('?') {
                if let Some((flag_name, text)) = rest.split_once(':') {
                    Placeholder::Conditional(flag_name, text)
                } else {
                    // Malformed conditional — treat as variable
                    Placeholder::Variable(&placeholder)
                }
            } else {
                Placeholder::Variable(&placeholder)
            };

            if let Some(value) = resolve(parsed) {
                result.push_str(&value);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn simple_variable() {
        let vars: HashMap<&str, &str> = [("name", "world")].into();
        let result = render("hello {name}!", |p| match p {
            Placeholder::Variable(name) => vars.get(name).map(|v| v.to_string()),
            _ => None,
        });
        assert_eq!(result, "hello world!");
    }

    #[test]
    fn env_var() {
        let result = render("port={$PORT}", |p| match p {
            Placeholder::EnvVar("PORT") => Some("3000".to_string()),
            _ => None,
        });
        assert_eq!(result, "port=3000");
    }

    #[test]
    fn conditional_true() {
        let result = render("cargo build {?release:--release}", |p| match p {
            Placeholder::Conditional("release", text) => Some(text.to_string()),
            _ => None,
        });
        assert_eq!(result, "cargo build --release");
    }

    #[test]
    fn conditional_false() {
        let result = render("cargo build {?release:--release}", |p| match p {
            Placeholder::Conditional(_, _) => None,
            _ => None,
        });
        assert_eq!(result, "cargo build ");
    }

    #[test]
    fn passthrough() {
        let result = render("cmd {--}", |p| match p {
            Placeholder::Passthrough => Some("--foo bar".to_string()),
            _ => None,
        });
        assert_eq!(result, "cmd --foo bar");
    }

    #[test]
    fn no_placeholders() {
        let result = render("plain text", |_| None);
        assert_eq!(result, "plain text");
    }

    #[test]
    fn unresolved_variable() {
        let result = render("hello {unknown}!", |_| None);
        assert_eq!(result, "hello !");
    }
}
