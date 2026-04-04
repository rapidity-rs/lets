# Shell Integration

lets generates dynamic shell completions that update automatically when you change your `lets.kdl`.

## Setup

Add one line to your shell config:

=== "zsh"

    Add to `~/.zshrc`:

    ```sh
    eval "$(LETS_COMPLETE=zsh command lets)"
    ```

=== "bash"

    Add to `~/.bashrc`:

    ```sh
    eval "$(LETS_COMPLETE=bash command lets)"
    ```

=== "fish"

    Add to `~/.config/fish/config.fish`:

    ```sh
    LETS_COMPLETE=fish command lets | source
    ```

Or run `lets self setup` to print the correct line for your shell:

```sh
$ lets self setup
eval "$(LETS_COMPLETE=zsh command lets)"
```

Restart your shell (or run `exec $SHELL`), then try pressing ++tab++ after `lets `.

## How it works

Unlike static completions that are generated once and baked in, lets uses **dynamic completions**. Here's what happens:

1. **Shell startup** — `eval "$(LETS_COMPLETE=zsh command lets)"` runs `lets` with the `LETS_COMPLETE` env var set. lets detects this, outputs a shell completion function, and exits.

2. **Tab press** — When you press tab, the shell calls the completion function. This re-invokes `lets` with completion context, which reads your `lets.kdl` and returns matching commands, arguments, and flags.

3. **Always fresh** — Because completions are resolved at tab time, changes to your `lets.kdl` are reflected immediately. No regeneration step needed.

!!! note "The `command` keyword"
    The `command` keyword in `command lets` ensures the shell runs the actual `lets` binary, bypassing any shell aliases. This prevents issues if you have `lets` aliased to something else (e.g., `cargo run --manifest-path ...`).

## fzf integration

If you use [fzf-tab](https://github.com/Aloxaf/fzf-tab) with zsh, lets commands automatically appear in the fzf fuzzy finder with descriptions. No extra configuration needed — fzf-tab hooks into zsh's native completion system.

## Static completions (alternative)

If you prefer static completions (or need them for a shell not supported by dynamic completions), use:

```sh
lets self completions bash >> ~/.bashrc
lets self completions zsh >> ~/.zshrc
lets self completions fish > ~/.config/fish/completions/lets.fish
```

!!! warning "Static completions require regeneration"
    Static completions are a snapshot. When you change your `lets.kdl`, you need to regenerate them. Dynamic completions are recommended for most users.

## Built-in commands

Internal lets commands live under the `self` subcommand:

| Command | Description |
|---|---|
| `lets self init` | Generate a starter `lets.kdl` |
| `lets self setup [shell]` | Print shell completion setup line |
| `lets self check` | Validate your `lets.kdl` |
| `lets self completions <shell>` | Generate static shell completions |

Top-level flags:

| Flag | Description |
|---|---|
| `--help` / `-h` | Show help |
| `--version` | Show version |
| `--list` | Show all commands as a tree |
| `--file` / `-f` | Path to an alternate `lets.kdl` |
| `--yes` / `-y` | Skip all confirmation prompts |
| `--dry-run` | Show what would execute |
