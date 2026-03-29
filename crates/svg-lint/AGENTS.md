# SVG-LINT KNOWLEDGE BASE

## OVERVIEW

Structural SVG diagnostics crate. Walks parsed trees, applies suppression directives, and reports byte-accurate diagnostics for the LSP.

## WHERE TO LOOK

| Task                         | Location       | Notes                                         |
| ---------------------------- | -------------- | --------------------------------------------- |
| Public lint entrypoints      | `src/lib.rs`   | `lint`, `lint_tree`                           |
| Rule engine / tree walk      | `src/rules.rs` | Suppression collection + element walk         |
| Diagnostic model             | `src/types.rs` | Codes, severity, payload                      |
| Suppression regression tests | `src/lib.rs`   | File, next-line, and unused suppression cases |

## CONVENTIONS

- Prefer `lint_tree` in callers that already own a parsed tree.
- The rule pipeline pre-collects suppressions and defined ids before walking elements.
- Foreign-namespace content under `foreignObject` is exempt from normal SVG child checks.
- Messages and codes are user-facing contract; LSP and integration tests depend on them.

## ANTI-PATTERNS

- Do not emit `InvalidChild` for nodes already flagged as `UnknownElement`.
- Do not treat XML infrastructure attrs (`xmlns`, `xml:*`) as unknown SVG attrs.
- Do not narrow the attribute-name kind allowlist without checking other grammar consumers.
- Do not break file-level, next-line, or unused-suppression semantics.

## NOTES

- `src/lib.rs` tests are the main semantic regression suite.
- Missing-reference diagnostics depend on definition collection staying aligned with `svg-references`.
