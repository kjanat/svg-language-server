# CRATES KNOWLEDGE BASE

## OVERVIEW

Workspace package boundary. Keep crate responsibilities narrow; cross-crate coupling goes through explicit APIs.

## STRUCTURE

```text
crates/
├── svg-language-server/  # integration hub, LSP protocol + feature wiring
├── svg-data/             # generated spec/compat catalog
├── svg-lint/             # diagnostics engine
├── svg-format/           # formatter lib + CLI
├── svg-color/            # color parsing/extraction/presentation
├── svg-references/       # definitions/references for ids/classes/custom props
└── svg-tree/             # shared tree-sitter traversal/query helpers
```

## WHERE TO LOOK

| Task                     | Location                                | Notes                                    |
| ------------------------ | --------------------------------------- | ---------------------------------------- |
| Add new LSP feature      | `crates/svg-language-server/src/lib.rs` | Wire handler + calls into leaf crates    |
| Add spec metadata        | `crates/svg-data/build.rs`              | Regenerate catalog from upstream data    |
| Add lint rule            | `crates/svg-lint/src/rules/mod.rs`      | Rules + suppression comments             |
| Tweak formatting policy  | `crates/svg-format/src/lib.rs`          | Structural output decisions              |
| Add color format support | `crates/svg-color/src/parse.rs`         | Keep parser/extractor/completion in sync |
| Change definition lookup | `crates/svg-references/src/lib.rs`      | Symbol extraction and resolution         |
| Change tree helpers      | `crates/svg-tree/src/lib.rs`            | Shared grammar/traversal invariants      |

## CONVENTIONS

- `svg-language-server` is the only integration hub; prefer leaf crates for domain logic.
- `svg-data` is generated at build-time; consumers treat it as read-only API.
- Shared parser stack is tree-sitter-based; shared helpers live in `svg-tree`, and node kind handling must match grammar names exactly.
- Keep API shape stable across crates; integration tests validate user-facing strings and protocol payloads.

## ANTI-PATTERNS

- Do not leak LSP transport types into leaf crates.
- Do not duplicate parser-kind allowlists across crates without explicit sync plan.
- Do not add runtime network fetches outside explicit compat-refresh path.
- Do not bypass crate boundaries by reaching into another crate's private implementation details.
