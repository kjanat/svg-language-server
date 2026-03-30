//! Generated SVG catalog and browser-compat lookup APIs.
//!
//! This crate exposes the baked element and attribute metadata consumed by the
//! language server, linter, formatter, and other workspace crates.

/// Browser-compat-data model types used by the generated SVG compatibility
/// catalog.
pub mod bcd;
mod catalog;
/// Category-based helpers for allowed-child and grouping queries.
pub mod categories;
/// Shared BCD JSON parsing helpers for runtime compat overlays.
pub mod compat_parse;
/// Public catalog type definitions.
pub mod types;

use std::{collections::HashMap, sync::LazyLock};

use catalog::{ATTRIBUTES, ELEMENTS};
pub use types::{
    AttributeDef, AttributeValues, BaselineStatus, BrowserSupport, ContentModel, ElementCategory,
    ElementDef,
};

static ELEMENT_MAP: LazyLock<HashMap<&'static str, &'static ElementDef>> =
    LazyLock::new(|| ELEMENTS.iter().map(|e| (e.name, e)).collect());

static ATTRIBUTE_MAP: LazyLock<HashMap<&'static str, &'static AttributeDef>> =
    LazyLock::new(|| ATTRIBUTES.iter().map(|a| (a.name, a)).collect());

#[must_use]
/// Look up a single SVG element definition by tag name.
pub fn element(name: &str) -> Option<&'static ElementDef> {
    ELEMENT_MAP.get(name).copied()
}

#[must_use]
/// Look up a single SVG attribute definition by attribute name.
pub fn attribute(name: &str) -> Option<&'static AttributeDef> {
    ATTRIBUTE_MAP.get(name).copied()
}

#[must_use]
/// Return the full generated SVG element catalog.
pub fn elements() -> &'static [ElementDef] {
    ELEMENTS
}

#[must_use]
/// Return the full generated SVG attribute catalog.
pub fn attributes() -> &'static [AttributeDef] {
    ATTRIBUTES
}

#[must_use]
/// Return the concrete child element names allowed under `parent`.
pub fn allowed_children(parent: &str) -> Vec<&'static str> {
    categories::allowed_children(parent)
}

#[must_use]
/// Return whether `parent` accepts foreign-namespace children.
pub fn allows_foreign_children(parent: &str) -> bool {
    element(parent).is_some_and(|el| matches!(el.content_model, ContentModel::Foreign))
}

fn attribute_applies_to(attr: &AttributeDef, element_name: &str) -> bool {
    // Older generated catalogs used an empty applicability list as the global marker.
    attr.elements.is_empty()
        || attr.elements.contains(&"*")
        || attr.elements.contains(&element_name)
}

#[must_use]
/// Return all attributes that apply to `element_name`, including global ones.
pub fn attributes_for(element_name: &str) -> Vec<&'static AttributeDef> {
    let Some(el) = element(element_name) else {
        return Vec::new();
    };
    let mut result: Vec<&'static AttributeDef> = Vec::new();
    for attr in ATTRIBUTES {
        if attribute_applies_to(attr, el.name) {
            result.push(attr);
        }
    }
    result
}

#[must_use]
/// Return all element names belonging to the given catalog category.
pub const fn elements_in_category(cat: ElementCategory) -> &'static [&'static str] {
    categories::elements_in_category(cat)
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn element_lookup() -> Result<(), Box<dyn Error>> {
        let rect = element("rect").ok_or("rect should exist")?;
        assert_eq!(rect.name, "rect");
        assert!(!rect.deprecated);
        assert!(matches!(rect.content_model, ContentModel::Void));
        Ok(())
    }

    #[test]
    fn element_not_found() {
        assert!(element("notanelement").is_none());
    }

    #[test]
    fn text_content_model() -> Result<(), Box<dyn Error>> {
        let text = element("text").ok_or("text should exist")?;
        assert!(matches!(text.content_model, ContentModel::Children(_)));
        Ok(())
    }

    #[test]
    fn foreign_object_content_model() -> Result<(), Box<dyn Error>> {
        let foreign_object = element("foreignObject").ok_or("foreignObject should exist")?;
        assert!(matches!(
            foreign_object.content_model,
            ContentModel::Foreign
        ));
        assert!(allows_foreign_children("foreignObject"));
        Ok(())
    }

    #[test]
    fn allowed_children_text() {
        let children = allowed_children("text");
        assert!(children.contains(&"tspan"), "text should allow tspan");
        assert!(!children.contains(&"rect"), "text should not allow rect");
    }

    #[test]
    fn allowed_children_void() {
        let children = allowed_children("rect");
        assert!(children.is_empty(), "void element should have no children");
    }

    #[test]
    fn attribute_lookup() -> Result<(), Box<dyn Error>> {
        let fill = attribute("fill").ok_or("fill should exist")?;
        assert!(matches!(fill.values, AttributeValues::Color));
        Ok(())
    }

    #[test]
    fn attribute_d_on_path() -> Result<(), Box<dyn Error>> {
        let d = attribute("d").ok_or("d should exist")?;
        assert!(d.elements.contains(&"path"));
        assert!(matches!(d.values, AttributeValues::PathData));
        Ok(())
    }

    #[test]
    fn attributes_for_rect() {
        let attrs = attributes_for("rect");
        let names: Vec<&str> = attrs.iter().map(|a| a.name).collect();
        assert!(names.contains(&"fill"), "rect should accept fill");
        assert!(names.contains(&"x"), "rect should accept x");
        assert!(!names.contains(&"d"), "rect should not accept d");
    }

    #[test]
    fn empty_elements_list_is_treated_as_global() {
        let attr = AttributeDef {
            name: "legacy-global",
            description: "",
            mdn_url: "",
            deprecated: false,
            experimental: false,
            spec_url: None,
            baseline: None,
            browser_support: None,
            values: AttributeValues::FreeText,
            elements: &[],
        };

        assert!(attribute_applies_to(&attr, "rect"));
        assert!(attribute_applies_to(&attr, "svg"));
    }

    #[test]
    fn elements_in_shape_category() {
        let shapes = elements_in_category(ElementCategory::Shape);
        assert!(shapes.contains(&"rect"));
        assert!(shapes.contains(&"circle"));
        assert!(shapes.contains(&"path"));
        assert!(!shapes.contains(&"g"));
    }

    #[test]
    fn all_elements_have_mdn_url() {
        for el in elements() {
            assert!(
                el.mdn_url.starts_with("https://developer.mozilla.org/"),
                "element {} missing MDN URL",
                el.name
            );
        }
    }
}
