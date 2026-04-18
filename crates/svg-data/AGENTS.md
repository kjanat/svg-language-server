# SVG-DATA KNOWLEDGE BASE

## OVERVIEW

Build-script generated SVG catalog crate. Produces baked metadata used by lint + hover/completion (no runtime file IO).

## WHERE TO LOOK

| Task                       | Location                     | Notes                                              |
| -------------------------- | ---------------------------- | -------------------------------------------------- |
| Add/adjust catalog fields  | `src/types.rs`, `src/lib.rs` | Runtime API and types exposed to consumers         |
| Change generation pipeline | `build.rs`                   | Pulls/merges MDN BCD + web-features + curated maps |
| Baseline compat parsing    | `build.rs` baseline helpers  | Keep shape compatible with runtime overlay logic   |
| Element category logic     | `src/categories.rs`          | Content-model/category constraints                 |
| BCD path handling          | `src/bcd.rs`                 | Attribute/element compat extraction                |
| Curated seed data          | `data/*.json`                | Inputs for generation bootstrap                    |

## CONVENTIONS

- Generated `src/catalog.rs` is output artifact; source of truth is pipeline + input data.
- Prefer deterministic generation and stable ordering to keep diffs reviewable.
- Preserve support for offline builds (`SVG_DATA_OFFLINE`) when touching network fetch behavior.
- Exposed APIs (`element`, `attribute`, `attributes_for`, `allowed_children`) are shared contract for multiple crates.

## ANTI-PATTERNS

- Do not hand-edit generated entries in `src/catalog.rs`.
- Do not assume BCD has one canonical compat path; check element-specific and global attribute paths.
- Do not drop `<style>`/`<script>` special handling from categories/content model logic.
- Do not introduce non-deterministic generation steps.

## NOTES

- Changes here ripple into lint diagnostics and LSP docs/completions; verify both after edits.

## TODO

- [ ] **MDN BCD per-file lookups (LSP runtime overlay):** `@mdn/browser-compat-data` ships
      individual JSON files per feature (e.g. `svg/elements/circle.json`), accessible via
      jsdelivr. The svg-compat worker already processes the full BCD bundle at build time;
      the LSP could additionally lazy-fetch individual per-feature files at runtime for
      fresher overrides, complementing the existing `RuntimeCompat` startup fetch. Any
      network access must remain opt-in so offline builds continue to work.
  - [ ] Eval. if we want to implement this in the first place.
