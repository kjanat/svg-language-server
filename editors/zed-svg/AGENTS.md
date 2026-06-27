# PROJECT KNOWLEDGE BASE

**Generated:** 2026-03-28T18:48:47Z\
**Commit:** 79ac45c\
**Branch:** lsp

## OVERVIEW

Zed SVG extension package. Rust `cdylib` wrapper + Zed language config/query
pack. The grammar source of truth lives in `../../grammars/tree-sitter-svg`.

## STRUCTURE

```text
./
├── src/lib.rs                  # extension entry; registers extension + LS command
├── extension.toml              # extension manifest; grammar pin + LS wiring
├── languages/svg/              # Zed-facing language config + queries
├── DISCOVERIES.md              # known breakages, gotchas
└── target/                     # build output (generated)
```

## WHERE TO LOOK

| Task                         | Location                                   | Notes                                       |
| ---------------------------- | ------------------------------------------ | ------------------------------------------- |
| Extension metadata/version   | `extension.toml`                           | Keep `version` aligned with `Cargo.toml`    |
| Extension runtime entry      | `src/lib.rs`                               | `register_extension!`, LS executable lookup |
| SVG editor behavior          | `languages/svg/config.toml`                | Node names must match grammar               |
| Highlight/injection behavior | `languages/svg/*.scm`                      | Keep aligned with grammar query changes     |
| Known failure modes          | `DISCOVERIES.md`                           | Add new gotchas here                        |
| Grammar internals            | `../../grammars/tree-sitter-svg/AGENTS.md` | Canonical grammar source                    |

## CODE MAP

| Symbol                    | Type       | Location     | Role                                    |
| ------------------------- | ---------- | ------------ | --------------------------------------- |
| `SvgExtension`            | struct     | `src/lib.rs` | Extension implementation                |
| `new`                     | method     | `src/lib.rs` | Init extension state                    |
| `language_server_command` | method     | `src/lib.rs` | Resolve `svg-language-server` from PATH |
| `register_extension!`     | macro call | `src/lib.rs` | Module registration entrypoint          |

## CONVENTIONS

- This is part of the top-level SVG monorepo. The extension crate is a Cargo
  workspace member.
- Grammar changes happen in `../../grammars/tree-sitter-svg`, then Zed query
  consumers in `languages/svg/*.scm` are synced as needed.
- The published `[grammars.svg]` pin in `extension.toml` must be bumped only to
  a commit that already contains the grammar path being referenced.
- `languages/svg/*.scm` are consumer queries for Zed; they may diverge from
  grammar queries, but divergence must be deliberate.
- Schema headers in TOML files use `kjanat/zed-editor` raw URLs (see
  `DISCOVERIES.md`).
- Formatting/lint intent for TOML in `tombi.toml` (aligned `=`, order-sensitive
  tables).

## ANTI-PATTERNS (THIS PROJECT)

- Editing generated artifacts: `target/**`, `*.wasm`.
- Editing a separate `tree-sitter-svg` checkout while expecting this extension
  to pick it up.
- Changing grammar node names without updating `languages/svg/config.toml` and
  related `.scm` queries.
- Updating only one side of the query pair
  (`../../grammars/tree-sitter-svg/queries/*` vs `languages/svg/*`) without
  explicit reason.
- Assuming `svg-language-server` exists; runtime expects it on PATH.

## UNIQUE STYLES

- Grammar kept generic XML-like; SVG-specific semantics pushed into query
  captures and typed attribute nodes.
- Path data (`d`) is treated as structured grammar surface, not plain string.
- Discoveries-first workflow: record surprising failures in `DISCOVERIES.md`
  immediately.

## COMMANDS

```bash
# root extension checks
cargo check

# build extension wasm artifact
cargo build --target wasm32-wasip2

# install language server expected by extension runtime
cargo install --path ../../crates/svg-language-server --bin svg-language-server

# dev install in Zed command palette
zed: Install Dev Extension
```

## NOTES

- Root `.gitignore` excludes build outputs such as `target/` and `*.wasm`.
- If Zed reports SVG not registered, first inspect query/node-name drift
  (`DISCOVERIES.md`).
- If grammar behavior must change: edit `../../grammars/tree-sitter-svg`, then
  sync `languages/svg/*.scm` as needed.
