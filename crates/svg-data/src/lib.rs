pub mod bcd;
mod catalog;
pub mod categories;
pub mod types;

pub use types::{
    AttributeDef, AttributeValues, BaselineStatus, ContentModel, ElementCategory, ElementDef,
};

use catalog::{ATTRIBUTES, ELEMENTS};

pub fn element(name: &str) -> Option<&'static ElementDef> {
    ELEMENTS.iter().find(|e| e.name == name)
}

pub fn attribute(name: &str) -> Option<&'static AttributeDef> {
    ATTRIBUTES.iter().find(|a| a.name == name)
}

pub fn elements() -> &'static [ElementDef] {
    ELEMENTS
}

pub fn attributes() -> &'static [AttributeDef] {
    ATTRIBUTES
}

pub fn allowed_children(parent: &str) -> Vec<&'static str> {
    categories::allowed_children(parent)
}

pub fn allows_foreign_children(parent: &str) -> bool {
    element(parent)
        .map(|el| matches!(el.content_model, ContentModel::Foreign))
        .unwrap_or(false)
}

fn attribute_applies_to(attr: &AttributeDef, element_name: &str) -> bool {
    // Older generated catalogs used an empty applicability list as the global marker.
    attr.elements.is_empty()
        || attr.elements.contains(&"*")
        || attr.elements.contains(&element_name)
}

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

pub fn elements_in_category(cat: ElementCategory) -> Vec<&'static str> {
    categories::elements_in_category(cat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_lookup() {
        let rect = element("rect").expect("rect should exist");
        assert_eq!(rect.name, "rect");
        assert!(!rect.deprecated);
        assert!(matches!(rect.content_model, ContentModel::Void));
    }

    #[test]
    fn element_not_found() {
        assert!(element("notanelement").is_none());
    }

    #[test]
    fn text_content_model() {
        let text = element("text").expect("text should exist");
        assert!(matches!(text.content_model, ContentModel::Children(_)));
    }

    #[test]
    fn foreign_object_content_model() {
        let foreign_object = element("foreignObject").expect("foreignObject should exist");
        assert!(matches!(
            foreign_object.content_model,
            ContentModel::Foreign
        ));
        assert!(allows_foreign_children("foreignObject"));
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
    fn attribute_lookup() {
        let fill = attribute("fill").expect("fill should exist");
        assert!(matches!(fill.values, AttributeValues::Color));
    }

    #[test]
    fn attribute_d_on_path() {
        let d = attribute("d").expect("d should exist");
        assert!(d.elements.contains(&"path"));
        assert!(matches!(d.values, AttributeValues::PathData));
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
