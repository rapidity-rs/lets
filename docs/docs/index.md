# lets

**A declarative CLI builder.** Define commands in KDL, get a production-quality CLI instantly.

```kdl
description "My project tasks"

build "cargo build"
test "cargo test"
lint "cargo clippy -- -D warnings"

db {
    description "Database commands"
    migrate "diesel migration run"
    reset "diesel database reset"
}
```

```
$ lets --help
My project tasks

Commands:
  build
  test
  lint
  db     Database commands

$ lets db migrate
Running migration...
```

No code. No build step. Just a config file.

## Why lets?

Most task runners make you choose between simplicity and power. Simple tools like `make` give you targets but no help text, no typed arguments, no completions. Powerful tools require learning a scripting language or a complex YAML schema.

**lets** gives you the UX of a hand-written [clap](https://github.com/clap-rs/clap) CLI — colored help, typo suggestions, tab completion, typed arguments — from a config file you can write in under a minute.

| | lets | Make | just | task |
|---|---|---|---|---|
| Config format | KDL | Makefile | Justfile | YAML |
| Typed args & flags | Yes | No | Limited | Limited |
| Shell completions | Auto-generated | No | Manual | Manual |
| Nested subcommands | Yes | No | No | Yes |
| Parallel deps | Yes | Yes | No | Yes |
| Interactive prompts | Built-in | No | No | No |
| Help text | Auto-generated | No | Yes | Yes |
| Arg validation | Choices, types | No | No | No |

## Install

```sh
cargo install lets-cli
```

## Quick start

```sh
# Generate a starter lets.kdl for your project
lets self init

# Set up shell completions (add to your .zshrc/.bashrc)
eval "$(lets self setup)"

# See what's available
lets --list

# Run a command
lets build
```

Ready to dive in? Start with the [Getting Started](getting-started.md) guide.
