# PR #14 Review Findings

Review target: PR #14, `origin/master...HEAD`.

Method: three independent `code-reviewer` subagents reviewed integration,
extractor/data, and automation/runtime. Oracle then reviewed every candidate for
accuracy and severity. No candidate was rejected as false positive.

## Severity Summary

Critical: none.

High:

1. `just verify` is broken after script migration.
2. Runtime BCD refresh does not override baked compat verdicts.
3. Vendored provenance hashes are recorded but not verified.

Medium:

1. Full spec inventory is not wired into LSP/lint behavior.
2. SVG Native/profile constraints are data-only in LSP.
3. Freshness automation detects drift but does not update data.
4. `spec_scan` omits `definitions-animations.xml`.
5. `spec_scan_repro` picks an arbitrary `svgwg-*` source dir.
6. Deno worker/deploy checks are outside `just verify`.
7. No general PR CI runs the project verification gate.
8. Freshness CI ignores BCD/web-features drift.
9. Freshness sentinel network calls/workflow lack timeouts.
10. SVG autodetection only honors root `version`.
11. LSP config cannot select edition-keyed inventories beyond `SpecSnapshotId`.

Low:

1. Freshness uses SVGWG repo HEAD, causing path-irrelevant false positives.
2. Runtime compat fetch has no setting/opt-out.
3. Publish workflow uses moving Node `latest`.
4. `svg-data` inventory docs are stale after inventory expansion.
5. `svg-data/build/AGENTS.md` still says `spec.rs` is orphaned.
6. `refresh-spec.yml` comment still says SVGWG `master`.

## Findings

### 1. High - `just verify` is broken after script migration

File: `justfile:96-108`

Issue: `just typecheck` still runs `bun --cwd=scripts typecheck`, but this PR
deletes `scripts/package.json`; the replacement task lives in
`scripts/deno.jsonc`.

Evidence: `justfile:96-99` calls Bun. `scripts/package.json` is absent.
`scripts/deno.jsonc:2-6` defines the replacement `typecheck` task.

Why it matters: `just verify` fails before release-config check, lint, or tests.
That breaks the repo's local quality gate exactly while this PR claims future
quality control.

Suggested fix: change `just typecheck` to
`deno task --config scripts/deno.jsonc typecheck`, or restore a
`scripts/package.json` with a matching Bun script.

### 2. High - runtime BCD refresh does not override baked compat verdicts

Files: `crates/svg-language-server/src/compat.rs:39-59`,
`crates/svg-language-server/src/hover.rs:353-418`,
`crates/svg-lint/src/rules/mod.rs:233-249`,
`crates/svg-lint/src/rules/mod.rs:503-529`

Issue: dynamic BCD fetch updates only a limited overlay. It does not recompute
or override `CompatVerdict`, which drives hover headline/status and lint
advisory hints.

Evidence: `RuntimeCompat::to_lint_overrides` keeps only `deprecated` and
`experimental` flags. Hover still calls `svg_data::compat_verdict_for_element`
and `svg_data::compat_verdict_for_attribute`. Lint still calls the same baked
verdict functions for attributes and elements.

Why it matters: newer BCD can change support, partial support, flags, versions,
notes, or removed/unsupported status, but the user-facing verdict/advisory can
stay stale. The PR only partially achieves dynamic up-to-date compat.

Suggested fix: parse runtime BCD into a runtime verdict shape and feed it into
hover and lint, or allow `compat_verdict_for_*` to accept runtime overrides.

### 3. High - vendored provenance hashes are recorded but not verified

Files: `crates/svg-data/tests/source_manifests.rs:5-11`,
`crates/svg-data/build/provenance_gate.rs:117-145`,
`crates/svg-data/data/sources/svgwg-19482daf/PROVENANCE.toml:88-95`

Issue: new `PROVENANCE.toml` files record `sha256`, `git_blob`, and sometimes
byte facts, but no test/build gate verifies those facts against the vendored
bytes.

Evidence: `source_manifests.rs` only checks old top-level manifest field
presence. `provenance_gate.rs` validates curated `source_id` references, not
source bytes. The new provenance file records exact hashes for files such as
`definitions-animations.xml`, but nothing checks them.

Why it matters: vendored spec files can be edited, reformatted, or corrupted
while provenance still claims pristine upstream bytes. That undercuts
reproducibility and trust in the extraction pipeline.

Suggested fix: add a verifier that walks `data/sources/**/PROVENANCE.toml`,
hashes referenced files, compares `sha256`/`bytes`/`git_blob`, and runs in tests
or the build gate.

### 4. Medium - full spec inventory is not wired into LSP/lint behavior

Files: `crates/svg-data/src/lib.rs:425-434`,
`crates/svg-language-server/src/completion.rs:437-460`,
`crates/svg-lint/src/rules/mod.rs:131-137`,
`crates/svg-lint/src/rules/mod.rs:233-249`,
`crates/svg-lint/src/rules/mod.rs:359`

Issue: this PR exposes full spec inventories, but LSP completion, hover lookup,
and lint still use the curated catalog APIs.

Evidence: `svg-data` says inventory access is additive and independent, and that
curated APIs remain authoritative for current LSP/lint behavior. Completion uses
`attributes_for_with_profile`, `allowed_children_with_profile`, and
`element_for_profile`. Lint uses `element_for_profile`, `attribute_for_profile`,
and `allowed_children_with_profile`.

Why it matters: the PR substantially improves data availability, but product
behavior remains curated-catalog driven. Fully automated spec procurement is not
fully active in LSP/linter behavior.

Suggested fix: either wire selected inventory classes into LSP/lint/completion
paths, or explicitly narrow the PR claim to exposed inventory data rather than
product behavior.

### 5. Medium - SVG Native/profile constraints are data-only in LSP

Files: `crates/svg-data/src/profile.rs:1-16`,
`crates/svg-language-server/src/lib.rs:95-123`,
`crates/svg-language-server/src/lib.rs:156-188`

Issue: SVG Native profile constraints are extracted and typed, but users cannot
select them and LSP/lint do not enforce them.

Evidence: `profile.rs` explicitly says the types are not wired into the LSP
profile axis. `ProfileConfig` stores only a `SpecSnapshotId`.
`resolve_profile_config` only resolves `svg.profile` through
`svg_data::resolve_profile_id`.

Why it matters: SVG Native data exists, but not as product functionality.
Diagnostics/completions/hover cannot reflect SVG Native constraints.

Suggested fix: model configured target as a real ADT, for example
`Snapshot(SpecSnapshotId)` versus `Profile(SvgNative)`, then apply profile
constraints in lint/completion/hover.

### 6. RESOLVED - Medium - freshness automation now updates data

Status: RESOLVED - `.github/workflows/refresh-spec.yml` now includes an
auto-refresh path that re-vendors sources, regenerates schemas, gates on tests,
and opens a review PR when drift is path-relevant.

Files: `.github/workflows/refresh-spec.yml:58-92`,
`crates/svg-language-server/src/freshness.rs:35-54`,
`crates/svg-language-server/src/lib.rs:668-684`

Issue: the scheduled workflow opens or comments on an issue when drift is
detected, and the LSP optionally warns. Neither path updates sources,
regenerates data, or opens a PR.

Evidence: the issue body tells humans to run `just refresh-editions` and
`just refresh-svgwg <commit>`. Runtime freshness message also tells users to
refresh manually.

Why it matters: this is an alerting system, not a fully automated update/QC
pipeline.

Suggested fix: add a scheduled bot PR path that runs refresh scripts,
regenerates data, runs format/lint/test/provenance checks, and labels the PR for
review.

### 7. Medium - `spec_scan` omits `definitions-animations.xml`

Files: `crates/svg-data/build/spec_scan.rs:461-490`,
`crates/svg-data/build/spec_xml.rs:1-60`,
`crates/svg-data/data/sources/svgwg-19482daf/PROVENANCE.toml:88-95`

Issue: `spec_scan` scans four `definitions*.xml` files, while the main spec XML
inventory reader scans five including `definitions-animations.xml`.

Evidence: `spec_scan.rs` lists `definitions.xml`, filters, masking, and
compositing only. `spec_xml.rs` documents and exposes five definition files,
with animations supplying `animate`, `animateMotion`, `animateTransform`, `set`,
and `mpath`. The provenance file confirms the animation definitions are
vendored.

Why it matters: the scanner/QC report can under-report animation/SMIL facts even
though the inventory pipeline knows about them.

Suggested fix: share `spec_xml::DEFINITION_FILES` or add
`definitions-animations.xml` to `spec_scan`; add reproduction assertions for the
five SMIL elements.

### 8. Medium - `spec_scan_repro` picks an arbitrary `svgwg-*` source dir

File: `crates/svg-data/tests/spec_scan_repro.rs:24-48`

Issue: the test locates the vendored checkout with
`read_dir().find(|name| starts_with("svgwg-"))`, but the PR adds multiple
`svgwg-*` dirs.

Evidence: changed files include `svgwg-19482daf` and `svgwg-bd0b7819`. The test
picks the first filesystem entry, which is not a stable oracle.

Why it matters: the test can scan the wrong pin depending on filesystem
ordering, causing flakiness or a false sense of correctness.

Suggested fix: pin the path from the relevant snapshot metadata/provenance, or
hardcode the intended source dir for this reproduction test.

### 9. Medium - Deno worker/deploy checks are outside `just verify`

Files: `justfile:101-108`, `workers/svg-compat/deno.jsonc:10-30`

Issue: `just verify` does not run Deno worker tests or checks, even though this
PR changes Deno config, lockfile, worker imports, static browser JS, and deploy
settings.

Evidence: `just verify` runs format-check, script typecheck, release config,
Rust lint, and Rust tests. Worker tasks exist in
`workers/svg-compat/deno.jsonc`, including `test`, but are not part of verify.

Why it matters: worker/deploy regressions can land without the main local gate
catching them.

Suggested fix: add a `test-deno` or workspace Deno check/test recipe and include
it in `just verify`.

### 10. Medium - no general PR CI runs the project verification gate

Files: `.github/workflows/release.yml:41-80`,
`.github/workflows/refresh-spec.yml:12-36`,
`.github/workflows/publish-npm-oidc.yml:1-28`

Issue: the checked-in workflows do not run `just verify`, `cargo test`,
`cargo clippy`, or Deno worker tests on ordinary PRs.

Evidence: grep found no `just verify`, `cargo test`, `cargo clippy`,
`deno test`, or `deno task` in `.github/workflows/*.yml`. The release workflow
runs cargo-dist planning/builds. The freshness workflow only runs
`spec-freshness`. The npm workflow only publishes downloaded artifacts.

Why it matters: the PR claims automated CI quality control, but there is no
general PR quality gate.

Suggested fix: add a CI workflow for PRs that runs `just verify` after fixing
it, plus Deno worker checks if they stay outside verify.

### 11. RESOLVED - Medium - freshness CI ignores BCD/web-features drift

Status: RESOLVED - `.github/workflows/refresh-spec.yml` now runs the compat
drift mode and threads the result into the same drift classification/refresh
path.

Files: `.github/workflows/refresh-spec.yml:30-36`,
`crates/svg-data/src/bin/spec-freshness.rs:1-19`,
`crates/svg-data/src/bin/spec-freshness.rs:90-160`,
`crates/svg-data/build/bcd.rs:12-21`

Issue: the freshness workflow checks W3C spec editions and SVGWG HEAD only. It
does not monitor `@mdn/browser-compat-data`, `web-features`, or the vendored
compat slice.

Evidence: the workflow runs only `spec-freshness`. The sentinel docs and code
cover W3C versions and SVGWG rolling pin. Build-time BCD is loaded from
`data/sources/svg-compat-data.json`, with optional overrides, but no CI drift
check.

Why it matters: BCD/web-features freshness is part of the stated goal, but stale
compat data will not trigger the freshness issue.

Suggested fix: add a compat freshness sentinel that compares the vendored
versions against latest `@mdn/browser-compat-data` and `web-features`, and
include it in the scheduled workflow.

### 12. PARTIAL / STALE - Medium - freshness timeout evidence is stale

Status: PARTIAL / STALE - the workflow now has `timeout-minutes`; the referenced
`crates/svg-data/src/bin/spec-freshness.rs` path is not present in the current
tree, so the old CLI-specific evidence no longer matches current code.

Files: `crates/svg-data/src/bin/spec-freshness.rs:71-88`,
`.github/workflows/refresh-spec.yml:21-36`

Issue: the CLI uses bare `ureq::get` with no configured timeout, and the
workflow has no `timeout-minutes`.

Evidence: runtime LSP freshness uses a 30 second `ureq::Agent` timeout in
`crates/svg-language-server/src/freshness.rs:89-94`; the CI sentinel does not.

Why it matters: a hung W3C/GitHub response can stall scheduled CI instead of
failing cleanly.

Suggested fix: use a configured `ureq::Agent` with a global timeout and add
workflow `timeout-minutes`.

### 13. Medium - SVG autodetection only honors root `version`

Files: `crates/svg-lint/src/version.rs:1-24`,
`crates/svg-lint/src/version.rs:30-51`, `crates/svg-data/src/lib.rs:197-216`

Issue: autodetection is limited to the root `<svg version="...">` attribute. It
does not inspect broader SVG characteristics or profile markers such as SVG
Tiny/Basic/native constraints.

Evidence: `version.rs` documents the limitation. `effective_profile` calls
`extract_declared_version` and `snapshot_for_svg_version_attr`.
`snapshot_for_svg_version_attr` maps only `1.0`, `1.1`, `2`, and `2.0`.

Why it matters: the PR only partially satisfies autodetection based on SVG
characteristics.

Suggested fix: either document the limitation in PR claims or add detection for
modeled profile/edition characteristics such as `baseProfile`, Tiny/Basic
values, or SVG Native constraints.

### 14. Medium - LSP config cannot select edition-keyed inventories beyond `SpecSnapshotId`

Files: `crates/svg-data/src/lib.rs:97-101`,
`crates/svg-data/src/lib.rs:183-195`,
`crates/svg-data/src/inventory.rs:300-306`,
`crates/svg-language-server/src/lib.rs:156-188`

Issue: the PR adds edition-keyed inventories for more versions than the curated
snapshot enum, but LSP config can only resolve `SpecSnapshotId` values.

Evidence: `ALL_SPEC_SNAPSHOTS` contains only four snapshots.
`resolve_profile_id` searches those snapshots. `inventory.rs` documents
`EditionId` as the additive counterpart for arbitrary editions, including
editions with no `SpecSnapshotId`. LSP config calls only `resolve_profile_id`.

Why it matters: users cannot configure the LSP to target edition inventory
entries like SVG 1.0 REC, SVG 1.1 PR, or older SVG2 CRs that exist only in the
additive inventory layer.

Suggested fix: either extend the runtime profile/config model to support
`EditionId`, or state that only curated `SpecSnapshotId` snapshots are
configurable.

### 15. Low - freshness uses SVGWG repo HEAD, causing path-irrelevant false positives

Files: `crates/svg-data/src/bin/spec-freshness.rs:113-154`,
`crates/svg-language-server/src/freshness.rs:62-75`,
`crates/svg-language-server/src/freshness.rs:96-105`

Issue: freshness compares the baked rolling pin to the default branch HEAD
commit, not to the tracked input file blobs/paths.

Evidence: both CLI and runtime freshness resolve the default branch and fetch
`/commits/{branch}`. The refresh scripts vendor a fixed set of paths, so those
path blobs are the meaningful inputs.

Why it matters: any upstream commit can mark the catalog stale even if no
consumed spec file changed.

Suggested fix: compare relevant path blob SHAs/tree entries from provenance, or
report repo HEAD movement separately from tracked-input drift.

### 16. Low - runtime compat fetch has no setting/opt-out

Files: `crates/svg-language-server/src/lib.rs:687-709`,
`crates/svg-language-server/src/compat.rs:62-69`

Issue: LSP initialization always spawns a runtime fetch to unpkg for latest BCD
and web-features. Spec freshness is opt-in, but compat freshness is
unconditional.

Evidence: `initialize` always spawns `fetch_runtime_compat`.
`fetch_runtime_compat` fetches `@mdn/browser-compat-data@latest` and
`web-features@latest`.

Why it matters: users who need offline/private/no-network editor sessions cannot
disable this network access.

Suggested fix: add an LSP setting such as `svg.compat_freshness_check` or
`svg.runtime_compat` with default documented behavior.

### 17. Low - publish workflow uses moving Node `latest`

File: `.github/workflows/publish-npm-oidc.yml:26-33`

Issue: the npm publish workflow sets `node-version: latest` and then installs
`npm@latest`.

Evidence: setup-node uses `latest`; the publish step also runs
`npm install -g npm@latest`.

Why it matters: release behavior can change under the repo without a code
change.

Suggested fix: pin a known-good supported Node line and npm version, or document
intentional moving-runtime policy.

### 18. Low - `svg-data` inventory docs are stale after inventory expansion

Files: `crates/svg-data/src/lib.rs:436-446`,
`crates/svg-data/src/inventory.rs:237-271`

Issue: crate-root docs still say only `Svg2EditorsDraft` carries a baked
inventory and older snapshots return `None`, but `inventory::for_snapshot` now
returns `Some` for all four snapshots.

Evidence: `lib.rs:438-440` says older snapshots return `None`.
`inventory.rs:258-271` says every snapshot has an inventory and matches all four
variants to `Some`.

Why it matters: public API docs mislead consumers about versioned inventory
coverage.

Suggested fix: update the root `spec_inventory` docs to match
`inventory::for_snapshot`.

### 19. Low - `svg-data/build/AGENTS.md` still says `spec.rs` is orphaned

Files: `crates/svg-data/build/AGENTS.md:7-14`,
`crates/svg-data/build/AGENTS.md:30-33`, `crates/svg-data/build.rs:25-34`

Issue: repo knowledge docs still say `build/spec.rs` is orphaned and not
declared, but this PR declares it as `mod spec`.

Evidence: `AGENTS.md` says `spec.rs` is orphaned in two places. `build.rs`
declares `#[path = "build/spec.rs"] mod spec;`.

Why it matters: agents and contributors will get wrong guidance about a now-live
extractor.

Suggested fix: update the build knowledge base to describe `spec.rs` as the
hermetic description extractor.

### 20. RESOLVED - Low - `refresh-spec.yml` comment still says SVGWG `master`

Status: RESOLVED - the workflow comment now says the editor's-draft default
branch is resolved dynamically.

File: `.github/workflows/refresh-spec.yml:3-10`

Issue: workflow comments say the sentinel checks the SVGWG editor's-draft
`master`, but code now resolves the upstream default branch dynamically.

Evidence: workflow comment line 5 names `master`; CLI/runtime code resolves the
default branch before checking commits.

Why it matters: small docs drift, but this area already had a branch-tracking
bug. Wrong wording invites regression.

Suggested fix: change the comment to "default branch".

## Goal Coverage

Fully automated procurement of SVG specs: partial. The PR adds vendored sources,
extractors, edition metadata, refresh scripts, and inventories. It does not
auto-update via CI PR, and provenance bytes are not verified.

BCD into the LSP/linter/etc.: partial. Baked BCD is integrated, and runtime
fetch overlays some fields. Runtime data does not update baked verdict/advisory
logic.

Dynamic fetch of more recent compat data: partial. The LSP fetches latest
BCD/web-features at startup, but uses it only for flags/baseline/browser chips
and does not expose a setting.

Versioned SVG specs: mostly yes for the curated snapshot axis. The PR has four
`SpecSnapshotId` snapshots and additional edition inventories. Product behavior
remains tied to curated snapshots.

Configurable spec in LSP settings: yes for `SpecSnapshotId` snapshots only. Not
yes for SVG Native constraints or edition-keyed inventories that have no
snapshot ID.

Autodetection based on SVG characteristics: limited. It detects root `version`
only.

Automated CI checking, updating, and quality control: weak/partial. Freshness
issue workflow exists, but no update PR, no BCD freshness, no provenance hash
verification, no Deno worker gate, no general PR verify workflow, and current
`just verify` is broken.

## Oracle Verdict

Oracle confirmed all 13 subagent candidate findings. Oracle adjusted severities
as follows:

`spec_scan` omitting animations: High to Medium, because main product inventory
may be okay but scanner/QC is incomplete.

Freshness detects but does not update: Low to Medium, because it directly
affects the automation goal.

Provenance not verified: Medium to High, because it undermines reproducibility
claims.

Oracle added three findings: no general PR CI verify, unconditional runtime
compat fetch, and root-version-only autodetection. Final correlation also split
out the edition-keyed config gap and three stale-doc findings: inventory API
docs, build AGENTS guidance, and the spec-freshness workflow comment.
