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
    derived::{AttributeMembershipFile, ElementMembershipFile, SnapshotOverlayFile},
    extraction::SourceManifest,
    snapshot_schema::{
        CategoriesFile, ElementAttributeMatrixFile, ExceptionsFile, GrammarFile, ReviewFile,
        SnapshotAttributeRecord, SnapshotElementRecord, SnapshotMetadataFile,
    },
};

/// Published base URL for the schema files. `fileMatch` consumers that honor
/// the catalog (`SchemaStore`-aware tools, `check-jsonschema --catalog`) fetch
/// each schema from this location. The `SchemaStore` catalog meta-schema
/// requires `format: uri` for `url`, so relative paths are not valid here.
/// Local-development LSP uses the relative paths in `.zed/settings.json`.
const RAW_SCHEMA_BASE_URL: &str = "https://raw.githubusercontent.com/kjanat/svg-language-server/master/crates/svg-data/data/schemas";

fn schema_url(file_name: &str) -> String {
    format!("{RAW_SCHEMA_BASE_URL}/{file_name}")
}

fn main() -> Result<(), Box<dyn Error>> {
    let schemas_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/schemas");
    fs::create_dir_all(&schemas_dir)?;

    let files: &[(&str, Schema)] = &[
        ("snapshot", schema_for!(SnapshotMetadataFile)),
        // Per-snapshot elements.json and attributes.json are top-level JSON
        // arrays, so they cannot carry an inline "$schema" key. The catalog
        // covers them instead.
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
        // Derived union artifacts are object wrappers, not bare arrays —
        // they need their own schemas distinct from the per-snapshot ones.
        ("union-elements", schema_for!(ElementMembershipFile)),
        ("union-attributes", schema_for!(AttributeMembershipFile)),
        ("overlay", schema_for!(SnapshotOverlayFile)),
    ];

    for (name, schema) in files {
        let path = schemas_dir.join(format!("{name}.schema.json"));
        let mut json = serde_json::to_string_pretty(schema)?;
        json.push('\n');
        fs::write(&path, &json)?;
        println!("{}", path.display());
    }

    // Catalog: maps file-glob patterns → schema URLs. Entries follow the
    // `SchemaStore` catalog format — `fileMatch` globs are matched against
    // workspace-root-relative paths, and `url` MUST be an absolute URI.
    // Tools: VS Code JSON language server, check-jsonschema --catalog, etc.
    // Local-development LSP uses relative paths in `.zed/settings.json` so
    // local schema edits take effect without going through the remote.
    let catalog = json!({
        "$schema": "https://json.schemastore.org/schema-catalog.json",
        "version": 1,
        "schemas": [
            {
                "name": "SVG Spec Snapshot Metadata",
                "description": "Per-snapshot metadata: id, date, status, pinned sources, ingestion info.",
                "fileMatch": ["**/specs/*/snapshot.json"],
                "url": schema_url("snapshot.schema.json")
            },
            {
                "name": "SVG Spec Elements",
                "description": "Per-snapshot element records. Top-level array — no inline $schema possible.",
                "fileMatch": ["**/specs/*/elements.json"],
                "url": schema_url("elements.schema.json")
            },
            {
                "name": "SVG Spec Attributes",
                "description": "Per-snapshot attribute records. Top-level array — no inline $schema possible.",
                "fileMatch": ["**/specs/*/attributes.json"],
                "url": schema_url("attributes.schema.json")
            },
            {
                "name": "SVG Union Element Membership",
                "description": "Derived union element membership across all canonical snapshots.",
                "fileMatch": ["**/derived/union/elements.json"],
                "url": schema_url("union-elements.schema.json")
            },
            {
                "name": "SVG Union Attribute Membership",
                "description": "Derived union attribute membership across all canonical snapshots.",
                "fileMatch": ["**/derived/union/attributes.json"],
                "url": schema_url("union-attributes.schema.json")
            },
            {
                "name": "SVG Snapshot Overlay",
                "description": "Adjacent snapshot diff for element and attribute membership.",
                "fileMatch": ["**/derived/overlays/*.json"],
                "url": schema_url("overlay.schema.json")
            },
            {
                "name": "SVG Spec Grammars",
                "description": "Attribute value grammar definitions keyed by grammar id.",
                "fileMatch": ["**/specs/*/grammars.json"],
                "url": schema_url("grammars.schema.json")
            },
            {
                "name": "SVG Spec Categories",
                "description": "Element category memberships used for content-model grouping.",
                "fileMatch": ["**/specs/*/categories.json"],
                "url": schema_url("categories.schema.json")
            },
            {
                "name": "SVG Element–Attribute Matrix",
                "description": "Per-element attribute applicability matrix for the snapshot.",
                "fileMatch": ["**/specs/*/element_attribute_matrix.json"],
                "url": schema_url("element_attribute_matrix.schema.json")
            },
            {
                "name": "SVG Spec Exceptions",
                "description": "Hand-curated exceptions and overrides applied on top of extracted data.",
                "fileMatch": ["**/specs/*/exceptions.json"],
                "url": schema_url("exceptions.schema.json")
            },
            {
                "name": "SVG Spec Review",
                "description": "Extraction review notes and confidence annotations per fact.",
                "fileMatch": ["**/specs/*/review.json"],
                "url": schema_url("review.schema.json")
            },
            {
                "name": "SVG Source Manifest",
                "description": "TOML source manifests already carry a #:schema comment; this entry covers JSON-tool consumers.",
                "fileMatch": ["**/sources/*.toml"],
                "url": schema_url("source-manifest.schema.json")
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
