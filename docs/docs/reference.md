# Reference

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
    sorted       // Sort commands alphabetically
    shell "zsh"  // Default shell for all commands
}
```

### `include`

Import commands from another KDL file. Paths are relative to the including file.

```kdl
include "tasks/db.kdl"
```

## Command nodes

Any top-level node that isn't `description`, `config`, or `include` defines a command.

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
```

| Property | Type | Description |
|---|---|---|
| *(first positional)* | string | Argument name (required) |
| *(remaining positional)* | strings | Allowed choices |
| `help` | string | Help text |
| `default` | string | Default value (makes arg optional) |

### `flag`

Boolean or valued flag.

```kdl
flag verbose
flag verbose "-v" help="Enable verbose output"
flag count "-c" type="int" default="3"
```

| Property | Type | Description |
|---|---|---|
| *(first positional)* | string | Flag name (required) |
| *(second positional)* | string | Short alias (e.g. `"-v"`) |
| `help` | string | Help text |
| `type` | string | Value type: `"string"`, `"int"`, `"float"` |
| `default` | string | Default value (valued flags only) |

### `deps`

Tasks to run in **parallel** before this command.

```kdl
deps "lint" "test"
deps "db migrate"
```

### `steps`

Tasks to run **sequentially** before this command.

```kdl
steps "lint" "test" "build"
```

### `before`, `after`

Shell commands to run before/after the main `run`.

```kdl
before "echo Starting..."
after "echo Done!"
```

### `env`

Set environment variables.

```kdl
env PORT="3000" RUST_LOG="debug"
```

### `env-file`

Load environment variables from a file. Explicit `env` values override.

```kdl
env-file ".env.local"
```

### `dir`

Set working directory.

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

### `prompt`

Text input bound to a variable.

```kdl
prompt name "What is your name?" default="world"
```

### `choose`

Selection menu bound to a variable.

```kdl
choose environment "dev" "staging" "prod"
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

Suppress output unless the command fails.

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
| `{name}` | Positional arg, valued flag, or interactive variable |
| `{?flag:text}` | Emit `text` if boolean flag is set |
| `{--}` | Passthrough arguments after `--` |
| `{$VAR}` | Environment variable |

## Execution order

1. `deps` (parallel)
2. `steps` (sequential)
3. Interactive: `choose`, `prompt`
4. `confirm`
5. `before` hook
6. `run` commands (sequential, with interpolation)
7. `after` hook
