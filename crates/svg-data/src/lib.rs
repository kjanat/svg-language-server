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
pub(crate) mod xlink;

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

/// Look up a single SVG element definition by tag name.
#[must_use]
pub fn element(name: &str) -> Option<&'static ElementDef> {
    ELEMENT_MAP.get(name).copied()
}

/// Look up a single SVG attribute definition by attribute name.
#[must_use]
pub fn attribute(name: &str) -> Option<&'static AttributeDef> {
    ATTRIBUTE_MAP.get(name).copied().or_else(|| {
        let canonical_name = xlink::canonical_svg_attribute_name(name);
        (canonical_name.as_ref() != name)
            .then(|| ATTRIBUTE_MAP.get(canonical_name.as_ref()).copied())
            .flatten()
    })
}

/// Return the full generated SVG element catalog.
#[must_use]
pub fn elements() -> &'static [ElementDef] {
    ELEMENTS
}

/// Return the full generated SVG attribute catalog.
#[must_use]
pub fn attributes() -> &'static [AttributeDef] {
    ATTRIBUTES
}

/// Return the concrete child element names allowed under `parent`.
#[must_use]
pub fn allowed_children(parent: &str) -> Vec<&'static str> {
    categories::allowed_children(parent)
}

/// Return whether `parent` accepts foreign-namespace children.
#[must_use]
pub fn allows_foreign_children(parent: &str) -> bool {
    element(parent).is_some_and(|el| matches!(el.content_model, ContentModel::Foreign))
}

fn attribute_applies_to(attr: &AttributeDef, element_name: &str) -> bool {
    // Empty elements list means global — current codegen uses "*" but older
    // build artifacts may still emit an empty list.
    attr.elements.is_empty()
        || attr.elements.contains(&"*")
        || attr.elements.contains(&element_name)
}

/// Return all attributes that apply to `element_name`, including global ones.
#[must_use]
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

/// Return all element names belonging to the given catalog category.
#[must_use]
pub const fn elements_in_category(cat: ElementCategory) -> &'static [&'static str] {
    categories::elements_in_category(cat)
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    const XLINK_ATTRIBUTE_NAMES: &[(&str, &str)] = &[
        ("xlink_actuate", "xlink:actuate"),
        ("xlink_arcrole", "xlink:arcrole"),
        ("xlink_href", "xlink:href"),
        ("xlink_role", "xlink:role"),
        ("xlink_show", "xlink:show"),
        ("xlink_title", "xlink:title"),
        ("xlink_type", "xlink:type"),
    ];
    const EMITTED_XLINK_ATTRIBUTE_NAMES: &[(&str, &str)] = &[
        ("xlink_actuate", "xlink:actuate"),
        ("xlink_href", "xlink:href"),
        ("xlink_show", "xlink:show"),
        ("xlink_title", "xlink:title"),
    ];

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
    fn xlink_alias_helper_canonicalizes_known_legacy_names() {
        for &(legacy_name, canonical_name) in XLINK_ATTRIBUTE_NAMES {
            assert_eq!(
                super::xlink::canonical_svg_attribute_name(legacy_name).as_ref(),
                canonical_name
            );
            assert_eq!(
                super::xlink::canonical_svg_attribute_name(canonical_name).as_ref(),
                canonical_name
            );
        }
    }

    #[test]
    fn xlink_attribute_lookup_is_canonical_and_backward_compatible() -> Result<(), Box<dyn Error>> {
        for &(legacy_name, canonical_name) in EMITTED_XLINK_ATTRIBUTE_NAMES {
            let canonical = attribute(canonical_name)
                .ok_or_else(|| format!("{canonical_name} should exist"))?;
            let legacy =
                attribute(legacy_name).ok_or_else(|| format!("{legacy_name} should alias"))?;
            assert!(std::ptr::eq(canonical, legacy));
            assert_eq!(canonical.name, canonical_name);
            assert!(
                canonical.deprecated,
                "{canonical_name} should be deprecated"
            );
        }

        let href = attribute("xlink:href").ok_or("xlink:href should exist")?;
        assert_eq!(
            href.mdn_url,
            "https://developer.mozilla.org/docs/Web/SVG/Attribute/xlink:href"
        );
        Ok(())
    }

    #[test]
    fn public_xlink_attribute_names_are_canonical() {
        let xlink_names: Vec<&str> = attributes()
            .iter()
            .filter(|attribute| attribute.name.starts_with("xlink"))
            .map(|attribute| attribute.name)
            .collect();

        assert!(
            !xlink_names.is_empty(),
            "catalog should include deprecated xlink attributes"
        );
        assert!(
            xlink_names.iter().all(|name| name.contains(':')),
            "public xlink names should use canonical colon syntax: {xlink_names:?}"
        );
    }

    #[test]
    fn attributes_for_use_only_exposes_canonical_xlink_names() {
        let attrs = attributes_for("use");
        let names: Vec<&str> = attrs.iter().map(|a| a.name).collect();
        assert!(names.contains(&"xlink:href"));
        assert!(!names.contains(&"xlink_href"));
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
