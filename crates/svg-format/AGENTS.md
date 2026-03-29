# SVG-FORMAT KNOWLEDGE BASE

## OVERVIEW

Formatter crate with both library API and `svg-format` CLI. Rebuilds SVG structure deterministically from the parse tree. Supports embedded content delegation, configurable blank-line policy, text-content whitespace modes, and comment-based ignore directives.

## WHERE TO LOOK

| Task                        | Location                                               | Notes                                                                      |
| --------------------------- | ------------------------------------------------------ | -------------------------------------------------------------------------- |
| Public options and defaults | `src/lib.rs`                                           | `FormatOptions` + enums (`BlankLines`, `TextContentMode`, etc.)            |
| Core formatting flow        | `src/lib.rs` `format_with_options`, `format_with_host` | Parse, ignore-file check, formatter dispatch                               |
| Tree walker                 | `src/lib.rs` `format_node`, `format_element_like`      | Node dispatch, sibling iteration, gap/blank-line emission                  |
| Ignore directives           | `src/lib.rs` `handle_ignore`, `is_ignore_directive`    | AST-based comment detection; line/range/file; configurable prefixes        |
| Blank-line handling         | `src/lib.rs` `emit_gap`, `source_blank_lines`          | Remove/Preserve/Truncate/Insert; comments attach downward in Insert mode   |
| Text content modes          | `src/lib.rs` `write_text_node`                         | Collapse (whitespace→single space), Maintain (preserve relative), Prettify |
| Embedded content delegation | `src/lib.rs` `try_format_embedded_text`, etc.          | Callback for `<style>` (CSS), `<script>` (JS), `<foreignObject>` (HTML)    |
| Ignore-file detection       | `src/lib.rs` `has_ignore_file_comment`                 | Recursive AST walk, not substring scan                                     |
| Source span preservation    | `src/lib.rs` `write_source_span`                       | Verbatim byte copying for ignored regions                                  |
| CLI flags and exit behavior | `src/main.rs`                                          | `clap` layer over lib API; all options exposed as CLI flags                |

## KEY DESIGN DECISIONS

- `FormatOptions` is `Clone` but not `Copy` (due to `ignore_prefixes: Vec<String>`).
- `<style>`/`<script>` content is `raw_text` in tree-sitter-svg, NOT `style_text_*`. The `style_text_*` node kinds are for inline style attribute values.
- `ignore-start` emits its leading gap via `emit_gap` (so `BlankLines` mode applies), then writes only the comment bytes. Content inside the range uses `write_source_span` from `prev_end` to preserve gaps verbatim.
- `ignore-next` writes only the node bytes (via `write_source_span(start, end)`), not from `prev_end`, because the gap before the ignored node was already emitted by the previous `write_line`.
- Insert mode uses `prev_was_comment` to suppress blank lines after comments (comments attach downward to the element they annotate).

## CONVENTIONS

- Output is structural and deterministic: canonical attr ordering, stable tag layout, stable blank-line policy.
- Parse failure or syntax error returns original source instead of guessing.
- Ignore directives are recognized from comment nodes, not raw substring scans.
- New formatter options need wiring in three places: `FormatOptions` + formatter logic + CLI flags + tests.
- Every behavioral assumption must have a test. Do not commit formatter changes without verifying idempotency.
- Run `just ci` (not just `cargo test -p svg-format`) before committing.

## ANTI-PATTERNS

- Do not treat formatter as whitespace-preserving pretty-print (except inside ignore ranges).
- Do not rewrite embedded CSS/JS/HTML unless the host callback explicitly returns formatted content.
- Do not add option fields in `src/lib.rs` without mirroring them in `src/main.rs`.
- Do not change ordering or ignore semantics without exact regression tests.
- Do not use raw substring matching on source for directive detection — use tree-sitter comment nodes.
- Do not write gaps from `prev_end` for `ignore-next` elements — the preceding `write_line` already emitted a newline.

## NOTES

- `src/lib.rs` is intentionally large; keep changes near the feature's existing helper/tests.
- Default indentation is tabs even though `indent_width` defaults to `2` for space mode.
- `ignore_prefixes` defaults to `["svg-format"]`; the dprint plugin adds `"dprint"` as a second prefix.
- Whitespace-only text nodes are skipped outside ignore ranges but preserved inside them.
