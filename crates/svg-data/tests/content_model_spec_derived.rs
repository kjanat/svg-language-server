//! Reproducibility gate: the committed SVG 2 Editor's Draft element content
//! models must equal the values derived from the vendored `definitions*.xml`.
//!
//! Regenerate with `cargo run -p svg-data --example derive_content_models`
//! (then `dprint fmt`) whenever the vendored spec changes. This test fails if
//! anyone hand-edits a content model out of agreement with the spec.

use std::{error::Error, path::PathBuf};

use svg_data::snapshot_schema::SnapshotElementRecord;

#[path = "../derivation/content_model.rs"]
mod content_model;

#[test]
fn ed_content_models_match_spec_derivation() -> Result<(), Box<dyn Error>> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/specs/Svg2EditorsDraft/elements.json");
    let records: Vec<SnapshotElementRecord> = serde_json::from_slice(&std::fs::read(&path)?)?;
    let derived = content_model::derive_ed_content_models()?;

    let mut mismatches = Vec::new();
    for record in &records {
        let Some(expected) = derived.get(&record.name) else {
            // Catalog element with no SVG 2 definition (removed after SVG 1.1);
            // its content model is not spec-derivable here.
            continue;
        };
        if record.content_model != *expected {
            mismatches.push(format!(
                "  {}: committed {:?} != derived {:?}",
                record.name, record.content_model, expected
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "{} element content model(s) drifted from the spec derivation; \
         run `cargo run -p svg-data --example derive_content_models`:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    Ok(())
}
