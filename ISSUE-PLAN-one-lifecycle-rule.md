# ISSUE PLAN: One Lifecycle Rule

## Problem Summary

The language server currently uses inconsistent rules for lifecycle state.

- Hover uses selected SVG profile plus compat metadata.
- Lint uses selected SVG profile plus compat metadata plus runtime overrides.
- Completion uses selected SVG profile, but does not fully use compat metadata and no longer uses runtime compat overrides.

This creates split behavior for the same symbol.

Examples of bad outcomes:

- A symbol can be valid in the selected profile and flagged deprecated in hover.
- The same symbol can produce a deprecated diagnostic in lint.
- The same symbol can still appear in completion as if it were fully normal.
- Or completion may hide/show items based on a rule that does not match hover/lint.

This is confusing for users and makes the server feel internally inconsistent.

## Three-Word Summary

One lifecycle rule.

## One-Paragraph Summary

The issue is that the server has two different sources of truth for symbol state: hover and lint combine profile membership with compat deprecation/experimental data, while completion currently only follows profile membership plus spec lifecycle and ignores runtime compat overrides. As a result, the same element or attribute can be treated as "valid but deprecated" in one feature and "normal" in another. The fix is to make completion use the same merged lifecycle rule as the rest of the server: if a symbol is supported in the selected profile, completion should still show it, but annotate it clearly as deprecated, obsolete, or experimental; if the symbol is unsupported in the selected profile, completion should hide it.

## Policy Decision

This plan follows the Oracle recommendation.

### Core Rule

Completion should include every symbol that is valid in the selected profile.

- Hide only `unsupported` symbols.
- Show `deprecated` symbols.
- Show `obsolete` symbols if they are still valid in the selected profile.
- Show `experimental` symbols.

### Annotation Rule

For symbols that are shown:

- `Deprecated` and `Obsolete` get deprecated tag metadata and explicit detail text.
- `Experimental` gets explicit detail text.
- `Stable` gets normal detail text.

### State Matrix

| State        | In selected profile? | Show in completion? | Deprecated tag? | Detail text          |
| ------------ | -------------------- | ------------------- | --------------- | -------------------- |
| Unsupported  | No                   | No                  | No              | None                 |
| Stable       | Yes                  | Yes                 | No              | Normal description   |
| Experimental | Yes                  | Yes                 | No              | `... [Experimental]` |
| Deprecated   | Yes                  | Yes                 | Yes             | `... [Deprecated]`   |
| Obsolete     | Yes                  | Yes                 | Yes             | `... [Obsolete]`     |

### Special Legacy Case

The XLink family should follow the same rule.

- In SVG 1.1, `xlink:href` should be shown if the selected profile supports it.
- In SVG 1.1, it should be annotated as deprecated or obsolete according to the merged lifecycle.
- In SVG 2+, it should be hidden if unsupported in that profile.
- If both `href` and `xlink:href` are valid in some context, `href` should rank above `xlink:href`, but this ranking adjustment is optional and not required for the first fix.

## Scope

## In Scope

- Make completion lifecycle behavior match hover and lint.
- Restore runtime compat influence on completion.
- Preserve profile-based filtering for unsupported symbols.
- Add tests covering merged lifecycle behavior.
- Clarify existing completion tests whose wording no longer matches intended behavior.

## Out of Scope

- Changing hover wording around profile lifecycle text.
- Changing lint semantics.
- Changing runtime compat merge behavior in `src/compat.rs`.
- Adding worker checks to `just ci`.
- Reworking ranking/sorting beyond basic correctness.
- Cross-crate refactor to share a lifecycle helper between lint and language server.

## Existing Behavior and Code Paths

### Hover

Hover already merges profile membership and compat information.

Relevant code:

- `crates/svg-language-server/src/lib.rs`
  - `build_element_hover_markdown(...)`
  - `build_attribute_hover_markdown(...)`
- `crates/svg-language-server/src/hover.rs`
  - `format_element_hover_with_profile(...)`
  - `format_attribute_hover_with_profile(...)`
  - `profile_lifecycle_hover_line(...)`

Current hover inputs:

- selected profile
- `svg_data::ProfileLookup`
- baked compat flags from catalog
- runtime compat overrides from `RuntimeCompat`

### Lint

Lint already merges profile membership and compat information.

Relevant code:

- `crates/svg-lint/src/rules/mod.rs`
  - `element_diagnostic_lifecycle(...)`
  - `attribute_diagnostic_lifecycle(...)`
  - `diagnostic_lifecycle(...)`

Current lint rule shape:

1. Start from profile lookup lifecycle.
2. Preserve `Deprecated` and `Obsolete` immediately.
3. Otherwise apply compat/runtime deprecated override.
4. Otherwise preserve `Experimental`.
5. Otherwise apply compat/runtime experimental override.
6. Otherwise remain `Stable`.

This is the behavior completion should mirror.

### Completion

Completion is the inconsistent path.

Relevant code:

- `crates/svg-language-server/src/lib.rs`
  - `completion_from_context(...)`
  - `completion(...)`
- `crates/svg-language-server/src/completion.rs`
  - `attribute_completion_items(...)`
  - `child_element_completion_items(...)`
  - `root_element_completion_items(...)`
  - `attribute_completion_item(...)`
  - `element_completion_item(...)`
  - `lifecycle_completion_detail(...)`

Current problem in completion:

- It uses profile-aware data from `svg_data`.
- It does not thread `RuntimeCompat` through the completion request path.
- It displays lifecycle text based on profile/spec lifecycle, not merged lifecycle.
- Attribute completion items are not tagged deprecated even when they should be.

## Desired End State

After the fix:

1. Completion uses selected profile to determine whether a symbol is eligible to appear.
2. Completion uses merged lifecycle state to determine annotation and deprecated tagging.
3. Hover, lint, and completion agree on the same symbol status.
4. Unsupported symbols stay hidden in completion.
5. Deprecated, obsolete, and experimental supported symbols remain discoverable but clearly annotated.

## Implementation Plan

### Step 1: Thread Runtime Compat Through Completion Path

#### File

- `crates/svg-language-server/src/lib.rs`

#### Current State

`completion(...)` reads the selected profile, then calls:

```rust
completion_from_context(source, &doc.tree, node, profile)
```

`completion_from_context(...)` does not receive runtime compat.

#### Required Change

Update completion flow to pass runtime compat into the completion context path, similar to hover.

#### Exact Changes

1. Update `completion_from_context(...)` signature to accept:
   - `runtime_compat: Option<&RuntimeCompat>`
2. In `completion(...)`, read `self.runtime_compat.read().await` before calling `completion_from_context(...)`.
3. Pass `runtime_compat.as_ref()` into `completion_from_context(...)`.
4. Thread that value further into:
   - `attribute_completion_items(...)`
   - `child_element_completion_items(...)`
   - `root_element_completion_items(...)`

#### Important Constraint

Do not change style completion behavior.

- CSS completions inside `<style>` stay untouched.
- Value completions for attribute values stay untouched.
- Only SVG symbol completion logic changes.

### Step 2: Add a Completion Lifecycle Merge Helper

#### File

- `crates/svg-language-server/src/completion.rs`

#### Goal

Create a small helper in the language server crate that mirrors lint lifecycle merge behavior for completion UI.

#### Why Local Helper Instead of Shared Cross-Crate Helper

- Smallest possible fix.
- Avoids refactoring crate boundaries just to remove a few lines of duplicated logic.
- Keeps change tightly scoped to completion behavior.

#### Suggested Helper Responsibilities

Input:

- profile/spec lifecycle from `ProfiledElement` or `ProfiledAttribute`
- baked compat flags from catalog item (`deprecated`, `experimental`)
- optional runtime compat override (`CompatOverride`)

Output:

- final effective lifecycle for completion display

#### Required Logic

Mirror lint behavior exactly:

1. If spec/profile lifecycle is `Deprecated` or `Obsolete`, return it immediately.
2. Else if runtime override exists and marks deprecated, return `Deprecated`.
3. Else if no runtime override and baked compat marks deprecated, return `Deprecated`.
4. Else if spec/profile lifecycle is `Experimental`, return `Experimental`.
5. Else if runtime override exists and marks experimental, return `Experimental`.
6. Else if no runtime override and baked compat marks experimental, return `Experimental`.
7. Else return `Stable`.

#### Important Semantics

- Runtime compat overrides replace baked compat flags for deprecated/experimental state.
- Runtime compat does not change profile membership.
- Runtime compat does not convert unsupported symbols into supported symbols.
- Obsolete remains stronger than compat deprecated.

### Step 3: Apply Merged Lifecycle to Attribute Completions

#### File

- `crates/svg-language-server/src/completion.rs`

#### Target Function

- `attribute_completion_items(...)`

#### Current State

- Receives profiled attributes.
- Filters out already-present attributes.
- Builds completion items from raw profiled lifecycle.
- Does not apply runtime compat.
- Does not tag deprecated attribute items.

#### Required Change

For each profiled attribute that is already supported in the selected profile:

1. Look up runtime compat override by canonical attribute name.
2. Compute effective lifecycle with the new helper.
3. Build completion item using effective lifecycle.

#### Behavior After Change

- Supported stable attribute => shown normally.
- Supported experimental attribute => shown with `[Experimental]`.
- Supported deprecated attribute => shown with deprecated tag and `[Deprecated]`.
- Supported obsolete attribute => shown with deprecated tag and `[Obsolete]`.
- Unsupported attribute => still not returned by profile-aware source, so still hidden.

### Step 4: Apply Merged Lifecycle to Child Element Completions

#### File

- `crates/svg-language-server/src/completion.rs`

#### Target Function

- `child_element_completion_items(...)`

#### Current State

- Returns profile-supported child elements.
- Builds items from raw profiled lifecycle.
- Does not apply runtime compat.

#### Required Change

For each profiled child element:

1. Look up runtime compat override by element name.
2. Compute effective lifecycle using the new helper.
3. Build item with effective lifecycle.

#### Behavior After Change

Same state handling as attributes.

### Step 5: Apply Merged Lifecycle to Root Element Completion

#### File

- `crates/svg-language-server/src/completion.rs`

#### Target Function

- `root_element_completion_items(...)`

#### Current State

- Uses `svg_data::element_for_profile(profile, "svg")`.
- Builds one root item from raw lifecycle.

#### Required Change

1. If root `svg` element is unsupported in profile, still return empty.
2. If present, compute effective lifecycle from baked compat plus runtime compat.
3. Build completion item from effective lifecycle.

#### Note

This is mostly for consistency, even if `svg` is unlikely to be deprecated in practice.

### Step 6: Update Completion Item Builders

#### File

- `crates/svg-language-server/src/completion.rs`

#### Target Functions

- `attribute_completion_item(...)`
- `element_completion_item(...)`

#### Required Changes

##### `attribute_completion_item(...)`

It currently only writes detail text.

It should also:

1. Detect whether lifecycle is `Deprecated` or `Obsolete`.
2. Set `deprecated: Some(true)` in those cases.
3. Set `tags: Some(vec![CompletionItemTag::DEPRECATED])` in those cases.

##### `element_completion_item(...)`

It already tags deprecated/obsolete states.

No major behavioral change needed, but ensure it continues to consume the effective lifecycle rather than raw lifecycle.

##### `lifecycle_completion_detail(...)`

Keep existing detail text convention:

- Stable => original description
- Experimental => `description [Experimental]`
- Deprecated => `description [Deprecated]`
- Obsolete => `description [Obsolete]`

### Step 7: Preserve Unsupported Filtering Exactly

#### Why This Matters

The fix must not accidentally reintroduce unsupported symbols into completion.

#### Required Rule

Membership remains controlled by profile-aware `svg_data` calls:

- `attributes_for_with_profile(...)`
- `allowed_children_with_profile(...)`
- `element_for_profile(...)`

Only symbols returned by these calls are eligible for completion.

Merged lifecycle logic runs only after membership is already confirmed.

This prevents bugs like:

- showing `xlink:href` in SVG 2
- showing profile-unsupported elements in legacy profiles

## Test Plan

### Test Category A: Unit Tests for Lifecycle Merge Helper

#### File

- `crates/svg-language-server/src/completion.rs`

#### Purpose

Prove the merge rule matches intended semantics without requiring LSP integration or network state.

#### Add Tests For

1. `Stable` spec lifecycle + baked deprecated => effective `Deprecated`
2. `Stable` spec lifecycle + runtime deprecated override => effective `Deprecated`
3. `Stable` spec lifecycle + baked experimental => effective `Experimental`
4. `Stable` spec lifecycle + runtime experimental override => effective `Experimental`
5. `Experimental` spec lifecycle + runtime deprecated override => effective `Deprecated`
6. `Deprecated` spec lifecycle + runtime experimental override => remains `Deprecated`
7. `Obsolete` spec lifecycle + runtime deprecated false => remains `Obsolete`
8. runtime override present with both flags false should suppress baked deprecated/experimental state and yield `Stable`

#### Why Test 8 Matters

Lint semantics treat runtime overrides as replacement flags, not additive hints. Completion should match that.

### Test Category B: Unit Tests for Completion Item Tagging

#### File

- `crates/svg-language-server/src/completion.rs`

#### Add Tests For

1. Deprecated attribute item sets:
   - detail suffix `[Deprecated]`
   - `deprecated: Some(true)`
   - deprecated tag
2. Obsolete attribute item sets:
   - detail suffix `[Obsolete]`
   - `deprecated: Some(true)`
   - deprecated tag
3. Experimental attribute item sets:
   - detail suffix `[Experimental]`
   - no deprecated tag
4. Stable attribute item sets:
   - plain description
   - no deprecated tag
5. Existing element tagging test remains valid.

### Test Category C: Integration Tests for Profile-Aware Completion

#### File

- `crates/svg-language-server/tests/completions.rs`

#### Existing Test To Update

`attribute_and_element_completion_filters_invalid_suggestions()` currently contains:

- `!attribute_labels.contains(&"xlink:href")`
- message: `deprecated attributes should not be suggested`

#### Problem With Existing Wording

The behavior being asserted is really profile exclusion, not a general deprecated-items policy.

#### Required Update

Keep the assertion if the selected default profile still excludes `xlink:href`, but rewrite message/comments to reflect the correct reason.

Example wording direction:

- `profile-unsupported attributes should not be suggested`

#### Existing Profile Test To Strengthen

`completions_follow_selected_profile()` should be expanded.

##### SVG 1.1 Assertions

For SVG 1.1:

1. `xlink:href` is present
2. the returned item carries deprecated tag metadata and/or deprecated field
3. item detail contains `[Deprecated]` or `[Obsolete]`, depending on effective lifecycle currently produced by data model

##### SVG 2 Assertions

For SVG 2:

1. `href` is present
2. `xlink:href` is absent

### Test Category D: Runtime Override Semantics

#### Preferred Location

- unit tests in `crates/svg-language-server/src/completion.rs`

#### Why Unit Test Instead of Integration Test

The integration test harness spins the full server binary and does not provide a clean deterministic hook for injecting runtime compat overrides.

A unit test can explicitly construct:

- a `ProfiledAttribute` or `ProfiledElement`
- a fake `CompatOverride`
- expected effective lifecycle

This makes the test deterministic and small.

## Detailed Work Breakdown

### Phase 1: Code Plumbing

1. Update `completion_from_context(...)` signature.
2. Update call site in `completion(...)`.
3. Update downstream completion helper signatures to accept runtime compat.

### Phase 2: Lifecycle Merge Logic

1. Add completion-local merge helper.
2. Add any tiny support helper needed for mapping baked flags to merged lifecycle.
3. Avoid introducing new public API unless clearly necessary.

### Phase 3: Item Construction

1. Apply merged lifecycle to attribute completions.
2. Apply merged lifecycle to child element completions.
3. Apply merged lifecycle to root element completions.
4. Add deprecated tagging for attribute items.

### Phase 4: Tests

1. Add unit tests for merge helper.
2. Add unit tests for attribute completion item tagging.
3. Update integration test wording around unsupported/default-profile exclusions.
4. Strengthen selected-profile completion integration test.

### Phase 5: Verification

Run targeted checks:

1. `cargo test -p svg-language-server completion`
2. If needed, `cargo test -p svg-language-server tests::` or the full crate test suite
3. If available and cheap, `cargo test -p svg-language-server`

## Acceptance Criteria

The issue is considered solved when all of the following are true.

### Behavior Criteria

1. Completion hides symbols unsupported in the selected profile.
2. Completion shows symbols supported in the selected profile even if deprecated.
3. Completion shows supported obsolete symbols and marks them deprecated.
4. Completion shows supported experimental symbols and marks them experimental in detail text.
5. Hover, lint, and completion agree on lifecycle classification for supported symbols.

### API/Protocol Criteria

1. Deprecated/obsolete completion items expose deprecated metadata where supported.
2. Completion detail text reflects effective lifecycle.
3. No unrelated completion payload changes occur.

### Regression Criteria

1. Style completions still work.
2. Value completions still work.
3. Comment and script context behavior stays unchanged.
4. Profile-aware filtering still works for SVG 1.1 vs SVG 2 attribute swaps.

### Code Quality Criteria

1. Change remains small and localized.
2. No broad refactor of hover/lint needed.
3. No weakening of type safety.
4. No duplication explosion beyond one small helper.

## Risks and Mitigations

### Risk 1: Attribute Deprecated Tagging Changes Client Rendering

Some editors may render deprecated completion items differently than before.

Mitigation:

- Keep detail text authoritative.
- Treat deprecated tag as additive metadata, not sole signal.

### Risk 2: Runtime Override Semantics Drift From Lint

If completion merge logic is not aligned with lint merge logic, inconsistency remains.

Mitigation:

- Mirror lint logic exactly.
- Add explicit unit tests for override precedence.

### Risk 3: Accidentally Showing Unsupported Symbols

If merged lifecycle logic is run before profile filtering or is allowed to bypass membership checks, unsupported symbols may leak into completion.

Mitigation:

- Keep profile-aware `svg_data` calls as the sole source of membership.
- Only compute merged lifecycle after the symbol is already confirmed present.

### Risk 4: Confusing Test Intent

Existing tests use `xlink:href` and may mix deprecated-vs-unsupported concepts.

Mitigation:

- Rewrite test names/messages to reflect true expected behavior.
- Separate unsupported profile cases from supported deprecated cases.

## Non-Goals and Follow-Up Work

These are valid future improvements, but they are not required to solve this issue.

1. Share a common lifecycle merge helper across `svg-lint` and `svg-language-server`
2. Improve completion ranking so modern replacements sort above deprecated legacy alternatives
3. Revisit hover wording for profile lifecycle vs compat lifecycle
4. Add Deno worker checks to `just ci`
5. Revisit runtime compat browser-support merge semantics

## Suggested Commit/PR Framing

If this work becomes a commit or PR, frame it as a consistency fix.

Suggested summary direction:

- align completion lifecycle with hover and lint
- restore runtime compat lifecycle annotations in completion
- keep profile-based filtering for unsupported symbols

## Final Definition of Done

Done means this exact sentence is true:

> For any SVG element or attribute, the language server now makes one consistent lifecycle decision across hover, lint, and completion: if the symbol is supported in the selected profile it is shown and annotated according to merged spec/compat/runtime lifecycle, and if it is unsupported in the selected profile it is hidden from completion and reported accordingly elsewhere.
