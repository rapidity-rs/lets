---
title: Upgrading
description: Breaking changes between lets releases and how to migrate.
---

Pin the version a config requires with `config { min-version "X.Y.Z" }` —
older binaries then fail with an upgrade hint instead of a confusing error.

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
