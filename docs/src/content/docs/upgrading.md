---
title: Upgrading
description: Breaking changes between lets releases and how to migrate.
---

Pin the version a config requires with `config { min-version "X.Y.Z" }` —
older binaries then fail with an upgrade hint instead of a confusing error.

## 0.4 → 0.5

0.5 is a UX release: better diagnostics, one colour decision, a rebuilt
picker. One config change is breaking; the rest matter only if you parse
lets' output.

### `flag color` is reserved

`--color auto|always|never` is now a built-in global flag, so a command
declaring its own `color` flag would silently shadow it. Like the other
built-ins, the name is now rejected at load:

```kdl
// before
build {
    flag color                       // quietly won over --color
    run "cargo build {?color:--color=always}"
}

// after — pick another name
build {
    flag colour
    run "cargo build {?colour:--color=always}"
}
```

`lets self check` reports the line if you have one.

### `--dry-run` output is a plan, not a list

Dry runs used to print one `[dry-run] <command>` line per command. They now
group commands under the task they belong to and name the phase each runs
in, and walk `deps` in declaration order rather than in parallel:

```
lint
  run          cargo clippy

deploy
  before       echo Starting deploy...
  run          scripts/deploy.sh
```

Anything grepping for the `[dry-run]` prefix needs updating. `--list --json`
is still the stable interface for tooling.

### `--summary` footer wording

The table's last line is now `3 tasks in 1.5s elapsed` rather than
`total: 1.5s`, making clear that it is wall clock and not the sum of the
rows above it.

### Escape leaves the picker

Backing out of the bare-`lets` picker returns you to the prompt. It used to
print full help, which is still what you get when there is nothing to pick
or when output is redirected.

## 0.3 → 0.4

0.4 hardens the config language. Everything below fails loudly at load time,
so `lets self check` finds every affected line at once.

### Literal braces must be escaped

Interpolation now rejects unknown `{placeholders}` instead of silently
deleting them. Shell syntax that uses braces needs `{{`/`}}`:

```kdl
// before (0.3 silently corrupted these)
first "awk '{print $1}' data.txt"
home "echo ${HOME}"

// after
first "awk '{{print $1}}' data.txt"
home "echo ${{HOME}}"          // or: echo {$HOME}
```

Go-template formats double up: `'{{{{.State.Running}}}}'` renders
`'{{.State.Running}}'`.

### Commands run from the config directory

Execution now roots at the directory containing `lets.kdl`, no matter where
you invoke from — matching `sources`, fingerprints, and every other config
path. `dir` and `env-file` also resolve against it. A task that needs the
caller's directory can `cd "$LETS_INVOCATION_DIR"`.

### Hooks and gates interpolate

`before`, `after`, `defer`, `precondition`, `status`, and `env` values now
interpolate exactly like `run`. If any of them contained literal braces,
escape them as above.

### Repeated nodes and typos are errors

- Single-value nodes (`description`, `dir`, `shell`, `before`, `after`,
  `confirm`, …) error when repeated; list-like nodes (`run`, `deps`,
  `steps`, `env`, `vars`, `alias`, `sources`, …) extend instead of
  last-one-wins.
- A child node whose name is close to a keyword (`descriptoin`,
  `envfile`) is an error naming the likely intent; write `cmd <name>` for
  a deliberate subcommand with such a name.
- `self` is reserved at the top level; sibling commands can't share names
  or aliases.

### Flags can't shadow the built-in globals

User flags named `file`, `yes`, `dry-run`, `output`, `watch`, `force`,
`jobs`, or `help` — or using the shorts `-f`, `-y`, `-j`, `-h` — are load
errors (they crashed or silently shadowed the built-ins before). Rename the
flag; `{?name:…}` conditionals and `$LETS_FLAG_*` exports follow the new
name automatically.

### `{--}` passthrough is quoted

Tokens after `--` are shell-quoted individually, so `lets test -- "a b"`
reaches the child as one argument. If a task relied on re-splitting, pass
separate tokens instead.
