# Full Snapshot SVG Data Implementation Plan

> Build snapshot-first SVG spec data, then derive union data and version diffs from reviewed facts.

**Goal:** Replace sparse profile exceptions with full per-snapshot normalized SVG spec data, structured grammar coverage, provenance, and derived overlays.

**Architecture:** Canonical data is checked in per snapshot under `crates/svg-data/data/specs/<snapshot-id>/`. Fetch manifests pin authority sources. Parsers normalize snapshot facts. Review reports validate each snapshot. Derived union/overlay artifacts are generated only after all snapshots pass review.

**Spec:** `docs/specs/2026-04-09-full-snapshot-svg-data-design.md`

---

## File Map

| File or Directory                                                        | Responsibility                                        |
| ------------------------------------------------------------------------ | ----------------------------------------------------- |
| `crates/svg-data/data/sources/*.toml`                                    | Pinned source manifests, checksums, fetch strategy    |
| `crates/svg-data/data/specs/<snapshot-id>/snapshot.json`                 | Snapshot metadata and pinned source references        |
| `crates/svg-data/data/specs/<snapshot-id>/elements.json`                 | Canonical element facts for one snapshot              |
| `crates/svg-data/data/specs/<snapshot-id>/attributes.json`               | Canonical attribute facts for one snapshot            |
| `crates/svg-data/data/specs/<snapshot-id>/grammars.json`                 | Structured grammar AST and typed refs                 |
| `crates/svg-data/data/specs/<snapshot-id>/categories.json`               | Element and attribute category membership             |
| `crates/svg-data/data/specs/<snapshot-id>/element_attribute_matrix.json` | Explicit applicability edges                          |
| `crates/svg-data/data/specs/<snapshot-id>/exceptions.json`               | Curated prose-only/source-bug corrections             |
| `crates/svg-data/data/specs/<snapshot-id>/review.json`                   | Completeness and accuracy audit report                |
| `crates/svg-data/build.rs`                                               | Deterministic codegen from reviewed snapshot truth    |
| `crates/svg-data/src/lib.rs`                                             | Runtime API reading generated snapshot-derived tables |
| `crates/svg-data/src/types.rs`                                           | Public catalog and grammar types                      |
| `crates/svg-lint/src/lib.rs`                                             | Profile-aware lint entry points using reviewed data   |
| `crates/svg-lint/src/rules/mod.rs`                                       | Diagnostics derived from snapshot truth               |
| `crates/svg-language-server/src/completion.rs`                           | Profile-aware completions from reviewed data          |
| `crates/svg-language-server/src/hover.rs`                                | Lifecycle and grammar-aware hover text                |
| `crates/svg-language-server/tests/*.rs`                                  | Integration regressions across snapshots              |

---

## Phase 1

- [ ] Define the pinned source manifests and normalized snapshot schema.
- [ ] Build shared extraction and provenance helpers.

## Phase 2

- [ ] Ingest `Svg11Rec20030114` into normalized checked-in snapshot data.
- [ ] Ingest `Svg11Rec20110816` into normalized checked-in snapshot data.

## Phase 3

- [ ] Ingest `Svg2Cr20181004` with TR-first authority and explicit foreign refs.
- [ ] Ingest `Svg2EditorsDraft20250914` from the pinned `w3c/svgwg` commit and structured definitions files.

## Phase 4

- [ ] Normalize SVG-owned value grammar across all snapshots.
- [ ] Add typed pinned references for foreign grammars and external modules.

## Phase 5

- [ ] Generate per-snapshot review reports and audit checks.
- [ ] Derive union data and version overlays from reviewed snapshot truth only.

## Phase 6

- [ ] Switch `svg-data` codegen/runtime APIs to the reviewed data pipeline.
- [ ] Rewire lint and LSP consumers after parity checks pass.
- [ ] Add matrix tests proving completeness and profile-aware behavior.

---

## Verification

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets --all-features -- -D clippy::all`
- `cargo test -p svg-data`
- `cargo test -p svg-lint`
- `cargo test -p svg-language-server`

Additional review gates:

- regenerated element sets match authoritative element indices
- regenerated attribute sets match authoritative attribute indices
- applicability matrix has no unsourced edges
- grammar coverage leaves no SVG-owned syntax opaque
- per-snapshot review reports are checked in and clean
