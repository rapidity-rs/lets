//! The `--list` command tree and its machine-readable form.
//!
//! Two renderings of one tree: an aligned, styled listing for people, and
//! compact JSON for tooling. Both are driven from the same `CommandTree`,
//! so they can never disagree about what a config contains.

use std::io::IsTerminal;

use crate::style::{self, DIM, GROUP, NAME, truncate};
use crate::tree;

/// Count total commands (including nested children) in a tree.
pub(crate) fn count_commands(tree: &tree::CommandTree) -> usize {
    fn count(commands: &[tree::CommandNode]) -> usize {
        commands.iter().map(|c| 1 + count(&c.children)).sum()
    }
    count(&tree.commands)
}

/// Print the command list as a tree with descriptions. With `show_hidden`,
/// hidden commands appear too, marked — so `self check` shows exactly what
/// was parsed, including nodes swallowed as unintended subcommands.
pub(crate) fn print_command_list(tree: &tree::CommandTree, show_hidden: bool) {
    if let Some(desc) = &tree.description {
        println!("{desc}");
        println!();
    }

    let visible: Vec<_> = tree
        .commands
        .iter()
        .filter(|c| show_hidden || !c.hide)
        .collect();
    let commands = sorted_if(&visible, tree.config.sorted);

    let mut rows = Vec::new();
    let count = commands.len();
    for (i, cmd) in commands.iter().enumerate() {
        collect_rows(
            cmd,
            "",
            i == count - 1,
            tree.config.sorted,
            show_hidden,
            &mut rows,
        );
    }
    print_rows(&rows);
}

/// One command's line, with the styled text and its visible width kept
/// apart: styling adds bytes that must not count toward alignment.
struct Row {
    /// Connectors, name, and signature, styled and ready to print.
    left: String,
    /// Printable width of `left`, for padding the description column.
    width: usize,
    /// Description plus any alias note, styled.
    right: Option<String>,
    /// Printable width of `right`, for deciding what fits.
    right_width: usize,
}

fn collect_rows(
    node: &tree::CommandNode,
    prefix: &str,
    is_last: bool,
    sorted: bool,
    show_hidden: bool,
    rows: &mut Vec<Row>,
) {
    let connector = if is_last { "└── " } else { "├── " };
    let signature = signature(node);

    // A group that only holds subcommands isn't something you can run;
    // reserve the command colour for things that are.
    let name_style = if node.is_runnable() { NAME } else { GROUP };

    let mut left = format!(
        "{}{}",
        style::out(DIM, format!("{prefix}{connector}")),
        style::out(name_style, &node.name)
    );
    let mut width = prefix.chars().count() + connector.chars().count() + node.name.chars().count();
    if !signature.is_empty() {
        left.push(' ');
        left.push_str(&style::out(DIM, &signature));
        width += 1 + signature.chars().count();
    }

    let mut right = String::new();
    let mut right_width = 0;
    if let Some(desc) = &node.description {
        right.push_str(desc);
        right_width += desc.chars().count();
    }
    for note in notes(node, show_hidden) {
        if !right.is_empty() {
            right.push(' ');
            right_width += 1;
        }
        right.push_str(&note);
        right_width += note.chars().count();
    }

    rows.push(Row {
        left,
        width,
        right: (!right.is_empty()).then(|| style::out(DIM, &right)),
        right_width,
    });

    let children: Vec<_> = node
        .children
        .iter()
        .filter(|c| show_hidden || !c.hide)
        .collect();
    let children = sorted_if(&children, sorted);
    let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
    let child_count = children.len();

    for (i, child) in children.iter().enumerate() {
        collect_rows(
            child,
            &child_prefix,
            i == child_count - 1,
            sorted,
            show_hidden,
            rows,
        );
    }
}

/// Parenthesised notes that follow the description.
fn notes(node: &tree::CommandNode, show_hidden: bool) -> Vec<String> {
    let mut notes = Vec::new();
    if !node.aliases.is_empty() {
        let label = if node.aliases.len() == 1 {
            "alias"
        } else {
            "aliases"
        };
        notes.push(format!("({label}: {})", node.aliases.join(", ")));
    }
    if let Some(msg) = &node.deprecated {
        notes.push(if msg.is_empty() {
            "(deprecated)".to_string()
        } else {
            format!("(deprecated: {msg})")
        });
    }
    if show_hidden && node.hide {
        notes.push("(hidden)".to_string());
    }
    notes
}

/// What the command takes on the command line. Args only: they are what
/// you must supply to run it, where flags are optional by nature and
/// `lets <cmd> --help` is one keystroke away.
pub(crate) fn signature(node: &tree::CommandNode) -> String {
    node.args
        .iter()
        .map(|arg| {
            // A choice arg's whole point is its choice set, so show it —
            // unless the list is long enough to crowd out the description.
            let name = if !arg.choices.is_empty() && arg.choices.len() <= 4 {
                arg.choices.join("|")
            } else {
                arg.name.clone()
            };
            if arg.rest {
                format!("[{name}...]")
            } else if arg.required && arg.default.is_none() {
                format!("<{name}>")
            } else {
                format!("[{name}]")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Descriptions line up in a column. On a terminal the column is capped so
/// a deep tree can't starve descriptions of room, and anything that still
/// overflows is truncated; redirected output is never truncated, so piping
/// the listing keeps every character.
fn layout(rows: &[Row], terminal: Option<usize>) -> Vec<String> {
    let longest = rows.iter().map(|r| r.width).max().unwrap_or(0);
    let column = match terminal {
        Some(width) => longest.min(width * 2 / 5),
        None => longest,
    };

    rows.iter()
        .map(|row| {
            let Some(right) = &row.right else {
                return row.left.clone();
            };
            // A name past the column still gets one clear space after it.
            let gap = column.saturating_sub(row.width) + 2;
            let right = match terminal {
                Some(terminal) if row.width + gap + row.right_width > terminal => {
                    truncate(right, terminal.saturating_sub(row.width + gap))
                }
                _ => right.clone(),
            };
            format!("{}{}{}", row.left, " ".repeat(gap), right)
        })
        .collect()
}

fn print_rows(rows: &[Row]) {
    for line in layout(rows, terminal_width()) {
        println!("{line}");
    }
}

/// Terminal width, or None when output is redirected and nothing should be
/// trimmed to fit.
fn terminal_width() -> Option<usize> {
    if !std::io::stdout().is_terminal() {
        return None;
    }
    terminal_size::terminal_size().map(|(w, _)| w.0 as usize)
}

fn sorted_if<'a>(commands: &[&'a tree::CommandNode], sorted: bool) -> Vec<&'a tree::CommandNode> {
    let mut items: Vec<_> = commands.to_vec();
    if sorted {
        items.sort_by(|a, b| a.name.cmp(&b.name));
    }
    items
}

/// Print the command tree as compact JSON for tooling (`lets --list --json`).
/// Hidden commands are included; consumers filter on `"hidden"`.
pub(crate) fn print_command_list_json(tree: &tree::CommandTree) {
    println!(
        "{{\"description\":{},\"commands\":{}}}",
        json_opt(tree.description.as_deref()),
        json_commands(&tree.commands)
    );
}

fn json_commands(commands: &[tree::CommandNode]) -> String {
    let items: Vec<String> = commands.iter().map(json_command).collect();
    format!("[{}]", items.join(","))
}

fn json_command(node: &tree::CommandNode) -> String {
    let args: Vec<String> = node
        .args
        .iter()
        .map(|a| {
            format!(
                "{{\"name\":{},\"help\":{},\"required\":{},\"rest\":{},\"type\":{},\
                 \"default\":{},\"env\":{},\"choices\":{}}}",
                json_str(&a.name),
                json_opt(a.help.as_deref()),
                a.required,
                a.rest,
                json_str(value_type_name(a.value_type.as_ref())),
                json_opt(a.default.as_deref()),
                json_opt(a.env.as_deref()),
                json_str_array(&a.choices),
            )
        })
        .collect();
    let flags: Vec<String> = node
        .flags
        .iter()
        .map(|f| {
            let type_name = match f.value_type.as_ref() {
                None => "bool",
                some => value_type_name(some),
            };
            format!(
                "{{\"name\":{},\"short\":{},\"help\":{},\"type\":{},\"default\":{},\
                 \"env\":{},\"choices\":{}}}",
                json_str(&f.name),
                f.short
                    .map_or_else(|| "null".to_string(), |c| json_str(&c.to_string())),
                json_opt(f.help.as_deref()),
                json_str(type_name),
                json_opt(f.default.as_deref()),
                json_opt(f.env.as_deref()),
                json_str_array(&f.choices),
            )
        })
        .collect();

    format!(
        "{{\"name\":{},\"description\":{},\"aliases\":{},\"hidden\":{},\"runnable\":{},\
         \"args\":[{}],\"flags\":[{}],\"commands\":{}}}",
        json_str(&node.name),
        json_opt(node.description.as_deref()),
        json_str_array(&node.aliases),
        node.hide,
        node.is_runnable(),
        args.join(","),
        flags.join(","),
        json_commands(&node.children),
    )
}

fn value_type_name(ty: Option<&tree::FlagType>) -> &'static str {
    match ty {
        Some(tree::FlagType::Int) => "int",
        Some(tree::FlagType::Float) => "float",
        Some(tree::FlagType::String) | None => "string",
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_opt(s: Option<&str>) -> String {
    s.map_or_else(|| "null".to_string(), json_str)
}

fn json_str_array(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| json_str(s)).collect();
    format!("[{}]", quoted.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Column at which `needle` starts, counting characters: `str::find`
    /// reports bytes, and escapes are not columns at all.
    fn column_of(line: &str, needle: &str) -> Option<usize> {
        let line = style::plain(line);
        line.find(needle).map(|byte| line[..byte].chars().count())
    }

    fn row(left: &str, right: Option<&str>) -> Row {
        Row {
            left: left.to_string(),
            width: left.chars().count(),
            right: right.map(str::to_string),
            right_width: right.map_or(0, |r| r.chars().count()),
        }
    }

    #[test]
    fn descriptions_line_up_in_one_column() {
        let rows = vec![
            row("├── ci", Some("Full pipeline")),
            row("├── build-release", Some("Ship it")),
            row("└── x", None),
        ];
        let lines = layout(&rows, None);

        let ci = column_of(&lines[0], "Full").unwrap();
        let build = column_of(&lines[1], "Ship").unwrap();
        assert_eq!(ci, build, "lines: {lines:#?}");
        // Two spaces past the longest name, which is 17 characters.
        assert_eq!(ci, 19, "lines: {lines:#?}");
        // A row with no description gets no trailing padding.
        assert_eq!(lines[2], "└── x");
    }

    #[test]
    fn redirected_output_keeps_every_character() {
        let long = "a description far longer than any terminal would ever be, kept whole";
        let rows = vec![row("├── task", Some(long))];
        assert!(layout(&rows, None)[0].ends_with(long));
    }

    #[test]
    fn a_terminal_caps_the_column_and_trims_the_rest() {
        let rows = vec![
            row(
                "├── a-very-long-command-name-indeed",
                Some("described at length here"),
            ),
            row("├── x", Some("short")),
        ];
        let lines = layout(&rows, Some(60));

        for line in &lines {
            let width = style::width(line);
            assert!(width <= 60, "{width} columns: {line:?}");
        }
        // The column is capped at two fifths of the width, so a long name
        // can't push every description off the screen.
        assert_eq!(column_of(&lines[1], "short"), Some(26), "lines: {lines:#?}");
    }

    fn node(name: &str) -> tree::CommandNode {
        tree::CommandNode {
            name: name.to_string(),
            span: (0, 0).into(),
            description: None,
            long_description: None,
            examples: None,
            hide: false,
            deprecated: None,
            args: Vec::new(),
            flags: Vec::new(),
            aliases: Vec::new(),
            run_policy: tree::RunPolicy::default(),
            vars: Vec::new(),
            sources: Vec::new(),
            generates: Vec::new(),
            preconditions: Vec::new(),
            status: Vec::new(),
            run: tree::RunConfig::default(),
            orch: tree::Orchestration::default(),
            env: tree::EnvConfig::default(),
            exec: tree::ExecConfig::default(),
            interactive: tree::Interactive::default(),
            children: Vec::new(),
        }
    }

    fn arg(name: &str, choices: &[&str], required: bool, rest: bool) -> tree::ArgDef {
        tree::ArgDef {
            name: name.to_string(),
            help: None,
            default: None,
            choices: choices.iter().map(|s| s.to_string()).collect(),
            value_type: None,
            rest,
            required,
            env: None,
        }
    }

    #[test]
    fn signatures_distinguish_required_optional_and_variadic() {
        let mut cmd = node("deploy");
        cmd.args = vec![arg("environment", &[], true, false)];
        assert_eq!(signature(&cmd), "<environment>");

        let mut cmd = node("greet");
        let mut optional = arg("name", &[], false, false);
        optional.default = Some("world".to_string());
        cmd.args = vec![optional];
        assert_eq!(signature(&cmd), "[name]");

        let mut cmd = node("test");
        cmd.args = vec![arg("files", &[], false, true)];
        assert_eq!(signature(&cmd), "[files...]");
    }

    #[test]
    fn short_choice_lists_show_the_choices_themselves() {
        let mut cmd = node("deploy");
        cmd.args = vec![arg("env", &["dev", "staging", "prod"], true, false)];
        assert_eq!(signature(&cmd), "<dev|staging|prod>");

        // Past a handful the list would crowd out the description.
        let mut cmd = node("pick");
        cmd.args = vec![arg("one", &["a", "b", "c", "d", "e"], true, false)];
        assert_eq!(signature(&cmd), "<one>");
    }
}
