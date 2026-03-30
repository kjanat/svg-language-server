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

## ANTI-PATTERNS

- Do not parse document repeatedly per request; reuse cached tree.
- Do not assume `attribute_name` is the only attribute node kind.
- Do not force protocol behavior that ignores client capabilities without explicit reason.
- Do not touch leaf-crate internals from here; extend leaf APIs first.

## NOTES

- `src/lib.rs` is the orchestrator; keep helpers cohesive and feature-local rather than growing `main.rs`.
- Test suite is feature-split; keep new cases near the protocol area they exercise.
