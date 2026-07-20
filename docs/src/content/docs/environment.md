---
title: Environment & Platform
description: Configure environment variables, working directories, shells, and platform-specific behavior.
---

Configure environment variables, working directories, shells, and platform-specific behavior.

## Variables (`vars`)

Define reusable values once, reference them anywhere with `{name}`. A
top-level `vars` block is visible to every command; a `vars` block inside a
command (or group) is scoped to it and its children, overriding globals:

```kdl
vars {
    registry "ghcr.io/acme"
    image "{registry}/app"       // vars can reference earlier vars
}

push "docker push {image}:latest"

deploy {
    vars {
        env-name "staging"       // overrides any global env-name
    }
    run "scripts/deploy.sh {env-name} {image}"
}
```

Resolution order for `{name}`: interactive bindings (`prompt`/`choose`),
then declared args and flags, then command vars, then group vars, then
globals. Var values are resolved once at config load and may also reference
the environment with `{$VAR}`. A var value that references an unknown name
fails when the config loads. Unlike `env`, vars are pure config-level
text substitution — they never touch the child process environment.

### Dynamic vars (`cmd=`)

A var declared with `cmd="…"` runs a shell command instead of holding static
text — the moral equivalent of `` x := `cmd` `` in just or `sh:` vars in
Taskfile:

```kdl
vars {
    sha cmd="git rev-parse --short HEAD"
    branch cmd="git branch --show-current"
}

tag "docker tag app registry/app:{sha}"
release "gh release create v-{sha} --target {branch}"
```

Dynamic vars are **lazy and cached**: the command runs with the config shell
in the project root the first time the var is referenced during an
invocation, and the trimmed stdout is reused everywhere else — even across
parallel tasks. A var no task references never runs. If the command fails,
the run aborts with an error naming the var.

Because their values only exist at run time, static var values can't
reference dynamic vars — reference `{sha}` directly where you need it. The
`cmd=` string itself may reference static vars and `{$VAR}`.

## Environment variables

Set environment variables for a command, or for every task at once under
`config`:

```kdl
config {
    env CI_PROJECT="acme"          // visible to every task
    env-file ".env.shared"         // project-wide env file
}

serve {
    env PORT="3000" RUST_LOG="debug"
    run "cargo run --bin server"
}
```

Precedence, later winning: config `env-file`, config `env`, task `env-file`,
task `env`.

Variables are set in the child process environment — they don't affect your
shell. Values may reference config vars (`{name}`) and the environment
(`{$VAR}`); args and flags are not in scope for `env` values — use the
`LETS_ARG_*` / `LETS_FLAG_*` exports instead.

## Env files

Load variables from a `.env` file:

```kdl
serve {
    env-file ".env.local"
    run "cargo run --bin server"
}
```

Supports standard `.env` syntax: comments, blank lines, quoted values, `export` prefix. The path is relative to the directory containing `lets.kdl`.

If both `env` and `env-file` are specified, explicit `env` values override env-file values:

```kdl
serve {
    env-file ".env"
    env PORT="9999"    // overrides PORT from .env
    run "cargo run"
}
```

## Working directory

Commands always run from the directory containing `lets.kdl`, no matter
where `lets` was invoked — discovery walks up to find the config, execution
roots at it. Use `dir` to run from somewhere else:

```kdl
frontend {
    dir "packages/web"
    run "npm run dev"
}
```

The path is relative to the `lets.kdl` file location; absolute paths are
used as-is.

Two variables are always exported to child processes:

| Variable | Value |
|---|---|
| `LETS_PROJECT_ROOT` | Directory containing `lets.kdl` |
| `LETS_INVOCATION_DIR` | Directory `lets` was invoked from |

A task that needs to operate on files where the user is standing can use
`cd "$LETS_INVOCATION_DIR"` explicitly.

## Shell override

By default, commands run via `sh -c`. Override per-command:

```kdl
script {
    shell "bash"
    run "echo $BASH_VERSION"
}
```

Or set a default shell for all commands in `config`:

```kdl
config {
    shell "zsh"
}

// All commands now use zsh unless they specify their own shell
build "cargo build"
```

Per-command `shell` overrides the global default.

## Platform guards

Restrict a command to specific platforms:

```kdl
install {
    platform "macos" "linux"
    run "echo installing"
}
```

Valid platforms: `"macos"`, `"linux"`, `"windows"`. Unrecognized platform names produce a parse error.

## Platform-specific run commands

Provide different commands for different operating systems:

```kdl
install {
    description "Install dependencies"
    run-macos "brew install libpq"
    run-linux "sudo apt-get install -y libpq-dev"
    run-windows "choco install libpq"
}
```

Platform-specific `run-*` commands take priority. If the current platform has no specific variant, the generic `run` is used as a fallback:

```kdl
install {
    run "echo unsupported platform"
    run-macos "brew install libpq"
    run-linux "apt-get install libpq-dev"
}
```

On macOS this runs `brew install libpq`. On Windows (no `run-windows`), it falls back to `echo unsupported platform`.

## Global config

The top-level `config` block sets defaults for all commands:

```kdl
config {
    sorted                // Sort commands alphabetically in help
    shell "zsh"           // Default shell for all commands
    output "interleaved"  // Output mode for tasks run via deps/steps
}
```

| Setting | Default | Description |
|---|---|---|
| `sorted` | `false` | Sort commands alphabetically in `--help` and `--list` |
| `shell` | `"sh"` | Default shell for executing commands |
| `output` | `"interleaved"` | Output mode for tasks run via deps/steps: `"interleaved"`, `"group"`, or `"prefixed"` — see [Output modes](/lets/orchestration/#output-modes) |
