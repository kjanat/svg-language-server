use crate::catalog::{ELEMENTS, generated_elements_in_category};
use crate::types::{ContentModel, ElementCategory};

/// Return all element names belonging to the given category.
pub fn elements_in_category(cat: ElementCategory) -> &'static [&'static str] {
    generated_elements_in_category(cat)
}

/// Concrete element names allowed as children of `parent`.
pub fn allowed_children(parent: &str) -> Vec<&'static str> {
    let Some(el) = ELEMENTS.iter().find(|e| e.name == parent) else {
        return Vec::new();
    };
    match &el.content_model {
        ContentModel::Children(cats) => {
            let mut names: Vec<&'static str> = cats
                .iter()
                .flat_map(|cat| elements_in_category(*cat).iter().copied())
                .collect();
            names.sort_unstable();
            names.dedup();
            names
        }
        ContentModel::Foreign | ContentModel::Void | ContentModel::Text => Vec::new(),
    }
}
