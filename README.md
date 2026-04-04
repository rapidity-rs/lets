# lets

A declarative CLI builder. Define commands in KDL, get a production-quality CLI instantly.

```kdl
description "My project tasks"

dev "cargo watch -x run"
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
  dev
  test
  lint
  db     Database commands

$ lets db migrate
Running migration...
```

No code. No build step. Just a config file.

## Why lets?

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

**lets** gives you the UX of a hand-written [clap](https://github.com/clap-rs/clap) CLI with the simplicity of a config file:

- Colored `--help` at every level, automatically generated
- Typo suggestions (`lets tset` -> "did you mean `test`?")
- Tab completion for bash, zsh, fish, elvish, PowerShell
- Typed arguments with validation and choices
- Flags with short aliases

## Install

### From source

```sh
cargo install lets-cli
```

### From GitHub releases

Download a prebuilt binary from the [releases page](https://github.com/rapidity-rs/lets/releases).

## Quick start

```sh
# Generate a starter lets.kdl for your project
lets self init

# See what's available
lets --list

# Run a command
lets build
```

## Features

### One-liner commands

The simplest form. A name and a shell command:

```kdl
build "cargo build"
test "cargo test"
dev "cargo watch -x run"
```

### Subcommands

Nest commands with KDL blocks:

```kdl
db {
    description "Database commands"
    migrate "diesel migration run"
    reset "diesel database reset"
    seed "cargo run --bin seed"
}
```

```
$ lets db migrate
```

### Arguments

Positional arguments with optional choices and defaults:

```kdl
deploy {
    arg environment "dev" "staging" "prod"
    run "scripts/deploy.sh {environment}"
}

greet {
    arg name default="world"
    run "echo hello {name}"
}
```

```
$ lets deploy staging
$ lets deploy banana  # error: invalid value 'banana' for '<environment>'
$ lets greet          # hello world
$ lets greet taylor   # hello taylor
```

### Flags

Boolean flags and typed value flags:

```kdl
build {
    flag release "-r" help="Build in release mode"
    flag jobs "-j" type="int" default="4"
    run "cargo build {?release:--release} -j {jobs}"
}
```

```
$ lets build --release -j 8
```

Interpolation syntax:
- `{name}` — positional arg or valued flag
- `{?flag:text}` — emit `text` only when boolean flag is set
- `{--}` — passthrough: everything after `--`
- `{$VAR}` — environment variable

### Dependencies & steps

Run tasks before your command, in parallel or sequentially:

```kdl
lint "cargo clippy"
test "cargo test"
build "cargo build --release"

ci {
    description "Run full CI pipeline"
    steps "lint" "test" "build"
}

release {
    deps "lint" "test"
    run "gh release create"
}
```

`deps` run in parallel. `steps` run sequentially. Both complete before the main `run`.

### Hooks

Run shell commands before and after the main command:

```kdl
deploy {
    before "echo Starting deploy..."
    after "echo Deploy complete!"
    run "scripts/deploy.sh"
}
```

### Environment variables

Set env vars, load `.env` files:

```kdl
serve {
    env PORT="3000" RUST_LOG="debug"
    env-file ".env.local"
    run "cargo run --bin server"
}
```

### Working directory & shell

```kdl
frontend {
    dir "packages/web"
    shell "bash"
    run "npm run dev"
}
```

### Platform-specific commands

```kdl
install {
    run-macos "brew install libpq"
    run-linux "sudo apt-get install -y libpq-dev"
}
```

### Interactive prompts

Built-in confirmation, text input, and selection menus:

```kdl
deploy {
    choose environment "dev" "staging" "prod"
    confirm "Deploy to {environment}?"
    run "scripts/deploy.sh {environment}"
}
```

Use `--yes` / `-y` to bypass all prompts (CI-friendly).

### Aliases

```kdl
test {
    alias "t"
    run "cargo test"
}
```

```
$ lets t  # same as lets test
```

### Timeout & retry

```kdl
health-check {
    timeout "30s"
    retry count=5 delay="2s"
    run "curl -f http://localhost:3000/health"
}
```

### Silent mode

Suppress output unless the command fails:

```kdl
lint {
    silent
    run "cargo clippy -- -D warnings"
}
```

### Include files

Split your config across files:

```kdl
include "tasks/db.kdl"
include "tasks/deploy.kdl"

build "cargo build"
```

### Shell completions

```sh
# Generate completions for your shell
lets self completions bash >> ~/.bashrc
lets self completions zsh >> ~/.zshrc
lets self completions fish > ~/.config/fish/completions/lets.fish
```

Completions are context-aware: they complete command names, argument choices, and flag names from your `lets.kdl`.

### Dry-run mode

See what would execute without running it:

```
$ lets --dry-run deploy staging
[dry-run] echo Starting deploy...
[dry-run] scripts/deploy.sh staging
[dry-run] echo Deploy complete!
```

### Hidden commands

Keep helper tasks out of help output:

```kdl
setup-db {
    hide
    run "diesel database setup"
}

ci {
    deps "setup-db"
    run "cargo test"
}
```

### Deprecated commands

Mark commands for removal with an optional migration message:

```kdl
old-deploy {
    deprecated "Use 'deploy' instead"
    run "scripts/old-deploy.sh"
}
```

### Multiple run commands

Execute multiple commands sequentially, stopping on first failure:

```kdl
deploy {
    run "cargo build --release"
    run "scp target/release/app server:/opt/"
    run "ssh server systemctl restart app"
}
```

### One-liner descriptions

Add help text to one-liners without block syntax:

```kdl
build "cargo build" description="Build the project"
```

## Built-in commands

| Command | Description |
|---|---|
| `lets --help` | Show help with all commands |
| `lets --list` | Show all commands in a tree |
| `lets --dry-run <cmd>` | Show what would run |
| `lets self init` | Generate a starter lets.kdl |
| `lets self check` | Validate your lets.kdl |
| `lets self completions <shell>` | Generate shell completions |

## KDL syntax primer

[KDL](https://kdl.dev) is a document language with a clean, readable syntax. Here's what you need to know:

```kdl
// This is a comment

// Node with a string argument
description "My project"

// Node with named properties
flag verbose "-v" help="Enable verbose output"

// Node with a block of children
db {
    migrate "diesel migration run"
    reset "diesel database reset"
}
```

See the full [lets.kdl spec reference](SPEC.md) for all supported nodes.

## License

MIT
