# Advanced

## Timeout

Kill a command if it runs longer than a duration:

```kdl
health-check {
    timeout "30s"
    run "curl -f http://localhost:3000/health"
}
```

Supported duration formats:

| Format | Example | Meaning |
|---|---|---|
| `Nms` | `500ms` | Milliseconds |
| `Ns` | `30s` | Seconds |
| `Nm` | `5m` | Minutes |
| `Nh` | `1h` | Hours |
| `N` | `30` | Seconds (plain number) |

When a command times out, the entire process group is killed — including any child processes it spawned.

## Retry

Retry a command on failure:

```kdl
health-check {
    retry count=5 delay="2s"
    run "curl -f http://localhost:3000/health"
}
```

| Property | Type | Description |
|---|---|---|
| `count` | int | Number of attempts |
| `delay` | string | Delay between retries (duration string) |

The command runs up to `count` times. If it succeeds on any attempt, execution continues normally. If all attempts fail, the last error is reported.

## Silent mode

Suppress stdout and stderr unless the command fails:

```kdl
lint {
    silent
    run "cargo clippy -- -D warnings"
}
```

On success, no output. On failure, the captured output is printed so you can debug. The `quiet` keyword is an alias for `silent`.

## Dry-run mode

See what would execute without running it:

```
$ lets --dry-run deploy staging
[dry-run] echo Starting deploy...
[dry-run] scripts/deploy.sh staging
[dry-run] echo Deploy complete!
```

Dry-run applies to all commands, deps, steps, and hooks. It's a global flag: `lets --dry-run <command>`.

## Include files

Split your config across multiple files:

```kdl
include "tasks/db.kdl"
include "tasks/deploy.kdl"

build "cargo build"
```

Included commands are merged into the top-level command list. Paths are relative to the including file.

!!! tip "Organization pattern"
    For large projects, organize by domain:

    ```
    lets.kdl
    tasks/
      db.kdl
      deploy.kdl
      frontend.kdl
    ```

## Reserved name escape (`cmd`)

Some words are reserved as KDL keywords (`description`, `run`, `arg`, `flag`, etc.). If you need a command with one of these names, prefix it with `cmd`:

```kdl
// Without cmd: "alias" would be parsed as the alias keyword
tools {
    cmd alias {
        description "Manage aliases"
        run "scripts/alias-manager.sh"
    }
}
```

The `cmd` prefix works at any level — top-level or nested:

```kdl
cmd include "echo this is a command, not a file include"
```

## Validation

At parse time, lets validates:

- All `deps`/`steps` references resolve to existing commands
- Referenced commands have no required arguments (deps/steps can't supply them)
- No dependency cycles exist (direct or indirect)
- Duration strings are valid
- Platform names are recognized (`macos`, `linux`, `windows`)

Run validation without executing anything:

```sh
lets self check
```

```
lets.kdl is valid (12 commands)
```
