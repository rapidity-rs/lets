# Interactive

lets has built-in support for confirmation prompts, text input, and selection menus. All interactive features can be bypassed with `--yes` / `-y` for CI environments.

## Confirmation (`confirm`)

Ask for yes/no confirmation before running a command:

```kdl
clean {
    confirm "This will delete the target/ directory. Continue?"
    run "cargo clean"
}
```

```
$ lets clean
This will delete the target/ directory. Continue? [y/N] y
```

The confirmation message supports interpolation:

```kdl
deploy {
    arg environment "staging" "prod"
    confirm "Deploy to {environment}?"
    run "scripts/deploy.sh {environment}"
}
```

```
$ lets deploy prod
Deploy to prod? [y/N]
```

## Text input (`prompt`)

Ask the user for text input, bound to a variable:

```kdl
greet {
    prompt name "What is your name?" default="world"
    run "echo hello {name}"
}
```

```
$ lets greet
What is your name? [world]: taylor
hello taylor
```

Properties:

| Property | Required | Description |
|---|---|---|
| *(first positional)* | Yes | Variable name |
| *(second positional)* | No | Prompt message (defaults to `"name: "`) |
| `default` | No | Default value (used with `--yes`) |

## Selection menu (`choose`)

Present an interactive selection menu:

```kdl
deploy {
    choose environment "dev" "staging" "prod"
    confirm "Deploy to {environment}?"
    run "scripts/deploy.sh {environment}"
}
```

```
$ lets deploy
? environment
> dev
  staging
  prod
Deploy to dev? [y/N]
```

The selected value is bound to the variable name for interpolation.

## Combining interactive features

Interactive elements are processed in order: `choose` first, then `prompt`, then `confirm`. This allows later elements to reference earlier ones:

```kdl
release {
    choose channel "stable" "beta" "nightly"
    prompt version "Version number?"
    confirm "Release {version} to {channel}?"
    run "scripts/release.sh {channel} {version}"
}
```

## CI mode (`--yes`)

The `--yes` / `-y` flag bypasses all interactive prompts:

- **`confirm`** — automatically answers yes
- **`prompt`** — uses the `default` value (empty string if no default)
- **`choose`** — uses the first choice

```sh
# Non-interactive deploy
lets deploy --yes
```

This is a global flag — it works on any command:

```sh
lets --yes deploy
```

!!! warning "Prompts without defaults"
    If a `prompt` has no `default` and `--yes` is used, the variable will be an empty string. Make sure prompts used in CI have sensible defaults.
