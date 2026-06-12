//! Reproduction test for the Rust spec scanner.
//!
//! Runs `build/spec_scan.rs` over the **vendored** svgwg checkout and
//! asserts the produced records equal the committed
//! `data/reviewed/spec_removals.json`. The comparison is semantic: both
//! sides are normalised to `serde_json::Value` and the `source_pin`
//! (which carries a wall-clock `generated_at`) is compared field by field
//! except `generated_at`, which is inherently non-reproducible.
//!
//! This proves the Rust port is a faithful reproduction of the Deno
//! `spec_scan.ts` output without wiring the scanner as a hard build gate.

#[path = "../build/spec_scan.rs"]
mod spec_scan;

use std::path::PathBuf;

use serde_json::Value;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Locate the svgwg `master/` directory to scan for a given pinned commit.
///
/// The vendored checkout dir is named `svgwg-<commit[..8]>`, so the scan
/// target is selected by the committed `source_pin.commit` rather than by
/// filesystem-listing order. Multiple `svgwg-*` checkouts coexist (the
/// editor's-draft *definitions* pin differs from the *spec-removals* scan
/// pin), so first-by-`read_dir` would non-deterministically pick the wrong
/// one. The `SPEC_SCAN_MASTER_OVERRIDE` env var points the scan at an
/// alternate `master/` dir — used to prove the port reproduces the committed
/// JSON against the *pristine* upstream bytes when the vendored copy has been
/// reformatted.
fn vendored_master(commit: &str) -> PathBuf {
    if let Ok(override_path) = std::env::var("SPEC_SCAN_MASTER_OVERRIDE") {
        return PathBuf::from(override_path);
    }
    let Some(prefix) = commit.get(..8) else {
        panic!("source_pin.commit is too short to form a checkout dir: {commit:?}");
    };
    let dir = manifest_dir()
        .join("data/sources")
        .join(format!("svgwg-{prefix}"));
    assert!(
        dir.is_dir(),
        "no vendored checkout for pin {commit} at {}",
        dir.display()
    );
    dir.join("master")
}

/// Render a concise record-level diff between produced and committed
/// arrays: records present only on one side, indexed by `(kind, name)`,
/// plus per-field provenance mismatches for records present on both.
fn list_diff(produced: Option<&[Value]>, committed: Option<&[Value]>) -> String {
    use std::collections::BTreeMap;

    fn key(value: &Value) -> String {
        format!(
            "{}::{}",
            value["kind"].as_str().unwrap_or("?"),
            value["name"].as_str().unwrap_or("?"),
        )
    }

    let produced: BTreeMap<String, &Value> = produced
        .unwrap_or(&[])
        .iter()
        .map(|v| (key(v), v))
        .collect();
    let committed: BTreeMap<String, &Value> = committed
        .unwrap_or(&[])
        .iter()
        .map(|v| (key(v), v))
        .collect();

    let mut lines = Vec::new();
    for k in produced.keys() {
        if !committed.contains_key(k) {
            lines.push(format!("  + only produced: {k}"));
        }
    }
    for (k, expected) in &committed {
        match produced.get(k) {
            None => lines.push(format!("  - only committed: {k}")),
            Some(got) if got != expected => lines.push(format!(
                "  ~ {k}\n      produced:  {}\n      committed: {}",
                got["provenance"], expected["provenance"]
            )),
            Some(_) => {}
        }
    }
    lines.join("\n")
}

#[test]
fn rust_scanner_reproduces_spec_removals_json() {
    let committed_path = manifest_dir().join("data/reviewed/spec_removals.json");
    let Ok(committed_text) = std::fs::read_to_string(&committed_path) else {
        panic!(
            "spec_removals.json not readable at {}",
            committed_path.display()
        );
    };
    let committed: Value = match serde_json::from_str(&committed_text) {
        Ok(value) => value,
        Err(error) => panic!("spec_removals.json is not valid JSON: {error}"),
    };

    // Pull the source pin from the committed file so the scanner is run
    // with the same provenance metadata. `generated_at` is excluded from
    // the comparison below.
    let pin = &committed["source_pin"];
    let (Some(repository), Some(commit), Some(generated_at)) = (
        pin["repository"].as_str(),
        pin["commit"].as_str(),
        pin["generated_at"].as_str(),
    ) else {
        panic!("source_pin is missing required string fields");
    };
    let commit_date = pin["commit_date"].as_str();

    let master = vendored_master(commit);
    let report =
        match spec_scan::scan_svg2_spec(&master, repository, commit, commit_date, generated_at) {
            Ok(report) => report,
            Err(error) => panic!(
                "scanner failed over inputs at {}: {error}",
                master.display()
            ),
        };

    let produced: Value = match serde_json::to_value(&report) {
        Ok(value) => value,
        Err(error) => panic!("report does not serialize: {error}"),
    };

    // schema_version is a scalar — direct compare.
    assert_eq!(
        produced.get("schema_version"),
        committed.get("schema_version")
    );

    // Compare every list field semantically, reporting the precise
    // record-level symmetric difference rather than dumping both arrays.
    let mut divergences = Vec::new();
    for field in [
        "defined_elements",
        "defined_attributes",
        "defined_properties",
        "removed_properties",
        "obsoleted_properties",
        "changelog_removals",
    ] {
        let diff = list_diff(
            produced
                .get(field)
                .and_then(Value::as_array)
                .map(Vec::as_slice),
            committed
                .get(field)
                .and_then(Value::as_array)
                .map(Vec::as_slice),
        );
        if !diff.is_empty() {
            divergences.push(format!("field `{field}`:\n{diff}"));
        }
    }
    assert!(
        divergences.is_empty(),
        "scanner output diverges from committed spec_removals.json:\n\n{}",
        divergences.join("\n\n"),
    );

    // source_pin: compare everything except the wall-clock generated_at.
    let produced_pin = &produced["source_pin"];
    assert_eq!(
        produced_pin["repository"],
        committed["source_pin"]["repository"]
    );
    assert_eq!(produced_pin["commit"], committed["source_pin"]["commit"]);
    assert_eq!(
        produced_pin["commit_date"],
        committed["source_pin"]["commit_date"]
    );
}
