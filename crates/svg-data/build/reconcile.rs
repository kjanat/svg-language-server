//! Build-time BCD ↔ spec-membership reconciliation check.
//!
//! Detects entries where BCD marks a feature `deprecated: true` while the
//! spec snapshot still lists the feature as present in the latest SVG
//! snapshot (`SpecLifecycle::Stable`). Such entries previously rendered
//! a self-contradictory hover (`**Deprecated**` + `**Stable in …**`);
//! this check makes the conflict a hard build error so it can't regress.
//!
//! An allowlist TOML file at `data/reviewed/bcd_spec_exceptions.toml`
//! documents legitimate disagreements (e.g. glyph-orientation-* are
//! defined-but-obsoleted in SVG 2 while BCD flags them deprecated). The
//! check **self-prunes**: exceptions that don't match any current
//! conflict are themselves errors, forcing the allowlist to shrink
//! when the spec catches up.
//!
//! Errors are **batched** into a single `cargo::error=` emission with
//! paste-ready TOML fix-it blocks, so developers see the whole picture
//! at once when a BCD bump flips multiple entries.

use std::{collections::HashSet, path::Path};

use serde::Deserialize;

use super::{bcd, types::SpecSnapshotId};

const EXCEPTION_FILE_PATH: &str = "data/reviewed/bcd_spec_exceptions.toml";
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

/// A single unresolved conflict — BCD says deprecated, spec says stable,
/// and no exception matched.
struct Conflict {
    kind: Kind,
    name: String,
    /// Element scope for attribute conflicts; `"*"` for elements and
    /// globally-applicable attributes.
    element: String,
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
    // Track the file so cargo reruns the build when the allowlist changes.
    println!("cargo::rerun-if-changed={}", exceptions_path.display());

    let exception_file = load_exceptions(&exceptions_path)?;
    let all_exceptions: Vec<(Kind, &Exception)> = exception_file
        .attributes
        .iter()
        .map(|e| (Kind::Attribute, e))
        .chain(exception_file.elements.iter().map(|e| (Kind::Element, e)))
        .collect();

    let mut matched: HashSet<ExceptionId> = HashSet::new();
    let mut conflicts: Vec<Conflict> = Vec::new();

    // Pass 1: walk BCD-deprecated attributes, detect conflicts.
    for facts in union_attributes {
        let Some(bcd_attr) = compat.attributes.get(&facts.name) else {
            continue;
        };
        if !bcd_attr.compat.deprecated {
            continue;
        }
        if !facts.present_in.contains(&latest_snapshot) {
            // Spec also says it's gone → both sources agree.
            continue;
        }
        let conflict = Conflict {
            kind: Kind::Attribute,
            name: facts.name.clone(),
            // Attributes with a bound element list use that; otherwise
            // global — match against the wildcard.
            element: attribute_scope(&facts.elements),
        };
        match find_matching_exception(&conflict, &all_exceptions) {
            Some(id) => {
                matched.insert(id);
            }
            None => conflicts.push(conflict),
        }
    }

    // Pass 2: walk BCD-deprecated elements, detect conflicts.
    for facts in union_elements {
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
        };
        match find_matching_exception(&conflict, &all_exceptions) {
            Some(id) => {
                matched.insert(id);
            }
            None => conflicts.push(conflict),
        }
    }

    // Pass 3: self-prune — exceptions that didn't match any live
    // conflict are rot and must be removed.
    let mut dead: Vec<(Kind, &Exception)> = Vec::new();
    for (kind, exception) in &all_exceptions {
        let id = ExceptionId::new(*kind, exception.name.clone(), exception.element.clone());
        if !matched.contains(&id) {
            dead.push((*kind, exception));
        }
    }

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
            "Conflict #{}: {header} `{}` on <{}>",
            idx + 1,
            conflict.name,
            conflict.element,
        );
        let _ = writeln!(
            out,
            "  BCD ({BCD_PACKAGE}@{bcd_version}): deprecated = true"
        );
        let _ = writeln!(
            out,
            "  Spec snapshot:                                    present → Stable"
        );
        out.push('\n');
        out.push_str("  Fix one of:\n");
        out.push_str("    1. Update data/specs/Svg2EditorsDraft20250914/ files to remove the\n");
        out.push_str("       feature (if the spec actually dropped it — verify at\n");
        out.push_str("       https://svgwg.org/svg2-draft/).\n");
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
