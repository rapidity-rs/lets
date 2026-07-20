# Examples

Each directory is a self-contained, runnable `lets` project — `cd` in and try
the commands listed at the top of its `lets.kdl`. Every example is validated
and dry-run by the test suite (`tests/examples.rs`), and rendered verbatim in
the [documentation](https://rapidity-rs.github.io/lets/examples/), so they
cannot drift from what the tool actually supports.

| Example | Shows |
|---|---|
| [`basics`](basics/) | One-liners, help text, nested subcommands, aliases |
| [`args-and-flags`](args-and-flags/) | Typed args, choices, defaults, boolean/valued flags, `{--}` passthrough |
| [`escaping`](escaping/) | Literal braces (`{{`/`}}`), shell-quoted `{--}`, `LETS_ARG_*` env exports |
| [`orchestration`](orchestration/) | Parallel `deps`, sequential `steps`, run-once semantics, arguments in references, hooks |
| [`output-modes`](output-modes/) | `interleaved` / `group` / `prefixed` output, `silent` |
| [`interactive`](interactive/) | `confirm`, `choose` with defaults, `prompt`, CI mode with `--yes` |
| [`environment`](environment/) | `env`, `env-file`, `{$VAR}`, `dir`, `shell`, per-platform commands |
| [`watch`](watch/) | `sources` globs and `--watch` re-runs |
| [`advanced`](advanced/) | `timeout`, `retry`, `silent`, hidden and deprecated commands, multiple `run`s, rich help |
| [`monorepo`](monorepo/) | `include` across files, per-package `dir`, parallel CI with prefixed output |
