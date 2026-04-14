//! Generate JSON Schema files for every checked-in snapshot data format, plus
//! a `catalog.json` that maps file-glob patterns to their schemas.
//!
//! Schemas are written to `data/schemas/` and referenced by `$schema` fields
//! in the generated snapshot JSON files.
//!
//! ```sh
//! cargo run -p svg-data --example generate_schemas
//! ```

use std::{error::Error, fs, path::Path};

use schemars::{Schema, schema_for};
use serde_json::json;
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
        // elements.json and attributes.json are top-level JSON arrays, so they
        // cannot carry an inline "$schema" key. The catalog covers them instead.
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

    // Catalog: maps file-glob patterns → schema URLs (relative to this file).
    // fileMatch globs are matched against workspace-root-relative paths.
    // url paths are relative to the catalog file itself.
    // Tools: VS Code JSON language server, check-jsonschema --catalog, etc.
    let catalog = json!({
        "$schema": "https://json.schemastore.org/schema-catalog.json",
        "version": 1,
        "schemas": [
            {
                "name": "SVG Spec Snapshot Metadata",
                "fileMatch": ["**/specs/*/snapshot.json"],
                "url": "snapshot.schema.json"
            },
            {
                "name": "SVG Spec Elements",
                "description": "Per-snapshot and union element records. Top-level array — no inline $schema possible.",
                "fileMatch": [
                    "**/specs/*/elements.json",
                    "**/derived/union/elements.json"
                ],
                "url": "elements.schema.json"
            },
            {
                "name": "SVG Spec Attributes",
                "description": "Per-snapshot and union attribute records. Top-level array — no inline $schema possible.",
                "fileMatch": [
                    "**/specs/*/attributes.json",
                    "**/derived/union/attributes.json"
                ],
                "url": "attributes.schema.json"
            },
            {
                "name": "SVG Spec Grammars",
                "fileMatch": ["**/specs/*/grammars.json"],
                "url": "grammars.schema.json"
            },
            {
                "name": "SVG Spec Categories",
                "fileMatch": ["**/specs/*/categories.json"],
                "url": "categories.schema.json"
            },
            {
                "name": "SVG Element–Attribute Matrix",
                "fileMatch": ["**/specs/*/element_attribute_matrix.json"],
                "url": "element_attribute_matrix.schema.json"
            },
            {
                "name": "SVG Spec Exceptions",
                "fileMatch": ["**/specs/*/exceptions.json"],
                "url": "exceptions.schema.json"
            },
            {
                "name": "SVG Spec Review",
                "fileMatch": ["**/specs/*/review.json"],
                "url": "review.schema.json"
            },
            {
                "name": "SVG Source Manifest",
                "description": "TOML source manifests already carry a #:schema comment; this entry covers JSON-tool consumers.",
                "fileMatch": ["**/sources/*.toml"],
                "url": "source-manifest.schema.json"
            }
        ]
    });

    let catalog_path = schemas_dir.join("catalog.json");
    let mut catalog_json = serde_json::to_string_pretty(&catalog)?;
    catalog_json.push('\n');
    fs::write(&catalog_path, &catalog_json)?;
    println!("{}", catalog_path.display());

    Ok(())
}
