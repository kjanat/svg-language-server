# svg-data — Data Provenance

This document explains where the data in `svg-data` comes from, how it flows
through the five-layer pipeline, and how to regenerate it.

---

## Five-Layer Pipeline

```
Layer 0  curated catalog (design-time)
   ↓
Layer 1  source manifests  (data/sources/*.toml)
   ↓
Layer 2  spec snapshots    (data/specs/<SnapshotId>/)
   ↓
Layer 3  derived union     (data/derived/union/)
   ↓
Layer 4  compiled catalog  (build.rs → OUT_DIR/catalog.rs)
```

### Layer 0 — Curated Catalog

`data/elements.json` and `data/attributes.json` are hand-maintained design
documents that record which attributes each element accepts, along with
human-readable descriptions and content models.

**Role in the pipeline:** `data/elements.json` is read by `build.rs` and used
to augment the per-snapshot profile attribute mappings. For every element listed
in the curated catalog, any attribute in its `attrs` list that also has a record
in that snapshot's `attributes.json` is automatically included in the
`generated_attribute_names_for_profile()` function — even if the edge is absent
from `element_attribute_matrix.json`. This makes the pipeline self-healing: the
matrix is regenerated deterministically from the binary, so curated edges are
never lost.

**Bootstrap constraint (attribute records only):** An attribute is included in a
snapshot profile only if it has a record in that snapshot's `attributes.json`
(because value syntax and other metadata must come from somewhere). When a new
attribute is discovered, the one-time manual step is to add its record to the
relevant `data/specs/<Snapshot>/attributes.json` files. After that, the edge
is automatic.

`data/attributes.json` is not currently read by `build.rs` or the seed
generator — it covers the global/presentation attributes and serves as a
reference for the MDN URLs and value types that feed into future tooling.

### Layer 1 — Source Manifests

`data/sources/<id>.toml` — one file per spec snapshot, plus
`data/sources/foreign-references.toml` for external specs (svg-animations,
css-masking-1, filter-effects-1, etc.).

Each manifest declares:

- The URL or git commit that pins the source for reproducible regeneration.
- Per-input `authority = "primary" | "supporting"` classification.
- Optional `path` for git-backed inputs (per-file granularity).

These manifests drive provenance metadata embedded in every snapshot record.

### Layer 2 — Spec Snapshots

`data/specs/<SnapshotId>/` — one directory per canonical snapshot:

| Snapshot ID                | Source                                         |
| -------------------------- | ---------------------------------------------- |
| `Svg11Rec20030114`         | SVG 1.1 First Edition (W3C REC 2003-01-14)     |
| `Svg11Rec20110816`         | SVG 1.1 Second Edition (W3C REC 2011-08-16)    |
| `Svg2Cr20181004`           | SVG 2 Candidate Recommendation (2018-10-04)    |
| `Svg2EditorsDraft20250914` | SVG 2 Editor's Draft (svgwg commit `19482daf`) |

Each snapshot directory contains seven files:

| File                            | Contents                                                                               |
| ------------------------------- | -------------------------------------------------------------------------------------- |
| `snapshot.json`                 | Pinned source list and schema version                                                  |
| `elements.json`                 | Element records (names, titles, categories, content model, attribute list, provenance) |
| `attributes.json`               | Attribute records (names, titles, value syntax, animatability, provenance)             |
| `grammars.json`                 | Shared grammar definitions referenced by attribute value syntaxes                      |
| `categories.json`               | Element/attribute category memberships                                                 |
| `element_attribute_matrix.json` | Edges: which attributes apply to which elements, with requirement level                |
| `exceptions.json`               | Hand-curated exceptions overriding derived data                                        |
| `review.json`                   | Derived audit report (regenerated automatically; check-in tracks drift)                |

**Authoritative source for the build pipeline.** `build.rs` reads these files
(not `data/elements.json`) to generate the compiled catalog.

#### Bootstrap constraint

The seed generator (`examples/generate_snapshot_seed.rs`) uses
`attributes_for_with_profile()` from the *compiled binary*, which reads
`element_attribute_matrix.json`. This creates a fixed point: the seed generator
can only reproduce what the compiled binary already knows.

To add a new attribute-element edge, follow the bootstrap procedure below.

### Layer 3 — Derived Union

`data/derived/union/elements.json` and `data/derived/union/attributes.json`
record, for each element/attribute, which snapshots it is `present_in`.

`data/derived/overlays/<from>__<to>.json` record per-snapshot deltas (new,
changed, removed) between adjacent snapshots.

Generated by:

```sh
cargo run -p svg-data --example generate_derived_membership
```

### Layer 4 — Compiled Catalog

`build.rs` reads layers 2–3 and generates `OUT_DIR/catalog.rs`, which is
`include!`-d by `src/catalog.rs`. The generated file contains:

- `pub static ELEMENTS: &[ElementDef]`
- `pub static ATTRIBUTES: &[AttributeDef]`
- `pub fn generated_attribute_names_for_profile(snapshot, element) -> &[&str]`
- `pub fn generated_known_element_snapshots(name) -> Option<&[SpecSnapshotId]>`
- `pub fn generated_known_attribute_snapshots(name) -> Option<&[SpecSnapshotId]>`

---

## Offline Build Mode

Set `SVG_DATA_OFFLINE=1` when building to skip fetching remote BCD compat data:

```sh
SVG_DATA_OFFLINE=1 cargo build -p svg-data
```

When offline mode is enabled:

- `build.rs` skips the local svg-compat worker CLI attempt
- `build.rs` skips the remote fetch to `svg-compat.kjanat.com`
- If cached compat data exists locally (`OUT_DIR/svg-compat-data.json`), it is reused
- If no cache exists, the build continues without compat data (prints warning)
- The snapshot seed generators and examples are unaffected (they read only local checked-in files)

This is useful in CI/CD environments with no network access or when reproducibility from cached sources is required.

---

## Foreign Specs

SVG 2 defers some attribute definitions to external specifications.
`data/sources/foreign-references.toml` pins the exact versions used.

| Source ID          | Spec                     | Used for                                                                 |
| ------------------ | ------------------------ | ------------------------------------------------------------------------ |
| `svg-animations`   | SVG Animations           | Animation timing/value attributes (`begin`, `dur`, `end`, `keyTimes`, …) |
| `css-masking-1`    | CSS Masking              | `clip-path`, `mask`, `clipPathUnits`, `maskUnits`, `maskContentUnits`    |
| `filter-effects-1` | Filter Effects           | `filter` attribute                                                       |
| `compositing-1`    | Compositing and Blending | `operator`                                                               |
| `css-values-3`     | CSS Values               | `<length>`, `<number-or-percentage>`                                     |
| `css-color-4`      | CSS Color                | `<color>`                                                                |
| `wai-aria-1.1`     | WAI-ARIA                 | `role`, `aria-*` attributes                                              |

---

## Regeneration Commands

### Normal round-trip (no new edges)

Re-run all four seed generators and regen the derived union:

```sh
for snap in Svg11Rec20030114 Svg11Rec20110816 Svg2Cr20181004 Svg2EditorsDraft20250914; do
  cargo run -p svg-data --example generate_snapshot_seed -- $snap
done
cargo run -p svg-data --example generate_derived_membership
```

### Adding a new attribute-element edge

When a spec gap is discovered (an attribute that belongs to an element but is
absent from the snapshot data):

1. **Confirm** the attribute is listed in `data/elements.json` under the
   element's `attrs` list (add it if not).

2. **Add the attribute record** to each relevant snapshot's `attributes.json`
   (e.g. `data/specs/Svg2Cr20181004/attributes.json`). This is required because
   `build.rs` only promotes a curated edge to a snapshot profile when the
   attribute already has a record in that snapshot — the record is where value
   syntax and other metadata live.

   Use the same JSON structure as adjacent records in the file. At minimum:
   ```json
   {
     "name": "my-attr",
     "title": "...",
     "value_syntax": { "kind": "opaque", "display": "<value>", "reason": "..." },
     "default_value": { "kind": "none" },
     "animatable": "unspecified",
     "provenance": [...]
   }
   ```

3. **Run the normal round-trip** — the curated catalog drives edge generation
   automatically:
   ```sh
   cargo build -p svg-data --examples
   for snap in Svg11Rec20030114 Svg11Rec20110816 Svg2Cr20181004 Svg2EditorsDraft20250914; do
     cargo run -p svg-data --example generate_snapshot_seed -- $snap
   done
   cargo run -p svg-data --example generate_derived_membership
   ```

4. **Verify** tests pass:
   ```sh
   cargo test -p svg-data
   ```

> **Note:** Manual injection of edges into `element_attribute_matrix.json` is
> no longer needed. `build.rs` reads `data/elements.json` and augments the
> profile attribute mappings automatically, so any element→attr pair listed in
> the curated catalog will be reproduced by the seed generator as long as the
> attribute has a record in that snapshot's `attributes.json`.

### Adding a new snapshot

1. Create a new `SpecSnapshotId` variant in `src/types.rs`.
2. Add a source manifest in `data/sources/<id>.toml`.
3. Add the snapshot to `canonical_snapshots()` in `build.rs` and update
   `LATEST_SNAPSHOT` if it is the newest.
4. Run `generate_snapshot_seed -- <new-snapshot-id>` to seed the initial data.
5. Run `generate_snapshot_review` if applicable.
6. Run `generate_derived_membership`.

---

## Placeholder Attributes

`data/placeholder_attribute_names.txt` lists upstream BCD/web-features IDs
that do not correspond to real serialized SVG attribute names (e.g.
`data_attributes`, `external_uri`). Both `build.rs` and
`examples/generate_snapshot_seed.rs` use `include_str!` on this file so the
blocklist stays in sync automatically.

---

## Data Quality

`tests/snapshot_reviews.rs` contains two regression tests:

- `checked_in_snapshot_reviews_match_derived_audit` — verifies that the
  checked-in `review.json` matches the deterministic audit output, and that no
  unresolved issues remain.
- `every_provenance_source_id_resolves_to_a_pinned_input` — verifies that every
  `source_id` referenced in provenance metadata resolves to a pinned source in
  `snapshot.json`.

`tests/svg11_first_snapshot.rs` and its siblings verify that the checked-in
snapshot data round-trips through the seed generator without drift.

`tests/spec_required_edges.rs` asserts that known spec-authoritative
element→attribute edges are present in the compiled catalog. Add entries here
whenever a spec gap is confirmed and fixed.
