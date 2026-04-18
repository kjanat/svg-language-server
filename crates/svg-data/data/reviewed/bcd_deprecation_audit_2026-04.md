# BCD-deprecated SVG attribute audit (2026-04)

## Purpose

Every SVG attribute marked `deprecated: true` in `@mdn/browser-compat-data` is classified against the SVG 2 Editor's Draft, SVG 2 CR, and SVG 1.1 to resolve ambiguity between BCD's stance and our spec snapshot membership. This audit is the input to the build-time three-way `reconcile_bcd_spec()` check and the seed for `bcd_spec_exceptions.toml`.

## Authoritative source

**Parser-driven**, not hand-curated. The `spec_removals.json` file in this directory is the machine-readable output of `workers/svg-compat/src/spec_scan.ts` reading a pinned commit of `https://github.com/w3c/svgwg`. Every classification below is backed by a structural pattern match against the spec source — either:

- a `<property>` or `<element>` or `<attribute>` declaration in `master/definitions*.xml` (the SVG 2 inventory files), OR
- a `<p class="note">…has been removed in SVG 2.</p>` inside a property section of `master/text.html`, OR
- a `<li>Removed the <span class="element|property|attr-name">…</span>…</li>` entry in `master/changes.html`.

The reconcile check consumes the same file and will fail the build if any BCD-deprecated feature or spec-removed feature is still present in our snapshot data without a documented exception. To regenerate after a svgwg bump:

```sh
deno run -A workers/svg-compat/src/cli.ts scan-spec \
  --svgwg-path=./svgwg \
  --out crates/svg-data/data/reviewed/spec_removals.json
```

## Inventory (12 attributes, 0 elements)

Generated via `deno run -A workers/svg-compat/src/cli.ts emit data | jq '.attributes | to_entries[] | select(.value.deprecated) | .key'`.

## Classification

Each entry has one of four verdicts:

- **Removed**: absent from SVG 2 normatively. Fix: remove from SVG 2 snapshots.
- **Obsoleted-in-spec**: defined by SVG 2 but the spec itself flags it as obsolete. Fix: allowlist via exception file (ideally we'd track "spec-deprecated" as a first-class signal but that's a separate plan).
- **Already-obsolete**: snapshot membership already reflects SVG 1.1-only presence; lifecycle derivation already yields `Obsolete`. No action.
- **Invisible**: not in the union catalog; LSP doesn't see it. No check fires. (Underrepresentation bug, out of scope for this audit.)

| Attribute                      | Verdict                 | Evidence                                                                                                                                                                                                                                                           | Action                                                                      |
| ------------------------------ | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------- |
| `baseProfile`                  | Removed                 | SVG 2 changes doc: *"Removed the baseProfile and version attributes from the 'svg' element."*                                                                                                                                                                      | Remove from `Svg2Cr20181004` + `Svg2EditorsDraft20250914` snapshot + union  |
| `version`                      | Removed                 | Same SVG 2 changes doc quote                                                                                                                                                                                                                                       | Remove from both SVG 2 snapshots + union                                    |
| `clip`                         | Removed                 | SVG 2 changes: *"Removed requirement for clip property support."* `clip` moved to CSS Masking Module 1 where it is itself deprecated in favour of `clip-path`.                                                                                                     | Remove from both SVG 2 snapshots + union                                    |
| `zoomAndPan`                   | Removed                 | SVG 2 changes: *"Remove zoomAndPan attribute and related text."*                                                                                                                                                                                                   | Remove from both SVG 2 snapshots + union                                    |
| `glyph-orientation-horizontal` | **Removed** (corrected) | Parser found `text.html:5197` — *"This property has been removed in SVG 2."* The earlier hand-classification as "obsoleted-but-defined" was wrong — a hand-wavy subagent summary, not a direct read of the spec prose.                                             | Removed from `Svg2Cr20181004` + `Svg2EditorsDraft20250914` snapshot + union |
| `glyph-orientation-vertical`   | Obsoleted-in-spec       | Parser found `text.html:5203` — *"This property applies only to vertical text. It has been obsoleted in SVG 2..."*                                                                                                                                                 | Exception file entry                                                        |
| `kerning`                      | **Removed**             | Parser found `text.html:5233` + `changes.html:647` — *"The 'kerning' property has been removed in SVG 2."* Absent from `definitions.xml` inventory. Not in our current catalog (BCD doesn't flag it deprecated, so earlier BCD-filtered audit missed it entirely). | No action — already absent, but now documented                              |
| `xlink:actuate`                | Already-obsolete        | Union membership is `[Svg11Rec20030114, Svg11Rec20110816]` — `union_lifecycle_expr()` already returns `Obsolete`                                                                                                                                                   | No action                                                                   |
| `xlink:href`                   | Already-obsolete        | Same — SVG 1.1 only                                                                                                                                                                                                                                                | No action                                                                   |
| `xlink:show`                   | Already-obsolete        | Same                                                                                                                                                                                                                                                               | No action                                                                   |
| `xlink:title`                  | Already-obsolete        | Same                                                                                                                                                                                                                                                               | No action                                                                   |
| `xml:lang`                     | Invisible               | Absent from `data/derived/union/attributes.json`; only in `placeholder_attribute_names.txt`. Not in Rust catalog → no conflict                                                                                                                                     | No action in this audit; file follow-up to add coverage                     |
| `xml:space`                    | Invisible               | Same                                                                                                                                                                                                                                                               | Same                                                                        |

## Snapshot surgery plan

**Edit `crates/svg-data/data/specs/Svg2EditorsDraft20250914/attributes.json`**: remove entries for `baseProfile`, `version`, `clip`, `zoomAndPan`.

**Edit `crates/svg-data/data/specs/Svg2Cr20181004/attributes.json`**: remove the same four entries (per the SVG 2 changes doc, all four were gone by CR-era).

**Edit `crates/svg-data/data/derived/union/attributes.json`**: remove `Svg2Cr20181004` and `Svg2EditorsDraft20250914` from `present_in` for the four removed attributes. (Union should cascade from snapshot sources in general; verify whether a regen script exists — if not, hand-edit is fine.)

**Leave untouched**:

- SVG 1.1 snapshot data for all twelve attributes (all were legitimately defined in SVG 1.1).
- The four `xlink:*` entries (already correct).
- `xml:lang` / `xml:space` (separate work to track them at all).

## Exception file seed

`crates/svg-data/data/reviewed/bcd_spec_exceptions.toml`:

> **Note (post-audit revision):** This section originally seeded *two*
> exceptions — one for `glyph-orientation-horizontal` and one for
> `glyph-orientation-vertical`. The parser-driven re-read of `text.html`
> later reclassified `glyph-orientation-horizontal` as **Removed**
> (see the table above, `text.html:5197`) so only the vertical sibling
> remains a legitimate spec disagreement. The seed shown here reflects
> the final committed state, not the original two-entry proposal.

```toml
[[attribute]]
name         = "glyph-orientation-vertical"
element      = "*"
bcd_says     = "deprecated"
spec_says    = "obsoleted-but-defined"
reason       = "SVG 2 text chapter §11.10.1.3 still defines the glyph-orientation-vertical attribute but calls it obsoleted for vertical text. BCD is correct to flag deprecation; the spec membership is correct to retain it. Accept the split until snapshot data can track an explicit spec-deprecated flag."
added        = "2026-04-15"
upstream_ref = "https://svgwg.org/svg2-draft/text.html"
```

## Follow-up work (not in scope)

1. Track `xml:lang` / `xml:space` in the union catalog — currently invisible to the LSP.
2. Add a first-class `spec_deprecated: bool` flag to the snapshot attribute schema so glyph-orientation-* can be classified as `SpecLifecycle::Deprecated` directly instead of through the exception mechanism.
3. Cross-reference BCD element deprecations — currently zero, but the reconcile check should cover elements too when upstream data adds any.
