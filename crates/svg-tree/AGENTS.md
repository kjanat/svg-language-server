# SVG-TREE KNOWLEDGE BASE

## OVERVIEW

Shared tree-sitter helper crate. Central place for traversal, ancestor lookup, deepest-node queries, and SVG attribute-name kind detection.

## WHERE TO LOOK

| Task                         | Location     | Notes                                             |
| ---------------------------- | ------------ | ------------------------------------------------- |
| Walk an SVG tree             | `src/lib.rs` | `walk_tree`; cursor-based DFS, no per-node allocs |
| Resolve cursor target        | `src/lib.rs` | `deepest_node_at`; fallback is root node          |
| Climb to semantic parent     | `src/lib.rs` | `find_ancestor_any`, `has_ancestor`               |
| Grab direct child by kind    | `src/lib.rs` | `child_of_kind`; first direct child only          |
| Keep attr-kind logic in sync | `src/lib.rs` | `is_attribute_name_kind`; typed suffixes matter   |

## CONVENTIONS

- Keep this crate transport-free and domain-light; it exists to dedupe tree-sitter mechanics shared by multiple crates.
- `deepest_node_at` may return anonymous leaves; callers often still need to climb to a named or semantic parent.
- Attribute-name matching must include both `attribute_name` and `*_attribute_name` grammar variants.
- Prefer adding shared traversal helpers here over reimplementing them in leaf crates.

## ANTI-PATTERNS

- Do not assume plain `attribute_name` covers all SVG attributes.
- Do not duplicate node-walk or ancestor helpers across crates without a sync plan.
- Do not add LSP-, lint-, or formatter-specific policy here; keep helpers generic.
- Do not change traversal semantics casually; multiple crates depend on exact behavior.

## NOTES

- Small crate, high blast radius: `svg-language-server`, `svg-lint`, `svg-color`, and `svg-references` all rely on these helpers.
