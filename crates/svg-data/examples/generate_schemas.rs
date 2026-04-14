//! Generate JSON Schema files for every checked-in snapshot data format.
//!
//! Schemas are written to `data/schemas/` and referenced by `$schema` fields
//! in the generated snapshot JSON files.
//!
//! ```sh
//! cargo run -p svg-data --example generate_schemas
//! ```

use std::{error::Error, fs, path::Path};

use schemars::{Schema, schema_for};
use svg_data::{
    extraction::SourceManifest,
    snapshot_schema::{
        CategoriesFile, ElementAttributeMatrixFile, ExceptionsFile, GrammarFile, ReviewFile,
        SnapshotAttributeRecord, SnapshotElementRecord, SnapshotMetadataFile,
    },
};

fn main() -> Result<(), Box<dyn Error>> {
    let schemas_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/schemas");
    fs::create_dir_all(&schemas_dir)?;

    let files: &[(&str, Schema)] = &[
        ("snapshot", schema_for!(SnapshotMetadataFile)),
        // elements.json and attributes.json are top-level JSON arrays.
        ("elements", schema_for!(Vec<SnapshotElementRecord>)),
        ("attributes", schema_for!(Vec<SnapshotAttributeRecord>)),
        ("grammars", schema_for!(GrammarFile)),
        ("categories", schema_for!(CategoriesFile)),
        (
            "element_attribute_matrix",
            schema_for!(ElementAttributeMatrixFile),
        ),
        ("exceptions", schema_for!(ExceptionsFile)),
        ("review", schema_for!(ReviewFile)),
        ("source-manifest", schema_for!(SourceManifest)),
    ];

    for (name, schema) in files {
        let path = schemas_dir.join(format!("{name}.schema.json"));
        let mut json = serde_json::to_string_pretty(schema)?;
        json.push('\n');
        fs::write(&path, &json)?;
        println!("{}", path.display());
    }

    Ok(())
}
