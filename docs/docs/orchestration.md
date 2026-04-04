# Orchestration

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

## Full execution order

For any command, the complete execution order is:

1. **`deps`** — parallel dependencies
2. **`steps`** — sequential steps
3. **Interactive** — `choose`, `prompt` (see [Interactive](interactive.md))
4. **`confirm`** — yes/no confirmation
5. **`before`** hook
6. **`run`** commands (sequential, with interpolation)
7. **`after`** hook

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
✔ lint (parallel with test, fmt)
✔ fmt
✔ lint
✔ test
✔ cargo build --release
Deploy to prod? [y/N] y
Deploying to prod...
<deploy output>
Deploy complete!
```
