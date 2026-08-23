//! Parsers for command child nodes: args, flags, prompts, and choices.

use crate::error::{Error, Result};
use crate::tree::{ArgDef, ChooseDef, FlagDef, FlagType, PromptDef};

use super::helpers::{named_bool, named_string, parse_string_list};

use kdl::KdlNode;

/// Parse a `type="…"` property into a `FlagType`. Unknown types are errors.
fn parse_value_type(node: &KdlNode, owner: &str) -> Result<Option<FlagType>> {
    match named_string(node, "type").as_deref() {
        None => Ok(None),
        Some("string") => Ok(Some(FlagType::String)),
        Some("int") => Ok(Some(FlagType::Int)),
        Some("float") => Ok(Some(FlagType::Float)),
        Some(other) => Err(Error::at(
            format!("invalid type '{other}' on '{owner}' (expected string, int, or float)"),
            node.span(),
        )),
    }
}

/// Parse an `arg` node into an `ArgDef`.
///
/// Supported forms:
///   arg name help="..." default="..."
///   arg environment "dev" "staging" "prod"
///   arg count type="int" env="COUNT"
///   arg name required=#false
///   arg files rest=#true
pub(super) fn parse_arg(node: &KdlNode) -> Result<ArgDef> {
    let positional = parse_string_list(node);

    let name = positional
        .first()
        .ok_or_else(|| {
            Error::at_with_help(
                "arg node requires a name as the first argument",
                node.span(),
                "write it as `arg name`",
            )
        })?
        .clone();

    // Remaining positional strings are choices.
    let choices: Vec<String> = positional[1..].to_vec();

    let help = named_string(node, "help");
    let default = named_string(node, "default");
    let value_type = parse_value_type(node, &name)?;
    let rest = named_bool(node, "rest").unwrap_or(false);
    // Plain args default to required (a default makes them optional);
    // rest args default to optional.
    let required = named_bool(node, "required").unwrap_or(!rest && default.is_none());
    let env = named_string(node, "env");

    Ok(ArgDef {
        name,
        help,
        default,
        choices,
        value_type,
        rest,
        required,
        env,
    })
}

/// Parse a `flag` node into a `FlagDef`.
///
/// Supported forms:
///   flag verbose                                    — boolean
///   flag preview "-p" help="Show what would happen" — boolean with short + help
///   flag count "-c" type="int" default="3"          — valued flag
///   flag format "-o" "json" "yaml" default="json"   — valued flag with choices
///   flag port type="int" env="PORT"                 — env fallback
pub(super) fn parse_flag(node: &KdlNode) -> Result<FlagDef> {
    let positional = parse_string_list(node);

    let name = positional
        .first()
        .ok_or_else(|| {
            Error::at_with_help(
                "flag node requires a name as the first argument",
                node.span(),
                "write it as `flag name`",
            )
        })?
        .clone();

    // A second positional like "-d" is the short alias; remaining positional
    // strings are choices (which imply a valued flag).
    let mut rest = &positional[1..];
    let mut short = None;
    if let Some(candidate) = rest.first()
        && let Some(stripped) = candidate.strip_prefix('-')
    {
        let mut chars = stripped.chars();
        match (chars.next(), chars.next()) {
            (Some(ch), None) => {
                short = Some(ch);
                rest = &rest[1..];
            }
            _ => {
                return Err(Error::at(
                    format!(
                        "invalid short alias '{candidate}' on flag '{name}' \
                         (expected a single character like \"-x\")"
                    ),
                    node.span(),
                ));
            }
        }
    }
    let choices: Vec<String> = rest.to_vec();

    let help = named_string(node, "help");
    let mut value_type = parse_value_type(node, &name)?;
    // Choices imply a valued string flag.
    if value_type.is_none() && !choices.is_empty() {
        value_type = Some(FlagType::String);
    }

    // Default can be a string property or an integer property in KDL.
    let default = named_string(node, "default").or_else(|| {
        node.entries()
            .iter()
            .find(|e| e.name().map(|n| n.value()) == Some("default"))
            .map(|e| e.value().to_string())
    });
    let env = named_string(node, "env");

    Ok(FlagDef {
        name,
        short,
        help,
        value_type,
        default,
        choices,
        env,
    })
}

/// Parse a `prompt` node into a `PromptDef`.
///
/// Supported form: `prompt name "What is your name?" default="world"`
pub(super) fn parse_prompt(node: &KdlNode) -> Result<PromptDef> {
    let positional = parse_string_list(node);

    let name = positional
        .first()
        .ok_or_else(|| {
            Error::at_with_help(
                "prompt node requires a name as the first argument",
                node.span(),
                "write it as `prompt name \"Question?\"`",
            )
        })?
        .clone();

    let message = positional
        .get(1)
        .cloned()
        .unwrap_or_else(|| format!("{name}: "));
    let default = named_string(node, "default");

    Ok(PromptDef {
        name,
        message,
        default,
    })
}

/// Parse a `choose` node into a `ChooseDef`.
///
/// Supported form: `choose environment "dev" "staging" "prod" default="dev"`
pub(super) fn parse_choose(node: &KdlNode) -> Result<ChooseDef> {
    let positional = parse_string_list(node);

    let name = positional
        .first()
        .ok_or_else(|| {
            Error::at_with_help(
                "choose node requires a name as the first argument",
                node.span(),
                "write it as `choose name \"one\" \"two\"`",
            )
        })?
        .clone();

    let choices = positional[1..].to_vec();
    let default = named_string(node, "default");

    if let Some(default) = &default
        && !choices.contains(default)
    {
        return Err(Error::at_with_help(
            format!("choose '{name}': default '{default}' is not one of the choices"),
            node.span(),
            format!("the choices are: {}", choices.join(", ")),
        ));
    }

    Ok(ChooseDef {
        name,
        choices,
        default,
    })
}
