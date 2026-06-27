//! JSON Schema emitted with the generated catalog.

use schemars::{JsonSchema, schema_for};
use serde_json::{Value, json};

use crate::catalog::{
    CatalogCompatDocument, CatalogCore, CatalogGraphDocument, CatalogManifest, CatalogSnapshot,
    CatalogTreeSitterDocument,
};

/// Version of the `catalog.json` data contract.
pub const CATALOG_SCHEMA_VERSION: u16 = 1;

/// File name for the generated JSON Schema.
pub const CATALOG_SCHEMA_FILE: &str = "catalog.schema.json";
/// File name for the generated core catalog JSON Schema.
pub const CATALOG_CORE_SCHEMA_FILE: &str = "catalog.core.schema.json";
/// File name for the generated compat catalog JSON Schema.
pub const CATALOG_COMPAT_SCHEMA_FILE: &str = "catalog.compat.schema.json";
/// File name for the generated graph catalog JSON Schema.
pub const CATALOG_GRAPH_SCHEMA_FILE: &str = "catalog.graph.schema.json";
/// File name for the generated tree-sitter projection JSON Schema.
pub const CATALOG_TREE_SITTER_SCHEMA_FILE: &str = "catalog.tree-sitter.schema.json";
/// File name for the generated snapshot overlay JSON Schema.
pub const CATALOG_SNAPSHOT_SCHEMA_FILE: &str = "catalog.snapshot.schema.json";

const CATALOG_SCHEMA_ID: &str = concat!(
    env!("CARGO_PKG_REPOSITORY"),
    "/raw/HEAD/crates/svg-data/data/catalog.schema.json"
);
const CATALOG_SCHEMA_TITLE: &str = "svg-data catalog v1";
const CATALOG_CORE_SCHEMA_TITLE: &str = "svg-data core catalog v1";
const CATALOG_COMPAT_SCHEMA_TITLE: &str = "svg-data compat catalog v1";
const CATALOG_GRAPH_SCHEMA_TITLE: &str = "svg-data graph catalog v1";
const CATALOG_TREE_SITTER_SCHEMA_TITLE: &str = "svg-data tree-sitter catalog v1";
const CATALOG_SNAPSHOT_SCHEMA_TITLE: &str = "svg-data snapshot overlay v1";

/// One generated schema document to write under `svg-data/data`.
pub struct CatalogSchemaDocument {
    /// Output file name.
    pub file_name: &'static str,
    /// Pretty JSON schema text.
    pub json: String,
}

/// Render the JSON Schema for the current catalog contract.
///
/// # Errors
/// Returns an error only if serializing the generated schema value fails.
pub fn catalog_schema_json() -> Result<String, serde_json::Error> {
    let mut schema = schema_value::<CatalogManifest>(CATALOG_SCHEMA_TITLE)?;
    merge_document_schema::<CatalogCore<'static>>(&mut schema, "CatalogCore")?;
    merge_document_schema::<CatalogCompatDocument<'static>>(&mut schema, "CatalogCompatDocument")?;
    merge_document_schema::<CatalogGraphDocument<'static>>(&mut schema, "CatalogGraphDocument")?;
    merge_document_schema::<CatalogTreeSitterDocument>(&mut schema, "CatalogTreeSitterDocument")?;
    merge_document_schema::<CatalogSnapshot>(&mut schema, "CatalogSnapshot")?;
    apply_catalog_schema_id(&mut schema);
    schema_text(schema)
}

/// Render every first-class schema document for the split catalog artifacts.
///
/// # Errors
/// Returns an error only if serializing a generated schema value fails.
pub fn catalog_schema_documents() -> Result<Vec<CatalogSchemaDocument>, serde_json::Error> {
    Ok(vec![
        CatalogSchemaDocument {
            file_name: CATALOG_SCHEMA_FILE,
            json: catalog_schema_json()?,
        },
        CatalogSchemaDocument {
            file_name: CATALOG_CORE_SCHEMA_FILE,
            json: schema_json::<CatalogCore<'static>>(CATALOG_CORE_SCHEMA_TITLE)?,
        },
        CatalogSchemaDocument {
            file_name: CATALOG_COMPAT_SCHEMA_FILE,
            json: schema_json::<CatalogCompatDocument<'static>>(CATALOG_COMPAT_SCHEMA_TITLE)?,
        },
        CatalogSchemaDocument {
            file_name: CATALOG_GRAPH_SCHEMA_FILE,
            json: schema_json::<CatalogGraphDocument<'static>>(CATALOG_GRAPH_SCHEMA_TITLE)?,
        },
        CatalogSchemaDocument {
            file_name: CATALOG_TREE_SITTER_SCHEMA_FILE,
            json: schema_json::<CatalogTreeSitterDocument>(CATALOG_TREE_SITTER_SCHEMA_TITLE)?,
        },
        CatalogSchemaDocument {
            file_name: CATALOG_SNAPSHOT_SCHEMA_FILE,
            json: schema_json::<CatalogSnapshot>(CATALOG_SNAPSHOT_SCHEMA_TITLE)?,
        },
    ])
}

fn schema_json<T: JsonSchema>(title: &str) -> Result<String, serde_json::Error> {
    schema_text(schema_value::<T>(title)?)
}

fn schema_value<T: JsonSchema>(title: &str) -> Result<Value, serde_json::Error> {
    let mut schema = serde_json::to_value(schema_for!(T))?;
    apply_catalog_metadata(&mut schema, title);
    Ok(schema)
}

fn schema_text(mut schema: Value) -> Result<String, serde_json::Error> {
    json_schema_sort::sort_schema(&mut schema);
    let mut text = serde_json::to_string_pretty(&schema)?;
    text.push('\n');
    Ok(text)
}

fn merge_document_schema<T: JsonSchema>(
    schema: &mut Value,
    name: &str,
) -> Result<(), serde_json::Error> {
    let mut document = serde_json::to_value(schema_for!(T))?;
    let Some(document_object) = document.as_object_mut() else {
        return Ok(());
    };
    let document_defs = document_object.remove("$defs");
    document_object.remove("$schema");

    let Some(schema_object) = schema.as_object_mut() else {
        return Ok(());
    };
    let defs_value = schema_object
        .entry("$defs")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let Some(defs) = defs_value.as_object_mut() else {
        return Ok(());
    };

    if let Some(Value::Object(document_defs)) = document_defs {
        for (key, value) in document_defs {
            defs.entry(key).or_insert(value);
        }
    }
    defs.insert(name.to_owned(), document);
    Ok(())
}

/// Add catalog-specific metadata that cannot be inferred from the Rust type.
fn apply_catalog_metadata(schema: &mut Value, title: &str) {
    if let Some(object) = schema.as_object_mut() {
        object.insert("title".to_owned(), Value::String(title.to_owned()));
    }
    if let Some(schema_version) = schema.pointer_mut("/properties/schema_version") {
        *schema_version = json!({ "const": CATALOG_SCHEMA_VERSION });
    }
}

fn apply_catalog_schema_id(schema: &mut Value) {
    if let Some(object) = schema.as_object_mut() {
        object.insert(
            "$id".to_owned(),
            Value::String(CATALOG_SCHEMA_ID.to_owned()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_valid_json_and_versioned() -> Result<(), Box<dyn std::error::Error>> {
        let schema: serde_json::Value = serde_json::from_str(&catalog_schema_json()?)?;
        assert_eq!(
            schema.pointer("/properties/schema_version/const"),
            Some(&serde_json::Value::from(CATALOG_SCHEMA_VERSION))
        );
        assert_eq!(
            schema.get("$id").and_then(serde_json::Value::as_str),
            Some(CATALOG_SCHEMA_ID)
        );
        let root_keys: Vec<&str> = schema
            .as_object()
            .ok_or("schema root is not an object")?
            .keys()
            .map(String::as_str)
            .collect();
        assert!(root_keys.starts_with(&["$id", "$schema", "$defs"]));
        let property_keys: Vec<&str> = schema
            .pointer("/properties")
            .and_then(serde_json::Value::as_object)
            .ok_or("schema properties is not an object")?
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            property_keys,
            [
                "commit",
                "compat",
                "core",
                "graph",
                "schema_version",
                "snapshots",
                "tree_sitter",
            ]
        );
        assert!(
            schema
                .pointer("/$defs/CatalogSnapshot/properties/lifecycle")
                .is_some(),
            "snapshot overlay schema must include lifecycle"
        );
        assert!(
            serde_json::to_string(
                schema
                    .pointer("/$defs/CatalogLifecycleStatus")
                    .ok_or("missing lifecycle status schema")?
            )?
            .contains("not_yet_introduced"),
            "lifecycle status schema must include not-yet-introduced"
        );
        Ok(())
    }

    #[test]
    fn split_artifact_schemas_are_valid_json_and_versioned()
    -> Result<(), Box<dyn std::error::Error>> {
        let documents = catalog_schema_documents()?;
        let file_names: Vec<&str> = documents
            .iter()
            .map(|document| document.file_name)
            .collect();
        assert_eq!(
            file_names,
            [
                CATALOG_SCHEMA_FILE,
                CATALOG_CORE_SCHEMA_FILE,
                CATALOG_COMPAT_SCHEMA_FILE,
                CATALOG_GRAPH_SCHEMA_FILE,
                CATALOG_TREE_SITTER_SCHEMA_FILE,
                CATALOG_SNAPSHOT_SCHEMA_FILE,
            ]
        );

        let expected = [
            (CATALOG_SCHEMA_FILE, ["commit", "core", "graph"].as_slice()),
            (
                CATALOG_CORE_SCHEMA_FILE,
                ["attributes", "elements"].as_slice(),
            ),
            (
                CATALOG_COMPAT_SCHEMA_FILE,
                ["browser_compat_data", "web_features"].as_slice(),
            ),
            (CATALOG_GRAPH_SCHEMA_FILE, ["edges", "nodes"].as_slice()),
            (
                CATALOG_TREE_SITTER_SCHEMA_FILE,
                ["attribute_buckets", "tokens"].as_slice(),
            ),
            (
                CATALOG_SNAPSHOT_SCHEMA_FILE,
                ["inventory", "profile"].as_slice(),
            ),
        ];

        for (file_name, required_keys) in expected {
            let Some(document) = documents
                .iter()
                .find(|document| document.file_name == file_name)
            else {
                panic!("schema document {file_name} must be generated");
            };
            let schema: serde_json::Value = serde_json::from_str(&document.json)?;
            assert_eq!(
                schema.pointer("/properties/schema_version/const"),
                Some(&serde_json::Value::from(CATALOG_SCHEMA_VERSION)),
                "{file_name} pins schema_version"
            );
            let required = schema
                .pointer("/required")
                .and_then(serde_json::Value::as_array)
                .ok_or("schema required list")?;
            for key in required_keys {
                assert!(
                    required.iter().any(|value| value.as_str() == Some(*key)),
                    "{file_name} root schema includes required key {key}"
                );
            }
        }

        Ok(())
    }
}
