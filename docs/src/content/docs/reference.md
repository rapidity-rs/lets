---
title: Reference
description: Complete reference for all nodes and properties supported in a lets.kdl file.
---

Complete reference for all nodes and properties supported in a `lets.kdl` file.

## Top-level nodes

### `description`

Top-level help text shown in `lets --help`.

```kdl
description "My project tasks"
```

### `config`

Global configuration.

```kdl
config {
    sorted                // Sort commands alphabetically
    shell "zsh"           // Default shell for all commands
    output "interleaved"  // Output mode for deps/steps tasks
}
```

| Node | Values | Description |
|---|---|---|
| `sorted` | — | Sort commands alphabetically in help and lists |
| `shell` | string | Default shell for all commands |
| `output` | `"interleaved"`, `"group"`, `"prefixed"` | Output mode for tasks run via `deps`/`steps` (default: `"interleaved"`) |
| `jobs` | integer ≥ 1 | Maximum number of tasks executing concurrently (default: unlimited; `--jobs` overrides) |
| `env` | properties | Environment variables applied to every task (task `env` overrides) |
| `env-file` | string | Project-wide .env file, relative to the config directory (task values override) |

Unknown config options are load errors.

Output modes:

- `"interleaved"` (default) — child tasks share the terminal directly.
- `"group"` — each task's merged stdout+stderr is buffered and printed as one `[label]` block when it finishes. Failed tasks still flush their output.
- `"prefixed"` — lines stream live, each tagged `[label]` with a per-task color.

The global `--output <mode>` flag overrides the config value. The root command itself is never prefixed or grouped, since it may be interactive.

### `include`

Import commands from another KDL file. Paths are relative to the including file.

```kdl
include "tasks/db.kdl"
```

### `vars`

Global variables usable in `{name}` interpolation everywhere. Static values
resolve at load time and may reference earlier vars or the environment
(`{$VAR}`). Also available inside a command or group, scoped to it and its
children. Declared args/flags and interactive bindings take precedence over
vars.

```kdl
vars {
    registry "ghcr.io/acme"
    image "{registry}/app"
    sha cmd="git rev-parse --short HEAD"
}
```

A var with `cmd="…"` is **dynamic**: the command runs with the config shell
in the project root the first time the var is referenced, and its trimmed
stdout is cached for the rest of the invocation — referenced by ten tasks,
it still runs once; referenced by none, it never runs. A failing command
fails the run with an error naming the var. Dynamic values resolve at run
time, so static vars can't reference them (the `cmd=` string itself may
reference static vars).

## Command nodes

Any top-level node that isn't `description`, `config`, `vars`, or `include` defines a command.

### One-liner syntax

```kdl
command-name "shell command"
command-name "shell command" description="Help text"
```

### Block syntax

```kdl
command-name {
    // child nodes...
}
```

### Strictness

Configs fail loudly at load rather than silently misbehaving:

- **Reserved names.** `self` is reserved at the top level (for `lets self …`).
  Sibling commands can't share a name or alias. Flags can't reuse the
  built-in global flags (`--file`, `--yes`, `--dry-run`, `--output`,
  `--watch`, `--force`, `--jobs`, `--help`) or their shorts
  (`-f`, `-y`, `-j`, `-h`).
- **Repeated nodes.** List-like nodes (`run`, `deps`, `steps`, `env`,
  `vars`, `alias`, `sources`, `generates`, `status`, `precondition`,
  `defer`, `platform`) extend when repeated. Single-value nodes
  (`description`, `dir`, `shell`, `before`, `after`, `confirm`, `timeout`,
  …) error on a repeat instead of silently replacing the first.
- **Typos.** An unknown child node close to a keyword (`descriptoin`,
  `envfile`) is an error naming the likely intent. Other unknown nodes
  become subcommands — `lets self check` prints the parsed tree (hidden
  commands included) so a swallowed typo is visible.

## Command child nodes

### `description`

Short help text shown in command lists.

```kdl
description "Deploy the application"
```

### `long-description`

Extended help text shown in `lets <command> --help`.

```kdl
long-description """
    Extended description with multiple lines.
    Shown when viewing the command's own help.
    """
```

### `examples`

Usage examples shown at the bottom of `--help`.

```kdl
examples """
    lets deploy staging
    lets deploy prod --dry-run
    """
```

### `run`

Shell command to execute. Multiple `run` nodes execute sequentially.

```kdl
run "cargo build --release"
run "echo done"
```

### `run-macos`, `run-linux`, `run-windows`

Platform-specific run commands. Falls back to `run` if no match.

```kdl
run-macos "brew install libpq"
run-linux "apt-get install libpq-dev"
```

### `arg`

Positional argument.

```kdl
arg name
arg name help="Description" default="value"
arg environment "dev" "staging" "prod"
arg count type="int" env="COUNT"
arg label required=#false
arg files rest=#true
```

| Property | Type | Description |
|---|---|---|
| *(first positional)* | string | Argument name (required) |
| *(remaining positional)* | strings | Allowed choices |
| `help` | string | Help text |
| `default` | string | Default value (makes arg optional) |
| `type` | string | Value type: `"string"`, `"int"`, `"float"` (not with choices) |
| `env` | string | Environment variable consulted when the arg isn't supplied |
| `required` | bool | `#false` makes the arg optional without a default |
| `rest` | bool | Variadic: captures all remaining values; must be the last arg |

A rest arg interpolates like `{--}`: each value shell-quoted, empty when
absent. An optional arg without a default can't be interpolated as plain
`{name}` (the value may not exist) — test presence with `{?name:text}` or
read `$LETS_ARG_NAME` with a shell fallback.

### `flag`

Boolean or valued flag.

```kdl
flag verbose
flag verbose "-v" help="Enable verbose output"
flag count "-c" type="int" default="3"
flag format "-o" "json" "yaml" default="json"
flag port type="int" env="PORT" default="3000"
```

| Property | Type | Description |
|---|---|---|
| *(first positional)* | string | Flag name (required) |
| *(second positional)* | string | Short alias (e.g. `"-v"`) |
| *(remaining positional)* | strings | Allowed choices (implies a valued string flag) |
| `help` | string | Help text |
| `type` | string | Value type: `"string"`, `"int"`, `"float"` (not with choices) |
| `default` | string | Default value (valued flags only) |
| `env` | string | Environment variable consulted when the flag isn't passed (valued flags only) |

Value resolution order: command line, then `env=`, then `default=`. A valued
flag with neither `default=` nor `env=` can't be interpolated as plain
`{name}`; use `{?name:text}` or `$LETS_FLAG_NAME`.

### `deps`

Tasks to run in **parallel** before this command.

```kdl
deps "lint" "test"
deps "db migrate"
deps "build release -t 8"
```

References may include arguments and flags. Trailing tokens are parsed by the target command's own CLI definition, with the same validation as the command line — unknown flags, missing required arguments, or invalid choices fail at config load time.

Every task runs **at most once per invocation**: if several commands in the graph depend on the same task, it executes a single time, and concurrent references wait for the in-flight run. A failed shared task fails every dependent. References with different arguments count as different tasks.

### `steps`

Tasks to run **sequentially** before this command.

```kdl
steps "lint" "test" "build"
```

Like `deps`, references may pass arguments and flags (validated at config load time), and each referenced task runs at most once per invocation.

### `before`, `after`

Shell commands to run before/after the main `run`.

```kdl
before "echo Starting..."
after "echo Done!"
```

### `defer`

Cleanup command that runs when the task settles — success, failure, or
interrupt. Repeatable; multiple defers run in reverse declaration order
(LIFO). A failing defer warns but doesn't change the task's result. Defers
run only if the task reached its body (they don't run when a precondition
failed, a dep failed, or `status` skipped the task).

```kdl
defer "docker compose down"
```

### `env`

Set environment variables. Values may reference config vars (`{name}`) and
the environment (`{$VAR}`).

```kdl
env PORT="3000" RUST_LOG="debug"
```

Every task also receives `LETS_PROJECT_ROOT` (the config-file directory),
`LETS_INVOCATION_DIR` (where lets was invoked), and `LETS_ARG_*` /
`LETS_FLAG_*` exports for its declared args and flags.

### `env-file`

Load environment variables from a file, relative to the directory containing
`lets.kdl`. Explicit `env` values override.

```kdl
env-file ".env.local"
```

### `dir`

Set the working directory, relative to the directory containing `lets.kdl`
(absolute paths are used as-is). Without `dir`, commands run from the
config-file directory — never from wherever `lets` happened to be invoked.

```kdl
dir "packages/web"
```

### `shell`

Override the shell (default: `sh` or global config shell).

```kdl
shell "bash"
```

### `platform`

Restrict to specific platforms: `"macos"`, `"linux"`, `"windows"`.

```kdl
platform "macos" "linux"
```

### `confirm`

Yes/no confirmation prompt. Supports interpolation. Bypassed with `--yes`.

```kdl
confirm "Deploy to {environment}?"
```

Confirmations run up front, before any task in the graph executes — a declined confirm aborts the run before any work starts. A confirm-guarded task pulled in as a dep still prompts; without a TTY the run fails rather than silently skipping the guard.

### `prompt`

Text input bound to a variable.

```kdl
prompt name "What is your name?" default="world"
```

### `choose`

Selection menu bound to a variable.

```kdl
choose environment "dev" "staging" "prod"
choose environment "dev" "staging" "prod" default="staging"
```

| Property | Type | Description |
|---|---|---|
| *(first positional)* | string | Variable name (required) |
| *(remaining positional)* | strings | Choices |
| `default` | string | Default choice; must be one of the choices (validated at load). The interactive cursor starts on it. |

Under `--yes`, `choose` uses its `default` — or fails with a clear error if none is set. It never silently picks the first option.

### `run-policy`

Whether references to this task memoize. The default `"once"` runs the task
at most once per invocation no matter how many places reference it;
`"always"` executes every reference (e.g. an intentional
`steps "clean" "build" "clean"`).

```kdl
run-policy "always"
```

### `sources`

File glob patterns this task depends on, relative to the directory containing
`lets.kdl`. Powers two features: [`--watch`](/lets/watch/) re-runs the
command when matching files change, and
[up-to-date checks](/lets/orchestration/#up-to-date-checks-sources--generates)
skip the task when their content is unchanged since the last successful run
(bypass with `--force`; fingerprints live in a self-gitignored `.lets/`
directory). For `--watch`, patterns are collected from the whole task graph
(deps and steps included). Invalid globs are rejected at load time.

```kdl
sources "src/**/*.rs" "Cargo.toml"
```

### `generates`

File glob patterns this task produces. Each must match at least one existing
file for the task to be considered up to date — delete the outputs and the
task re-runs even with unchanged sources. Only meaningful together with
`sources`.

```kdl
generates "dist/**"
```

### `precondition`

A shell command that must exit successfully for the task to run; checked
before the task's deps. Repeatable — all must pass. The optional `message`
replaces the command in the error shown to the user. Runs with the task's
`shell`, `env`, and `dir`; produces no output. Printed but not evaluated
under `--dry-run`.

```kdl
precondition "test -f .env" message="Copy .env.example to .env first"
```

### `status`

Shell commands that decide whether the task is already up to date: when ALL
exit successfully, the task's hooks and `run` commands are skipped (deps and
steps still run first). Bypassed by `--force`; printed but not evaluated
under `--dry-run`.

```kdl
status "test -d node_modules"
```

### `alias`

Alternative names for this command.

```kdl
alias "t" "tst"
```

### `timeout`

Kill after a duration. Formats: `ms`, `s`, `m`, `h`, or plain seconds.

```kdl
timeout "30s"
```

### `retry`

Retry on failure.

```kdl
retry count=3 delay="2s"
```

### `silent` / `quiet`

Buffer the command's output and show it only if the command fails. When the task runs as a dep, the flushed output is labeled with the task name.

```kdl
silent
```

### `hide`

Hide from `--help` and `--list`. Command still works when invoked directly.

```kdl
hide
```

### `deprecated`

Mark as deprecated with optional message.

```kdl
deprecated
deprecated "Use 'new-cmd' instead"
```

### `cmd`

Escape reserved names as subcommands.

```kdl
cmd alias {
    run "echo managing aliases"
}
```

## Interpolation

| Syntax | Description |
|---|---|
| `{name}` | Positional arg, valued flag, interactive variable, or var |
| `{?name:text}` | Emit `text` if a boolean flag is set or an optional value was provided |
| `{--}` | Passthrough arguments after `--`, each shell-quoted |
| `{$VAR}` | Environment variable (task env first, then process; empty when unset) |
| `{{` / `}}` | Literal `{` / `}` (e.g. `awk '{{print $1}}'`) |

Placeholders resolve in `run`, `run-*`, `before`, `after`, `defer`,
`precondition`, `status`, and `confirm` strings, and in `env` values
(vars and `{$VAR}` only there). Unknown placeholders are load errors —
`lets self check` catches them before anything runs.

Args and flags are additionally exported to child processes as
`LETS_ARG_<NAME>` / `LETS_FLAG_<NAME>` (uppercased, `-` → `_`); boolean
flags export `1` only when set.

## Execution order

1. Interaction (`choose`, `prompt`, `confirm`) — collected serially up front across the **whole task graph**, including deps/steps targets, before anything executes. A declined confirm aborts the run.
2. `deps` (parallel, each task at most once per invocation)
3. `steps` (sequential, each task at most once per invocation)
4. `before` hook
5. `run` commands (sequential, with interpolation)
6. `after` hook
