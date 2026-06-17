//! JSON Schema emitted with the generated catalog.

use schemars::schema_for;
use serde_json::{Value, json};

use crate::catalog::Catalog;

/// Version of the `catalog.json` data contract.
pub const CATALOG_SCHEMA_VERSION: u16 = 1;

/// File name for the generated JSON Schema.
pub const CATALOG_SCHEMA_FILE: &str = "catalog.schema.json";

const CATALOG_SCHEMA_ID: &str = "https://github.com/kjanat/svg-language-server/raw/HEAD/crates/svg-data/data/catalog.schema.json";
const CATALOG_SCHEMA_TITLE: &str = "svg-data catalog v1";

/// Render the JSON Schema for the current catalog contract.
///
/// # Errors
/// Returns an error only if serializing the generated schema value fails.
pub fn catalog_schema_json() -> Result<String, serde_json::Error> {
    let mut schema = serde_json::to_value(schema_for!(Catalog))?;
    apply_catalog_metadata(&mut schema);
    json_schema_sort::sort_schema(&mut schema);
    let mut text = serde_json::to_string_pretty(&schema)?;
    text.push('\n');
    Ok(text)
}

/// Add catalog-specific metadata that cannot be inferred from the Rust type.
fn apply_catalog_metadata(schema: &mut Value) {
    if let Some(object) = schema.as_object_mut() {
        object.insert(
            "$id".to_owned(),
            Value::String(CATALOG_SCHEMA_ID.to_owned()),
        );
        object.insert(
            "title".to_owned(),
            Value::String(CATALOG_SCHEMA_TITLE.to_owned()),
        );
    }
    if let Some(schema_version) = schema.pointer_mut("/properties/schema_version") {
        *schema_version = json!({ "const": CATALOG_SCHEMA_VERSION });
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
                "attributes",
                "commit",
                "compat",
                "elements",
                "schema_version"
            ]
        );
        Ok(())
    }
}
