# Full Snapshot SVG Data — Design Spec

> Ingest pinned SVG snapshots as complete structured data with provenance, value grammar, and derived version overlays.

## Goal

Turn `svg-data` into a snapshot-first catalog instead of a union catalog with sparse exceptions. Each tracked SVG spec snapshot should be stored as normalized, modular, reviewable data before any union view, lifecycle derivation, or runtime overlay is generated.

Tracked snapshots:

- `Svg11Rec20030114`
- `Svg11Rec20110816`
- `Svg2Cr20181004`
- `Svg2EditorsDraft20250914`

## Primary Requirements

- Store each snapshot independently as canonical checked-in data.
- Preserve provenance for every extracted fact.
- Include value grammar as first-class structured data, not only display strings.
- Keep derived union views and version diffs as generated artifacts only.
- Run per-snapshot completeness and accuracy review before rewiring consumers.
- Keep build-time generation deterministic and offline-friendly after fetch.

## Source Strategy

### SVG 1.1 First Edition

- Authority: W3C TR snapshot, `eltindex.html`, `attindex.html`, dated flattened DTD.
- Role: full historical snapshot, not a derived diff.

### SVG 1.1 Second Edition

- Authority: W3C TR snapshot, zip bundle, `eltindex.html`, `attindex.html`, dated flattened DTD.
- Role: full stable baseline and validation spine.

### SVG 2 Candidate Recommendation

- Authority: W3C TR snapshot.
- GitHub history may assist extraction, but TR remains ground truth.
- External module references must remain explicit.

### SVG 2 Editor's Draft

- Authority: pinned `w3c/svgwg` commit for `2025-09-14`.
- Primary structured inputs: `publish.xml`, `definitions.xml`, companion definitions files, and chapter HTML.
- Rendered draft indices are validation targets, not the only source.

## Canonical Data Model

Canonical checked-in snapshot data lives under `crates/svg-data/data/specs/<snapshot-id>/`.

Required files per snapshot:

- `snapshot.json`: id, title, date, status, pinned sources, ingestion version
- `elements.json`: full element records with categories, content model, permitted attrs, provenance
- `attributes.json`: full attribute records with applicability, defaults, animatability, provenance
- `grammars.json`: structured grammar definitions and refs
- `categories.json`: element and attribute category membership
- `element_attribute_matrix.json`: explicit applicability edges and requiredness
- `exceptions.json`: curated prose-only or source-bug corrections with justification
- `review.json`: completeness report, counts, unresolved items, manual audit notes

Derived artifacts live separately under `crates/svg-data/data/derived/` and must never be hand-authored:

- `union/`
- `overlays/`

## Grammar Model

Grammar data is first-class and must support shared and per-attribute syntax.

Supported node kinds should include:

- `keyword`
- `datatype_ref`
- `grammar_ref`
- `sequence`
- `choice`
- `optional`
- `zero_or_more`
- `one_or_more`
- `comma_separated`
- `space_separated`
- `repeat`
- `literal`
- `opaque`
- `foreign_ref`

Target high-value grammars include:

- path data
- points
- transform lists
- paint
- viewBox
- preserveAspectRatio
- number-or-percentage
- URL reference forms
- marker/orient enums
- geometry properties
- presentation attributes

`opaque` is an allowed ingest escape hatch, but not an acceptable final state for SVG-owned syntax.

## Provenance

Every normalized fact must carry provenance with at least:

- source id
- source kind
- pinned ref or URL
- locator or anchor
- extraction confidence

Facts that lack provenance are not canonical.

## External Specifications

External definitions referenced by SVG 2 should use pinned typed foreign references in phase 1.

Initial foreign-reference coverage:

- animations
- filter effects
- masking
- compositing
- ARIA
- CSS-backed presentation/value grammars

Do not fully ingest those ecosystems in phase 1. Store typed references with pins and targets so the data remains truthful and expandable.

## Derived Layers

After all snapshots pass review:

- derive canonical union entities
- derive version-to-version overlays/diffs
- derive lifecycle classifications from snapshot truth

Overlays are diffs between snapshots, not the primary storage model.

## Non-Goals

- Do not treat browser compat as spec truth.
- Do not store raw fetched source bundles in git.
- Do not fold DOM/IDL/interface metadata into phase 1.
- Do not rewire lint/LSP onto partially reviewed snapshot data.

## Acceptance Criteria

Snapshot data is complete enough to power consumers only when all are true:

- all normative SVG elements are accounted for
- all normative SVG attributes are accounted for
- element-to-attribute applicability is explicit
- SVG-owned grammars are normalized, not left opaque
- foreign grammars are represented as pinned typed refs
- content-model data is represented and reviewed
- every fact has provenance
- every snapshot has a checked-in review report
- derived union and overlays are reproducible from snapshot truth alone
