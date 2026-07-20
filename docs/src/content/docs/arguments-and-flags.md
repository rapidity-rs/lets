---
title: Arguments & Flags
description: Accept and validate user input with positional arguments, boolean and valued flags, and interpolation placeholders.
---

Arguments and flags let your commands accept user input. lets validates inputs, generates help text, and interpolates values into your shell commands automatically.

## Positional arguments

Define with the `arg` node:

```kdl
greet {
    arg name
    run "echo hello {name}"
}
```

```
$ lets greet taylor
hello taylor

$ lets greet
error: required argument missing: <name>
```

### Default values

Make an argument optional with `default`:

```kdl
greet {
    arg name default="world"
    run "echo hello {name}"
}
```

```
$ lets greet          # hello world
$ lets greet taylor   # hello taylor
```

### Choices

Restrict values to a set of allowed options:

```kdl
deploy {
    arg environment "dev" "staging" "prod"
    run "scripts/deploy.sh {environment}"
}
```

```
$ lets deploy staging   # OK
$ lets deploy banana    # error: invalid value 'banana' for '<environment>'
```

### Help text

```kdl
greet {
    arg name help="Who to greet" default="world"
    run "echo hello {name}"
}
```

### Multiple arguments

Commands can have multiple positional arguments:

```kdl
copy {
    arg source
    arg dest
    run "cp {source} {dest}"
}
```

```
$ lets copy file.txt backup/
```

## Flags

Flags come in two types: **boolean** (on/off) and **valued** (takes a value).

### Boolean flags

```kdl
build {
    flag release "-r" help="Build in release mode"
    run "cargo build {?release:--release}"
}
```

```
$ lets build              # cargo build
$ lets build --release    # cargo build --release
$ lets build -r           # cargo build --release
```

The `{?release:--release}` syntax means: "if the `release` flag is set, emit `--release`; otherwise emit nothing."

### Valued flags

Flags that take a value use the `type` property:

```kdl
deploy {
    flag replicas "-r" type="int" default="3"
    run "deploy --replicas {replicas}"
}
```

```
$ lets deploy                 # deploy --replicas 3
$ lets deploy --replicas 5    # deploy --replicas 5
$ lets deploy -r 5            # deploy --replicas 5
```

Supported types:

| Type | Example | Validation |
|---|---|---|
| `"string"` | `--name taylor` | Any string |
| `"int"` | `--count 5` | Must be an integer |
| `"float"` | `--factor 1.5` | Must be a number |

If no `type` is specified, the flag is boolean.

## Interpolation syntax

Placeholders in `run`, `before`, `after`, `defer`, `precondition`, `status`,
and `confirm` strings are replaced with values at execution time:

| Syntax | Description | Example |
|---|---|---|
| `{name}` | Positional arg, valued flag, or interactive variable | `echo {name}` |
| `{?flag:text}` | Emit `text` if boolean flag is set, empty otherwise | `{?verbose:--verbose}` |
| `{--}` | All arguments after `--`, each shell-quoted | `cargo test {--}` |
| `{$VAR}` | Environment variable (node env first, then process env) | `echo {$HOME}` |
| `{{` / `}}` | Literal `{` / `}` | `awk '{{print $1}}'` |

Placeholders are checked when the config loads: a `{name}` that doesn't match
a declared arg, valued flag, interactive variable, or var is an error — it
never silently renders as an empty string. `{$VAR}` is the exception: the
environment is only knowable at run time, and an unset variable renders empty,
matching shell behavior.

### Literal braces

Shell syntax that uses braces must be escaped with `{{` and `}}`:

```kdl
first-column "awk '{{print $1}}' data.txt"
fallback "echo ${{EDITOR:-vi}}"
inspect "docker inspect --format '{{{{.State.Running}}}}' app"
```

### Passthrough arguments

The `{--}` placeholder passes through everything after `--`. Each token is
shell-quoted, so arguments with spaces or special characters arrive intact:

```kdl
test {
    run "cargo test {--}"
}
```

```
$ lets test -- --nocapture test_name
# runs: cargo test --nocapture test_name
$ lets test -- "some test name"
# runs: cargo test 'some test name'   (one argument, not three)
```

### Conditional text

The `{?flag:text}` syntax emits text only when a boolean flag is set:

```kdl
build {
    flag verbose "-v"
    flag release "-r"
    run "cargo build {?verbose:--verbose} {?release:--release}"
}
```

```
$ lets build              # cargo build
$ lets build -v -r        # cargo build --verbose --release
```

### Environment variable interpolation

Reference environment variables with `{$VAR}`:

```kdl
serve {
    env PORT="3000"
    run "echo listening on {$PORT}"
}
```

The node's `env` values are checked first, then the process environment.

### Values as environment variables

Every arg and flag is also exported to the child process environment as
`LETS_ARG_<NAME>` / `LETS_FLAG_<NAME>` (uppercased, `-` becomes `_`).
Boolean flags export `1` only when set. This is the safe way to handle
values that may contain spaces or shell metacharacters — quote the variable
in your script instead of splicing a placeholder into the command line:

```kdl
greet {
    arg name
    run "echo \"hello $LETS_ARG_NAME\""
}
```

```
$ lets greet "Taylor Faucett"
hello Taylor Faucett
```

## Combining args and flags

A command can have both arguments and flags:

```kdl
deploy {
    description "Deploy to an environment"
    arg environment "dev" "staging" "prod"
    flag force "-f" help="Skip safety checks"
    flag replicas "-r" type="int" default="3"
    run "deploy.sh {environment} --replicas {replicas} {?force:--force}"
}
```

```
$ lets deploy staging -f -r 5
# runs: deploy.sh staging --replicas 5 --force
```
