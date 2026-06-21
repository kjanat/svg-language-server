# SVG LANGUAGE PACK KNOWLEDGE

## OVERVIEW

Zed-side SVG language behavior: config + query captures consumed by the
extension.

## WHERE TO LOOK

| Task                        | Location                  | Notes                                                      |
| --------------------------- | ------------------------- | ---------------------------------------------------------- |
| Language registration knobs | `config.toml`             | `grammar = "svg"`, bracket behavior, auto-close node names |
| Highlight captures          | `highlights.scm`          | Keep node names synced with grammar                        |
| Injections                  | `injections.scm`          | CSS/JS/HTML injection routing                              |
| Indent behavior             | `indents.scm`             | Must handle recovery nodes like `erroneous_end_tag`        |
| Tag/symbol navigation       | `outline.scm`, `tags.scm` | Keep captures stable for editor nav                        |

## CONVENTIONS

- Query files mirror grammar-side names (`highlights`, `injections`, `locals`,
  `tags`, etc.).
- Drift from `grammars/tree-sitter-svg/queries/*.scm` is allowed only when
  required by Zed behavior; document why in commit/PR text.
- `jsx_tag_auto_close` node names in `config.toml` must stay exact (`start_tag`,
  `end_tag`, `element`, `name`, `erroneous_end_tag`).
- Keep capture namespaces editor-friendly (`@tag`, `@attribute`,
  `@punctuation.delimiter`, etc.).

## ANTI-PATTERNS

- Renaming/using stale node names in queries (can prevent SVG language load).
- Updating grammar queries without checking these consumer queries.
- Treating path data captures as generic strings; preserve structured captures.

## COMMANDS

```bash
# run from repository root
export TREE_SITTER_SVG="$PWD/grammars/tree-sitter-svg"

# run from repo root; these fail on stale node names in Zed-side queries
tree-sitter highlight \
  --scope source.svg \
  --grammar-path "$TREE_SITTER_SVG" \
  --query-paths "$PWD/editors/zed-svg/languages/svg/highlights.scm" \
  "$TREE_SITTER_SVG/test/highlight/tags.svg"

tree-sitter query "$PWD/editors/zed-svg/languages/svg/outline.scm" \
  --scope source.svg \
  --grammar-path "$TREE_SITTER_SVG" \
  "$TREE_SITTER_SVG/test/highlight/tags.svg"

tree-sitter query "$PWD/editors/zed-svg/languages/svg/indents.scm" \
  --scope source.svg \
  --grammar-path "$TREE_SITTER_SVG" \
  "$TREE_SITTER_SVG/test/highlight/tags.svg"
```

## NOTES

- Known breakage examples live in [`DISCOVERIES.md`] at the extension root.
- `cargo check` only validates the Rust wrapper in `src/lib.rs`; stale `.scm`
  node names show up only in query-aware tools or in Zed at runtime.
- Use `grammars/tree-sitter-svg` as the local grammar source when running the
  commands above from the repository root.

[`DISCOVERIES.md`]: ../../DISCOVERIES.md
