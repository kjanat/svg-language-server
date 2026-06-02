# SVG-LANGUAGE-SERVER KNOWLEDGE BASE

## OVERVIEW

Library-first binary crate that owns JSON-RPC/LSP protocol flow, document state, and fanout into lint/format/color/references/data/tree crates.

## WHERE TO LOOK

| Task                            | Location                                    | Notes                                               |
| ------------------------------- | ------------------------------------------- | --------------------------------------------------- |
| Server entry + document state   | `src/lib.rs`                                | `run_stdio_server`, `DocumentState`, request wiring |
| Hover/completion                | `src/hover.rs`, `src/completion.rs`         | Reads `svg-data` metadata + runtime compat overlay  |
| Diagnostics publish             | `src/diagnostics.rs`, `src/code_actions.rs` | Bridges lint output to LSP diagnostics              |
| Formatting endpoint             | `src/lib.rs` formatting request handler     | Maps editor options into `svg-format` options       |
| Colors endpoint                 | `src/lib.rs` color handlers                 | Calls `svg-color` extract/presentation              |
| Definitions/references endpoint | `src/definition.rs`, `src/stylesheets.rs`   | URI rebasing + `svg-references` target lookup       |
| UTF-16 / byte mapping           | `src/positions.rs`                          | Never hand-roll offsets                             |
| E2E protocol tests              | `tests/*.rs`, `tests/support/mod.rs`        | Spawns process, tests real protocol payloads        |

## CONVENTIONS

- Keep `DocumentState` as source of truth: raw source + parsed tree cached per document.
- Use byte<->UTF-16 helpers for every range conversion; never hand-roll offsets.
- Client-facing labels/messages are effectively API; integration tests assert them.
- Runtime compat fetch is additive metadata; behavior must degrade cleanly when fetch fails.
- `src/main.rs` is a thin wrapper; substantive behavior belongs in `src/lib.rs` and feature modules.
- Profile-aware features take a `svg_data::SpecSnapshotId` parameter; route every
  catalog-driven lookup through that profile so completions, hover, and
  diagnostics agree on a single snapshot per request.

## PROFILE-AWARE COMPLETION & HOVER

The active spec snapshot (SVG 1.1 vs SVG 2 vs an editor's draft) shapes which
elements, attributes, and attribute values surface to the editor. Resolution
flows top-down:

1. `ProfileConfig` (`src/lib.rs`) holds the workspace-level resolved profile
   plus a `force` flag, derived from `svg.profile` / `svg.force_profile`
   settings via `resolve_profile_config`.
2. Per request, `effective_profile_for(doc)` consults
   `svg_lint::effective_profile`, which can downgrade to SVG 1.1 when the
   document's root `<svg version="1.1">` says so (unless `force` is set).
3. The effective `SpecSnapshotId` is threaded into `completion_from_context`
   and `hover_for_position`, which forwards it into:
   - `attribute_completion_items`, `child_element_completion_items`,
     `root_element_completion_items` → filter via `attribute_for_profile` /
     `element_for_profile` so attributes/elements unsupported in the active
     snapshot disappear.
   - `value_completions` (this PR) → resolves attribute value lists through
     `AttributeDef::values_for_profile`, so SVG 1.1 `display` keeps the CSS2
     keywords (`run-in`/`compact`/`marker`) that the union default drops.
   - `format_attribute_hover_with_profile` /
     `format_element_hover_with_profile` → renders value constraints,
     profile-lifecycle status (deprecated/removed/experimental), and
     compat verdicts for the active snapshot.

When adding a new completion or hover surface, take `SpecSnapshotId` as a
parameter rather than reading a default, and prefer `AttributeDef::values_for_profile`
over `&attr.values` so per-snapshot overrides keep surfacing.

## ANTI-PATTERNS

- Do not parse document repeatedly per request; reuse cached tree.
- Do not assume `attribute_name` is the only attribute node kind.
- Do not force protocol behavior that ignores client capabilities without explicit reason.
- Do not touch leaf-crate internals from here; extend leaf APIs first.
- Do not read raw `AttributeDef::values` or `attributes()` from catalog-driven
  completion/hover paths; always resolve through the active profile so
  per-snapshot overrides surface.

## NOTES

- `src/lib.rs` is the orchestrator; keep helpers cohesive and feature-local rather than growing `main.rs`.
- Test suite is feature-split; keep new cases near the protocol area they exercise.
