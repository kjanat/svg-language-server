# SVG-DATA BUILD KNOWLEDGE BASE

## OVERVIEW

Build-script internals for compat fetch/cache, spec scraping, and deterministic Rust codegen.

## WHERE TO LOOK

| Task                      | Location      | Notes                                                                                                                                                                 |
| ------------------------- | ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Fetch + merge compat data | `bcd.rs`      | BCD + web-features cache/read/merge pipeline                                                                                                                          |
| Fetch spec descriptions   | `spec.rs`     | **ORPHANED** — not declared via `mod spec` in `../build.rs`; dead today, slated to revive for P3 descriptions (see `docs/specs/2026-06-03-spec-derivation-design.md`) |
| Emit Rust source          | `codegen.rs`  | String escaping + stable Rust literal generation                                                                                                                      |
| See orchestration entry   | `../build.rs` | Wires curated JSON + compat + spec text into output                                                                                                                   |

## CONVENTIONS

- Fail soft on network/cache issues: emit cargo warnings and keep generation usable with partial or empty compat data.
- Preserve offline mode semantics via `SVG_DATA_OFFLINE`.
- Keep generated output deterministic: stable ordering, stable escaping, no timestamped content.
- Spec scraping is heuristic by necessity; validate extracted text quality when changing selectors or truncation logic.

## ANTI-PATTERNS

- Do not turn transient fetch failures into hard build failures without strong reason.
- Do not assume one BCD path per attribute; global and element-specific entries are merged.
- Do not emit nondeterministic codegen output.
- Do not change cache file names or fetch URLs casually; build outputs and offline behavior depend on them.

## NOTES

- `spec.rs` is currently **orphaned** (no `mod spec;` in `../build.rs`); when revived
  it strips HTML with simple text heuristics, not a full HTML parser.
- `bcd.rs` merges deprecation/experimental/spec-url/baseline/browser-support signals conservatively across sources.
