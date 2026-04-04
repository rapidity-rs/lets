//! Parsers for command child nodes: args, flags, prompts, and choices.

use crate::error::{Error, Result};
use crate::tree::{ArgDef, ChooseDef, FlagDef, FlagType, PromptDef};

use super::helpers::{named_string, parse_string_list};

use kdl::KdlNode;

/// Parse an `arg` node into an `ArgDef`.
///
/// Supported forms:
///   arg name help="..." default="..."
///   arg environment "dev" "staging" "prod"
pub(super) fn parse_arg(node: &KdlNode) -> Result<ArgDef> {
    let positional = parse_string_list(node);

    let name = positional
        .first()
        .ok_or_else(|| Error::ParseNoSpan {
            message: "arg node requires a name as the first argument".to_string(),
        })?
        .clone();

    // Remaining positional strings are choices.
    let choices: Vec<String> = positional[1..].to_vec();

    let help = named_string(node, "help");
    let default = named_string(node, "default");

    Ok(ArgDef {
        name,
        help,
        default,
        choices,
    })
}

/// Parse a `flag` node into a `FlagDef`.
///
/// Supported forms:
///   flag verbose                                    — boolean
///   flag dry-run "-d" help="Show what would happen" — boolean with short + help
///   flag count "-c" type="int" default="3"          — valued flag
pub(super) fn parse_flag(node: &KdlNode) -> Result<FlagDef> {
    let positional = parse_string_list(node);

    let name = positional
        .first()
        .ok_or_else(|| Error::ParseNoSpan {
            message: "flag node requires a name as the first argument".to_string(),
        })?
        .clone();

    // Second positional string like "-d" is the short alias.
    let short = positional.get(1).and_then(|s| {
        let s = s.strip_prefix('-').unwrap_or(s);
        let mut chars = s.chars();
        let ch = chars.next()?;
        if chars.next().is_none() {
            Some(ch)
        } else {
            None
        }
    });

    let help = named_string(node, "help");

    let value_type = named_string(node, "type").map(|t| match t.as_str() {
        "int" => FlagType::Int,
        "float" => FlagType::Float,
        _ => FlagType::String,
    });

    // Default can be a string property or an integer property in KDL.
    let default = named_string(node, "default").or_else(|| {
        node.entries()
            .iter()
            .find(|e| e.name().map(|n| n.value()) == Some("default"))
            .map(|e| e.value().to_string())
    });

    Ok(FlagDef {
        name,
        short,
        help,
        value_type,
        default,
    })
}

/// Parse a `prompt` node into a `PromptDef`.
///
/// Supported form: `prompt name "What is your name?" default="world"`
pub(super) fn parse_prompt(node: &KdlNode) -> Result<PromptDef> {
    let positional = parse_string_list(node);

    let name = positional
        .first()
        .ok_or_else(|| Error::ParseNoSpan {
            message: "prompt node requires a name as the first argument".to_string(),
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
/// Supported form: `choose environment "dev" "staging" "prod"`
pub(super) fn parse_choose(node: &KdlNode) -> Result<ChooseDef> {
    let positional = parse_string_list(node);

    let name = positional
        .first()
        .ok_or_else(|| Error::ParseNoSpan {
            message: "choose node requires a name as the first argument".to_string(),
        })?
        .clone();

    let choices = positional[1..].to_vec();

    Ok(ChooseDef { name, choices })
}
