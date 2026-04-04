# Environment & Platform

Configure environment variables, working directories, shells, and platform-specific behavior.

## Environment variables

Set environment variables for a command:

```kdl
serve {
    env PORT="3000" RUST_LOG="debug"
    run "cargo run --bin server"
}
```

Variables are set in the child process environment — they don't affect your shell.

## Env files

Load variables from a `.env` file:

```kdl
serve {
    env-file ".env.local"
    run "cargo run --bin server"
}
```

Supports standard `.env` syntax: comments, blank lines, quoted values, `export` prefix.

If both `env` and `env-file` are specified, explicit `env` values override env-file values:

```kdl
serve {
    env-file ".env"
    env PORT="9999"    // overrides PORT from .env
    run "cargo run"
}
```

## Working directory

Run a command from a different directory:

```kdl
frontend {
    dir "packages/web"
    run "npm run dev"
}
```

The path is relative to the `lets.kdl` file location.

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
    sorted       // Sort commands alphabetically in help
    shell "zsh"  // Default shell for all commands
}
```

| Setting | Default | Description |
|---|---|---|
| `sorted` | `false` | Sort commands alphabetically in `--help` and `--list` |
| `shell` | `"sh"` | Default shell for executing commands |
