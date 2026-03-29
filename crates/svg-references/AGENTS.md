# SVG-REFERENCES KNOWLEDGE BASE

## OVERVIEW

Shared symbol lookup crate for local `id`, CSS class, and custom property definitions/references across SVG and embedded CSS.

## WHERE TO LOOK

| Task                           | Location                                  | Notes                                   |
| ------------------------------ | ----------------------------------------- | --------------------------------------- |
| Resolve clicked target         | `src/lib.rs` `definition_target_at`       | SVG node dispatch + inline CSS fallback |
| Collect `id` definitions       | `src/lib.rs` `collect_id_definitions`     | Tree walk over SVG ids                  |
| Collect class/custom-prop defs | `src/lib.rs` stylesheet helpers           | CSS parse + span rebasing               |
| Extract inline stylesheets     | `src/lib.rs` `collect_inline_stylesheets` | Only real `<style>` raw text            |

## CONVENTIONS

- Public API stays leaf-crate pure: spans + definition enums, no LSP transport types.
- Deepest-node lookup must correct anonymous leaves to named parents before dispatch.
- Inline CSS is reparsed with `tree-sitter-css` and rebased back to SVG row/col space.
- Class references in `class="..."` and custom property references in `var(...)` both resolve here.

## ANTI-PATTERNS

- Do not treat arbitrary raw text as stylesheet content; only real `<style>` elements count.
- Do not collect attribute selectors such as `[class~=foo]` as class definitions.
- Do not lose absolute span mapping when converting CSS nodes back into SVG positions.
- Do not drop custom property handling from either SVG property names or CSS `var(...)` references.

## NOTES

- This crate is mostly single-file by design; split only if the public surface grows meaningfully.
