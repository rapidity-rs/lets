# KDL Primer

[KDL](https://kdl.dev) is a document language designed to be readable, writable, and unambiguous. If you've used XML, JSON, YAML, or TOML, KDL will feel familiar — but cleaner.

You don't need to learn the full KDL spec to use lets. This page covers everything you need.

## Nodes

A KDL document is a list of **nodes**. Each node has a name and optional arguments:

```kdl
build "cargo build"
```

Here `build` is the node name and `"cargo build"` is a string argument.

## Named properties

Nodes can have named properties using `key=value` syntax:

```kdl
flag verbose "-v" help="Enable verbose output"
```

Here `verbose` and `"-v"` are positional arguments, and `help="Enable verbose output"` is a named property.

## Blocks

Nodes can have **children** inside curly braces:

```kdl
db {
    description "Database commands"
    migrate "diesel migration run"
    reset "diesel database reset"
}
```

Children are just more nodes. Nesting can go as deep as you need.

## Strings

KDL supports several string formats:

```kdl
// Regular strings (most common)
description "Deploy the application"

// Strings with escape sequences
examples "line one\nline two"

// Multi-line strings (triple-quoted, auto-dedented)
long-description """
    This is a multi-line string.
    The leading whitespace is stripped based on
    the indentation of the closing triple-quote.
    """
```

!!! tip "Multi-line strings"
    Triple-quoted strings (`"""..."""`) automatically strip leading indentation based on the position of the closing `"""`. This keeps your KDL file clean without affecting the output.

## Comments

```kdl
// Single-line comment

/* Multi-line
   comment */

build "cargo build" // Inline comment
```

## Numbers and booleans

```kdl
retry count=3          // Integer
config {
    sorted true        // Boolean (but in lets, bare `sorted` also works)
}
```

## That's it

You now know enough KDL to use every feature of lets. The key patterns are:

- **One-liner**: `name "command"` — a node with a string argument
- **Block**: `name { children... }` — a node with child nodes
- **Properties**: `key="value"` — named values on a node

For the full KDL spec, see [kdl.dev](https://kdl.dev).
