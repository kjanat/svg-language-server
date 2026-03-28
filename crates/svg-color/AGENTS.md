# SVG-COLOR KNOWLEDGE BASE

## OVERVIEW

Color subsystem for SVG/CSS extraction and LSP color presentations, including custom props and `color-mix(...)` resolution in embedded style blocks.

## WHERE TO LOOK

| Task                         | Location                     | Notes                                                |
| ---------------------------- | ---------------------------- | ---------------------------------------------------- |
| Parse supported color syntax | `src/parse.rs`               | Token-level parser for supported CSS color functions |
| Extract ranges from SVG/CSS  | `src/extract.rs`             | Walks tree-sitter SVG/CSS nodes + range mapping      |
| Generate presentation labels | `src/present.rs`             | Hex/rgb/hsl/named output formatting                  |
| Named color lookup data      | `src/named_colors.rs`        | Canonical 148-name table + lookup                    |
| Public API surface           | `src/lib.rs`, `src/types.rs` | Exports used by LSP crate                            |

## CONVENTIONS

- Keep parse/extract/presentation function support aligned; add format in all layers.
- Preserve byte-range precision; extraction ranges feed direct editor replacements.
- Named-color ordering assumptions in `present.rs` must stay synced with `named_colors.rs`.
- Favor deterministic labels (stable decimal formatting, stable ordering).

## ANTI-PATTERNS

- Do not add parser support without extractor + completion compatibility checks.
- Do not collapse CSS and SVG node-kind handling into one naive path.
- Do not emit named colors for semi-transparent results.
- Do not skip regression tests for mixed inline-style + attribute extraction.

## NOTES

- This crate is shared by LSP methods; keep allocations/branches reasonable for per-keystroke usage.
