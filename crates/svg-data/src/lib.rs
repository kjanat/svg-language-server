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

pub fn attributes_for(element_name: &str) -> Vec<&'static AttributeDef> {
    let Some(el) = element(element_name) else {
        return Vec::new();
    };
    let mut result: Vec<&'static AttributeDef> = Vec::new();
    for attr in ATTRIBUTES {
        let applies = attr.elements.contains(&"*") || attr.elements.contains(&el.name);
        if applies {
            result.push(attr);
        }
    }
    result
}

pub fn elements_in_category(cat: ElementCategory) -> Vec<&'static str> {
    categories::elements_in_category(cat)
}
