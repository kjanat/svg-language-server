# DOCS KNOWLEDGE BASE

## OVERVIEW

Design history and implementation plans. Files are dated, paired, and meant to capture intent, scope, and verification for major features.

## STRUCTURE

```text
docs/
├── plans/  # task checklists, file maps, verification commands
└── specs/  # architecture, scope, testing, non-goals
```

## WHERE TO LOOK

| Task                              | Location                                | Notes                                      |
| --------------------------------- | --------------------------------------- | ------------------------------------------ |
| Plan feature work                 | `plans/*.md`                            | Checkbox steps + concrete file map         |
| Recover architecture intent       | `specs/*.md`                            | Goal, constraints, non-goals               |
| Match a plan to its design doc    | same date prefix in `plans/` + `specs/` | Files are meant to travel together         |
| Find manual verification commands | `plans/*.md`                            | Usually more concrete than prose summaries |

## CONVENTIONS

- Filenames are date-prefixed: `YYYY-MM-DD-*`.
- Plans point at a matching spec and use checkbox task lists.
- Specs describe goal, architecture, testing, and non-goals before implementation.
- Verification sections are historical guidance; current command truth still lives in `justfile`.

## ANTI-PATTERNS

- Do not edit a plan without checking the matching spec.
- Do not assume docs still match code; verify against source when behavior changed.
- Do not treat plan snippets as copy-paste safe without checking current APIs and file paths.

## NOTES

- Current docs capture two milestones: the initial color-only LSP and the later full SVG intelligence expansion.
