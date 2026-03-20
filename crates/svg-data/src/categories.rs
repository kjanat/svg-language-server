use crate::catalog::ELEMENTS;
use crate::types::{ContentModel, ElementCategory};

/// Return all element names belonging to the given category.
pub fn elements_in_category(cat: ElementCategory) -> Vec<&'static str> {
    match cat {
        ElementCategory::Container => vec![
            "svg", "g", "defs", "symbol", "marker", "clipPath", "mask", "pattern", "a",
        ],
        ElementCategory::Shape => vec![
            "rect", "circle", "ellipse", "line", "polyline", "polygon", "path",
        ],
        ElementCategory::Text => vec!["text", "tspan", "textPath"],
        ElementCategory::Gradient => vec!["linearGradient", "radialGradient", "stop"],
        ElementCategory::Filter => vec!["filter"],
        ElementCategory::Descriptive => vec!["title", "desc", "metadata"],
        ElementCategory::Structural => vec!["use", "image", "foreignObject", "switch"],
        ElementCategory::Animation => {
            vec!["animate", "animateMotion", "animateTransform", "set"]
        }
        ElementCategory::PaintServer => vec!["linearGradient", "radialGradient", "pattern"],
        ElementCategory::ClipMask => vec!["clipPath", "mask"],
        ElementCategory::LightSource => vec!["feDistantLight", "fePointLight", "feSpotLight"],
        ElementCategory::FilterPrimitive => vec![
            "feBlend",
            "feColorMatrix",
            "feComponentTransfer",
            "feComposite",
            "feConvolveMatrix",
            "feDiffuseLighting",
            "feDisplacementMap",
            "feFlood",
            "feGaussianBlur",
            "feImage",
            "feMerge",
            "feMorphology",
            "feOffset",
            "feSpecularLighting",
            "feTile",
            "feTurbulence",
        ],
        ElementCategory::NeverRendered => vec!["style", "script"],
    }
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
                .flat_map(|cat| elements_in_category(*cat))
                .collect();
            names.sort_unstable();
            names.dedup();
            names
        }
        ContentModel::Foreign | ContentModel::Void | ContentModel::Text => Vec::new(),
    }
}
