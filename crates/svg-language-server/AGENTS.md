# SVG-LANGUAGE-SERVER KNOWLEDGE BASE

## OVERVIEW

Binary crate that owns JSON-RPC/LSP protocol flow, document state, and fanout into lint/format/color/references/data crates.

## WHERE TO LOOK

| Task                            | Location                                                    | Notes                                              |
| ------------------------------- | ----------------------------------------------------------- | -------------------------------------------------- |
| Document lifecycle              | `src/main.rs` (`did_open`, `did_change`, `update_document`) | Parse once; reuse tree across features             |
| Hover/completion                | `src/main.rs` hover/completion handlers                     | Reads `svg-data` metadata + runtime compat overlay |
| Diagnostics publish             | `src/main.rs` `publish_lint_diagnostics`                    | Bridges lint output to LSP diagnostics             |
| Formatting endpoint             | `src/main.rs` formatting handler                            | Maps editor options into `svg-format` options      |
| Colors endpoint                 | `src/main.rs` color handlers                                | Calls `svg-color` extract/presentation             |
| Definitions/references endpoint | `src/main.rs` goto methods                                  | Calls `svg-references` target lookup               |
| E2E protocol tests              | `tests/integration.rs`                                      | Spawns process, tests real protocol payloads       |

## CONVENTIONS

- Keep `DocumentState` as source of truth: raw source + parsed tree cached per document.
- Use byte<->UTF-16 helpers for every range conversion; never hand-roll offsets.
- Client-facing labels/messages are effectively API; integration tests assert them.
- Runtime compat fetch is additive metadata; behavior must degrade cleanly when fetch fails.

## ANTI-PATTERNS

- Do not parse document repeatedly per request; reuse cached tree.
- Do not assume `attribute_name` is the only attribute node kind.
- Do not force protocol behavior that ignores client capabilities without explicit reason.
- Do not touch leaf-crate internals from here; extend leaf APIs first.

## NOTES

- Main file is intentionally large orchestrator; keep helpers cohesive and feature-local.
- Test harness uses manual `Content-Length` framing; keep that when adding integration cases.
