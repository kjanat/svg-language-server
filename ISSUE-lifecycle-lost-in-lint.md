# Issue: Obsolete Lifecycle Is Computed, Then Lost in Lint

> Status: RESOLVED in current code. `emit_lifecycle_diag_in_tag(...)` now emits
> `SpecLifecycle::Obsolete` with `codes.obsolete` and
> `format_obsolete_message(...)`.

## Summary

Older `svg-lint` builds computed `SpecLifecycle::Obsolete`, but the final
diagnostic emitter dropped that state on the floor. Current code no longer has
that loss point.

That made lint the odd surface out in affected builds:

- hover can show `Obsolete in ...` or `Obsolete after ...`
- completion can still surface obsolete symbols in supported profiles
- lint said nothing

This was a real product bug, not just an architecture smell. If a feature is
still parseable but obsolete in the selected profile, the editor should not
force the user to discover that only by hovering.

## Problem

The old loss point was `crates/svg-lint/src/rules/mod.rs`:

- `diagnostic_lifecycle(...)` preserved `SpecLifecycle::Obsolete`
- `emit_lifecycle_diag_in_tag(...)` only emitted for `Deprecated` and
  `Experimental`
- `Obsolete` was grouped with `Stable` and produced no diagnostic

Current code has a dedicated `SpecLifecycle::Obsolete` match arm that emits
`DiagnosticCode::ObsoleteElement` / `DiagnosticCode::ObsoleteAttribute`.

## Why This Matters

- Diagnostics are the primary authoring feedback surface. Hover is secondary.
- Silent obsolete features were easy to keep shipping by accident.
- The repo already wants one lifecycle rule across hover, lint, and completion.
- When lint dropped `Obsolete`, the three surfaces could not agree.

## Concrete User Impact

In affected builds, the same symbol could be treated three different ways.

Example: `xlink:href`

- hover test already expects obsolete lifecycle text
  - `crates/svg-language-server/tests/definitions_and_hover.rs:327-382`
- completion policy work already treats obsolete as a visible lifecycle state
  - `crates/svg-language-server/src/completion.rs:689-705`
  - `crates/svg-language-server/src/completion.rs:712-726`
- lint emitted no obsolete diagnostic at all

Net effect: a user could hover a symbol, see that it is obsolete, and still get
no diagnostic telling them to change it.

## Evidence

### Old loss point: lint preserved obsolete, then dropped it

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

- old `crates/svg-lint/src/rules/mod.rs:531-550`

```rust
match lifecycle {
    SpecLifecycle::Deprecated => ...,
    SpecLifecycle::Experimental => ...,
    SpecLifecycle::Stable | SpecLifecycle::Obsolete => {}
}
```

Current code instead has:

```rust
SpecLifecycle::Obsolete => push_diag_in_tag(..., codes.obsolete, ...)
```

### Hover still surfaces obsolete profile state

- `crates/svg-language-server/src/hover.rs:347-395`

Hover can render:

- `**Obsolete in Svg11Rec20110816**`
- `**Obsolete after Svg11Rec20110816**`

So the information was not missing from the data path; it was missing from lint
output.

### Completion already models obsolete as a displayable state

- `crates/svg-language-server/src/completion.rs:689-705`
- `crates/svg-language-server/src/completion.rs:712-726`

Completion detail text and deprecated tagging already treat `Obsolete` as
meaningful UI state, even though the completion path had its own lifecycle
inconsistency problems.

## Scope

This issue is intentionally narrower than `ISSUE-PLAN-one-lifecycle-rule.md`.

This issue was about one bug:

- lint lost `Obsolete`

This issue is not about solving every lifecycle inconsistency at once.

## Decision

Lint should emit an author-visible signal for supported-but-obsolete symbols.

Policy split:

- `svg-data` owns lifecycle facts
- `svg-lint` decides how those facts become diagnostics
- `svg-language-server` renders those facts in hover/completion

This specific bug should be fixed in lint, but the fix should not invent new
source-of-truth rules inside hover.

## Resolution

The fix has landed in `emit_lifecycle_diag_in_tag(...)`:

- `Obsolete` emits a warning-level diagnostic
- elements use `DiagnosticCode::ObsoleteElement`
- attributes use `DiagnosticCode::ObsoleteAttribute`
- messages flow through `format_obsolete_message(...)`

### Preferred follow-up

Use a shared effective-lifecycle helper so lint and completion stop
re-implementing lifecycle merge rules separately.

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

- when a profile-supported symbol is obsolete, publish diagnostics should
  contain a user-visible lifecycle diagnostic

## Related Work

- `ISSUE-PLAN-one-lifecycle-rule.md`
  - broader lifecycle consistency across hover, lint, completion
- `ISSUE-PLAN-compat-ux.md`
  - richer verdict modeling, alias metadata, obsolete/compat reconciliation

## Bottom Line

The repo already knows when a symbol is obsolete.

Lint is the only layer that computes that truth and then throws it away.

Fix that first. Then unify the rest.
