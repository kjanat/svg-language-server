//! Build-time referential integrity gate for provenance `source_id` values.
//!
//! Every `FactProvenance` record in the checked-in snapshot data carries a
//! `source_id` string. That string MUST resolve to a `pinned_sources[].input_id`
//! in the snapshot's `snapshot.json` — otherwise the review report has a
//! dangling reference that points at nothing, and future maintainers lose the
//! chain back to the authoritative source.
//!
//! A test in `crates/svg-data/tests/snapshot_reviews.rs` already enforces this
//! invariant at `cargo test` time. This module ports the same check into the
//! build script so the invariant is enforced earlier — at `cargo check` — and
//! cannot be bypassed by anyone who only runs `cargo build`. The two checks
//! are redundant by design: the test catches it if the gate is ever removed
//! or disabled, the gate catches it before any code emission so a bad
//! snapshot edit can't poison the generated catalog.
//!
//! Errors are **batched** into a single `cargo::error=` emission — one per
//! build — listing every dangling reference with its location (snapshot, file,
//! record kind, `source_id` value) so a developer touching the snapshot data
//! sees the whole picture at once.
//!
//! The types here deliberately mirror only the provenance-bearing fields of
//! the checked-in files. Unknown fields are ignored by serde's default
//! behaviour, so a schema bump that adds new fields won't break this gate.

use std::{fmt::Write as _, fs, path::Path};

use serde::Deserialize;

use super::types::SpecSnapshotId;

const SPECS_DIR: &str = "data/specs";

/// Minimal shape of `snapshot.json`. Only `pinned_sources[].input_id` is read
/// — everything else is ignored via serde's default unknown-field handling.
#[derive(Debug, Deserialize)]
struct SnapshotMetadataFile {
    #[serde(default)]
    pinned_sources: Vec<PinnedSource>,
}

#[derive(Debug, Deserialize)]
struct PinnedSource {
    input_id: String,
}

/// Minimal provenance shape — just `source_id`, the field the gate validates.
#[derive(Debug, Deserialize)]
struct MinimalProvenance {
    source_id: String,
}

/// A record that carries a `provenance` list. Used by every file that has
/// per-entry provenance (elements, attributes, grammars, matrix edges,
/// exceptions). Serde ignores all other fields on the wrapped record.
#[derive(Debug, Deserialize)]
struct ProvenancedRecord {
    #[serde(default)]
    provenance: Vec<MinimalProvenance>,
}

/// Shape of `grammars.json` — wraps a list of grammar definitions, each of
/// which carries its own provenance list.
#[derive(Debug, Deserialize)]
struct GrammarFile {
    #[serde(default)]
    grammars: Vec<ProvenancedRecord>,
}

/// Shape of `categories.json` — element and attribute memberships are
/// separate arrays, each carrying provenance per membership row.
#[derive(Debug, Deserialize)]
struct CategoriesFile {
    #[serde(default)]
    element_categories: Vec<ProvenancedRecord>,
    #[serde(default)]
    attribute_categories: Vec<ProvenancedRecord>,
}

/// Shape of `element_attribute_matrix.json`.
#[derive(Debug, Deserialize)]
struct ElementAttributeMatrixFile {
    #[serde(default)]
    edges: Vec<ProvenancedRecord>,
}

/// Shape of `exceptions.json`.
#[derive(Debug, Deserialize)]
struct ExceptionsFile {
    #[serde(default)]
    exceptions: Vec<ProvenancedRecord>,
}

/// A single dangling-reference violation collected during the sweep.
struct Violation {
    snapshot: SpecSnapshotId,
    file: &'static str,
    record_kind: &'static str,
    bad_source_id: String,
}

/// Mutable accumulator shared across every per-file check in a single
/// snapshot sweep. Bundling the `snapshot` id, the `allowed` list, and
/// both result vectors into one struct lets `check_records` take a single
/// `&mut SweepState` instead of four separate parameters, keeping the
/// per-call signature small and the call sites readable.
struct SweepState<'a> {
    snapshot: SpecSnapshotId,
    allowed: &'a [String],
    violations: &'a mut Vec<Violation>,
    io_errors: &'a mut Vec<String>,
}

/// Run the provenance gate against every canonical snapshot under
/// `data/specs/`. Returns `Ok(())` when every `source_id` resolves, otherwise
/// emits a single batched `cargo::error=` and returns `Err` so the build
/// halts before any code is generated.
pub fn run(manifest_dir: &Path, snapshots: &[SpecSnapshotId]) -> Result<(), String> {
    let mut violations: Vec<Violation> = Vec::new();
    let mut io_errors: Vec<String> = Vec::new();

    for &snapshot in snapshots {
        let snapshot_root = manifest_dir.join(SPECS_DIR).join(snapshot.as_str());

        // Re-run the build if any of the six files change, so a snapshot
        // edit is picked up without a clean rebuild.
        for filename in PROVENANCED_FILES {
            let path = snapshot_root.join(filename);
            println!("cargo::rerun-if-changed={}", path.display());
        }
        let snapshot_json_path = snapshot_root.join("snapshot.json");
        println!("cargo::rerun-if-changed={}", snapshot_json_path.display());

        match load_allowed_source_ids(&snapshot_json_path) {
            Ok(allowed) => {
                let mut state = SweepState {
                    snapshot,
                    allowed: &allowed,
                    violations: &mut violations,
                    io_errors: &mut io_errors,
                };
                sweep_snapshot(&snapshot_root, &mut state);
            }
            Err(e) => io_errors.push(e),
        }
    }

    if violations.is_empty() && io_errors.is_empty() {
        return Ok(());
    }

    let message = render_error(&violations, &io_errors);
    println!("cargo::error={}", message.replace('\n', "%0A"));
    Err(format!(
        "provenance gate failed ({} dangling source_id{}, {} i/o error{})",
        violations.len(),
        if violations.len() == 1 { "" } else { "s" },
        io_errors.len(),
        if io_errors.len() == 1 { "" } else { "s" },
    ))
}

/// Six files per snapshot carry provenance. `snapshot.json` itself is the
/// source of truth for allowed `input_id` values, so it's loaded separately.
const PROVENANCED_FILES: &[&str] = &[
    "elements.json",
    "attributes.json",
    "grammars.json",
    "categories.json",
    "element_attribute_matrix.json",
    "exceptions.json",
];

fn load_allowed_source_ids(path: &Path) -> Result<Vec<String>, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let metadata: SnapshotMetadataFile = serde_json::from_str(&text)
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
    Ok(metadata
        .pinned_sources
        .into_iter()
        .map(|s| s.input_id)
        .collect())
}

/// Walk every provenanced file under `snapshot_root`, decoding just the
/// provenance shoulder of each record, and push any dangling `source_id`
/// value into `state.violations`.
fn sweep_snapshot(snapshot_root: &Path, state: &mut SweepState<'_>) {
    if let Some(elements) =
        parse_file::<Vec<ProvenancedRecord>>(&snapshot_root.join("elements.json"), state.io_errors)
    {
        walk_records(state, "elements.json", "element", elements.iter());
    }
    if let Some(attributes) = parse_file::<Vec<ProvenancedRecord>>(
        &snapshot_root.join("attributes.json"),
        state.io_errors,
    ) {
        walk_records(state, "attributes.json", "attribute", attributes.iter());
    }
    if let Some(grammars) =
        parse_file::<GrammarFile>(&snapshot_root.join("grammars.json"), state.io_errors)
    {
        walk_records(state, "grammars.json", "grammar", grammars.grammars.iter());
    }
    // categories.json carries TWO provenance-bearing sections — load and
    // parse the file once, walk both halves separately so each violation
    // still reports the correct `record_kind`.
    if let Some(categories) =
        parse_file::<CategoriesFile>(&snapshot_root.join("categories.json"), state.io_errors)
    {
        walk_records(
            state,
            "categories.json",
            "element_categories",
            categories.element_categories.iter(),
        );
        walk_records(
            state,
            "categories.json",
            "attribute_categories",
            categories.attribute_categories.iter(),
        );
    }
    if let Some(matrix) = parse_file::<ElementAttributeMatrixFile>(
        &snapshot_root.join("element_attribute_matrix.json"),
        state.io_errors,
    ) {
        walk_records(
            state,
            "element_attribute_matrix.json",
            "matrix_edge",
            matrix.edges.iter(),
        );
    }
    if let Some(exceptions) =
        parse_file::<ExceptionsFile>(&snapshot_root.join("exceptions.json"), state.io_errors)
    {
        walk_records(
            state,
            "exceptions.json",
            "exception",
            exceptions.exceptions.iter(),
        );
    }
}

/// Read + deserialize a snapshot file into `T`. On failure, push a
/// human-readable error onto `io_errors` and return `None` so the caller
/// can skip the walk without aborting the rest of the sweep — every
/// snapshot still gets a chance to report its own problems.
fn parse_file<T>(path: &Path, io_errors: &mut Vec<String>) -> Option<T>
where
    T: serde::de::DeserializeOwned,
{
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            io_errors.push(format!("failed to read {}: {e}", path.display()));
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(parsed) => Some(parsed),
        Err(e) => {
            io_errors.push(format!("failed to parse {}: {e}", path.display()));
            None
        }
    }
}

/// Validate every `source_id` in `records` against `state.allowed`,
/// pushing one [`Violation`] per dangling reference. Tagged with
/// `file_name` + `record_kind` so the batched error message points at
/// the exact section a maintainer needs to look at.
fn walk_records<'a, I>(
    state: &mut SweepState<'_>,
    file_name: &'static str,
    record_kind: &'static str,
    records: I,
) where
    I: IntoIterator<Item = &'a ProvenancedRecord>,
{
    for record in records {
        for provenance in &record.provenance {
            if !state.allowed.iter().any(|id| id == &provenance.source_id) {
                state.violations.push(Violation {
                    snapshot: state.snapshot,
                    file: file_name,
                    record_kind,
                    bad_source_id: provenance.source_id.clone(),
                });
            }
        }
    }
}

fn render_error(violations: &[Violation], io_errors: &[String]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "provenance gate failed ({} dangling source_id{}, {} i/o error{}).",
        violations.len(),
        if violations.len() == 1 { "" } else { "s" },
        io_errors.len(),
        if io_errors.len() == 1 { "" } else { "s" },
    );
    out.push('\n');

    if !violations.is_empty() {
        out.push_str("Dangling provenance.source_id values:\n");
        for v in violations {
            let _ = writeln!(
                out,
                "  - [{}] {} ({}): source_id = {:?}",
                v.snapshot.as_str(),
                v.file,
                v.record_kind,
                v.bad_source_id,
            );
        }
        out.push('\n');
        out.push_str("Fix: either\n");
        out.push_str("  1. Add the missing input to data/specs/<snapshot>/snapshot.json\n");
        out.push_str("     under pinned_sources[], OR\n");
        out.push_str("  2. Update the offending record to use an existing input_id.\n");
        out.push_str("  snapshot.json is the authoritative list of pinnable sources.\n");
        out.push('\n');
    }

    if !io_errors.is_empty() {
        out.push_str("I/O or parse errors:\n");
        for err in io_errors {
            let _ = writeln!(out, "  - {err}");
        }
        out.push('\n');
    }

    out.push_str("Source: crates/svg-data/build/provenance_gate.rs::run\n");
    out
}
