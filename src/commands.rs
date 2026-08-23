//! Built-in commands and the list/tree formatter.

use std::path::PathBuf;

use crate::error;
use crate::style::{self, DIM, NAME};
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
    let count = commands.len();

    for (i, cmd) in commands.iter().enumerate() {
        let is_last = i == count - 1;
        print_tree_node(cmd, "", is_last, tree.config.sorted, show_hidden);
    }
}

fn print_tree_node(
    node: &tree::CommandNode,
    prefix: &str,
    is_last: bool,
    sorted: bool,
    show_hidden: bool,
) {
    let connector = if is_last { "└── " } else { "├── " };
    let mut suffix = node
        .description
        .as_deref()
        .map(|d| format!(" {}", style::out(DIM, d)))
        .unwrap_or_default();
    if node.hide {
        suffix.push_str(&format!(" {}", style::out(DIM, "(hidden)")));
    }
    println!(
        "{}{}{suffix}",
        style::out(DIM, format!("{prefix}{connector}")),
        style::out(NAME, &node.name),
    );

    let children: Vec<_> = node
        .children
        .iter()
        .filter(|c| show_hidden || !c.hide)
        .collect();
    let children = sorted_if(&children, sorted);
    let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
    let child_count = children.len();

    for (i, child) in children.iter().enumerate() {
        let child_is_last = i == child_count - 1;
        print_tree_node(child, &child_prefix, child_is_last, sorted, show_hidden);
    }
}

fn sorted_if<'a>(commands: &[&'a tree::CommandNode], sorted: bool) -> Vec<&'a tree::CommandNode> {
    let mut items: Vec<_> = commands.to_vec();
    if sorted {
        items.sort_by(|a, b| a.name.cmp(&b.name));
    }
    items
}

/// Handle `lets self setup [shell]` — print the shell init line.
pub(crate) fn handle_self_setup() -> error::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let positional: Vec<&str> = args[1..]
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(|s| s.as_str())
        .collect();

    let shell = positional.get(2).copied().unwrap_or_else(|| {
        // Auto-detect from $SHELL env var.
        // We can't return a reference to a local, so just match common shells.
        let shell_env = std::env::var("SHELL").unwrap_or_default();
        if shell_env.ends_with("/fish") {
            "fish"
        } else if shell_env.ends_with("/bash") {
            "bash"
        } else {
            "zsh"
        }
    });

    match shell {
        "zsh" => println!(r#"eval "$(LETS_COMPLETE=zsh command lets)""#),
        "bash" => println!(r#"eval "$(LETS_COMPLETE=bash command lets)""#),
        "fish" => println!(r#"LETS_COMPLETE=fish command lets | source"#),
        other => {
            return Err(error::Error::Other(format!(
                "unsupported shell '{other}' (supported: zsh, bash, fish)"
            )));
        }
    }

    Ok(())
}

/// Create a new `lets.kdl` with project-appropriate starter tasks.
pub(crate) fn cmd_init() -> error::Result<()> {
    let path = PathBuf::from("lets.kdl");
    if path.exists() {
        return Err(error::Error::Other(
            "lets.kdl already exists in this directory".to_string(),
        ));
    }

    let mut tasks = Vec::new();

    // Detect project type and suggest tasks.
    if PathBuf::from("Cargo.toml").exists() {
        tasks.push(r#"build "cargo build""#);
        tasks.push(r#"test "cargo test""#);
        tasks.push(r#"run "cargo run""#);
        tasks.push(r#"lint "cargo clippy -- -D warnings""#);
    } else if PathBuf::from("package.json").exists() {
        tasks.push(r#"install "npm install""#);
        tasks.push(r#"dev "npm run dev""#);
        tasks.push(r#"build "npm run build""#);
        tasks.push(r#"test "npm test""#);
        tasks.push(r#"lint "npm run lint""#);
    } else if PathBuf::from("pyproject.toml").exists() || PathBuf::from("setup.py").exists() {
        tasks.push(r#"install "pip install -e .""#);
        tasks.push(r#"test "pytest""#);
        tasks.push(r#"lint "ruff check .""#);
    } else if PathBuf::from("go.mod").exists() {
        tasks.push(r#"build "go build ./...""#);
        tasks.push(r#"test "go test ./...""#);
        tasks.push(r#"lint "golangci-lint run""#);
    } else if PathBuf::from("Makefile").exists() {
        tasks.push(r#"build "make build""#);
        tasks.push(r#"test "make test""#);
    } else {
        tasks.push(r#"hello "echo hello from lets!""#);
    }

    let mut content = String::from("description \"My project tasks\"\n\n");
    for task in &tasks {
        content.push_str(task);
        content.push('\n');
    }

    std::fs::write(&path, &content)
        .map_err(|e| error::Error::Other(format!("failed to write lets.kdl: {e}")))?;

    println!("Created lets.kdl with {} task(s)", tasks.len());
    Ok(())
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
