# Contributing

## Prerequisites

- Rust nightly (pinned in `rust-toolchain.toml`)
- [just](https://github.com/casey/just)
- [dprint](https://dprint.dev/)

## Quick start

```sh
just build     # build all crates
just test      # run all tests
just lint      # run clippy
just ci        # full preflight (lint + test + format check + dist check)
```

## Workspace structure

```
crates/
  svg-language-server/  LSP binary — thin orchestrator over domain crates
  svg-format/           structural SVG formatter (library + CLI)
  svg-lint/             structural diagnostics engine
  svg-color/            color parsing, extraction, and presentation
  svg-data/             generated SVG catalog from BCD + W3C spec
  svg-references/       symbol extraction (id, class, custom property)
  svg-tree/             shared tree-sitter traversal helpers
```

See the dependency graph in [`README.md`](README.md).

## Code standards

The workspace enforces strict lint rules in `Cargo.toml`:

- `unsafe_code = "forbid"`
- `unwrap_used = "deny"` / `expect_used = "deny"`
- `clippy::pedantic` and `clippy::nursery` at warn level
- `missing_docs = "warn"`

Use `?` with `Option`/`Result` instead of `unwrap()`/`expect()`. Use
`tracing::warn!` for recoverable failures. Tests return
`Result<(), Box<dyn std::error::Error>>` and use `.ok_or("context")?`.

## Adding a new lint rule

1. Add a variant to `DiagnosticCode` in `crates/svg-lint/src/types.rs`
2. Add `as_str`, `FromStr`, and `Display` entries for the new variant
3. Implement the check in `crates/svg-lint/src/rules/mod.rs`
4. Add tests in `crates/svg-lint/src/lib.rs`

## Adding a new LSP feature

1. Add a handler method in `crates/svg-language-server/src/lib.rs`
2. Register the capability in `server_capabilities()`
3. Add an integration test in `crates/svg-language-server/tests/`

## Offline builds

Set `SVG_DATA_OFFLINE=1` to skip network fetches in the `svg-data` build
script. Cached BCD and spec data in `target/` will be used instead.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/):
`type(scope): subject`. Keep the subject under 50 characters. Prefer
multiline bodies for non-trivial changes.
