---
title: Watch Mode
description: Re-run commands automatically when the files they depend on change.
---

Declare the files a task depends on with `sources`, then run it with
`--watch` to re-run automatically on changes:

```kdl
dev {
    sources "src/**/*.rs" "Cargo.toml" "templates/**"
    run "cargo run"
}

test {
    sources "src/**" "tests/**"
    run "cargo test"
}
```

```sh
lets --watch dev
```

```
[watch] watching 3 pattern(s) under /path/to/project
   Compiling myapp v0.1.0
[watch] 1 change(s) detected, restarting
   Compiling myapp v0.1.0
```

## How it works

- On each matching change, the running invocation is stopped — the **entire
  process tree** receives `SIGTERM`, then `SIGKILL` after a 2-second grace
  period — and the command re-runs from scratch. This works equally for
  long-running servers (restart) and finite tasks like tests (re-run).
- Filesystem events are debounced (250ms), so an editor save burst triggers
  one restart, not five.
- Each re-run is a fresh invocation: deps run again (still
  [at most once per invocation](/lets/orchestration/#run-once-semantics)),
  and the config file is re-parsed — editing `lets.kdl` itself also triggers
  a restart with the new definition.

## Sources come from the whole task graph

`--watch` collects `sources` from the invoked command **and everything it
reaches via `deps` and `steps`**:

```kdl
compile {
    sources "src/**"
    run "make build"
}

serve {
    deps "compile"
    run "./bin/server"
}
```

`lets --watch serve` watches `src/**` even though `serve` itself declares no
sources. A command whose graph declares no sources at all is an error under
`--watch`.

## Patterns

`sources` accepts glob patterns, matched relative to the directory containing
`lets.kdl`:

```kdl
sources "src/**/*.rs"      // all Rust files under src/
sources "Cargo.toml"       // a single file
sources "assets/**" "*.css"
```

Invalid globs are rejected when the config loads.

## Interactive tasks

Prompts would re-appear on every restart, so `--watch` refuses interactive
task graphs unless you pass `--yes` (prompts then use their defaults, and
`choose` requires a `default="..."`):

```sh
lets --watch --yes deploy-preview
```

:::tip[Pairs well with output modes]
For parallel deps under watch, `--output prefixed` keeps restart output
readable: `lets --watch --output prefixed dev`. See
[Output modes](/lets/orchestration/#output-modes).
:::

## Relationship to up-to-date checks

`sources` is designed to also power fingerprinting (skip a task when its
sources haven't changed since the last run) in a future release — declare
your patterns once and both features use them.
