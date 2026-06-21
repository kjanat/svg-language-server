# svg-data Spec-Derivation Map & Rust-Only `build.rs` Design

**Date:** 2026-06-03 **Status:** design / roadmap (supersedes the speculative
phasing in issue #9) **Constraint:** the production derivation pipeline runs
**entirely in Rust `build.rs` / `build/` modules — no TypeScript in the build.**
TypeScript is allowed only as throwaway prototyping/verification (validate a
parser, then port to Rust).

This document maps every dataset under `crates/svg-data/data/` to its upstream
spec artifact, judges whether it can be **deterministically and reproducibly
derived**, and specifies the Rust parse strategy. It was produced by a read-only
exploration that inspected the **real upstream artifacts** (W3C TR
`propidx`/`eltindex`/`attindex`/`svgdtd`, vendored svgwg `definitions*.xml`),
not just the repo. See `PIPELINE.md` for the current as-built data flow.

All findings were cross-checked against the live tree:

- `mod spec` is **not** declared in `build.rs:12-25` → `build/spec.rs` is
  orphaned.
- `build/bcd.rs` shells out to `deno` (`bcd.rs:189-194`).
- `[build-dependencies]` = `schemars, serde, serde_json, toml, ureq`; `regex` +
  `winnow` are present transitively in `Cargo.lock`; `quick-xml`/`roxmltree`/
  `scraper` are **not**.
- svgwg is a **gitignored, untracked local discovery clone**
  (`git ls-files svgwg` = 0, no `.gitmodules`) — not a tracked submodule. Its
  HEAD was `bd0b7819` when this was written; the ED provenance pin `19482daf`
  (see §0) was merely absent from that stale clone.

---

## 0. SVG2-ED provenance pin vs. the discovery clone (not a repo bug)

**Correction to an earlier characterization (incl. PIPELINE.md):** `svgwg/` is a
**gitignored, untracked local clone** used only for discovery — not a tracked
submodule, and none of its files are committed. So this is **not** a repo
reproducibility bug; it is a provenance pin pointing at a commit a *stale local
clone* simply hadn't fetched.

The `Svg2EditorsDraft20250914` dataset records provenance pin `19482daf…` (the
svgwg commit dated 2025-09-14 it was captured from). When this was written the
local clone sat behind at `bd0b7819`, so that object failed `git cat-file` —
fixed by a plain `git -C svgwg fetch` (the ED moves fast: one fetch
fast-forwarded ~2891 commits and rewrote `text.html` almost entirely, which is
exactly why a frozen pin exists).

| SHA                     | Role                                                                                                                                                          |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `19482daf` (2025-09-14) | **provenance pin** of the ED snapshot data + manifests (`svg2-ed-20250914.toml`, `SOURCES.md`, `snapshot_schema.rs`, the 6 `Svg2EditorsDraft20250914/*.json`) |
| `bd0b7819`              | the then-checked-out clone HEAD; used by `spec_removals.json` + orphaned `build/spec.rs:11`                                                                   |

**Decision (Q-PIN):** the clone is throwaway — the build may `git clone` svgwg
freely during discovery. Because we **vendor** (Q-VENDOR), the pin's only job is
to record *which commit the vendored artifacts were captured at*. There is no
reason to stay on `19482daf` specifically; it's just the existing dated
snapshot. Re-capturing at a newer commit is a deliberate, separate data refresh
(date the new snapshot accordingly). What must hold: the vendored artifacts and
the recorded source SHA move together, and `spec_removals.json` / `spec.rs` get
re-pinned to the **same** captured commit so the two stop disagreeing.

---

## A. Edition catalog & versioning model (frozen vs rolling + freshness)

**Requirement:** the SVG 2 Editor's Draft must **not** be a dated Rust enum
variant that has to be bumped on every refresh. Only **frozen, immutable**
editions are hard-set in Rust; the ED is a **single undated** variant whose
captured commit/date is **data**, and the LSP reports whether its baked compat
data is still current.

### Frozen editions — hard-set Rust variants, vendored once, never touched

All immutable (dated TR URLs); SVG 1.x is final forever. Adding a variant per
edition is a one-time cost.

| Edition        | Date       | Frozen source URL                                         |
| -------------- | ---------- | --------------------------------------------------------- |
| SVG 1.0 REC    | 2001-09-04 | `https://www.w3.org/TR/2001/REC-SVG-20010904/`            |
| SVG 1.1 FE REC | 2003-01-14 | `https://www.w3.org/TR/2003/REC-SVG11-20030114/` *(have)* |
| SVG 1.1 SE PR  | 2011-06-09 | `https://www.w3.org/TR/2011/PR-SVG11-20110609/`           |
| SVG 1.1 SE REC | 2011-08-16 | `https://www.w3.org/TR/2011/REC-SVG11-20110816/` *(have)* |
| SVG 2 CR       | 2016-09-15 | `https://www.w3.org/TR/2016/CR-SVG2-20160915/`            |
| SVG 2 CR       | 2018-08-07 | `https://www.w3.org/TR/2018/CR-SVG2-20180807/`            |
| SVG 2 CR       | 2018-10-04 | `https://www.w3.org/TR/2018/CR-SVG2-20181004/` *(have)*   |

### Rolling edition — one undated variant

`Svg2EditorsDraft` (drop the `20250914` suffix). Captured svgwg commit + date
live in `snapshot.json` (data) → refreshing the ED = regenerate its data, **no
Rust edit**.\
`SpecSnapshotId::LATEST = Svg2EditorsDraft`.\
Sources: `https://svgwg.org/svg2-draft/` (+ `single-page.html`), repo
`https://github.com/w3c/svgwg`.

Today the enum (`src/types.rs:410`) has 4 dated variants with
`LATEST = Svg2EditorsDraft20250914`, referenced in **37 places across 16
files**; renaming to an undated `Svg2EditorsDraft` (date → `snapshot.json` data)
is the mechanical change that kills the date-bumping toil.

### Freshness / usability signal (LSP feature — the point of capturing editions)

- **Frozen editions**: never stale — report "final"; for SVG 2 link the
  `https://www.w3.org/TR/SVG2/` "latest published" pointer for context.
- **Rolling ED**: record captured commit/date at build time; the LSP compares it
  against the live ED (`github.com/w3c/svgwg` HEAD / `svgwg.org/svg2-draft`) and
  tells the user *"your SVG 2 ED compat data is from `<date>` (`<short-sha>`);
  latest is `<date>` — N commits behind / current."* Network check stays opt-in
  so offline use degrades gracefully.

### Edition discovery & freshness via the W3C API

Don't hardcode the edition list — the **W3C API** (`api.w3.org`, public, no
auth, JSON, ISO-8601 dates, rate limit 6000/IP/10min) is the authoritative
source:

- `GET /specifications/{shortname}/versions?embed=1` → every published version
  with `date`, `status`, `uri` (the dated TR URL). Shortnames: **`SVG`** (1.0),
  **`SVG11`**, **`SVG2`**.
- `GET /specifications/{shortname}/versions/latest` → redirect to the latest
  published version — drives the "is there a newer published edition?" check.
- For the rolling ED (not on `/TR/`), freshness compares against the svgwg git
  repo HEAD (`github.com/w3c/svgwg`).

Authoritative milestone inventory (REC/PR/CR — pulled live 2026-06-03; WDs
omitted, available but low value):

| shortname | date                    | status       | URI                                      |
| --------- | ----------------------- | ------------ | ---------------------------------------- |
| SVG       | 2001-09-04              | REC          | `…/TR/2001/REC-SVG-20010904/`            |
| SVG       | 2001-07-19              | PR           | `…/TR/2001/PR-SVG-20010719/`             |
| SVG       | 2000-11-02 / 2000-08-02 | CR           | `…/TR/2000/CR-SVG-2000{1102,0802}/`      |
| SVG11     | 2011-08-16              | REC (SE)     | `…/TR/2011/REC-SVG11-20110816/` *(have)* |
| SVG11     | 2011-06-09              | PR (SE)      | `…/TR/2011/PR-SVG11-20110609/`           |
| SVG11     | 2003-01-14              | REC (FE)     | `…/TR/2003/REC-SVG11-20030114/` *(have)* |
| SVG11     | 2002-11-15 / 2002-04-30 | PR / CR (FE) | `…/TR/2002/…`                            |
| SVG2      | 2018-10-04              | CR           | `…/TR/2018/CR-SVG2-20181004/` *(have)*   |
| SVG2      | 2018-08-07              | CR           | `…/TR/2018/CR-SVG2-20180807/`            |
| SVG2      | 2016-09-15              | CR           | `…/TR/2016/CR-SVG2-20160915/`            |

The user's requested editions map 1:1 to these REC/PR/CR milestones. **Capture
priority = REC/PR/CR; WDs optional.** Build vs runtime split: the build derives
from **vendored** dated artifacts (the API may be used offline-gated at
*capture* time to resolve URLs + record `status`/`date` per snapshot); the
**LSP** hits the API (opt-in) only for the runtime freshness signal.

#### What the API actually returns (payload + scope)

> **Scope: bibliographic metadata ONLY — no spec content.** The W3C API carries
> *publication* metadata (versions, dates, status, editors, groups) and at most
> a one-paragraph abstract per series (`description`). It contains **zero**
> technical spec data — no elements, attributes, properties, value grammars, or
> content models. All of that still comes from parsing the **vendored spec
> documents** (propidx.html, `definitions*.xml`, DTD, chapter HTML). The API is
> purely the *edition-index + freshness* layer, never a content source.

Format: **HAL+JSON** — every response is a pagination envelope
`{ page, limit, pages, total, _links, _embedded }`; related resources are linked
(`_links`) or inlined (`_embedded`) per `?embed=1`. Payloads are tiny:

| Request                                             | Bytes                         |
| --------------------------------------------------- | ----------------------------- |
| `/specifications/{SVG,SVG11,SVG2}/versions?embed=1` | ~17–21 KB each (~55 KB total) |
| same, no `embed` (links only)                       | ~2.3 KB each                  |
| single version detail                               | ~1.1 KB                       |
| series root `/specifications/SVG2`                  | ~1.2 KB                       |

Per-version object (11 typed fields, all the index/freshness layer needs):
`status`, `rec-track` (bool), `editor-draft` (ED URL), `uri` (dated TR URL —
**the vendor target**), `date` (ISO 8601), `implementation-feedback-due`,
`informative` (bool), `title`, `shortlink` (the `/TR/SVG2/` latest pointer),
`process-rules`, plus `_links` (self/editors/deliverers/specification/
predecessor-version). `…/versions/latest` is a **302 redirect** to the latest
dated version resource — latest-published date with one HEAD, no body parse.

The whole SVG edition universe is ~55 KB of structured JSON over 3 GETs → cheap
to **vendor as a static edition index** and refresh occasionally; a Rust struct
over `_embedded.version-history[]` (`serde_json`, ignore `_links`) is all the
build needs.

### How editions get populated

Each frozen edition's snapshot data is **derived by the pipeline** (vendor the
TR artifact → parse in `build.rs` → generate/audit) — **not hand-seeded**.
"Capture these editions" therefore means *vendor their artifacts + run the
derivers*, not transcribe more snapshots by hand (which is the toil issue #9
exists to kill).

### Profiles (SVG Native & the SVG family) — constraint layers, not versions

A **profile** is a *subset* of a base edition, not a point-in-time version. The
repo already models profiles (`baseProfile`; profile verdicts in
`crates/svg-language-server/src/hover.rs:216`), so these slot into the existing
profile axis rather than the version axis.

**SVG Native** (the immediate ask):

- Source: `svgwg/specs/svg-native/index.bs` — a **Bikeshed** doc in the svgwg
  repo (`Title: SVG Native`, `Shortname: svg-native`, `Status: ED`,
  `Group: SVG`); published at `https://svgwg.org/specs/svg-native/`. **Rolling
  like the SVG2 ED** (not on `/TR/` — the W3C API knows the shortname but has no
  dated versions). → treat as an **undated `SvgNative` profile**, capture
  commit/date as data, freshness vs svgwg git HEAD.
- **This is real spec data** (unlike the W3C API). SVG Native is defined as
  *reductive differences* from SVG 2 Secure Static Mode: explicit lists of
  **unsupported** elements / attributes / properties / values (e.g. no `text`/
  `tspan`/`marker`/`pattern`/`symbol`/`switch`/`style`; no `display`/`color`/
  `pointer-events`/`clip`; no percentage or relative lengths) **plus a few
  supported-only allowlists** (transform-bearing elements; units
  px/pt/pc/mm/cm/in; image formats JPEG/PNG/APNG; `gradientUnits=userSpaceOnUse`
  only).
- Derivation: parse the reductive prose ("X is not supported by SVG Native") and
  the supported-only lists into a **constraint set layered over SVG 1.1/SVG 2**
  — heuristic prose parsing (like `spec_scan`), from the generated HTML or the
  `.bs` source. Output feeds the profile axis so the LSP can flag *"not
  available in the SVG Native profile."*

**Broader SVG family** (related, out-of-scope for now but the same two axes):
other *profiles* — **SVG Tiny 1.2**, **SVG Basic**, **Mobile** (all subsets, all
on the W3C API) — and SVG 2 *modules* — **Markers, Strokes, Paths, Integration,
AAM** (`svg-markers`, `svg-strokes`, `svg-paths`, `svg-integration`,
`svg-aam-1.0`). Worth capturing later via the same profile/module model; flagged
so the design's edition/profile split anticipates them.

---

## 1. Derivability matrix

Every dataset under `crates/svg-data/data/` (paths abbreviated `data/...`).
"Derivable" = can be deterministically regenerated (or drift-audited) from a
pinned spec artifact.

| #  | Source                                                                                                                                                                       | Repo path                                                                                                                                                                       | Maintained today                                                                | Upstream artifact                                                                        | Derivable      | Rust parse strategy                                                                                                                   | Risk                                                                                                                                            |
| -- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- | -------------- | ------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| 1  | enum-* keyword grammars (presentation props: stroke-linejoin, fill-rule, text-anchor, visibility, display, overflow, pointer-events, font-style/-weight, dominant-baseline…) | `data/specs/*/grammars.json`                                                                                                                                                    | seed then hand-curated keyword lists (`examples/generate_snapshot_seed.rs:442`) | SVG1.1 `propidx.html` Values column                                                      | **yes**        | `tl`/`scraper` over `table.proptable tbody tr` → `winnow` value-syntax parser; strip `[inherit]`/`<datatype>` → `Choice{keyword}`     | SVG2 propidx omits many enums → need SVG1.1; per-edition pin                                                                                    |
| 2  | enum-* element-value attrs (spreadMethod, gradientUnits, in, accumulate, additive)                                                                                           | `data/specs/*/grammars.json`                                                                                                                                                    | same as #1                                                                      | chapter prose (`pservers.html`, `filters.html`, `animate.html`) `dl.attrdef-values`      | **yes**        | `tl`: `<dt>Value</dt>`→sibling `<dd>` → `winnow`; name→file map from `definitions.xml` href anchors                                   | NOT in propidx; `definitions.xml` omits enum values; multi-file scrape; `in` mixes keyword+datatype                                             |
| 3  | preserve-aspect-ratio                                                                                                                                                        | `data/specs/*/grammars.json`                                                                                                                                                    | seed then curated                                                               | `coords.html` prose BNF                                                                  | **partial**    | **const-gen** (closed 10-align + meet/slice; edition-key `defer`)                                                                     | prose not a table; low payoff vs const                                                                                                          |
| 4  | transform-list                                                                                                                                                               | `data/specs/*/grammars.json`                                                                                                                                                    | seed then curated                                                               | `coords.html` `<pre>` BNF; SVG2→CSS Transforms 1                                         | **partial**    | 6 fn names = const; deriving needs `winnow` over `transform ::=`                                                                      | `<pre>` BNF; SVG2 indirection                                                                                                                   |
| 5  | color                                                                                                                                                                        | `data/specs/Svg11*/grammars.json` (SVG2→ForeignRef)                                                                                                                             | generated-from-catalog                                                          | `types.html` `<color>`; SVG2→CSS Color 4                                                 | **no**         | const ref; runtime parser                                                                                                             | open datatype                                                                                                                                   |
| 6  | length / number-or-percentage                                                                                                                                                | `data/specs/Svg11*/grammars.json` (SVG2→ForeignRef)                                                                                                                             | generated-from-catalog                                                          | `types.html`; CSS Values 3                                                               | **no**         | const ref; runtime parser                                                                                                             | open datatype                                                                                                                                   |
| 7  | points + coordinate-pair                                                                                                                                                     | `data/specs/*/grammars.json`                                                                                                                                                    | generated-from-catalog                                                          | `shapes.html` BNF                                                                        | **no** (const) | const; runtime `winnow`                                                                                                               | `coordinate-pair` is dead data (never special-cased)                                                                                            |
| 8  | url-reference                                                                                                                                                                | `data/specs/*/grammars.json`                                                                                                                                                    | generated-from-catalog                                                          | `types.html` `<FuncIRI>`                                                                 | **no**         | const ref; SVG2 foreign url maps **hard-coded** `build.rs:1018-1024`                                                                  | new url props missed                                                                                                                            |
| 9  | view-box                                                                                                                                                                     | `data/specs/*/grammars.json`                                                                                                                                                    | generated-from-catalog                                                          | `coords.html` prose                                                                      | **no** (const) | const 4-number Sequence                                                                                                               | none                                                                                                                                            |
| 10 | path-data                                                                                                                                                                    | `data/specs/*/grammars.json`                                                                                                                                                    | generated-from-catalog                                                          | `paths.html` BNF                                                                         | **no**         | opaque `DatatypeRef`; bespoke runtime `winnow` path parser                                                                            | feeding BNF to a generic parser is wrong                                                                                                        |
| 11 | element presence per edition                                                                                                                                                 | `data/specs/*/elements.json` (seed); `data/derived/union/elements.json`, `overlays/*.json` (**generated** Rust, `examples/generate_derived_membership.rs`→`src/derived.rs:156`) | seed=hand; union/overlays=generated                                             | SVG2 `definitions*.xml` `<element>`; SVG1.1 flat DTD / eltindex                          | **partial**    | `roxmltree` over `definitions*.xml` (local_name==`element`); fold via `derived.rs:260` `membership_records`                           | overcount trap (17 `<elementcategory>`); 5 SMIL elems only in foreign anim spec; SVG1.1 not vendored                                            |
| 12 | attribute presence + value_syntax link                                                                                                                                       | `data/specs/*/attributes.json`                                                                                                                                                  | hand-curated                                                                    | SVG2 `definitions.xml` `<attribute>` (name/href/animatable/elements); SVG1.1 DTD ATTLIST | **partial**    | presence: `roxmltree`/DTD lexer; **value_syntax: NOT derivable** — load curated + assert `grammar_id` resolves                        | value_syntax editorial; DTD not vendored; attindex template empty                                                                               |
| 13 | element↔attribute matrix                                                                                                                                                     | `data/specs/*/element_attribute_matrix.json`                                                                                                                                    | seed from hardcoded Rust tables (`generate_snapshot_seed.rs:306`), frozen       | **3 different per edition**: SVG1.1 flat DTD; SVG2-CR TR HTML; SVG2-ED `definitions.xml` | **partial**    | SVG2-ED: `roxmltree`+category split (easy); SVG1.1: DTD-entity-expansion lexer (`winnow`+`regex`, hard); SVG2-CR: `scraper` (brittle) | 3 parsers; local `definitions.xml` partial vs frozen 4434 edges; requirement uniformly "optional"                                               |
| 14 | element category memberships                                                                                                                                                 | `data/specs/*/categories.json`                                                                                                                                                  | generated by seed from compiled catalog (self-referential)                      | SVG2 `<elementcategory>`; SVG1.1 DTD `%*.class`                                          | **partial**    | `roxmltree` split `elements=`; SVG1.1 `winnow`/`regex`                                                                                | taxonomy mismatch (16 repo ids vs ~12 spec); filter_primitive/light_source/transfer_function have NO upstream dfn; **not consumed by build.rs** |
| 15 | content models (children)                                                                                                                                                    | `data/specs/*/elements.json` `content_model`                                                                                                                                    | hand-transcribed                                                                | SVG2 `definitions.xml` `contentmodel`/`elementcategories`; SVG1.1 DTD                    | **partial**    | `roxmltree` (SVG2); `winnow` 2-pass entity resolver (SVG1.1)                                                                          | prose `<x:contentmodel>` (a, textPath, switch) not machine-encoded                                                                              |
| 16 | curated catalog elements                                                                                                                                                     | `data/elements.json`                                                                                                                                                            | hand-curated                                                                    | MDN / BCD (descriptions); `definitions.xml` (structural)                                 | **partial**    | mdn_url=template; structural=`roxmltree`; description=brittle scrape                                                                  | **only `{name,attrs}` load-bearing** (`build.rs:356-362`); other 7 fields dead                                                                  |
| 17 | curated catalog attributes                                                                                                                                                   | `data/attributes.json`                                                                                                                                                          | hand-curated                                                                    | MDN; grammars                                                                            | **partial**    | n/a                                                                                                                                   | **NO build.rs consumer — fully orphaned/legacy**                                                                                                |
| 18 | spec prose → catalog description                                                                                                                                             | `data/specs/*/{elements,attributes}.json` `title`                                                                                                                               | hand-transcribed spec text                                                      | svgwg chapter HTML lead `<p>` at anchor                                                  | **yes**        | `roxmltree` anchor + `scraper`/`tl` `[id$=Element] + p` (algorithm in orphaned `build/spec.rs:88-204`)                                | heuristic prose selection; per-record pins ≠ single SHA                                                                                         |
| 19 | BCD svg subtree                                                                                                                                                              | consumed `build/bcd.rs`; produced by Deno worker; pinned `workers/svg-compat/deno.json:12` (@mdn/browser-compat-data@7.3.11)                                                    | **generated** via `deno run` shell-out                                          | npm `@mdn/browser-compat-data@7.3.11` data.json svg subtree                              | **yes**        | Vendor sliced `data/sources/bcd-7.3.11.svg.json` + sha256; `serde_json`; port `lib/parse.ts`+`lib/build.ts`                           | npm data not spec; ~500 lines TS must port byte-equivalent; `?bcd=latest` non-reproducible                                                      |
| 20 | web-features baseline                                                                                                                                                        | consumed via BCD join; pinned `deno.json:21` (web-features@3.23.0)                                                                                                              | **generated** via worker                                                        | npm `web-features@3.23.0` data.json                                                      | **yes**        | Vendor sliced + sha256; `serde_json`; port `parseBaseline`/`extractBaseline`                                                          | must vendor both as coherent pair; untyped date strings (warn-never-discard)                                                                    |
| 21 | spec_removals.json                                                                                                                                                           | `data/reviewed/spec_removals.json`; consumed `build/reconcile.rs:49,248`                                                                                                        | **generated** by Deno `spec_scan.ts`                                            | svgwg `definitions*.xml` + `text.html` + `changes.html` (vendored)                       | **yes**        | `quick-xml` + `regex`; port 3 scan fns → `build/spec_scan.rs`                                                                         | HTML prose heuristic; glyph-orientation dual-record quirk                                                                                       |
| 22 | bcd_spec_exceptions.toml                                                                                                                                                     | `data/reviewed/bcd_spec_exceptions.toml`                                                                                                                                        | **curated** (human verdict)                                                     | SVG2 prose citation                                                                      | **no**         | `toml` parse only (`reconcile.rs:48`)                                                                                                 | **stays manual**; self-prunes via build error                                                                                                   |
| 23 | source manifests (4)                                                                                                                                                         | `data/sources/svg*.toml`                                                                                                                                                        | hand-transcribed                                                                | TR URLs / svgwg commit                                                                   | **partial**    | `toml`+serde                                                                                                                          | provenance ledger, not a build input                                                                                                            |
| 24 | foreign-references.toml                                                                                                                                                      | `data/sources/foreign-references.toml`                                                                                                                                          | curated allowlist                                                               | external W3C/CSSWG specs                                                                 | **no**         | `toml` (gates `source_id`, `provenance_gate.rs:177`)                                                                                  | **stays manual**                                                                                                                                |
| 25 | placeholder_attribute_names.txt                                                                                                                                              | `data/placeholder_attribute_names.txt`                                                                                                                                          | curated blocklist                                                               | none (synthetic keys)                                                                    | **no**         | `include_str!` + split (already pure Rust, `build.rs:754`)                                                                            | **stays manual**                                                                                                                                |
| 26 | JSON Schemas                                                                                                                                                                 | `data/schemas/*.schema.json` + `catalog.json`                                                                                                                                   | **already generated** from Rust types                                           | Rust types (internal contract)                                                           | **yes (done)** | `schemars::schema_for!` on `src/types.rs`/`src/derived.rs` (`examples/generate_schemas.rs:40-66`); types derive `JsonSchema`          | currently a manual `cargo run --example generate_schemas` step — optionally wire into build as a drift-audit gate                               |

---

## 2. Per-edition upstream artifact availability

| Edition                          | propidx                                                              | eltindex                                                                        | attindex                                                                          | DTD                                                   | definitions.xml                                 | Pin                                     |
| -------------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- | ----------------------------------------------------- | ----------------------------------------------- | --------------------------------------- |
| **SVG 1.1 FE (REC 2003-01-14)**  | TR `…/2003/REC-SVG11-20030114/propidx.html`                          | TR, flat `<ul><li><a>`                                                          | TR, real `<table>`                                                                | TR `…/DTD/svg11-flat-20030114.dtd` (**not vendored**) | N/A                                             | **dated TR URL** (immutable)            |
| **SVG 1.1 SE (REC 2011-08-16)**  | TR `…/2011/REC-SVG11-20110816/propidx.html`                          | TR, flat list (verified)                                                        | TR, real table (verified)                                                         | `…/DTD/svg11-flat-20110816.dtd` (**not vendored**)    | N/A                                             | **dated TR URL**; archive zip available |
| **SVG 2 CR (2018-10-04)**        | TR `…/2018/CR-SVG2-20181004/propidx.html`                            | TR, rendered (verified)                                                         | TR, published table                                                               | removed in SVG2                                       | not pinned for CR                               | **dated TR URL**                        |
| **SVG 2 ED (svgwg, 2025-09-14)** | `svgwg/master/propidx.html` (**inline-rendered, usable; to vendor**) | `svgwg/master/eltindex.html` = **`<edit:elementindex/>` placeholder — USELESS** | `svgwg/master/attindex.html` = **`<edit:attributetable/>` placeholder — USELESS** | removed                                               | `svgwg/master/definitions*.xml` (**to vendor**) | **git SHA — provenance pin (see §0)**   |

> The table covers only the four editions inspected so far. The additional
> frozen editions in §A (SVG 1.0 2001, SVG 1.1 PR 2011-06, SVG 2 CR 2016-09 /
> 2018-08) follow the same dated-TR shape (propidx/eltindex/attindex + DTD for
> SVG 1.x; CR HTML for SVG 2 CR) but their per-artifact availability must be
> **verified at capture time**, not assumed.

**Divergences that matter:**

- **Indexes**: SVG1.1 ships pre-rendered eltindex/attindex on W3C TR
  (scrapeable). SVG2-ED's are unexpanded `<edit:*/>` templates → zero rows; ED
  inventory **must** come from `definitions.xml`, SVG2-CR from the published
  dated TR index.
- **Machine-readability**: SVG2 has structured `definitions*.xml`; SVG1.1 has
  only the flattened SGML DTD + rendered HTML — no XML.
- **`propidx.html` is the exception** — inline-rendered even in `svgwg/master`,
  the only SVG2 index usable directly from the clone.
- **Pinning**: SVG1.1 + SVG2-CR use immutable dated TR URLs (stable; vendor them
  per Q-VENDOR so the build is network-free). SVG2-ED uses a git SHA as a
  **provenance pin** on the captured/vendored artifacts (§0) — not a constraint
  on the throwaway discovery clone.

---

## 3. `build.rs` architecture (Rust-only)

Extend the existing `#[path]` modules (`build/bcd.rs`, `reconcile.rs`,
`provenance_gate.rs`, `verdict.rs`, `codegen.rs`). Reuse:

- **`ensure_cached(url, dest, offline)`** (`build.rs:364`) — `ureq` fetch →
  cache, offline branch, 24h TTL (`CACHE_MAX_AGE_SECS`). No new fetch infra.
- **generate-OR-audit-and-gate** — mirror `reconcile.rs:248-280`: derive in
  Rust, diff against checked-in data, `cargo::error!` on undocumented drift. New
  derivers feed the **same** gates (`provenance_gate.rs:83`, `reconcile.rs`
  3-signal, `derived.rs:184` `UnresolvedReview`) so `codegen.rs` is unchanged.

| Module                        | Purpose                                                                                                                               | Crate               | Replaces                              |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | ------------------- | ------------------------------------- |
| `build/spec_scan.rs` (new)    | Port `spec_scan.ts`: `definitions*.xml` + `text.html`/`changes.html` → audit `spec_removals.json`                                     | `quick-xml`+`regex` | `workers/svg-compat/src/spec_scan.ts` |
| `build/bcd.rs` (extend)       | Drop `deno` shell-out (`bcd.rs:189`); parse **vendored** sliced BCD + web-features JSON (+sha256); port `lib/parse.ts`+`lib/build.ts` | `serde_json`        | Deno worker (build path)              |
| `build/spec_xml.rs` (new)     | Shared `definitions*.xml` reader (element/attribute/category w/ indirection)                                                          | `roxmltree`         | —                                     |
| `build/dtd.rs` (new, P4 only) | Flat-DTD entity-expansion lexer → SVG1.1 enums/ATTLIST edges                                                                          | `winnow`+`regex`    | hand-baked SVG1.1 data                |
| `build/value_syntax.rs` (new) | CSS value-def-syntax / `<pre>` BNF → `GrammarNode`; strip `[inherit]`                                                                 | `winnow` (present)  | hand-authored enum leaves             |
| `build/membership.rs` (new)   | Call existing `src/derived.rs::build_membership_artifacts` from the build (today only `examples/`)                                    | reuse               | manual `cargo run --example`          |

**`[build-dependencies]` additions:** `quick-xml`, `roxmltree`, `tl` (prefer
over `scraper`; escalate to `scraper` only if `[id$=Element] + p` selectors
prove necessary). `regex`/`winnow` already transitive → zero risk. Avoid
`chumsky` (large) and `html5ever`.

**Vendor-over-fetch (decided, Q-VENDOR):** capture svgwg artifacts from a
build-time clone and vendor them; vendor the gaps as sliced files + sha256
(`data/sources/svg11-flat-*.dtd`, SVG1.1/SVG2-CR rendered indexes,
`bcd-<ver>.svg.json`, `web-features-<ver>.json`); build-time fetch only behind
`SVG_DATA_OFFLINE`; `cargo::rerun-if-changed` each vendored file (pattern at
`build.rs:425-431`).

---

## 4. Corrected, evidence-based phasing (replaces issue #9 P1-P4)

| Phase                 | What                                                                                            | Why                                                                                     | Inputs (all vendored)                         |
| --------------------- | ----------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- | --------------------------------------------- |
| **P0**                | Port `spec_scan.ts` → `build/spec_scan.rs`                                                      | Only true parser-first dataset; removes last build TS from the gate; pure pattern-match | `definitions*.xml`/`text.html`/`changes.html` |
| **P0**                | De-Deno BCD/web-features (vendor sliced JSON + sha256)                                          | Removes `deno` shell-out; hermetic/offline build; `serde` present                       | npm slices                                    |
| **P1**                | SVG2-ED presence + matrix + categories from `definitions*.xml`                                  | Cleanest structured source; reuse `derived.rs` fold                                     | `definitions*.xml` + `category_map.toml`      |
| **P2**                | Enum leaves: presentation props from SVG2 `propidx.html`                                        | The one usable in-repo SVG2 index                                                       | `propidx.html` + `winnow`                     |
| **P3**                | Enum leaves: element-value attrs from chapter prose; `title` descriptions                       | Multi-file scrape, heuristic; revive `spec.rs` logic                                    | chapter HTML + `tl`                           |
| **P4**                | SVG1.1 presence/matrix/enums from flat DTD (vendor DTD first)                                   | Hardest: bespoke DTD lexer, non-hermetic until vendored                                 | flat DTD                                      |
| **Never auto-derive** | structured grammars (#3-10); curated exceptions (#22,24,25); prose content models; taxonomy map | const-gen or curated                                                                    | const / `toml`                                |

**Net correction vs issue #9:** the old ordering inverted difficulty (easy
const-gen grammars "later", hard heterogeneous matrix/categories "early"),
pointed at wrong sources (MDN for descriptions, propidx for all enums, single
`definitions.xml` for the whole matrix), and missed the two highest-leverage
TS-removal wins (spec_scan port + BCD de-Deno) entirely. **P0 (de-Deno +
spec_scan port) precedes everything; ED work (P1) just needs the vendor capture
pinned to one agreed commit (§0) — provenance hygiene, not a blocker.**

---

## 5. Stays curated (not machine-derivable)

| Dataset / field                                                                                               | Why no machine source                                                              | Mechanism                                                                                                                                   |
| ------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `value_syntax` grammar_id/foreign_ref linkage                                                                 | editorial attr→grammar mapping; `definitions.xml` has no value field               | keep curated; build.rs **validates only** (`grammar_id` resolves, cf. `build.rs:1052`)                                                      |
| structured grammars (#3-10)                                                                                   | open datatypes / fixed shapes / hard-coded foreign forwards (`build.rs:1018-1024`) | **const-gen**; only enum leaves derive                                                                                                      |
| `bcd_spec_exceptions.toml`                                                                                    | human verdict where deterministic sources legitimately disagree                    | existing allowlist + `added` + `upstream_ref`; **self-prunes** via `reconcile.rs` error — **the template for all reviewed-exception files** |
| `foreign-references.toml`                                                                                     | editorial; gates allowed `source_id`                                               | curated allowlist                                                                                                                           |
| `placeholder_attribute_names.txt`                                                                             | synthetic non-SVG keys                                                             | curated blocklist (`include_str!`)                                                                                                          |
| prose content models (a, textPath, switch)                                                                    | free prose, not encoded                                                            | curated overlay via existing `exceptions.json`                                                                                              |
| category taxonomy renames (filter_primitive/light_source/transfer_function — no upstream `<elementcategory>`) | repo invents categories from DTD groupings                                         | **new `category_map.toml`**                                                                                                                 |
| descriptions (MDN-style prose)                                                                                | MDN HTML JS-heavy/unstable                                                         | hand-written, or redirect to derivable spec `title` (#18)                                                                                   |

**New reviewed-exception files** (all following the `bcd_spec_exceptions.toml`
entry + `added` + `upstream_ref` + self-prune-on-no-match pattern):
`category_map.toml`, `content_model_overrides.toml`, optional
`value_syntax_overrides.toml`.

---

## 6. Decisions (resolved 2026-06-03)

- **Q-EDITIONS → frozen are hard-set, ED is rolling/undated + freshness (see
  §A).** Capture all reachable frozen editions (SVG 1.0 2001, SVG 1.1 FE 2003,
  SVG 1.1 PR 2011-06, SVG 1.1 SE 2011-08, SVG 2 CR 2016-09 / 2018-08 / 2018-10)
  as hard-set Rust variants, derived once from their dated TR artifacts. The
  Editor's Draft becomes a single **undated** `Svg2EditorsDraft` variant whose
  captured commit/date is data (no Rust bump on refresh). The LSP surfaces a
  freshness signal: frozen = "final"; ED = baked-capture vs live, "N commits
  behind / current."
- **Q-PIN → provenance, not a constraint.** `svgwg/` is a gitignored throwaway
  discovery clone; the build may `git clone` it freely. The pin only records
  which commit the vendored artifacts were captured at. No need to stay on
  `19482daf`; re-capturing at a newer commit is a deliberate, separately-dated
  data refresh. Keep the vendored artifacts and recorded SHA in lockstep, and
  re-pin `spec_removals.json` / `spec.rs` to the **same** captured commit. (See
  §0.)
- **Q-VENDOR → vendor.** Vendor the un-vendored artifacts (SVG1.1 DTD + indexes,
  SVG2-CR HTML, BCD/web-features slices) so the build is hermetic and
  network-free; build-time fetch stays as the `SVG_DATA_OFFLINE`-gated fallback
  only.
- **Q-INHERIT → strip.** Drop `[inherit]` from derived enums, matching the
  existing `grammars.json` shape.
- **Q-BCD-FALLBACK → hard error.** With a vendored BCD slice always present, a
  load failure is a **build error** (no silent empty-map degrade). The
  non-reproducible `?bcd=latest` path stays **only** for the worker / UI
  preview, never the build.
- **Q-TAXONOMY → re-align.** Re-align the category taxonomy to SVG2
  `<elementcategory>` names to shrink the manual mapping surface (the
  filter-subtree inventions still need a small curated map, but the bulk
  aligns).
- **Q-ORPHANS → revive + delete.** Revive `build/spec.rs` as the P3 description
  scraper; delete dead `data/attributes.json` (no consumer); fix the stale
  `build/AGENTS.md` entry that still lists `spec.rs` as live.
- **Q-SCHEMAS → already done; documented.** `data/schemas/*` are **already**
  schemars-generated from the Rust types (`examples/generate_schemas.rs`); the
  types are the source of truth, the committed schemas are the build artifact.
  Recorded in `crates/svg-data/AGENTS.md` so it isn't re-litigated. Optional
  follow-up: wire the generator into the build as a drift-audit gate.
