//! Regenerate the SVG 2 Editor's Draft element content models from the spec.
//!
//! Reads `data/specs/Svg2EditorsDraft/elements.json`, replaces each record's
//! `content_model` with the value derived from the vendored `definitions*.xml`
//! (see [`content_model::derive_ed_content_models`]), and writes the file back
//! with every other field untouched. Run:
//!
//! ```sh
//! cargo run -p svg-data --example derive_content_models
//! dprint fmt 'crates/svg-data/**/*.json'
//! ```
//!
//! The [`tests/content_model_spec_derived`](../tests/content_model_spec_derived.rs)
//! gate fails if the committed file drifts from this derivation.

use std::{error::Error, path::PathBuf};

use svg_data::snapshot_schema::SnapshotElementRecord;

#[path = "../derivation/content_model.rs"]
mod content_model;

fn main() -> Result<(), Box<dyn Error>> {
    let path = elements_path();
    let mut records: Vec<SnapshotElementRecord> = serde_json::from_slice(&std::fs::read(&path)?)?;
    let derived = content_model::derive_ed_content_models()?;

    let mut changed = 0_usize;
    let mut undefined = Vec::new();
    for record in &mut records {
        match derived.get(&record.name) {
            Some(model) => {
                if record.content_model != *model {
                    record.content_model = model.clone();
                    changed += 1;
                }
            }
            // Present in the catalog but not in the SVG 2 definitions (e.g.
            // elements removed after SVG 1.1). Their content model is not
            // spec-derivable here, so leave the committed value in place.
            None => undefined.push(record.name.clone()),
        }
    }

    let mut json = serde_json::to_string_pretty(&records)?;
    json.push('\n');
    std::fs::write(&path, json)?;

    println!("updated {changed} content model(s) in {}", path.display());
    if !undefined.is_empty() {
        println!(
            "left {} non-SVG2 element(s) untouched (no definitions.xml entry): {}",
            undefined.len(),
            undefined.join(", ")
        );
    }
    Ok(())
}

fn elements_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/specs/Svg2EditorsDraft/elements.json")
}
