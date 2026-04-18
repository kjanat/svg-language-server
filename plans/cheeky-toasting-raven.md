# Plan: Sophisticated compat UX — unify signals, render qualifiers, flag obsolete, fix false-stable

## Context

The browser-support-preservation refactor shipped. The worker `/data.json` and the static Rust catalog now carry **every** upstream BCD/web-features signal end-to-end: 11 per-browser fields (`version_added`, `version_qualifier`, `supported`, `version_removed`, `version_removed_qualifier`, `partial_implementation`, `prefix`, `alternative_name`, `flags`, `notes`, `raw_value_added`) plus full baseline date sub-objects with qualifiers.

**But the rendering layer drops most of it**, and in one case actively lies. Using `baseProfile` as the canary:

| Surface                             | Current output                                                                                                                                                                                       | Problem                                                                                                                                                                                   |
| ----------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| LSP hover                           | `~~The baseProfile SVG attribute.~~` <br> `**Deprecated**` <br> `**Stable in Svg2EditorsDraft20250914**` <br> `![icon] _Limited availability_` <br> `Chrome 1 \| Edge 12 \| Firefox 1.5 \| Safari 3` | Says **both** "Deprecated" and "Stable in SVG 2" — actively contradictory. Baseline is Limited but every engine shows a concrete version, with no explanation. No user-facing verdict.    |
| Worker dashboard chips              | Render `chip-partial`, `chip-removed`, `chip-prefixed`, `chip-flagged`, `chip-unsupported` CSS classes — **none are styled**. Rich info only in `title` tooltip.                                     | Signals are dark matter: accessible to screen readers that surface `title`, invisible to sighted users.                                                                                   |
| `BaselineBadge`                     | Shows `status + glyph + year`.                                                                                                                                                                       | Never surfaces `low_date.raw` / `high_date.raw` — the exact upstream dates are hidden.                                                                                                    |
| Stats grid                          | 4 hardcoded tiles: elements, attributes, deprecated, limited.                                                                                                                                        | No visibility into the new signal inventory (partial, removed, flagged, explicit-false) that we worked hard to preserve.                                                                  |
| `svg-lint`                          | Only reads `compat_flags.deprecated` / `compat_flags.experimental`. Silently ignores `SpecLifecycle::Obsolete` (`crates/svg-lint/src/rules/mod.rs:550`).                                             | No diagnostic for "you're using an attribute that was removed from the spec". The LSP hover complains but lint stays quiet.                                                               |
| Hover `format_browser_support_line` | `Chrome 1 \| Edge 12 \| ...` — plain `version_added` string.                                                                                                                                         | **The preserved `version_qualifier` is never rendered.** `baseline-shift` at Chrome `≤80` displays as "Chrome 80" — factually wrong. Same bug exists for `feGaussianBlur` cousins in BCD. |

**Root cause of the `baseProfile` contradiction** (verified by explore-agent):

`crates/svg-data/data/derived/union/attributes.json:106` lists `baseProfile` as `present_in` → `Svg2EditorsDraft20250914`. `build.rs::union_lifecycle_expr()` (lines 668-676) then classifies it as `SpecLifecycle::Stable`. But `baseProfile` was **removed from SVG 2** — it's not in the spec anymore. The membership data is wrong, not the derivation logic. Same story for `version` (the SVG version attribute, not a package version).

### Two binding principles for this plan

1. **One source of truth per signal, reconciled at build time, not render time.** If BCD says "deprecated" and the spec snapshot says "stable", that's a build-time error — fail the build, force the fix. Consumers should never have to decide which source to trust.

2. **The hover must answer one question first: should I use this?** Everything else is evidence. The current hover is a wall of independent facts that the user has to synthesize. A great UX does the synthesis and then shows its work.

### Intended outcome

- LSP hover for `baseProfile` reads something like:
  ```
  > ❌ baseProfile — removed from SVG 2, avoid in new documents

  _The baseProfile attribute described the minimum SVG language profile..._

  **Status:** Removed from `Svg2Cr20181004` onward · BCD deprecated · Not Baseline

  **Browser support** (historical, before removal):
  Chrome 1 · Edge 12 · Firefox 1.5 · Safari 3

  [MDN Reference](...) · [Spec history](...)
  ```
  — one clear verdict, one status line that consolidates all agreeing signals, supporting evidence below.

- LSP hover for `baseline-shift` (a Limited-baseline CSS property with `≤80` qualifier) reads:
  ```
  > ⚠ baseline-shift — Limited availability, avoid without fallback

  _..._

  **Status:** Limited availability · SVG 2 standards track

  **Browser support:**
  Chrome ≤80 · Edge ≤80 · Firefox ✗ · Safari ≤13.1

  - Chrome: partial implementation — only the default `sRGB` value is rendered correctly
  - Firefox: never supported

  [MDN Reference](...) · [Spec](...)
  ```
  — qualifier glyphs render, partial-impl notes surface as sub-bullets, Firefox's explicit `false` shows `✗`.

- Dashboard chips get colour + icons for every state class. The 5 unstyled CSS classes become meaningful.
- Stats grid surfaces new tiles (partial, removed, flagged, unsupported-anywhere) so the data we preserve is legible at a glance.
- `svg-lint` gains `obsolete-feature` + `partial-implementation-info` rules that consume the same unified verdict helper the hover uses. They cannot disagree — same function call.
- Build fails loudly when BCD and the spec membership file disagree on an entry, catching data drift at the `cargo check` stage.

---

## Architecture

Three structural additions:

1. **`CompatVerdict`** — a new shared type in `svg-data` that fuses BCD deprecation, spec lifecycle, baseline tier, and per-browser signals into a single `recommendation` + `reasoning` value. **One source of truth for both hover and lint.**

2. **`svg_data::lint_data::check_bcd_spec_agreement()`** — a build-time cross-check that walks every catalog entry and asserts BCD-deprecated entries are not also marked `SpecLifecycle::Stable` in the latest snapshot. Currently we'd have ~2-4 offenders (`baseProfile`, `version`, possibly `contentStyleType`, `contentScriptType`). Fails `cargo build` with a clear message per offender.

3. **Structured hover builder** — `hover::CompatMarkdownBuilder` replaces the loose `parts.push(...)` pattern with a typed builder that emits a consistent section layout. Ensures every entry gets the same structural skeleton.

```
crates/svg-data/src/
├── types.rs              UPDATED: add CompatVerdict, VerdictRecommendation, VerdictReason enums
├── lib.rs                UPDATED: expose compute_compat_verdict() helper
├── verdict.rs            NEW: pure function that fuses BCD + spec + baseline + browser signals
└── data/derived/union/attributes.json   UPDATED: fix baseProfile + version membership

crates/svg-data/build.rs  UPDATED:
  - emit BCD↔spec agreement check that errors on conflict
  - emit removed-in-snapshot metadata (optional: when was it dropped)

crates/svg-language-server/src/
├── hover.rs              REWRITTEN around CompatMarkdownBuilder:
│                           - compute_compat_verdict() once per hover
│                           - builder.headline(verdict), .status(verdict),
│                             .browser_chips(), .per_browser_notes(), .links()
│                           - format_browser_support_line renders qualifier glyphs
│                           - per-browser sub-bullets for version_removed,
│                             partial_implementation, prefix, notes, alternative_name
├── compat.rs             UPDATED: RuntimeCompat uses CompatVerdict for override path
└── diagnostics_helpers   (if needed — for lint integration)

crates/svg-lint/src/rules/mod.rs  UPDATED:
  - obsolete_feature rule fires on SpecLifecycle::Obsolete
  - partial_implementation_info rule fires when any browser has partial flag
  - existing deprecated rule consumes CompatVerdict for richer messaging

workers/svg-compat/
├── static/style.css      UPDATED: add CSS rules for chip-removed, chip-partial,
│                           chip-flagged, chip-prefixed, chip-unsupported
├── static/browsers/      ADD: partial.svg, removed.svg, flagged.svg, prefixed.svg
│                           (simple overlay glyphs, each ~300 bytes)
├── src/view.ts           UPDATED: extend PageStats with new counts
├── src/components/
│   ├── BaselineBadge.tsx UPDATED: richer title attribute with low_date.raw / high_date.raw
│   ├── BrowserSupport.tsx UPDATED: variant-specific status glyph selection
│   ├── StatsGrid.tsx     UPDATED: new tile rows
│   └── (no new components — extend existing)
└── src/render.tsx        UPDATED: populate new stat fields
```

The `CompatVerdict` model is the architectural linchpin. Both the hover markdown and the lint diagnostic paths call `compute_compat_verdict(def)` and render against the result. If we ever find them disagreeing, it's a bug in one **renderer**, not in the data or logic — the call sites are structurally identical.

---

## Phase 1 — `CompatVerdict` type + computation

### 1.1 `crates/svg-data/src/types.rs`

Add a new public sum type that encodes the user-facing verdict:

```rust
/// Recommendation level for a compat verdict. Maps 1:1 to an LSP
/// diagnostic severity so a lint rule and a hover badge can't disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictRecommendation {
    /// Safe to use today. Baseline wide, not deprecated, not partial.
    Safe,
    /// Use with care: partial implementation, prefix needed, flag required,
    /// or non-Baseline status. Behaviour may differ from the spec.
    Caution,
    /// Avoid in new work: deprecated in BCD or in the selected spec profile.
    Avoid,
    /// Do not use: explicitly removed from the current spec, or explicitly
    /// unsupported in all tracked engines.
    Forbid,
}

/// A single reason contributing to a verdict. Renderers consume these
/// as bullet points / glyphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictReason {
    /// BCD marks the feature deprecated.
    BcdDeprecated,
    /// BCD marks the feature experimental.
    BcdExperimental,
    /// The feature is absent from the currently-selected spec snapshot.
    ProfileObsolete { last_seen: SpecSnapshotId },
    /// The feature is experimental in the current profile.
    ProfileExperimental,
    /// Baseline "limited" across major browsers.
    BaselineLimited,
    /// Baseline "newly available" — less than ~30 months in all engines.
    BaselineNewly {
        since: u16,
        qualifier: Option<BaselineQualifier>,
    },
    /// Some tracked browser ships a partial implementation.
    PartialImplementationIn(&'static str),
    /// Some tracked browser needs a vendor prefix.
    PrefixRequiredIn {
        browser: &'static str,
        prefix: &'static str,
    },
    /// Some tracked browser gates the feature behind a preference or flag.
    BehindFlagIn(&'static str),
    /// Some tracked browser explicitly does not support the feature.
    UnsupportedIn(&'static str),
    /// Some tracked browser removed support at a specific version.
    RemovedIn {
        browser: &'static str,
        version: &'static str,
        qualifier: Option<BaselineQualifier>,
    },
}

/// A fully-reconciled compatibility verdict. Both hover and lint rules
/// consume this — they never inspect raw `deprecated` / `baseline` /
/// `browser_support` fields directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompatVerdict {
    pub recommendation: VerdictRecommendation,
    /// One-line summary shown in hover headline. Static strings only
    /// so the struct stays `Copy`. The renderer interpolates the name.
    pub headline_template: &'static str,
    /// Reasons, in priority order. First few are shown in a short
    /// "Status:" line, remainder available as sub-bullets.
    pub reasons: &'static [VerdictReason],
}
```

The `&'static [VerdictReason]` constraint means each entry's reasons are computed at **build time** and interned. This keeps `AttributeDef` / `ElementDef` `Copy` and avoids per-hover allocation.

### 1.2 `crates/svg-data/src/verdict.rs` — new module

Pure function that takes a `&AttributeDef` / `&ElementDef` plus a selected `SpecSnapshotId` and produces a `CompatVerdict`. Rules (in priority order):

1. Spec-obsolete in selected profile → `Forbid` + `ProfileObsolete`.
2. All tracked browsers explicitly unsupported (`supported: Some(false)` for chrome/edge/firefox/safari) → `Forbid` + `UnsupportedIn` per browser.
3. BCD deprecated → `Avoid` + `BcdDeprecated`.
4. Spec-deprecated in selected profile → `Avoid` + `ProfileObsolete { last_seen }`.
5. Any `partial_implementation` → `Caution` + `PartialImplementationIn`.
6. `prefix` or `flags` required anywhere → `Caution` + respective reason.
7. Baseline `limited` → `Caution` + `BaselineLimited`.
8. Baseline `newly` → `Caution` + `BaselineNewly`.
9. Anything with `version_removed` → `Caution` + `RemovedIn` (future-removal warning).
10. Otherwise → `Safe`.

Multiple reasons can apply; the function collects them into a const-interned slice via the build script. The **final recommendation is the strongest (most restrictive) reason present**. This guarantees "Deprecated + Stable in spec" becomes `Avoid` with a `BcdDeprecated` reason — no more contradictory output.

### 1.3 Rule priority algorithm — exact decision tree

`VerdictRecommendation` forms a total order: `Safe(0) < Caution(1) < Avoid(2) < Forbid(3)`. Each reason maps to a tier via a const function:

```rust
const fn reason_tier(r: VerdictReason) -> VerdictRecommendation {
    match r {
        VerdictReason::ProfileObsolete { .. } => VerdictRecommendation::Forbid,
        VerdictReason::UnsupportedIn(_) | VerdictReason::RemovedIn { .. } => {
            // Only Forbid when ALL four tracked browsers are UnsupportedIn.
            // Individual removals → Caution. This is resolved in the collection
            // phase (see verdict.rs), not in this per-reason mapping.
            VerdictRecommendation::Caution
        }
        VerdictReason::BcdDeprecated | VerdictReason::ProfileExperimental => {
            VerdictRecommendation::Avoid
        }
        // Everything else is Caution.
        _ => VerdictRecommendation::Caution,
    }
}
```

**Fully-unsupported special case**: when all four browsers have `supported: Some(false)`, the collection loop promotes the reason set to `Forbid`. This is checked as a post-collection step, not per-reason.

**Final verdict**:

```
recommendation = max(reason_tier(r) for r in collected_reasons)
reasons = collected_reasons sorted by tier desc, then by fixed listing order for tie-breaking
```

Multiple reasons at the same tier are all preserved — they all appear in the Status line and sub-bullets. There's no early-exit; every applicable rule is evaluated independently and all matching reasons are collected.

**`None` reasons from non-applicable rules are simply not added to the collection.** There is no "unknown" tier — every possible state either produces a reason or is silently absent.

### 1.4 Build-time emission strategy

**Where**: `verdict.rs` result is emitted as a new field on `AttributeDef` and `ElementDef` in the generated `catalog.rs`:

```rust
pub struct AttributeDef {
    // ... existing fields ...
    /// Pre-computed verdicts per spec snapshot.
    pub verdicts: &'static [(SpecSnapshotId, CompatVerdict)],
}
```

`build.rs` emits this field in the same loop that emits `browser_support`, `baseline`, etc. The value is a reference to a named const slice.

**Interning reason slices**: `build.rs` collects all distinct reason vectors, deduplicates them by content, and emits named const arrays:

```rust
const REASONS_BCD_DEP: &[VerdictReason] = &[VerdictReason::BcdDeprecated];
const REASONS_PROFILE_OBS_SVG2CR: &[VerdictReason] = &[VerdictReason::ProfileObsolete {
    last_seen: SpecSnapshotId::Svg11Rec20110816,
}];
// ...
```

Each `AttributeDef` then references the appropriate slice pointer. Deduplication is by `Vec<VerdictReason>` equality check in the build script's collection map — typical SVG dataset has ~50 distinct combinations across ~200 entries, so this is fast.

**Human-readable Rust**: the emitted code is valid Rust that can be diffed and reviewed. Binary serialization would be opaque and break on struct layout changes.

**`CompatVerdict` is `Copy`**: `headline_template: &'static str` is a format key, not a rendered string. Interpolation happens in a **separate non-Copy formatter**:

```rust
// In hover.rs — non-Copy, allocates.
fn format_verdict_headline(verdict: CompatVerdict, feature_name: &str) -> String {
    let glyph = match verdict.recommendation { ... };
    format!("> {glyph} {feature_name} — {}", verdict.headline_template)
}
```

The `Copy` struct holds only the `&'static str` template key; the allocating formatter is called once per hover render.

### 1.5 Worker ↔ Rust data flow — no serialization needed

**`CompatVerdict` is a Rust-only concept.** The worker (Deno/TypeScript) does NOT consume Rust-computed verdicts. The worker's rendering layer (`BrowserSupport.tsx`, `BaselineBadge.tsx`, `StatsGrid.tsx`) already has all the signals from the TypeScript `BrowserVersion`/`Baseline` types in `/data.json` — it computes its own visual state directly from those signals using the same logic.

Phase 3 (worker CSS/stats) never touches verdicts. It works exclusively with the data already present in `SvgCompatOutput`. No `verdicts.json` endpoint is needed; no JSON schema for verdicts is needed. The verdict abstraction lives entirely in Rust and is consumed only by the hover and lint layers. This keeps the TypeScript side simple and avoids a cross-runtime contract.

### 1.6 `crates/svg-data/src/lib.rs`

New public helpers:

```rust
pub fn compat_verdict_for_element(
    def: &'static ElementDef,
    profile: SpecSnapshotId,
) -> CompatVerdict;

pub fn compat_verdict_for_attribute(
    def: &'static AttributeDef,
    profile: SpecSnapshotId,
) -> CompatVerdict;
```

Both do a linear scan of `def.verdicts` for the matching snapshot, returning `Safe` with no reasons if not found (defensive fallback — shouldn't happen for covered snapshots).

### 1.7 `BaselineQualifier` glyph map

Already implemented as `format_baseline_qualifier()` in `hover.rs:570-576`. The mapping:

```rust
const fn qualifier_glyph(q: BaselineQualifier) -> &'static str {
    match q {
        BaselineQualifier::Before => "≤",
        BaselineQualifier::After => "≥",
        BaselineQualifier::Approximately => "~",
    }
}
```

**Same function reused for version qualifiers.** `BrowserVersionView::Version` changes from a bare `&'static str` to a two-field struct `{ version: &'static str, qualifier: Option<BaselineQualifier> }`. The glyph is prepended in `format_browser_support_line` using this same function — no new mapping needed.

---

## Phase 2 — Hover rewrite via `CompatMarkdownBuilder`

### 2.1 `crates/svg-language-server/src/hover.rs`

Replace the sprawling `format_{element,attribute}_hover_with_profile` with a structured builder:

```rust
struct CompatMarkdownBuilder {
    sections: Vec<Section>,
}

enum Section {
    Headline(String),              // blockquote-quoted verdict headline
    Description(String),           // italic MDN-style description
    Status(Vec<String>),           // "Status: reason · reason · reason"
    BrowserChips(String),          // Chrome ≤80 · Edge ≤80 · Firefox ✗ · Safari ≤13.1
    BrowserNotes(Vec<String>),     // per-browser sub-bullets for partial/prefix/notes
    ValueConstraints(Vec<String>), // enum / transform / etc.
    Links(Vec<String>),            // MDN · Spec
}
```

The builder renders each section to markdown with consistent spacing (single blank line between, no stray trailing whitespace), then joins. Every hover gets the same skeleton, so the user's eye learns the shape.

### 2.2 `format_browser_support_line` fix — render qualifier glyphs

Current:

```rust
BrowserVersionView::Version(version) => format!("{name} {version}"),
```

New:

```rust
BrowserVersionView::Version { version, qualifier } => {
    let glyph = qualifier.map(qualifier_glyph).unwrap_or("");
    format!("{name} {glyph}{version}")
}
```

`BrowserVersionView::Version` becomes `{ version: &str, qualifier: Option<BaselineQualifier> }`. `baked_browser_version` passes through `v.version_qualifier` — one extra field, already in the struct.

Separator becomes `·` instead of `|` for prose-style readability (the pipe made sense for a monospace grid; bullets read better in rendered markdown).

### 2.3 Per-browser sub-bullets — data source confirmed

All 11 `BrowserVersion` fields are already in the baked static catalog (`catalog.rs`, emitted by `build/codegen.rs`). No additional fetch needed — hover reads directly from `attr.browser_support`.

**The `notes` text** (e.g. `"Only the default value of sRGB is implemented"` for Chrome on `color-interpolation`) comes from BCD's `notes` field, which is already ingested into `BrowserVersion.notes: &'static [&'static str]`. Sub-bullets render the first note string; not hard-coded.

Sub-bullet composition rules per-browser, in order (all applicable rules contribute lines):

- `partial_implementation: true` → `"partial — {notes[0] if present, else empty}"`
- `prefix` present → `"requires {prefix} prefix"`
- `version_removed` present → `"removed in {qualifier_glyph}{version_removed}"`
- `notes` (without `partial`) → note strings joined with `·`
- `alternative_name` present → `"ships as`{alternative_name}`"`
- `flags` non-empty → `"behind flag {flag.name}"`

If no browser would produce a sub-bullet, the `BrowserNotes` section is omitted entirely.

**Test fixture confirmation**: `color-interpolation` has `partial_implementation: true` + notes on Chrome in live BCD (confirmed in prior session). `baseline-shift` has `≤80` version qualifiers for Chrome + Edge. Both are real live entries.

Example rendering:

```
Chrome ≤80 · Edge ≤80 · Firefox ✗ · Safari ≤13.1

- Chrome: partial — only the default value of sRGB is implemented
- Firefox: never supported in any release
```

### 2.4 Headline synthesis

The headline is a markdown blockquote whose content is driven by `VerdictRecommendation`:

| Recommendation | Glyph | Template                                     |
| -------------- | ----- | -------------------------------------------- |
| `Safe`         | ✓     | `{name} — safe to use today`                 |
| `Caution`      | ⚠     | `{name} — {short_reason}, use with care`     |
| `Avoid`        | ⊘     | `{name} — {short_reason}, avoid in new work` |
| `Forbid`       | ✗     | `{name} — {short_reason}, do not use`        |

`short_reason` is derived from the first reason in the verdict's reason list (e.g. `BcdDeprecated` → `"deprecated"`, `ProfileObsolete` → `"removed from SVG 2"`, `BaselineLimited` → `"limited availability"`).

The headline is rendered as `> ✗ baseProfile — removed from SVG 2, do not use`, which GitHub-flavoured markdown renders as a visually distinct blockquote. LSP clients that support markdown render it with a left border and muted background — a clean attention-grabber.

### 2.5 Status line

Single line, bullet-separated, sourced from `verdict.reasons` (first 3-5 entries). Example:

`**Status:** Removed from Svg2Cr20181004 · BCD deprecated · Not Baseline`

This replaces the old split between `**Deprecated**` and `**Stable in Svg2EditorsDraft**`. Because the verdict is pre-reconciled, there's no way for this line to contradict the headline — they both read from the same `CompatVerdict`.

### 2.6 Existing hover tests

`tests/definitions_and_hover.rs` currently asserts:

- `hover_renders_baseline_qualifier_for_fegaussianblur` — must be updated to assert the new Status line format (still contains `≤2021`, but in a different structural position).
- `hover_marks_glyph_orientation_horizontal_unsupported_across_chromium_firefox` — must be updated to assert `Chrome ✗`, `Firefox ✗` in the new `·`-separated chip line.
- Add new test: `hover_headline_for_baseprofile_reads_removed_from_svg2` — asserts the blockquote headline exists and contains "removed from SVG 2".
- Add new test: `hover_renders_per_browser_sub_bullets_for_partial_implementation` — uses `color-interpolation` (which has `partial_implementation: true` on chrome in the live data) as fixture.

---

## Phase 3 — Dashboard chip visual language

### 3.1 CSS for unstyled chip states

**First step when implementing**: read `workers/svg-compat/static/style.css` to verify whether `--color-danger`, `--color-warn`, `--color-caution`, `--color-info` custom properties exist in the `:root` rule. If they do not exist, introduce them in `:root` alongside the existing `--chip-bg-base` and similar tokens. If they do exist, reuse them as-is.

`workers/svg-compat/static/style.css`:

```css
.chip-unsupported {
	--chip-bg: color-mix(in oklab, var(--color-danger) 12%, var(--chip-bg-base));
	--chip-border: color-mix(in oklab, var(--color-danger) 40%, transparent);
	color: var(--color-danger-fg);
}
.chip-removed {
	--chip-bg: color-mix(in oklab, var(--color-warn) 10%, var(--chip-bg-base));
	--chip-border: color-mix(in oklab, var(--color-warn) 40%, transparent);
	text-decoration: line-through;
	text-decoration-color: color-mix(
		in oklab,
		var(--color-warn) 70%,
		transparent
	);
}
.chip-partial {
	--chip-bg: color-mix(in oklab, var(--color-caution) 10%, var(--chip-bg-base));
	--chip-border: color-mix(in oklab, var(--color-caution) 40%, transparent);
}
.chip-flagged {
	--chip-bg: color-mix(in oklab, var(--color-info) 10%, var(--chip-bg-base));
	--chip-border: color-mix(in oklab, var(--color-info) 40%, transparent);
	font-style: italic;
}
.chip-prefixed::after {
	content: "•";
	margin-left: 0.25em;
	font-size: 0.75em;
	vertical-align: super;
	color: var(--color-info);
}
```

Uses the existing `--color-*` semantic tokens if present; otherwise introduce them in the `:root` rule alongside the existing palette. `color-mix` is widely supported in 2024+ browsers, matches the project's modern-baseline CSS.

Dark-mode variant via the existing `@media (prefers-color-scheme: dark)` block — same shape, different intensity multipliers.

### 3.2 Status glyph overlays

Instead of adding new SVG assets, reuse the existing `check.svg` / `cross.svg` mask and use a CSS `mask-image` filter to tint it by state. `chip-unsupported` flips the mask to `cross.svg`; `chip-partial` / `chip-prefixed` / `chip-removed` keep `check.svg` but tint it.

`BrowserSupport.tsx` already selects the mask based on `hasData`. Extend so `supported === false` → `cross.svg`, partial/removed → keep check but add the state class (CSS does the tinting). Zero new assets needed.

### 3.3 `BaselineBadge.tsx` — richer `title`

Currently `title` is absent. Add:

```tsx
function baselineTitle(baseline: Baseline): string {
	const parts: string[] = [];
	if (baseline.low_date?.raw) {
		parts.push(`entered baseline ${baseline.low_date.raw}`);
	}
	if (baseline.high_date?.raw) {
		parts.push(`widely available ${baseline.high_date.raw}`);
	}
	if (baseline.raw_status) {
		parts.push(`(unknown upstream status: ${baseline.raw_status})`);
	}
	return parts.join(" · ");
}
```

Surfaces `low_date.raw` / `high_date.raw` end-to-end. Zero layout change, pure accessibility win.

### 3.4 Stats grid extensions

`view.ts::PageStats` gains new fields:

```ts
export interface PageStats {
	elements: number;
	attributes: number;
	deprecated: number;
	limited: number;
	// NEW:
	partial: number; // count of entries with partial_implementation in any browser
	removed: number; // count of entries with version_removed in any browser
	flagged: number; // count of entries with flags in any browser
	unsupportedSomewhere: number; // count with at least one `supported: false`
}
```

`buildPageModel` computes these with a single pass over elements + attributes.

`StatsGrid.tsx` adds a second row of tiles for the new counts, using muted styling so the "structural" counts (elements, attributes) remain primary.

### 3.5 Dashboard tests

`main_test.ts` additions:

- `renderHtml adds chip-partial class for color-interpolation entry`
- `StatsGrid reports non-zero partial count`
- `BaselineBadge title contains both low_date.raw and high_date.raw`

---

## Phase 4 — Lint rules driven by `CompatVerdict`

### 4.1 New rules

`crates/svg-lint/src/rules/mod.rs`:

```rust
// Rule: obsolete-feature
//   severity: warning
//   fires when: verdict.recommendation == Forbid and any reason is ProfileObsolete
//   message: "<name> was removed from <snapshot>; it is no longer defined in the current SVG spec profile"

// Rule: partial-implementation
//   severity: info
//   fires when: verdict has any PartialImplementationIn reason
//   message: "<name> is partially implemented in <browser>: <note>"

// Rule: prefix-required
//   severity: info
//   fires when: verdict has any PrefixRequiredIn reason
//   message: "<name> requires <prefix> in <browser>"

// Rule: behind-flag
//   severity: info
//   fires when: verdict has any BehindFlagIn reason
//   message: "<name> is only available behind <flag> in <browser>"
```

### 4.2 Existing deprecated rule — richer message

Replace the current hardcoded `"<name> is deprecated"` with a message that reads from `CompatVerdict` so the lint and hover agree. If the verdict's top reason is `BcdDeprecated`, the message includes "deprecated in BCD"; if it's `ProfileObsolete { last_seen }`, the message includes "last present in `{last_seen}`".

### 4.3 Exception matching semantics

An exception entry matches when **both** `name` and `element` agree:

- `element = "*"` wildcard matches an attribute used on any element (including element-specific use).
- Specific element (`element = "svg"`) matches only that element.
- **Precedence**: specific-element exception wins over wildcard when both exist.

For `xlink:href`: a single `element = "*"` exception covers all `<use>`, `<image>`, `<tref>` uses. No three separate entries needed. The matcher finds any exception where `e.name == attr_name && (e.element == "*" || e.element == use_context)`.

Exception IDs (used for the self-pruning check) are `(kind, name, element)` tuples.

### 4.4 Reliability hook — build-time agreement check (hard error + allowlist)

Decision: **Option A, hard error with exception allowlist**, per Oracle consultation. Warnings would recreate the exact failure mode that let `baseProfile` sit broken for months. The allowlist preserves legitimate both-sources-correct disagreements like `xlink:href` without forcing us to mangle either data source.

`crates/svg-data/build.rs` — `reconcile_bcd_spec()`:

```rust
fn reconcile_bcd_spec(
    bcd_elements: &HashMap<String, CompatEntry>,
    bcd_attributes: &HashMap<String, BcdAttribute>,
    union_elements: &[UnionElement],
    union_attrs: &[UnionAttribute],
    exceptions: &[BcdSpecException],
) -> Result<(), BuildError> {
    let mut conflicts = Vec::new();
    let mut matched_exceptions = HashSet::new();

    // Pass 1: find real conflicts.
    for (name, bcd_flag, kind) in iter_bcd_deprecation_flags(bcd_elements, bcd_attributes) {
        let present_in = membership_for(kind, name, union_elements, union_attrs);
        let in_latest = present_in.contains(&SpecSnapshotId::Svg2EditorsDraft20250914);

        let conflict = bcd_flag && in_latest;
        if !conflict {
            continue;
        }

        // Is this disagreement already known?
        if let Some(exception) = exceptions.iter().find(|e| e.matches(kind, name)) {
            matched_exceptions.insert(exception.id());
            continue;
        }

        conflicts.push(Conflict {
            kind,
            name: name.clone(),
            bcd_says: "deprecated",
            spec_says: "stable",
        });
    }

    // Pass 2: self-prune — exceptions that didn't match a real conflict are rot.
    let mut dead = Vec::new();
    for exception in exceptions {
        if !matched_exceptions.contains(&exception.id()) {
            dead.push(exception);
        }
    }

    if conflicts.is_empty() && dead.is_empty() {
        return Ok(());
    }

    // Batch everything into a single error emission so developers see
    // the whole picture at once instead of one-per-line noise.
    let message = render_reconciliation_error(&conflicts, &dead);
    println!("cargo::error={message}");
    Err(BuildError::BcdSpecMismatch)
}
```

**Key mechanics** (from Oracle's refinement):

1. **Batched emission.** One `cargo::error` that lists every conflict + every dead exception. No death-by-papercuts when a BCD bump flips 15 entries at once.

2. **Self-pruning.** Exceptions that don't match any current conflict are themselves an error. When the spec catches up and the disagreement resolves, the allowlist must shrink. Prevents rot.

3. **Paste-ready fix-its.** The error message includes a pre-filled TOML block the developer can paste directly into the exception file with just a `<WHY>` + `<URL>` edit:

```
cargo::error=BCD/spec reconciliation failed (2 conflicts, 0 stale exceptions).

Conflict #1: attribute `baseProfile` on <svg>
  BCD (@mdn/browser-compat-data@7.3.11): deprecated = true
  Spec snapshot:                         present_in = [..., Svg2EditorsDraft20250914] → Stable

  Fix one of:
    1. Update data/specs/Svg2EditorsDraft20250914/attributes.json and remove `baseProfile`
       (if the spec actually dropped it — verify at https://svgwg.org/svg2-draft/)
    2. Add to data/reviewed/bcd_spec_exceptions.toml:

       [[attribute]]
       name = "baseProfile"
       element = "svg"
       bcd_says = "deprecated"
       spec_says = "stable"
       reason = "<WHY — one sentence>"
       added = "2026-04-13"
       upstream_ref = "<URL to primary source>"

Conflict #2: attribute `version` on <svg>
  [...]

Source: crates/svg-data/build.rs::reconcile_bcd_spec
```

**Exception file** — `crates/svg-data/data/reviewed/bcd_spec_exceptions.toml`. Single file, TOML (dprint-formatted), with nested `[[element]]` / `[[attribute]]` tables for scope:

```toml
[[attribute]]
name         = "xlink:href"
element      = "*"                                                                                                                                 # "*" = global, or a specific tag
bcd_says     = "not_deprecated"
spec_says    = "obsolete"
reason       = "Removed from SVG 2 but every browser still parses it for backwards compatibility. Both sources are correct from their own frame."
added        = "2026-04-13"
upstream_ref = "https://svgwg.org/svg2-draft/changes.html#attributes"
```

Required fields per entry:

- `name`: attribute or element name
- `element`: element scope (`*` for global; specific tag for element-bound attrs)
- `bcd_says` / `spec_says`: the literal conflict so the allowlist self-documents
- `reason`: one-sentence human explanation
- `added`: ISO date the exception was added (for rot audits)
- `upstream_ref`: URL to primary source so a future maintainer can verify in 30s

**Local-vs-CI parity.** The hard error fires identically in local `cargo check` and CI. No env-var escape hatch. If a developer needs to work through a pipeline while a BCD bump is pending, they use a WIP exception entry with `reason = "WIP: resolving in #<PR-number>"` — explicit, trackable, self-pruning.

### 4.4 Data audit — every BCD-deprecated attribute

Scope decision: full audit of **every** currently-BCD-deprecated SVG attribute (and element, if any), not just the two known wrong. Benefit: one clean pass through the spec now costs less than repeated targeted fixes over the next year, and it gives us a vetted baseline the build check can then enforce forever.

**Inventory step** (read-only, runs before any edits):

1. Dump the full BCD-deprecated attribute list from the current worker JSON via `deno run -A workers/svg-compat/src/cli.ts emit data`, filtered through `jq` to `[.attributes | to_entries[] | select(.value.deprecated == true) | .key]`. Expected count: ~20 based on prior audits (`baseProfile`, `version`, `contentStyleType`, `contentScriptType`, `zoomAndPan`, possibly several xlink:* aliases, `requiredFeatures`, `xml:base`, `xml:lang` — the exact list lands in a review document).

2. For each entry, cross-reference against **three** authoritative sources in order:
   - **SVG 2 Editor's Draft** (`https://svgwg.org/svg2-draft/`) — ground truth for the latest snapshot.
   - **SVG 2 CR (2018-10-04)** (`https://www.w3.org/TR/2018/CR-SVG2-20181004/`) — the locked CR snapshot we reference as `Svg2Cr20181004`.
   - **SVG 1.1 2E** (`https://www.w3.org/TR/SVG11/`) — the SVG 1.1 reference.

3. For each entry, record **one of four** verdicts:
   - **Removed in SVG 2**: attribute is not defined in SVG 2 Editor's Draft or CR. Fix: remove from `data/specs/Svg2EditorsDraft20250914/attributes.json` and `data/specs/Svg2Cr20181004/attributes.json`; remove from `data/derived/union/attributes.json` membership. Lifecycle becomes `Obsolete` in the later profiles.
   - **Deprecated in SVG 2 (still defined, marked "legacy")**: attribute is present but the spec explicitly flags it as legacy/deprecated. Fix: keep in membership, add a `deprecated: true` field to the snapshot attribute entry, extend `union_lifecycle_expr()` to honour it, let the build-time check pass because both sources agree.
   - **Legitimate disagreement (spec removed, browsers still ship)**: add to `bcd_spec_exceptions.toml` with a `reason` citing the upstream URL. Example: `xlink:href` lives here.
   - **BCD is wrong**: rare but happens (e.g. BCD lags behind a spec revert). Fix: file an upstream BCD issue and add a WIP exception entry referencing the BCD issue URL.

4. Output of the audit is a single Markdown review document at `crates/svg-data/data/reviewed/bcd_deprecation_audit_2026-04.md` capturing: (a) every BCD-deprecated attribute, (b) its classification, (c) the upstream URLs consulted, (d) the fix applied (data edit, exception, upstream bug). This file lives in git as the audit trail — next year's reviewer reads it to understand the current state.

**Snapshot surgery scope — which snapshots to edit**:

The rule is: `present_in` must only include snapshots where the attribute is normatively defined. SVG 1.1 entries are CORRECT and must NOT be removed — `baseProfile` was real in SVG 1.1. Only SVG 2 snapshots where the attribute was removed need fixing.

- If removed from SVG 2 entirely (CR + ED): remove from both `Svg2Cr20181004` and `Svg2EditorsDraft20250914`.
- If present in CR but removed in the ED: remove only from `Svg2EditorsDraft20250914`.
- SVG 1.1 snapshots: never touched by this audit.

For `baseProfile`: its correct `present_in` after the fix is `["Svg11Rec20030114", "Svg11Rec20110816"]`. The `union_lifecycle_expr()` then sees it's absent from the latest snapshot and returns `Obsolete` automatically — no logic change needed.

**Known anchor points** — these are the expected results for the three most-studied cases, which the audit must validate:

| Attribute                                          | Classification                            | Fix                                                                       |
| -------------------------------------------------- | ----------------------------------------- | ------------------------------------------------------------------------- |
| `baseProfile`                                      | Removed in SVG 2 (CR-era spec dropped it) | Remove from Svg2Cr20181004 + Svg2EditorsDraft20250914 snapshots and union |
| `version` (the SVG `version` attribute on `<svg>`) | Removed in SVG 2                          | Same                                                                      |
| `xlink:href`                                       | Legitimate disagreement                   | Exception entry with `spec_says = obsolete`, `bcd_says = not_deprecated`  |

**Snapshot surgery**: the union file is derived from the per-snapshot attribute files, so fixes must cascade:

1. Edit `crates/svg-data/data/specs/Svg2EditorsDraft20250914/attributes.json` — remove the attribute entry (or add an explicit `status: "removed"` marker if we prefer to track "was once here, gone now").
2. Edit `crates/svg-data/data/specs/Svg2Cr20181004/attributes.json` if the attribute was already gone at the CR stage.
3. Regenerate the union: run whatever script currently regenerates `data/derived/union/attributes.json` from the snapshot sources. If no script exists, hand-edit (it's just membership arrays).
4. `cargo check -p svg-data` should now produce `Obsolete` lifecycle for the fixed entries. The build check should stop complaining.
5. Add the now-obsolete attribute to the `obsolete_feature` lint fixture so we have a regression test.

Expected downstream effect: `lifecycle_for_profile` returns `Obsolete` for the removed entries in the latest profile, lint fires `obsolete-feature`, hover shows the new `Forbid` headline with a `ProfileObsolete { last_seen }` reason.

### 4.5 Lint tests

`crates/svg-lint/tests/`:

- New test: `obsolete_feature_fires_on_baseProfile_in_svg2_profile`
- New test: `partial_implementation_fires_on_color_interpolation`
- Update existing deprecated test to assert the richer message format.

---

## Phase 5 — Hover tests + docs + verification

### 5.1 Hover integration tests

`crates/svg-language-server/tests/definitions_and_hover.rs`:

Replace / add:

- **`hover_baseprofile_verdict_is_forbid`**: hovers `<svg baseProfile="full">`, asserts output contains `> ✗ baseProfile`, `**Status:**`, and `removed from`.
- **`hover_renders_version_qualifier_glyphs`**: uses `baseline-shift` (live `≤80` in BCD), asserts `Chrome ≤80` appears.
- **`hover_renders_per_browser_partial_note`**: uses `color-interpolation` (live `partial_implementation: true`), asserts a markdown sub-bullet contains `Chrome:` and `partial`.
- **`hover_no_contradictory_status_lines`**: hovers any deprecated attribute, asserts `Stable in` does NOT appear anywhere in the markdown.
- Update `hover_renders_baseline_qualifier_for_fegaussianblur` — new structural position but same `≤2021` assertion.
- Update `hover_marks_glyph_orientation_horizontal_unsupported_across_chromium_firefox` — new separator (`·`) + new `✗` glyph.

### 5.2 Unit tests for `compat_verdict`

`crates/svg-data/src/verdict.rs`:

- `safe_for_stable_rect` — a `<rect>` in SVG 2 → `Safe`, no reasons.
- `forbid_for_obsolete_baseprofile` — after the data fix, baseProfile → `Forbid` with `ProfileObsolete` reason.
- `avoid_for_bcd_deprecated_only` — a hypothetical attribute deprecated in BCD but not spec-obsolete → `Avoid`.
- `caution_for_partial_only` — any entry with only `partial_implementation` triggers → `Caution`.
- `priority_ordering` — when multiple reasons apply, recommendation takes the strongest.

### 5.3 End-to-end verification

**Rust side:**

1. `cargo check --workspace` — clean, no `cargo::error` from the new BCD↔spec agreement check (assuming data fixes land).
2. `cargo test --workspace` — all green, including new verdict + hover + lint tests.
3. `cargo clippy --workspace --all-targets -- -D warnings` — clean.
4. Open an SVG document with `<svg baseProfile="full">` in a real LSP-enabled editor, hover `baseProfile` → confirm the new blockquote headline renders visibly distinct.
5. Same for `color-interpolation` on a `<path>` → confirm per-browser sub-bullets with partial note.
6. Run `svg-lint` over a fixture file containing deprecated attributes → confirm new rule IDs fire (`obsolete-feature`, `partial-implementation`, `prefix-required`, `behind-flag`).

**Worker side:**

1. `cd workers/svg-compat && deno task test` — all green.
2. `deno task dev` and visit `http://localhost:8000/` — confirm colored chip variants render for entries with `partial_implementation` / `version_removed` / etc. No unstyled "dark matter" chips.
3. `BaselineBadge` hover tooltip surfaces `low_date.raw` / `high_date.raw`.
4. Stats grid shows non-zero counts for the new tiles.
5. Filter input works with new keywords (`removed`, `partial`, `flagged`, `unsupported`).

**Cross-cutting:**
6. Regenerate data via `deno run -A workers/svg-compat/src/cli.ts emit data --out /tmp/svg-compat-data.json`, copy to Rust build cache, `cargo test` — confirm everything still roundtrips.
7. Sanity check the build-time agreement check catches a real regression: manually revert the baseProfile data fix, run `cargo check`, confirm it fails with a clear error message.

---

## Files to modify

| File                                                                  | Kind      | Summary                                                                                                                                                          |
| --------------------------------------------------------------------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/svg-data/src/types.rs`                                        | UPDATED   | Add `VerdictRecommendation`, `VerdictReason`, `CompatVerdict`; add `verdicts: &'static [(SpecSnapshotId, CompatVerdict)]` field to `AttributeDef` + `ElementDef` |
| `crates/svg-data/src/verdict.rs`                                      | NEW       | Pure verdict computation rules (10-rule priority algorithm) + build-time emission helpers                                                                        |
| `crates/svg-data/src/lib.rs`                                          | UPDATED   | Re-export verdict types, `compat_verdict_for_*` public API (linear scan over `def.verdicts`)                                                                     |
| `crates/svg-data/build.rs`                                            | UPDATED   | Compute verdicts per entry per profile; emit interned reason const slices; emit `reconcile_bcd_spec()` with batched errors + self-pruning                        |
| `crates/svg-data/build/codegen.rs`                                    | UPDATED   | `format_verdict()` — emits `&'static [(SpecSnapshotId, CompatVerdict)]` literal syntax                                                                           |
| `crates/svg-data/data/derived/union/attributes.json`                  | UPDATED   | Remove `baseProfile`, `version` from Svg2 membership                                                                                                             |
| `crates/svg-data/data/specs/Svg2EditorsDraft20250914/attributes.json` | UPDATED   | Same fix in upstream snapshot source                                                                                                                             |
| `crates/svg-data/data/reviewed/bcd_spec_exceptions.toml`              | NEW       | Allowlist of legitimate BCD-spec disagreements                                                                                                                   |
| `crates/svg-language-server/src/hover.rs`                             | REWRITTEN | `CompatMarkdownBuilder`, verdict-driven headline/status, qualifier glyphs, per-browser sub-bullets                                                               |
| `crates/svg-language-server/tests/definitions_and_hover.rs`           | UPDATED   | New regression guards, updated existing tests for new structure                                                                                                  |
| `crates/svg-lint/src/rules/mod.rs`                                    | UPDATED   | New `obsolete_feature`, `partial_implementation`, `prefix_required`, `behind_flag` rules; deprecated rule consumes verdict                                       |
| `crates/svg-lint/tests/`                                              | UPDATED   | Tests for new rules                                                                                                                                              |
| `workers/svg-compat/static/style.css`                                 | UPDATED   | CSS for `chip-unsupported`, `chip-removed`, `chip-partial`, `chip-flagged`, `chip-prefixed`                                                                      |
| `workers/svg-compat/src/components/BrowserSupport.tsx`                | UPDATED   | State-class-driven mask selection (no new SVG assets)                                                                                                            |
| `workers/svg-compat/src/components/BaselineBadge.tsx`                 | UPDATED   | Richer `title` attribute with `low_date.raw` / `high_date.raw`                                                                                                   |
| `workers/svg-compat/src/components/StatsGrid.tsx`                     | UPDATED   | New stat tiles                                                                                                                                                   |
| `workers/svg-compat/src/view.ts`                                      | UPDATED   | Extended `PageStats` + computation in `buildPageModel`                                                                                                           |
| `workers/svg-compat/src/render.tsx`                                   | UPDATED   | Populate new stat fields                                                                                                                                         |
| `workers/svg-compat/src/main_test.ts`                                 | UPDATED   | New dashboard render tests                                                                                                                                       |

No new files in the worker (extensions only). Three new files in svg-data (`verdict.rs`, exception TOML, optional `BcdSpecError` type).

---

## Out of scope

- **Container queries for the chip layout** — earlier attempt showed container-type breaks table auto-layout. Keep the viewport breakpoint approach.
- **Full snapshot-review rewrite** — we're only fixing `baseProfile` + `version`. A full audit of membership data against authoritative SVG 2 spec is a separate plan.
- **Runtime BCD refetch** — `RuntimeCompat` / `compat_parse::extract_browser_versions` is a separate code path that still uses the simpler `Unknown | Version(String)` shape for the unpkg-live overlay. It keeps working unchanged; the verdict layer uses the baked catalog as its source.
- **LSP markdown rendering quirks** — some LSP clients render `>` blockquotes inconsistently. The headline must still be legible as plain text if the client strips quoting. This is a UX consideration handled by making the glyph+name prefix stand on its own even without the `>` marker.
- **Lint-severity configuration UI** — new rules land at their default severities. Runtime reconfigurability is a follow-up.
- **Animated diff / changelog rendering on the dashboard** — pure nice-to-have.

---

## Unresolved questions

Minor copy/severity choices that don't affect structure — resolve during implementation:

- baseProfile headline wording: `"removed from SVG 2"` vs `"no longer in SVG 2"` vs `"obsolete in SVG 2"`. Caveman preference: shortest = best.
- `partial-implementation` lint severity: `info` or `hint`. Probably `hint` — it's signal, not a problem you need to fix.
- `BaselineBadge` title copy: human-readable (`"widely available ≤2021-04-02"`) vs raw-ish (`"widely ≤2021"`). Human-readable probably wins; the title is a tooltip, not a CSV.
- CLI `--verdict` flag that dumps computed verdicts alongside `/data.json` for regression diffing across web-features releases. Nice-to-have; out of scope for the initial landing but worth noting as a follow-up.

Resolved via user answers + Oracle consultation:

- **Scope**: full pipeline (Rust hover + lint + verdict + worker dashboard CSS + stats grid + data audit).
- **Build check strictness**: Option A — hard error with exception allowlist, batched emission, self-pruning, paste-ready fix-it messages.
- **Hover glyphs**: Unicode symbols (`✓` / `⚠` / `⊘` / `✗`). Not emoji, not plain text.
- **Data audit**: full audit of every BCD-deprecated attribute against SVG 2 ED + SVG 2 CR + SVG 1.1. Audit output lives in `bcd_deprecation_audit_2026-04.md`.
