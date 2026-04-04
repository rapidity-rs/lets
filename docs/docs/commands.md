# Commands

Commands are the building blocks of a lets CLI. Every node in your `lets.kdl` that isn't a reserved keyword becomes a command.

## One-liners

The simplest form — a name and a shell command:

```kdl
build "cargo build"
test "cargo test"
dev "cargo watch -x run"
```

```
$ lets build
$ lets test
$ lets dev
```

You can add a description inline:

```kdl
build "cargo build" description="Build the project"
```

## Block syntax

For commands that need more than a name and a shell string, use block syntax:

```kdl
build {
    description "Build the project"
    flag release "-r" help="Build in release mode"
    run "cargo build {?release:--release}"
}
```

The `run` node specifies the shell command. Everything else configures how the command appears and behaves.

## Multiple run commands

A command can have multiple `run` nodes. They execute sequentially, stopping on first failure:

```kdl
deploy {
    description "Deploy the application"
    run "cargo build --release"
    run "scp target/release/app server:/opt/"
    run "ssh server systemctl restart app"
}
```

Each `run` gets its own shell invocation with proper exit code checking. If the second command fails, the third never runs.

## Descriptions

There are two levels of description:

- **`description`** — shown in the parent command list (`lets --help`)
- **`long-description`** — shown when viewing the command's own help (`lets deploy --help`)

```kdl
deploy {
    description "Deploy the application"
    long-description """
        Deploy the application to the target environment.
        Runs database migrations, builds the release artifact,
        and restarts the service.
        """
    run "scripts/deploy.sh"
}
```

If `long-description` is not set, `--help` falls back to `description`.

## Examples

Show usage examples at the bottom of a command's `--help`:

```kdl
deploy {
    description "Deploy the application"
    examples """
        lets deploy staging
        lets deploy prod --dry-run
        lets deploy prod --yes
        """
    arg environment "staging" "prod"
    run "scripts/deploy.sh {environment}"
}
```

Examples are auto-indented in the help output — write them without leading spaces.

## Subcommands

Nest commands inside blocks to create subcommand groups:

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
$ lets db reset
```

Subcommands can nest arbitrarily deep:

```kdl
cloud {
    description "Cloud operations"
    aws {
        deploy "scripts/aws-deploy.sh"
        logs "scripts/aws-logs.sh"
    }
    gcp {
        deploy "scripts/gcp-deploy.sh"
    }
}
```

```
$ lets cloud aws deploy
$ lets cloud gcp deploy
```

A parent command can have both its own `run` and child subcommands:

```kdl
db {
    description "Database commands"
    run "psql $DATABASE_URL"

    migrate "diesel migration run"
    reset "diesel database reset"
}
```

Here `lets db` opens a psql shell, while `lets db migrate` runs migrations.

## Aliases

Give commands shorter alternative names:

```kdl
test {
    alias "t"
    run "cargo test"
}

build {
    alias "b" "bld"
    run "cargo build"
}
```

```
$ lets t          # same as lets test
$ lets b          # same as lets build
```

Aliases appear in the help output next to the command name.

## Hidden commands

Hide commands from `--help` and `--list`. The command still works when invoked directly or referenced via `deps`/`steps`:

```kdl
setup-db {
    hide
    run "diesel database setup"
}

test {
    deps "setup-db"
    run "cargo test"
}
```

`setup-db` won't appear in `lets --help`, but `lets setup-db` still works, and it runs automatically before `test` via the `deps` reference.

## Deprecated commands

Mark commands for removal. They remain visible in help with a styled indicator, and print a warning when invoked:

```kdl
old-deploy {
    deprecated "Use 'deploy' instead"
    description "Deploy (legacy method)"
    run "scripts/old-deploy.sh"
}
```

```
$ lets --help
  old-deploy  Deploy (legacy method) (deprecated: Use 'deploy' instead)

$ lets old-deploy
warning: 'old-deploy' is deprecated. Use 'deploy' instead
...
```

## Reserved names

Most node names are treated as commands, but some are reserved as keywords (`description`, `run`, `arg`, `flag`, etc.). If you need a command with a reserved name, use the `cmd` prefix:

```kdl
tools {
    cmd alias {
        description "Manage aliases"
        run "scripts/alias-manager.sh"
    }
    cmd flag {
        description "Manage feature flags"
        run "scripts/flag-manager.sh"
    }
}
```

!!! tip "Typo detection"
    If you accidentally misspell a keyword (e.g., `descrption` instead of `description`), lets will warn you: *"unknown node 'descrption' — did you mean 'description'?"*. The misspelled node is still treated as a subcommand, so nothing breaks, but the warning helps you catch mistakes.
