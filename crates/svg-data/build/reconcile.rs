//! Build-time three-way reconciliation check.
//!
//! Cross-checks three independent signal sources for every SVG feature:
//!
//! 1. **BCD** (`@mdn/browser-compat-data`) — the `deprecated` / `experimental`
//!    flags embedded in the worker's `/data.json`.
//! 2. **Snapshot membership** — the `present_in` list from
//!    `data/derived/union/{elements,attributes}.json`, which drives
//!    `SpecLifecycle::Stable` / `Obsolete` derivation.
//! 3. **Spec scanner output** — `data/reviewed/spec_removals.json`, emitted
//!    by `workers/svg-compat/src/spec_scan.ts` scanning the local svgwg
//!    clone (`definitions*.xml` + `text.html` + `changes.html`). This is
//!    the AUTHORITATIVE signal for "removed in SVG 2" / "obsoleted in
//!    SVG 2" because it comes directly from the spec prose and inventory
//!    files — not BCD's after-the-fact deprecation flag.
//!
//! ## Conflict rules
//!
//! - **Spec-removed + snapshot-present**: the scanner says the feature
//!   was removed (either in `text.html` `"has been removed in SVG 2"`
//!   prose, or in the `changes.html` changelog), but we still list it
//!   in the latest SVG 2 snapshot. FIX = snapshot surgery.
//! - **BCD-deprecated + snapshot-present + spec-not-removed**: BCD says
//!   deprecated but the spec still defines the feature as stable. This is
//!   the original BCD↔spec conflict that prompted the whole reconcile
//!   layer — tolerated via the exception file when both sources are
//!   correct from their own frame (e.g. glyph-orientation-vertical is
//!   "defined but obsoleted" in SVG 2 while BCD marks it deprecated).
//!
//! An allowlist TOML file at `data/reviewed/bcd_spec_exceptions.toml`
//! documents legitimate disagreements. The check **self-prunes**:
//! exceptions that don't match any current conflict are themselves
//! errors, forcing the allowlist to shrink when the spec catches up.
//!
//! Errors are **batched** into a single `cargo::error=` emission with
//! paste-ready TOML fix-it blocks, so developers see the whole picture
//! at once when a BCD bump or spec bump flips multiple entries.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use serde::Deserialize;

use super::{bcd, types::SpecSnapshotId};

const EXCEPTION_FILE_PATH: &str = "data/reviewed/bcd_spec_exceptions.toml";
const SPEC_REMOVALS_PATH: &str = "data/reviewed/spec_removals.json";
const BCD_PACKAGE: &str = "@mdn/browser-compat-data";

/// The declared shape of a single exception entry.
///
/// Fields are mandatory on purpose — each exception is a load-bearing
/// piece of documentation for a future maintainer, not a throwaway
/// suppression.
#[derive(Debug, Deserialize)]
struct Exception {
    name: String,
    element: String,
    bcd_says: String,
    spec_says: String,
    reason: String,
    added: String,
    upstream_ref: String,
}

/// Top-level shape of the TOML exception file.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExceptionFile {
    #[serde(rename = "attribute")]
    attributes: Vec<Exception>,
    #[serde(rename = "element")]
    elements: Vec<Exception>,
}

/// Exception kind — mirrors the TOML table name so error messages
/// report the right header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Attribute,
    Element,
}

impl Kind {
    const fn header(self) -> &'static str {
        match self {
            Self::Attribute => "attribute",
            Self::Element => "element",
        }
    }
}

/// Unique identity of an exception entry for the self-pruning pass.
/// Two exceptions with the same `(kind, name, element)` refer to the
/// same conflict.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ExceptionId {
    kind: Kind,
    name: String,
    element: String,
}

impl ExceptionId {
    const fn new(kind: Kind, name: String, element: String) -> Self {
        Self {
            kind,
            name,
            element,
        }
    }
}

/// A single unresolved conflict. The variant distinguishes which
/// source pair disagrees so the batched error message can give a
/// targeted fix-it block.
struct Conflict {
    kind: Kind,
    name: String,
    /// Element scope for attribute conflicts; `"*"` for elements and
    /// globally-applicable attributes.
    element: String,
    /// Which rule fired.
    rule: ConflictRule,
    /// Source-scanner provenance when the conflict was flagged by the
    /// spec scanner (not populated for BCD↔spec-only conflicts).
    spec_evidence: Option<String>,
}

/// The three reconciliation rules the check enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConflictRule {
    /// Spec scanner says the feature was removed in SVG 2, but our
    /// snapshot membership still lists it in the latest snapshot.
    /// Authoritative — the spec prose trumps both BCD and snapshot.
    SpecRemovedButSnapshotPresent,
    /// BCD flags the feature deprecated, snapshot says stable, and
    /// the spec scanner did NOT flag it as removed. Either source
    /// could be right; must be documented via an exception or
    /// fixed in the data.
    BcdDeprecatedButSnapshotStable,
}

impl ConflictRule {
    const fn headline(self) -> &'static str {
        match self {
            Self::SpecRemovedButSnapshotPresent => "Spec-removed but snapshot-present",
            Self::BcdDeprecatedButSnapshotStable => "BCD-deprecated but snapshot-stable",
        }
    }
}

/// Minimal shape of a single `spec_removals.json` fact. Only fields
/// the reconcile check actually reads are declared here.
#[derive(Debug, Deserialize)]
struct SpecFact {
    name: String,
    kind: String,
    #[serde(default)]
    provenance: Option<SpecProvenance>,
}

#[derive(Debug, Deserialize)]
struct SpecProvenance {
    file: String,
    line: u32,
    #[serde(default)]
    text: String,
}

/// Top-level shape of `spec_removals.json`. We only deserialize the
/// lists the reconcile check consumes; other fields pass through
/// untouched.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SpecReport {
    removed_properties: Vec<SpecFact>,
    obsoleted_properties: Vec<SpecFact>,
    changelog_removals: Vec<SpecFact>,
}

impl SpecReport {
    /// Normalise the scanner output into a lookup map keyed by
    /// `(kind, name)`. Used so the reconcile loop can answer "has the
    /// spec scanner flagged X as removed?" in O(1).
    fn removal_index(&self) -> HashMap<(String, String), String> {
        let mut index = HashMap::new();
        // text.html per-property overrides are the most authoritative.
        for fact in &self.removed_properties {
            let key = (fact.kind.clone(), fact.name.clone());
            index.insert(key, describe_provenance(fact));
        }
        // changes.html entries cover elements + attributes not in text.html.
        // Lower-priority — text.html wins if both mention the same feature.
        for fact in &self.changelog_removals {
            let key = (fact.kind.clone(), fact.name.clone());
            index
                .entry(key)
                .or_insert_with(|| describe_provenance(fact));
        }
        index
    }
}

fn describe_provenance(fact: &SpecFact) -> String {
    fact.provenance.as_ref().map_or_else(
        || "(no provenance)".to_string(),
        |p| format!("{}:{} — {}", p.file, p.line, p.text),
    )
}

/// Run the reconciliation check over BCD + union membership data.
///
/// Returns `Ok(())` when the state is clean (every conflict has a
/// matching exception AND every exception matches a live conflict).
/// On failure, emits a single batched `cargo::error=` with all
/// conflicts + stale exceptions, then returns `Err`.
pub fn run(
    manifest_dir: &Path,
    compat: &bcd::CompatData,
    union_elements: &[UnionElementFacts],
    union_attributes: &[UnionAttributeFacts],
    latest_snapshot: SpecSnapshotId,
    bcd_version: &str,
) -> Result<(), String> {
    let exceptions_path = manifest_dir.join(EXCEPTION_FILE_PATH);
    let spec_removals_path = manifest_dir.join(SPEC_REMOVALS_PATH);
    // Track both files so cargo reruns the build when either changes.
    println!("cargo::rerun-if-changed={}", exceptions_path.display());
    println!("cargo::rerun-if-changed={}", spec_removals_path.display());

    let exception_file = load_exceptions(&exceptions_path)?;
    let all_exceptions: Vec<(Kind, &Exception)> = exception_file
        .attributes
        .iter()
        .map(|e| (Kind::Attribute, e))
        .chain(exception_file.elements.iter().map(|e| (Kind::Element, e)))
        .collect();

    let spec_report = load_spec_report(&spec_removals_path)?;
    let spec_removals = spec_report.removal_index();

    let mut matched: HashSet<ExceptionId> = HashSet::new();
    let mut conflicts: Vec<Conflict> = Vec::new();

    // Pass 1: spec-scanner says feature was removed, snapshot still lists it.
    detect_spec_removed_conflicts(
        union_attributes,
        union_elements,
        latest_snapshot,
        &spec_removals,
        &mut conflicts,
    );

    // Pass 2 + 3: BCD-deprecated but snapshot-stable (original rule).
    detect_bcd_deprecated_conflicts(
        compat,
        union_attributes,
        union_elements,
        latest_snapshot,
        &all_exceptions,
        &mut conflicts,
        &mut matched,
    );

    // Pass 4: self-prune — exceptions that didn't match any live conflict.
    let dead: Vec<(Kind, &Exception)> = all_exceptions
        .iter()
        .filter_map(|(kind, exception)| {
            let id = ExceptionId::new(*kind, exception.name.clone(), exception.element.clone());
            (!matched.contains(&id)).then_some((*kind, *exception))
        })
        .collect();

    if conflicts.is_empty() && dead.is_empty() {
        return Ok(());
    }

    let message = render_error(&conflicts, &dead, bcd_version);
    // Single batched emission — one cargo::error containing every
    // problem so developers see the whole picture at once.
    println!("cargo::error={}", message.replace('\n', "%0A"));
    Err(format!(
        "BCD/spec reconciliation failed ({} conflict{}, {} stale exception{})",
        conflicts.len(),
        if conflicts.len() == 1 { "" } else { "s" },
        dead.len(),
        if dead.len() == 1 { "" } else { "s" },
    ))
}

/// Populate `conflicts` with every entry the spec scanner flagged as
/// removed that the snapshot still lists in the latest snapshot. This
/// is Pass 1 of `run()` — the authoritative rule, since the scanner
/// reads spec prose directly.
fn detect_spec_removed_conflicts(
    union_attributes: &[UnionAttributeFacts],
    union_elements: &[UnionElementFacts],
    latest_snapshot: SpecSnapshotId,
    spec_removals: &HashMap<(String, String), String>,
    conflicts: &mut Vec<Conflict>,
) {
    // The `attribute` kind in spec_removals.json collapses both
    // `attribute` and `property` — properties in SVG are attributes
    // in our data model.
    let attribute_removed_key = |name: &str| -> Option<String> {
        spec_removals
            .get(&("attribute".to_string(), name.to_string()))
            .or_else(|| spec_removals.get(&("property".to_string(), name.to_string())))
            .cloned()
    };
    let element_removed_key = |name: &str| -> Option<String> {
        spec_removals
            .get(&("element".to_string(), name.to_string()))
            .cloned()
    };

    for facts in union_attributes {
        let Some(evidence) = attribute_removed_key(&facts.name) else {
            continue;
        };
        if !facts.present_in.contains(&latest_snapshot) {
            continue;
        }
        conflicts.push(Conflict {
            kind: Kind::Attribute,
            name: facts.name.clone(),
            element: attribute_scope(&facts.elements),
            rule: ConflictRule::SpecRemovedButSnapshotPresent,
            spec_evidence: Some(evidence),
        });
    }

    for facts in union_elements {
        let Some(evidence) = element_removed_key(&facts.name) else {
            continue;
        };
        if !facts.present_in.contains(&latest_snapshot) {
            continue;
        }
        conflicts.push(Conflict {
            kind: Kind::Element,
            name: facts.name.clone(),
            element: "*".to_string(),
            rule: ConflictRule::SpecRemovedButSnapshotPresent,
            spec_evidence: Some(evidence),
        });
    }
}

/// Populate `conflicts` with entries flagged by the classic BCD↔spec
/// rule (BCD says deprecated, snapshot says stable, spec scanner didn't
/// flag it as removed). Skips entries already flagged by
/// [`detect_spec_removed_conflicts`] — that rule dominates.
fn detect_bcd_deprecated_conflicts<'exc>(
    compat: &bcd::CompatData,
    union_attributes: &[UnionAttributeFacts],
    union_elements: &[UnionElementFacts],
    latest_snapshot: SpecSnapshotId,
    all_exceptions: &'exc [(Kind, &'exc Exception)],
    conflicts: &mut Vec<Conflict>,
    matched: &mut HashSet<ExceptionId>,
) {
    let already_flagged: HashSet<(Kind, String)> =
        conflicts.iter().map(|c| (c.kind, c.name.clone())).collect();

    for facts in union_attributes {
        if already_flagged.contains(&(Kind::Attribute, facts.name.clone())) {
            continue;
        }
        let Some(bcd_attr) = compat.attributes.get(&facts.name) else {
            continue;
        };
        if !bcd_attr.compat.deprecated {
            continue;
        }
        if !facts.present_in.contains(&latest_snapshot) {
            continue;
        }
        let conflict = Conflict {
            kind: Kind::Attribute,
            name: facts.name.clone(),
            element: attribute_scope(&facts.elements),
            rule: ConflictRule::BcdDeprecatedButSnapshotStable,
            spec_evidence: None,
        };
        match find_matching_exception(&conflict, all_exceptions) {
            Some(id) => {
                matched.insert(id);
            }
            None => conflicts.push(conflict),
        }
    }

    for facts in union_elements {
        if already_flagged.contains(&(Kind::Element, facts.name.clone())) {
            continue;
        }
        let Some(bcd_el) = compat.elements.get(&facts.name) else {
            continue;
        };
        if !bcd_el.deprecated {
            continue;
        }
        if !facts.present_in.contains(&latest_snapshot) {
            continue;
        }
        let conflict = Conflict {
            kind: Kind::Element,
            name: facts.name.clone(),
            element: "*".to_string(),
            rule: ConflictRule::BcdDeprecatedButSnapshotStable,
            spec_evidence: None,
        };
        match find_matching_exception(&conflict, all_exceptions) {
            Some(id) => {
                matched.insert(id);
            }
            None => conflicts.push(conflict),
        }
    }
}

/// Facts about a union attribute needed by the reconciliation pass.
///
/// Extracted so the check doesn't take a hard dependency on the full
/// `UnionAttribute` build struct (which carries unrelated codegen state).
pub struct UnionAttributeFacts {
    pub name: String,
    pub present_in: Vec<SpecSnapshotId>,
    /// Element scope — `*` for global, or specific element names.
    pub elements: Vec<String>,
}

/// Facts about a union element needed by the reconciliation pass.
pub struct UnionElementFacts {
    pub name: String,
    pub present_in: Vec<SpecSnapshotId>,
}

fn load_exceptions(path: &Path) -> Result<ExceptionFile, String> {
    if !path.exists() {
        // No allowlist yet — treat as empty. Callers still error on
        // any current conflict.
        return Ok(ExceptionFile::default());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

/// Load the committed spec scanner report. Absent file → empty report
/// (no spec-removed conflicts surfaced, but the BCD↔spec check still runs).
///
/// The report is produced by the Deno CLI command
/// `deno run -A workers/svg-compat/src/cli.ts scan-spec --svgwg-path=./svgwg
/// --out crates/svg-data/data/reviewed/spec_removals.json`.
fn load_spec_report(path: &Path) -> Result<SpecReport, String> {
    if !path.exists() {
        println!(
            "cargo::warning=reconcile: no spec_removals.json at {} — skipping spec→snapshot checks. \
             Regenerate via `deno run -A workers/svg-compat/src/cli.ts scan-spec --out {}`.",
            path.display(),
            path.display(),
        );
        return Ok(SpecReport::default());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

fn attribute_scope(elements: &[String]) -> String {
    if elements.iter().any(|e| e == "*") || elements.is_empty() {
        "*".to_string()
    } else if elements.len() == 1 {
        elements[0].clone()
    } else {
        // Multi-element attribute: use `*` for the conflict scope and
        // let the exception match against it. Element-specific
        // exceptions (rare) will not match and are caller-driven.
        "*".to_string()
    }
}

fn find_matching_exception(
    conflict: &Conflict,
    exceptions: &[(Kind, &Exception)],
) -> Option<ExceptionId> {
    // Specific-element exception wins over wildcard when both exist.
    let mut specific: Option<ExceptionId> = None;
    let mut wildcard: Option<ExceptionId> = None;
    for (kind, exception) in exceptions {
        if *kind != conflict.kind || exception.name != conflict.name {
            continue;
        }
        let id = ExceptionId::new(*kind, exception.name.clone(), exception.element.clone());
        if exception.element == conflict.element {
            specific = Some(id);
        } else if exception.element == "*" {
            wildcard = Some(id);
        }
    }
    specific.or(wildcard)
}

/// Render the fix-it block for a [`ConflictRule::SpecRemovedButSnapshotPresent`]
/// conflict. The spec scanner's provenance line (`file:line — quoted prose`)
/// is the headline evidence; the fix is always a snapshot surgery.
fn render_spec_removed_fix(out: &mut String, conflict: &Conflict) {
    use std::fmt::Write as _;
    let evidence = conflict
        .spec_evidence
        .as_deref()
        .unwrap_or("(no provenance)");
    let _ = writeln!(out, "  Spec scanner: REMOVED at {evidence}");
    let _ = writeln!(
        out,
        "  Snapshot:     present in Svg2EditorsDraft20250914 → Stable"
    );
    out.push('\n');
    out.push_str("  Fix: remove this feature from the SVG 2 snapshot data.\n");
    out.push_str("  The spec scanner (svgwg text.html / changes.html) is the\n");
    out.push_str("  authoritative signal here — it reads the primary source.\n");
    out.push_str("  Run `deno run -A workers/svg-compat/src/cli.ts scan-spec` to\n");
    out.push_str("  regenerate spec_removals.json after a svgwg bump.\n");
    out.push_str("  Edit these files to delete the feature:\n");
    out.push_str("    data/specs/Svg2EditorsDraft20250914/attributes.json (or elements.json)\n");
    out.push_str("    data/specs/Svg2EditorsDraft20250914/element_attribute_matrix.json\n");
    out.push_str("    data/specs/Svg2Cr20181004/ (same three files, if also removed at CR)\n");
    out.push_str("    data/derived/union/attributes.json (membership list)\n");
    out.push_str("  Then bump the per-snapshot review.json counts + the overlay file.\n");
    out.push('\n');
}

/// Render the fix-it block for a [`ConflictRule::BcdDeprecatedButSnapshotStable`]
/// conflict. Two paths: snapshot surgery OR an exception file entry
/// with a paste-ready TOML stanza.
fn render_bcd_deprecated_fix(
    out: &mut String,
    conflict: &Conflict,
    header: &str,
    bcd_version: &str,
) {
    use std::fmt::Write as _;
    let _ = writeln!(
        out,
        "  BCD ({BCD_PACKAGE}@{bcd_version}): deprecated = true"
    );
    let _ = writeln!(
        out,
        "  Snapshot:     present → Stable (spec scanner: not-removed)"
    );
    out.push('\n');
    out.push_str("  Fix one of:\n");
    out.push_str("    1. Update data/specs/Svg2EditorsDraft20250914/ files to remove the\n");
    out.push_str("       feature (if the spec actually dropped it — run scan-spec to\n");
    out.push_str("       regenerate spec_removals.json and re-check).\n");
    out.push_str("    2. Add an entry to data/reviewed/bcd_spec_exceptions.toml:\n\n");
    let _ = writeln!(out, "       [[{header}]]");
    let _ = writeln!(out, "       name = \"{}\"", conflict.name);
    let _ = writeln!(out, "       element = \"{}\"", conflict.element);
    out.push_str("       bcd_says = \"deprecated\"\n");
    out.push_str("       spec_says = \"stable\"\n");
    out.push_str("       reason = \"<WHY — one sentence>\"\n");
    out.push_str("       added = \"2026-04-15\"\n");
    out.push_str("       upstream_ref = \"<URL to primary source>\"\n");
    out.push('\n');
}

fn render_error(conflicts: &[Conflict], dead: &[(Kind, &Exception)], bcd_version: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "BCD/spec reconciliation failed ({} conflict{}, {} stale exception{}).",
        conflicts.len(),
        if conflicts.len() == 1 { "" } else { "s" },
        dead.len(),
        if dead.len() == 1 { "" } else { "s" },
    );
    out.push('\n');

    for (idx, conflict) in conflicts.iter().enumerate() {
        let header = conflict.kind.header();
        let _ = writeln!(
            out,
            "Conflict #{}: [{}] {header} `{}` on <{}>",
            idx + 1,
            conflict.rule.headline(),
            conflict.name,
            conflict.element,
        );
        match conflict.rule {
            ConflictRule::SpecRemovedButSnapshotPresent => {
                render_spec_removed_fix(&mut out, conflict);
            }
            ConflictRule::BcdDeprecatedButSnapshotStable => {
                render_bcd_deprecated_fix(&mut out, conflict, header, bcd_version);
            }
        }
    }

    if !dead.is_empty() {
        out.push_str("Stale exceptions (listed but no live conflict — remove from the TOML):\n");
        for (kind, exception) in dead {
            let _ = writeln!(
                out,
                "  - [[{}]] name = {:?}  element = {:?}",
                kind.header(),
                exception.name,
                exception.element,
            );
            let _ = writeln!(out, "    added: {}", exception.added);
            let _ = writeln!(out, "    reason: {}", exception.reason);
        }
        out.push('\n');
    }

    out.push_str("Source: crates/svg-data/build/reconcile.rs::run\n");
    out
}
