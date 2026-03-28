# PROJECT KNOWLEDGE BASE

**Generated:** 2026-03-28T18:00:19Z\
**Commit:** `79cbf25`\
**Branch:** `master`

## OVERVIEW

Rust workspace for SVG tooling. Main product is `svg-language-server` (LSP) backed by crate split: catalog (`svg-data`), lint (`svg-lint`), format (`svg-format`), color (`svg-color`), references (`svg-references`).

## STRUCTURE

```text
./
├── crates/
│   ├── svg-language-server/   # LSP binary, protocol glue, feature orchestration
│   ├── svg-data/              # generated SVG catalog + compat metadata
│   ├── svg-lint/              # structural diagnostics
│   ├── svg-format/            # formatter lib + CLI
│   ├── svg-color/             # color parse/extract/presentation
│   └── svg-references/        # id/class/custom-prop refs + definitions
├── docs/
│   ├── plans/
│   └── specs/
├── samples/                   # manual fixtures and behavior examples
├── justfile                   # canonical task runner
├── .dprint.json               # formatting policy (rustfmt/tombi wired via dprint)
└── DISCOVERIES.md             # parser/LSP gotchas and invariants
```

## WHERE TO LOOK

| Task                                 | Location                                                             | Notes                                            |
| ------------------------------------ | -------------------------------------------------------------------- | ------------------------------------------------ |
| Add or debug LSP method              | `crates/svg-language-server/src/main.rs`                             | Single orchestrator; most handlers live here     |
| Change lint behavior                 | `crates/svg-lint/src/rules.rs`                                       | Rule engine + suppression handling               |
| Add hover/completion metadata        | `crates/svg-data/build.rs`, `crates/svg-data/src/lib.rs`             | Build-time catalog generation + runtime API      |
| Change formatter output              | `crates/svg-format/src/lib.rs`                                       | Attribute layout/sort and tag layout policy      |
| Change color extraction/presentation | `crates/svg-color/src/extract.rs`, `crates/svg-color/src/present.rs` | CSS + SVG extraction and output labels           |
| Change definition/reference lookup   | `crates/svg-references/src/lib.rs`                                   | Shared symbol model for ids/classes/custom props |
| Validate E2E feature behavior        | `crates/svg-language-server/tests/integration.rs`                    | Spawns binary, sends JSON-RPC manually           |

## CODE MAP

| Symbol Area                                                     | Type          | Location                                 | Role                                                      |
| --------------------------------------------------------------- | ------------- | ---------------------------------------- | --------------------------------------------------------- |
| `Backend` and LSP handlers                                      | struct + impl | `crates/svg-language-server/src/main.rs` | Wiring for hover/completion/diagnostics/format/color/defs |
| `lint_tree`                                                     | function      | `crates/svg-lint/src/lib.rs`             | Core lint entry for parsed trees                          |
| catalog lookup funcs (`element`, `attribute`, `attributes_for`) | functions     | `crates/svg-data/src/lib.rs`             | Spec truth API consumed by LSP + lint                     |
| `format_with_options`                                           | function      | `crates/svg-format/src/lib.rs`           | Deterministic structural formatting                       |
| `extract_colors_from_tree`                                      | function      | `crates/svg-color/src/extract.rs`        | Color ranges from SVG + inline CSS                        |
| `definition_target_at`                                          | function      | `crates/svg-references/src/lib.rs`       | Definition jump target resolution                         |

## CONVENTIONS

- Use `just` targets as source of truth for dev flow (`check`, `lint`, `test`, `ci`).
- Formatting is `dprint` first; Rust/TOML/justfile formatting delegated through dprint exec plugins.
- Tree-sitter parse-once reuse pattern in LSP: document state stores source + tree; features consume same tree.
- Workspace split is intentional: protocol glue in one crate, domain logic in leaf crates.
- `svg-data` is generated; build pipeline can fetch/cached MDN/web-features data.

## ANTI-PATTERNS (THIS PROJECT)

- Do not edit generated catalog output in `svg-data`; change `build.rs` inputs/pipeline instead.
- Do not assume one generic attribute node kind; tree-sitter-svg uses multiple typed attribute name kinds.
- Do not parse inside `<style>`/`<script>` as XML child elements; treat content as raw text/injections.
- Do not rely on deprecated `xlink:*` attrs (notably `xlink:href`); use modern equivalents.
- Do not skip UTF-16/byte conversion helpers when mapping tree-sitter ranges to LSP positions.

## UNIQUE STYLES

- Integration test style is protocol-level and process-backed, not only API unit tests.
- Diagnostic suppression comments are embedded in SVG source (rule-level and code-level).
- Formatter behavior is structural and deterministic, not whitespace-preserving pretty-print.

## COMMANDS

```bash
just check
just lint
just test
just ci
just run-lsp
```

## NOTES

- Root `target/` can be large; ignore during repo exploration and docs generation.
- `samples/` is excluded from dprint; treat it as fixtures/examples, not style source.
- Compat parsing logic exists both build-time (`svg-data`) and runtime (`svg-language-server`); keep behavior aligned when touched.
