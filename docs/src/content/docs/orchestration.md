---
title: Orchestration
description: Compose commands with parallel deps, sequential steps, lifecycle hooks, and output modes.
---

lets provides several ways to compose commands: run tasks before your command, chain steps together, and hook into the execution lifecycle.

## Dependencies (`deps`)

Dependencies run **in parallel** before the main command:

```kdl
lint "cargo clippy"
test "cargo test"

release {
    deps "lint" "test"
    run "gh release create"
}
```

When you run `lets release`, both `lint` and `test` start simultaneously. Once all deps finish, `release` runs. If any dep fails, the main command never executes.

### Run-once semantics

Every task runs **at most once per invocation**, no matter how many places reference it. In a diamond — two deps that share a common dependency — the shared task executes a single time; concurrent references wait for the in-flight run to finish instead of starting a duplicate:

```kdl
shared "cargo build"
lint { deps "shared"; run "cargo clippy" }
test { deps "shared"; run "cargo test" }

ci {
    deps "lint" "test"
}
```

`lets ci` builds once, not twice. If a shared task fails, every task depending on it fails.

References with different arguments are different tasks: `deps "build fast"` and `deps "build slow"` each run.

### Nested references

Reference subcommands with space-separated paths:

```kdl
db {
    migrate "diesel migration run"
}

deploy {
    deps "db migrate"
    run "scripts/deploy.sh"
}
```

### Passing arguments

Tokens after the command path are parsed as that command's own arguments and flags — the same parsing and validation as invoking it from the command line:

```kdl
build {
    arg mode default="debug"
    flag jobs "-j" type="int" default="1"
    run "cargo build --profile {mode} -j {jobs}"
}

release {
    deps "build release -j 8"
    run "gh release create"
}
```

Invalid references — unknown flags, missing required arguments, values outside an `arg`'s choices — are rejected when the config loads, not midway through a run.

## Sequential steps (`steps`)

Steps run **in sequence** before the main command:

```kdl
lint "cargo clippy"
test "cargo test"
build "cargo build --release"

ci {
    description "Full CI pipeline"
    steps "lint" "test" "build"
}
```

Steps execute in order: `lint`, then `test`, then `build`. If any step fails, execution stops.

### Steps vs deps

| | `deps` | `steps` |
|---|---|---|
| Execution | Parallel | Sequential |
| Order guaranteed | No | Yes |
| Use when | Tasks are independent | Order matters |

### Combining deps and steps

You can use both on the same command:

```kdl
setup-db "diesel database setup"
run-migrations "diesel migration run"
lint "cargo clippy"
test "cargo test"

ci {
    deps "setup-db" "run-migrations"
    steps "lint" "test"
    run "echo CI complete!"
}
```

Execution order: deps run first (in parallel), then steps (in sequence), then the main command.

## Hooks (`before` / `after`)

Run shell commands immediately before or after the main `run`:

```kdl
deploy {
    before "echo Starting deploy..."
    after "echo Deploy complete!"
    run "scripts/deploy.sh"
}
```

```
$ lets deploy
Starting deploy...
<deploy output>
Deploy complete!
```

Hooks are simple shell strings — they don't support arguments or interpolation from the command's args/flags (use `run` for that).

## Output modes

When several tasks run via `deps` or `steps`, control how their output reaches the terminal with `output` in the top-level `config` block:

```kdl
config {
    output "prefixed"
}
```

- **`interleaved`** (default) — child tasks share the terminal; output appears as it is produced.
- **`group`** — each task's merged stdout+stderr is buffered and printed as one `[label]` block when the task finishes. Failed tasks still flush their buffered output.
- **`prefixed`** — lines stream live, each tagged `[label]` with a per-task color.

The global `--output <mode>` flag overrides the config value (long-only flag; there is no `-o`):

```sh
lets --output group ci
```

`silent` on a task means its output is buffered and shown only on failure (labeled when the task runs as a dep). The root command itself — the one you invoked — is never prefixed or grouped, since it may be interactive.

## Full execution order

For any command, the complete execution order is:

1. **Interactive** — every `choose`, `prompt`, and `confirm` across the whole task graph (deps and steps targets included) runs serially up front, before anything executes. A declined confirmation aborts the run before any work starts. A confirm-guarded task pulled in as a dep still prompts; without a TTY the run fails instead of silently skipping the guard. See [Interactive](/lets/interactive/).
2. **`deps`** — parallel dependencies (run-once)
3. **`steps`** — sequential steps (run-once)
4. **`before`** hook
5. **`run`** commands (sequential, with interpolation)
6. **`after`** hook

## Real-world example

A complete CI/CD pipeline:

```kdl
description "My project"

lint {
    description "Run linters"
    silent
    run "cargo clippy -- -D warnings"
}

test {
    description "Run tests"
    run "cargo test"
}

fmt {
    description "Check formatting"
    run "cargo fmt --check"
}

check {
    description "Code quality checks"
    steps "fmt" "lint" "test"
}

build {
    description "Build release binary"
    deps "check"
    run "cargo build --release"
}

deploy {
    description "Deploy to production"
    arg environment "staging" "prod"
    deps "build"
    confirm "Deploy to {environment}?"
    before "echo Deploying to {environment}..."
    run "scripts/deploy.sh {environment}"
    after "echo Deploy complete!"
}
```

```
$ lets deploy prod
Deploy to prod? [y/N] y
✔ fmt
✔ lint
✔ test
✔ cargo build --release
Deploying to prod...
<deploy output>
Deploy complete!
```
