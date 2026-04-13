# Issue: Obsolete Lifecycle Is Computed, Then Lost in Lint

## Summary

`svg-lint` already computes `SpecLifecycle::Obsolete`, but the final diagnostic emitter drops that state on the floor.

That makes lint the odd surface out:

- hover can show `Obsolete in ...` or `Obsolete after ...`
- completion can still surface obsolete symbols in supported profiles
- lint says nothing

This is a real product bug, not just an architecture smell. If a feature is still parseable but obsolete in the selected profile, the editor should not force the user to discover that only by hovering.

## Problem

The bug is in `crates/svg-lint/src/rules/mod.rs`:

- `diagnostic_lifecycle(...)` preserves `SpecLifecycle::Obsolete` immediately
  - `crates/svg-lint/src/rules/mod.rs:554-563`
- `emit_lifecycle_diag_in_tag(...)` only emits for `Deprecated` and `Experimental`
  - `crates/svg-lint/src/rules/mod.rs:531-550`
- `Obsolete` is grouped with `Stable` and produces no diagnostic

So the pipeline currently does this:

1. profile lookup returns `Obsolete`
2. lint merge logic keeps `Obsolete`
3. emitter discards it

That is why the lifecycle is effectively lost in lint.

## Why This Matters

- Diagnostics are the primary authoring feedback surface. Hover is secondary.
- Silent obsolete features are easy to keep shipping by accident.
- The repo already wants one lifecycle rule across hover, lint, and completion.
- As long as lint drops `Obsolete`, the three surfaces cannot agree.

## Concrete User Impact

Today, the same symbol can be treated three different ways.

Example: `xlink:href`

- hover test already expects obsolete lifecycle text
  - `crates/svg-language-server/tests/definitions_and_hover.rs:327-382`
- completion policy work already treats obsolete as a visible lifecycle state
  - `crates/svg-language-server/src/completion.rs:689-705`
  - `crates/svg-language-server/src/completion.rs:712-726`
- lint emits no obsolete diagnostic at all

Net effect: a user can hover a symbol, see that it is obsolete, and still get no diagnostic telling them to change it.

## Evidence

### Lint preserves obsolete, then drops it

- `crates/svg-lint/src/rules/mod.rs:554-580`

```rust
fn diagnostic_lifecycle(
    spec_lifecycle: SpecLifecycle,
    compat_flags: CompatFlags,
    override_flags: Option<&CompatFlags>,
) -> SpecLifecycle {
    if matches!(
        spec_lifecycle,
        SpecLifecycle::Deprecated | SpecLifecycle::Obsolete
    ) {
        return spec_lifecycle;
    }
    ...
}
```

- `crates/svg-lint/src/rules/mod.rs:531-550`

```rust
match lifecycle {
    SpecLifecycle::Deprecated => ...,
    SpecLifecycle::Experimental => ...,
    SpecLifecycle::Stable | SpecLifecycle::Obsolete => {}
}
```

### Hover still surfaces obsolete profile state

- `crates/svg-language-server/src/hover.rs:347-395`

Hover can render:

- `**Obsolete in Svg11Rec20110816**`
- `**Obsolete after Svg11Rec20110816**`

So the information is not missing from the data path. It is only missing from lint output.

### Completion already models obsolete as a displayable state

- `crates/svg-language-server/src/completion.rs:689-705`
- `crates/svg-language-server/src/completion.rs:712-726`

Completion detail text and deprecated tagging already treat `Obsolete` as meaningful UI state, even though the completion path still has its own lifecycle inconsistency problems.

## Scope

This issue is intentionally narrower than `ISSUE-PLAN-one-lifecycle-rule.md`.

This issue is about one bug:

- lint loses `Obsolete`

This issue is not about solving every lifecycle inconsistency at once.

## Decision

Lint should emit an author-visible signal for supported-but-obsolete symbols.

Policy split:

- `svg-data` owns lifecycle facts
- `svg-lint` decides how those facts become diagnostics
- `svg-language-server` renders those facts in hover/completion

This specific bug should be fixed in lint, but the fix should not invent new source-of-truth rules inside hover.

## Proposed Fix

### Immediate fix

Make `emit_lifecycle_diag_in_tag(...)` emit for `SpecLifecycle::Obsolete`.

Smallest acceptable version:

- treat `Obsolete` the same as deprecated for severity and suppression behavior
- emit warning-level diagnostic
- message shape can be simple, e.g. `{subject} is obsolete`

That restores user-visible feedback without blocking on a larger refactor.

### Preferred follow-up

Use a shared effective-lifecycle helper so lint and completion stop re-implementing lifecycle merge rules separately.

That broader alignment belongs with:

- `ISSUE-PLAN-one-lifecycle-rule.md`

But this lint bug should not wait for the entire completion refactor.

## Diagnostic Shape

Two acceptable options:

### Option A: Minimal

- reuse existing deprecated diagnostic codes
- keep warning severity
- differentiate only by message text: `is obsolete`

Pros:

- smallest change
- minimal wire/test churn

Cons:

- obsolete vs deprecated remains collapsed at code level

### Option B: Better

- add dedicated `ObsoleteElement` / `ObsoleteAttribute` diagnostic codes
- keep warning severity

Pros:

- clearer suppression and UX semantics
- future-proof for quick-fixes and docs

Cons:

- wider API/test churn

If the goal is to land quickly, Option A is fine first.

## Non-Goals

- redesigning hover wording
- solving completion lifecycle drift end-to-end
- adding alias/replacement metadata such as `legacy_alias_of` / `replacement`
- deciding `font-stretch` vs `font-width`
- moving `xml:*` / `xmlns:*` policy out of hover
- full XLink cleanup

Those are real follow-ups, but they are not required to fix the lint bug.

## Verification

Add tests that prove obsolete lifecycle no longer disappears.

### Lint unit coverage

Add or update tests in `crates/svg-lint/src/lib.rs` to assert:

1. a supported obsolete symbol emits a diagnostic
2. unsupported symbols still do not also emit deprecated/obsolete
3. suppression comments still suppress the obsolete diagnostic correctly

### Protocol coverage

Add/update LSP diagnostic coverage in:

- `crates/svg-language-server/tests/diagnostics_and_actions.rs`

Goal:

- when a profile-supported symbol is obsolete, publish diagnostics should contain a user-visible lifecycle diagnostic

## Related Work

- `ISSUE-PLAN-one-lifecycle-rule.md`
  - broader lifecycle consistency across hover, lint, completion
- `ISSUE-PLAN-compat-ux.md`
  - richer verdict modeling, alias metadata, obsolete/compat reconciliation

## Bottom Line

The repo already knows when a symbol is obsolete.

Lint is the only layer that computes that truth and then throws it away.

Fix that first. Then unify the rest.
