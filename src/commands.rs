//! Built-in commands and the list/tree formatter.

use std::path::PathBuf;

use crate::error;
use crate::tree;

/// Count total commands (including nested children) in a tree.
pub(crate) fn count_commands(tree: &tree::CommandTree) -> usize {
    fn count(commands: &[tree::CommandNode]) -> usize {
        commands.iter().map(|c| 1 + count(&c.children)).sum()
    }
    count(&tree.commands)
}

/// Print the command list as a tree with descriptions.
pub(crate) fn print_command_list(tree: &tree::CommandTree) {
    if let Some(desc) = &tree.description {
        println!("{desc}");
        println!();
    }

    let visible: Vec<_> = tree.commands.iter().filter(|c| !c.hide).collect();
    let commands = sorted_if(&visible, tree.config.sorted);
    let count = commands.len();

    for (i, cmd) in commands.iter().enumerate() {
        let is_last = i == count - 1;
        print_tree_node(cmd, "", is_last, tree.config.sorted);
    }
}

fn print_tree_node(node: &tree::CommandNode, prefix: &str, is_last: bool, sorted: bool) {
    let connector = if is_last { "└── " } else { "├── " };
    let desc = node
        .description
        .as_deref()
        .map(|d| format!(" \x1b[2m{d}\x1b[0m"))
        .unwrap_or_default();
    println!(
        "\x1b[2m{prefix}{connector}\x1b[0m\x1b[1;36m{}\x1b[0m{desc}",
        node.name
    );

    let children: Vec<_> = node.children.iter().filter(|c| !c.hide).collect();
    let children = sorted_if(&children, sorted);
    let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
    let child_count = children.len();

    for (i, child) in children.iter().enumerate() {
        let child_is_last = i == child_count - 1;
        print_tree_node(child, &child_prefix, child_is_last, sorted);
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
