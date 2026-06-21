# PROJECT KNOWLEDGE BASE

**Generated:** 2026-03-30\
**Commit:** `346abb2`\
**Branch:** `split`

## OVERVIEW

SVG tooling workspace. The repo now acts as the umbrella for the language
server, parser grammar, editor integrations, spec data, linting, formatting,
color analysis, reference lookup, and shared tree-sitter helpers.

## STRUCTURE

```text
./
├── crates/
│   ├── svg-language-server/   # LSP binary, protocol glue, feature orchestration
│   ├── svg-data/              # generated SVG catalog + compat metadata
│   ├── svg-data-regen/        # deterministic spec/catalog regeneration
│   ├── svg-lint/              # structural diagnostics + suppression handling
│   ├── svg-format/            # structural formatter lib + CLI
│   ├── svg-color/             # color parse/extract/presentation
│   ├── svg-references/        # id/class/custom-prop refs + definitions
│   └── svg-tree/              # shared tree-sitter traversal/query helpers
├── grammars/
│   └── tree-sitter-svg/       # SVG Tree-sitter grammar + queries/bindings
├── editors/
│   └── zed-svg/               # Zed extension for SVG
├── .github/workflows/         # release + npm publish automation
├── docs/
│   ├── plans/                 # dated implementation checklists
│   ├── specs/                 # dated design docs + non-goals
│   └── patches/               # archived downstream patch bundles
├── samples/                   # manual SVG fixtures and smoke-test files
├── scripts/                   # Bun/TS release helper tooling
├── justfile                   # canonical task runner
├── .dprint.jsonc              # formatting policy and exec plugins
├── tombi.toml                 # TOML style + lint rules
└── DISCOVERIES.md             # parser/LSP gotchas and invariants
```

## WHERE TO LOOK

| Task                                          | Location                                                                   | Notes                                                |
| --------------------------------------------- | -------------------------------------------------------------------------- | ---------------------------------------------------- |
| Add or debug LSP method                       | `crates/svg-language-server/src/lib.rs`                                    | Main async server/orchestrator                       |
| Change lint behavior                          | `crates/svg-lint/src/rules/mod.rs`                                         | Rule engine + suppression handling                   |
| Add hover/completion metadata                 | `crates/svg-data/build.rs`, `crates/svg-data/src/lib.rs`                   | Build-time catalog generation + runtime API          |
| Change formatter output                       | `crates/svg-format/src/lib.rs`                                             | Attribute layout/sort, ignore directives, tag policy |
| Change color extraction/presentation          | `crates/svg-color/src/extract.rs`, `crates/svg-color/src/present.rs`       | CSS + SVG extraction and output labels               |
| Change definition/reference lookup            | `crates/svg-references/src/lib.rs`                                         | Shared symbol model for ids/classes/custom props     |
| Change shared tree traversal                  | `crates/svg-tree/src/lib.rs`                                               | Shared node-walk, ancestor, and kind helpers         |
| Change parser grammar or grammar-side queries | `grammars/tree-sitter-svg/grammar.js`, `grammars/tree-sitter-svg/queries/` | Tree-sitter SVG source of truth                      |
| Change Zed extension behavior                 | `editors/zed-svg/`                                                         | Extension manifest, Rust wrapper, Zed queries        |
| Validate E2E feature behavior                 | `crates/svg-language-server/tests/*.rs`                                    | Spawns binary, speaks raw JSON-RPC                   |
| Change release automation                     | `.github/workflows/*.yml`, `dist-workspace.toml`                           | `release.yml` generated; publish workflow custom     |
| Check design intent / file maps               | `docs/plans/*.md`, `docs/specs/*.md`                                       | Dated plan/spec pairs with verification guidance     |
| Repro behavior manually                       | `samples/`                                                                 | Manual fixtures; not wired into automated test runs  |

## CODE MAP

| Symbol Area                                                     | Type        | Location                                | Role                                                      |
| --------------------------------------------------------------- | ----------- | --------------------------------------- | --------------------------------------------------------- |
| `run_stdio_server` and `SvgLanguageServer`                      | fn + struct | `crates/svg-language-server/src/lib.rs` | Wiring for hover/completion/diagnostics/format/color/defs |
| `check_all`                                                     | function    | `crates/svg-lint/src/rules/mod.rs`      | Lint walk + suppression application                       |
| catalog lookup funcs (`element`, `attribute`, `attributes_for`) | functions   | `crates/svg-data/src/lib.rs`            | Spec truth API consumed by LSP + lint                     |
| `format_with_options`, `format_with_host`                       | functions   | `crates/svg-format/src/lib.rs`          | Deterministic structural formatting + embedded delegation |
| `extract_colors_from_tree`                                      | function    | `crates/svg-color/src/extract.rs`       | Color ranges from SVG + inline CSS                        |
| `definition_target_at`                                          | function    | `crates/svg-references/src/lib.rs`      | Definition jump target resolution                         |
| `walk_tree`, `deepest_node_at`, `is_attribute_name_kind`        | functions   | `crates/svg-tree/src/lib.rs`            | Shared traversal + grammar-kind invariants                |

## CONVENTIONS

- Use `just` targets as source of truth for dev flow; `just verify` is the local
  preflight.
- Formatting is `dprint` first; Rust/TOML/justfile formatting is delegated
  through exec plugins.
- Tree-sitter parse-once reuse pattern in LSP: document state stores source +
  tree; leaf crates consume shared trees where possible.
- Workspace split is intentional: LSP crate integrates; leaf crates own domain
  logic and stay free of transport types.
- Release automation is split: `dist-workspace.toml` drives generated
  `release.yml`, while `publish-npm-oidc.yml` is hand-maintained.
- `docs/plans/*` and `docs/specs/*` are date-paired design history, not
  generated output.
- `grammars/tree-sitter-svg` is the canonical grammar in this repo. Keep parser
  node names, grammar queries, and editor query consumers aligned.
- `editors/zed-svg` is a workspace member, but its published grammar pin still
  needs an explicit revision bump after the grammar import exists on the remote.

## ANTI-PATTERNS (THIS PROJECT)

- Do not edit generated catalog output in `svg-data`; change `build.rs` or
  `data/*.json` instead.
- Do not assume one generic attribute node kind; tree-sitter-svg uses multiple
  typed attribute name kinds.
- Do not parse inside `<style>`/`<script>` as XML child elements; treat content
  as raw text/injections.
- Do not rely on deprecated `xlink:*` attrs (notably `xlink:href`) as preferred
  behavior.
- Do not skip UTF-16/byte conversion helpers when mapping tree-sitter ranges to
  LSP positions.
- Do not treat `samples/` as canonical formatting or style source.
- Do not edit a second checkout of `tree-sitter-svg` when changing grammar
  behavior; use `grammars/tree-sitter-svg`.
- Do not update Zed query files without checking whether grammar-side query
  files need the same node-name/capture update.

## UNIQUE STYLES

- Integration coverage is protocol-level and process-backed, not only API unit
  tests.
- Diagnostic suppression comments live in SVG source and are regression-tested.
- Formatter behavior is structural and deterministic, not whitespace-preserving
  pretty-print.
- Plans/specs are checked in as dated architecture history with explicit
  verification sections.

## COMMANDS

```bash
just format
just format-check
just lint
just test
just verify
just run-lsp
```

## NOTES

- No hosted CI config is checked in; `just verify` is the effective repo
  preflight.
- `samples/` is excluded from dprint; treat it as fixtures/examples only.
- Compat parsing logic exists both build-time (`svg-data`) and runtime
  (`svg-language-server`); keep behavior aligned when touched.
- `scripts/` is small and release-focused; command truth still lives in
  `justfile` and `docs/releasing.md`.
- Root `target/` can be large; ignore during repo exploration and docs
  generation.
