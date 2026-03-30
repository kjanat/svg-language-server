# SVG-LANGUAGE-SERVER TESTS KNOWLEDGE BASE

## OVERVIEW

Feature-split protocol tests that build the real server binary, speak raw JSON-RPC over stdio, and assert user-facing LSP behavior.

## STRUCTURE

```text
tests/
├── support/mod.rs              # subprocess harness + framing helpers
├── completions.rs              # completion protocol cases
├── definitions_and_hover.rs    # hover + goto-definition cases
├── diagnostics_and_actions.rs  # publishDiagnostics + code actions
└── colors_and_formatting.rs    # color + formatting flows
```

## WHERE TO LOOK

| Task                          | Location                     | Notes                                            |
| ----------------------------- | ---------------------------- | ------------------------------------------------ |
| Start real server subprocess  | `support/mod.rs`             | Builds `svg-language-server`, spawns stdio child |
| Send/read JSON-RPC            | `support/mod.rs`             | Manual `Content-Length` framing                  |
| Wait for push notifications   | `diagnostics_and_actions.rs` | `publishDiagnostics` is async; drain carefully   |
| Add completion coverage       | `completions.rs`             | Inline SVG strings + cursor offsets              |
| Add hover/definition coverage | `definitions_and_hover.rs`   | End-to-end LSP assertions                        |
| Add color/format coverage     | `colors_and_formatting.rs`   | Capability + request/response checks             |

## CONVENTIONS

- Keep tests protocol-level: exercise requests/notifications against the spawned binary, not internal functions.
- Prefer inline SVG snippets over fixture files unless the sample is truly large or reused.
- Treat labels, codes, and capability payloads as API; assert concrete user-facing values.
- Drain or filter queued notifications carefully before asserting on later responses.

## ANTI-PATTERNS

- Do not replace the harness with mocked transport for behavior that should stay end-to-end.
- Do not assume responses arrive in strict request order when notifications can interleave.
- Do not add fragile cursor offsets without tying them to source search when practical.
- Do not mute stderr/stdout protocol details in the harness without understanding failure modes.

## NOTES

- `support/mod.rs` caches the built binary path with `OnceLock`.
- Current docs/plans may still mention an older `tests/integration.rs`; current suite is split by feature.
