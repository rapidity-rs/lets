//! KDL node accessor utilities and small parsers.

use std::str::FromStr;
use std::time::Duration;

use kdl::KdlNode;
use miette::SourceSpan;

use crate::error::{Error, Result};
use crate::tree::{Config, Platform};

/// Extract the first positional string argument from a KDL node.
pub(super) fn first_string_arg(node: &KdlNode) -> Option<String> {
    node.entries()
        .iter()
        .find(|e| e.name().is_none())
        .and_then(|e| e.value().as_string().map(|s| s.to_string()))
}

/// Extract the first boolean argument from a KDL node.
/// Returns None if there are no arguments (caller decides default).
pub(super) fn first_bool_arg(node: &KdlNode) -> Option<bool> {
    node.entries()
        .iter()
        .find(|e| e.name().is_none())
        .and_then(|e| e.value().as_bool())
}

/// Extract a named property from a KDL node.
pub(super) fn named_string(node: &KdlNode, key: &str) -> Option<String> {
    node.entries()
        .iter()
        .find(|e| e.name().map(|n| n.value()) == Some(key))
        .and_then(|e| e.value().as_string().map(|s| s.to_string()))
}

/// Extract a named integer property from a KDL node.
pub(super) fn named_int(node: &KdlNode, key: &str) -> Option<i64> {
    node.entries()
        .iter()
        .find(|e| e.name().map(|n| n.value()) == Some(key))
        .and_then(|e| e.value().as_integer().map(|v| v as i64))
}

/// Parse a duration string like "30s", "5m", "1h", "500ms".
pub(super) fn parse_duration(s: &str) -> std::result::Result<Duration, String> {
    let s = s.trim();
    if let Some(ms) = s.strip_suffix("ms") {
        return ms
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| format!("invalid duration '{s}'"));
    }
    if let Some(secs) = s.strip_suffix('s') {
        return secs
            .parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| format!("invalid duration '{s}'"));
    }
    if let Some(mins) = s.strip_suffix('m') {
        return mins
            .parse::<u64>()
            .map(|m| Duration::from_secs(m * 60))
            .map_err(|_| format!("invalid duration '{s}'"));
    }
    if let Some(hrs) = s.strip_suffix('h') {
        return hrs
            .parse::<u64>()
            .map(|h| Duration::from_secs(h * 3600))
            .map_err(|_| format!("invalid duration '{s}'"));
    }
    // Plain number = seconds.
    s.parse::<u64>()
        .map(Duration::from_secs)
        .map_err(|_| format!("invalid duration '{s}'"))
}

/// Parse the `config` block into a `Config`.
pub(super) fn parse_config(node: &KdlNode) -> Config {
    let mut config = Config::default();
    if let Some(children) = node.children() {
        for child in children.nodes() {
            match child.name().value() {
                "sorted" => {
                    config.sorted = first_bool_arg(child).unwrap_or(true);
                }
                "shell" => {
                    config.shell = first_string_arg(child);
                }
                _ => {}
            }
        }
    }
    config
}

/// Parse environment variables from an `env` node.
/// KDL syntax: `env PORT="3000" RUST_LOG="debug"`
pub(super) fn parse_env(node: &KdlNode) -> Vec<(String, String)> {
    node.entries()
        .iter()
        .filter_map(|e| {
            let key = e.name()?.value().to_string();
            let value = e.value().as_string()?.to_string();
            Some((key, value))
        })
        .collect()
}

/// Parse a list of platform entries, returning an error for unrecognized values.
pub(super) fn parse_platform_list(node: &KdlNode) -> Result<Vec<Platform>> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .filter_map(|e| {
            let s = e.value().as_string()?;
            Some(Platform::from_str(s).map_err(|msg| Error::ParseNoSpan { message: msg }))
        })
        .collect()
}

/// Parse a list of positional string arguments from a node.
pub(super) fn parse_string_list(node: &KdlNode) -> Vec<String> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .filter_map(|e| e.value().as_string().map(|s| s.to_string()))
        .collect()
}

/// Parse task references from a `deps` or `steps` node.
/// Each positional string is split on whitespace to support nested paths like `"db migrate"`.
pub(super) fn parse_task_refs(node: &KdlNode) -> Vec<(Vec<String>, SourceSpan)> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .filter_map(|e| {
            let path: Vec<String> = e
                .value()
                .as_string()?
                .split_whitespace()
                .map(String::from)
                .collect();
            Some((path, e.span()))
        })
        .collect()
}
