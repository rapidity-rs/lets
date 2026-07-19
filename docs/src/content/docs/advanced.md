---
title: Advanced
description: Timeouts, retries, silent mode, dry-run, include files, reserved names, and validation.
---

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

Buffer a command's output and show it only on failure:

```kdl
lint {
    silent
    run "cargo clippy -- -D warnings"
}
```

The command's merged stdout and stderr are buffered. On success, nothing is printed. On failure, the buffered output is flushed so you can debug — and when the task ran as a dep, the output is labeled with the task name. The `quiet` keyword is an alias for `silent`.

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

:::tip[Organization pattern]
For large projects, organize by domain:

```
lets.kdl
tasks/
  db.kdl
  deploy.kdl
  frontend.kdl
```
:::

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
- Arguments and flags in `deps`/`steps` references parse against the target's CLI definition — unknown flags, missing required arguments, and invalid choices fail at config load time
- No dependency cycles exist (direct or indirect)
- Duration strings are valid
- `choose` defaults are one of the listed choices
- Platform names are recognized (`macos`, `linux`, `windows`)

Run validation without executing anything:

```sh
lets self check
```

```
lets.kdl is valid (12 commands)
```
