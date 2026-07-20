---
title: Getting Started
description: Go from zero to a working lets CLI in under 5 minutes.
---

This guide takes you from zero to a working `lets` CLI in under 5 minutes.

## Install

```sh
cargo install lets-cli
```

This installs the `lets` binary to `~/.cargo/bin/`.

## Create your first lets.kdl

In your project root, run:

```sh
lets self init
```

This detects your project type and generates a starter `lets.kdl`. For a Rust project, you'll get:

```kdl
description "My project tasks"

build "cargo build"
test "cargo test"
run "cargo run"
lint "cargo clippy -- -D warnings"
```

## Try it out

```sh
# See all available commands
lets --help

# See commands as a tree
lets --list

# Run a command
lets build

# Get help for a specific command
lets build --help
```

## Run from anywhere

`lets` finds your config like `git` finds `.git`: it walks up from the
current directory until it hits a `lets.kdl`. Commands always execute from
the directory containing that file, no matter how deep in the project you
are when you invoke them — `cd src/deep/module && lets build` behaves
exactly like running `lets build` at the root.

Two environment variables tell your scripts where things are:

- `LETS_PROJECT_ROOT` — the directory containing `lets.kdl`
- `LETS_INVOCATION_DIR` — the directory you invoked `lets` from

## Set up shell completions

For the best experience, add dynamic completions to your shell. This gives you tab completion for all your commands, arguments, and flags — and it updates automatically when you change your `lets.kdl`.

The easiest way is `lets self setup`, which detects your shell. Add to your shell rc (`~/.zshrc`, `~/.bashrc`, or `~/.config/fish/config.fish`):

```sh
eval "$(lets self setup)"
```

Or wire it up manually for your shell:

### zsh

Add to `~/.zshrc`:

```sh
eval "$(LETS_COMPLETE=zsh command lets)"
```

### bash

Add to `~/.bashrc`:

```sh
eval "$(LETS_COMPLETE=bash command lets)"
```

### fish

Add to `~/.config/fish/config.fish`:

```sh
LETS_COMPLETE=fish command lets | source
```

Restart your shell and try pressing <kbd>Tab</kbd> after `lets ` — you should see your commands.

:::tip[fzf integration]
If you use [fzf-tab](https://github.com/Aloxaf/fzf-tab) with zsh, lets commands will automatically appear in the fzf fuzzy finder with descriptions. No extra configuration needed.
:::

## Add descriptions

One-liner commands are great for getting started, but you'll want help text. You can add it inline:

```kdl
build "cargo build" description="Build the project"
test "cargo test" description="Run the test suite"
```

Or use block syntax for more options:

```kdl
build {
    description "Build the project"
    run "cargo build"
}
```

Both produce the same result in `lets --help`.

## What's next?

- [KDL Primer](/lets/kdl-primer/) — learn the config language
- [Commands](/lets/commands/) — one-liners, blocks, subcommands
- [Arguments & Flags](/lets/arguments-and-flags/) — typed inputs with validation
- [Orchestration](/lets/orchestration/) — deps, steps, hooks
