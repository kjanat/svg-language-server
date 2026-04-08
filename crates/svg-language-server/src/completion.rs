use std::collections::HashSet;

use svg_data::{AttributeValues, ContentModel};
use svg_tree::{find_ancestor_any, is_attribute_name_kind};
use tower_lsp_server::ls_types::{
    CompletionItem, CompletionItemKind, CompletionItemTag, CompletionTextEdit, InsertTextFormat,
    Range, TextEdit,
};

use crate::positions::position_for_byte_offset;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CssCompletionContext {
    Selector,
    Property,
    Value,
}

const CSS_PROPERTY_NAMES: &[&str] = &[
    "alignment-baseline",
    "clip-path",
    "clip-rule",
    "color",
    "color-interpolation",
    "color-rendering",
    "cursor",
    "display",
    "dominant-baseline",
    "fill",
    "fill-opacity",
    "fill-rule",
    "filter",
    "flood-color",
    "flood-opacity",
    "font-family",
    "font-size",
    "font-style",
    "font-weight",
    "image-rendering",
    "lighting-color",
    "marker-end",
    "marker-mid",
    "marker-start",
    "mask",
    "mix-blend-mode",
    "opacity",
    "overflow",
    "paint-order",
    "pointer-events",
    "shape-rendering",
    "stop-color",
    "stop-opacity",
    "stroke",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-opacity",
    "stroke-width",
    "text-anchor",
    "text-decoration-color",
    "transform",
    "transform-box",
    "transform-origin",
    "vector-effect",
    "visibility",
];

const COMPLETION_TRIGGER_CHARACTERS: &[&str] = &["<", " ", "\"", "'", ":", "-"];

pub fn completion_trigger_characters() -> Vec<String> {
    COMPLETION_TRIGGER_CHARACTERS
        .iter()
        .map(|value| (*value).to_owned())
        .collect()
}

pub fn style_completion_items(
    source: &[u8],
    tree: &tree_sitter::Tree,
    byte_offset: usize,
) -> Option<Vec<CompletionItem>> {
    let stylesheet = svg_references::collect_inline_stylesheets(source, tree)
        .into_iter()
        .find(|stylesheet| {
            let end = stylesheet.start_byte + stylesheet.css.len();
            (stylesheet.start_byte..=end).contains(&byte_offset)
        })?;

    let css_offset = byte_offset.saturating_sub(stylesheet.start_byte);
    Some(css_completion_items(&stylesheet.css, css_offset))
}

fn completion_item(label: impl Into<String>, kind: CompletionItemKind) -> CompletionItem {
    CompletionItem {
        label: label.into(),
        kind: Some(kind),
        ..Default::default()
    }
}

fn detailed_completion_item(
    label: impl Into<String>,
    kind: CompletionItemKind,
    detail: impl Into<String>,
) -> CompletionItem {
    CompletionItem {
        detail: Some(detail.into()),
        ..completion_item(label, kind)
    }
}

fn snippet_completion_item(
    label: impl Into<String>,
    kind: CompletionItemKind,
    snippet: impl Into<String>,
) -> CompletionItem {
    CompletionItem {
        insert_text: Some(snippet.into()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..completion_item(label, kind)
    }
}

fn detailed_snippet_completion_item(
    label: impl Into<String>,
    kind: CompletionItemKind,
    snippet: impl Into<String>,
    detail: impl Into<String>,
) -> CompletionItem {
    CompletionItem {
        detail: Some(detail.into()),
        ..snippet_completion_item(label, kind, snippet)
    }
}

fn replace_completion_item(
    label: impl Into<String>,
    kind: CompletionItemKind,
    detail: impl Into<String>,
    range: Range,
    new_text: impl Into<String>,
) -> CompletionItem {
    CompletionItem {
        detail: Some(detail.into()),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
            range,
            new_text.into(),
        ))),
        ..completion_item(label, kind)
    }
}

fn css_completion_items(css: &str, byte_offset: usize) -> Vec<CompletionItem> {
    let context = css_completion_context(css, byte_offset);
    let custom_properties =
        svg_references::collect_custom_property_definitions_from_stylesheet(css, 0, 0);

    match context {
        CssCompletionContext::Selector => css_selector_completions(),
        CssCompletionContext::Property => css_property_completions(),
        CssCompletionContext::Value => css_value_completions(&custom_properties),
    }
}

fn css_completion_context(css: &str, byte_offset: usize) -> CssCompletionContext {
    let offset = byte_offset.min(css.len());
    let before = &css[..offset];

    let last_open = before.rfind('{');
    let last_close = before.rfind('}');
    let in_block = match (last_open, last_close) {
        (Some(open), Some(close)) => open > close,
        (Some(_), None) => true,
        _ => false,
    };

    if !in_block {
        return CssCompletionContext::Selector;
    }

    let block_start = last_open.map_or(0, |idx| idx + 1);
    let block_prefix = &before[block_start..];
    let declaration_start = block_prefix
        .rfind(';')
        .map_or(block_start, |idx| block_start + idx + 1);
    let declaration_prefix = &before[declaration_start..];

    if declaration_prefix.contains(':') {
        CssCompletionContext::Value
    } else {
        CssCompletionContext::Property
    }
}

fn css_selector_completions() -> Vec<CompletionItem> {
    let mut items = vec![
        detailed_completion_item(":root", CompletionItemKind::KEYWORD, "CSS root selector"),
        detailed_snippet_completion_item(
            ".",
            CompletionItemKind::REFERENCE,
            ".$0",
            "Class selector",
        ),
        detailed_snippet_completion_item("#", CompletionItemKind::REFERENCE, "#$0", "ID selector"),
    ];

    items.extend(svg_data::elements().iter().map(|element| {
        detailed_completion_item(
            element.name,
            CompletionItemKind::CLASS,
            "SVG element selector",
        )
    }));

    items
}

fn css_property_completions() -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = CSS_PROPERTY_NAMES
        .iter()
        .map(|property| {
            snippet_completion_item(
                *property,
                CompletionItemKind::PROPERTY,
                format!("{property}: $0;"),
            )
        })
        .collect();

    items.push(detailed_snippet_completion_item(
        "--custom-property",
        CompletionItemKind::VARIABLE,
        "--$1: $0;",
        "CSS custom property",
    ));

    items
}

fn css_value_completions(custom_properties: &[svg_references::NamedSpan]) -> Vec<CompletionItem> {
    let mut items = vec![
        css_value_keyword("none"),
        css_value_keyword("currentColor"),
        css_value_keyword("transparent"),
        css_value_keyword("inherit"),
        css_value_function("var()", "var(--$0)", "CSS custom property reference"),
        css_value_function("url()", "url(#$0)", "SVG fragment reference"),
        css_value_function("rgb()", "rgb($0)", "RGB color"),
        css_value_function("hsl()", "hsl($0)", "HSL color"),
        css_value_function("hwb()", "hwb($0)", "HWB color"),
        css_value_function("lab()", "lab($0)", "Lab color"),
        css_value_function("lch()", "lch($0)", "LCH color"),
        css_value_function("oklab()", "oklab($0)", "Oklab color"),
        css_value_function("oklch()", "oklch($0)", "Oklch color"),
        css_value_function(
            "color-mix()",
            "color-mix(in oklch, $1, $2)",
            "Mixed color expression",
        ),
    ];

    let mut seen = std::collections::HashSet::new();
    for property in custom_properties {
        if !seen.insert(property.name.clone()) {
            continue;
        }
        let reference = format!("var({})", property.name);
        items.push(detailed_completion_item(
            reference.clone(),
            CompletionItemKind::VARIABLE,
            "CSS custom property",
        ));
    }

    items
}

fn css_value_keyword(keyword: &str) -> CompletionItem {
    completion_item(keyword, CompletionItemKind::VALUE)
}

fn css_value_function(label: &str, snippet: &str, detail: &str) -> CompletionItem {
    detailed_snippet_completion_item(label, CompletionItemKind::FUNCTION, snippet, detail)
}

pub fn is_comment_like_context(node: tree_sitter::Node<'_>) -> bool {
    find_ancestor_any(
        node,
        &[
            "comment",
            "cdata_section",
            "doctype",
            "processing_instruction",
            "xml_declaration",
        ],
    )
    .is_some()
}

pub fn is_embedded_non_svg_context(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let text_like = find_ancestor_any(
        node,
        &[
            "text",
            "raw_text",
            "style_text_double",
            "style_text_single",
            "script_text_double",
            "script_text_single",
        ],
    );
    let Some(text_like) = text_like else {
        return false;
    };

    matches!(
        enclosing_element_name(text_like, source),
        Some("style" | "script" | "foreignObject")
    )
}

fn collect_existing_attribute_names(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    names: &mut HashSet<String>,
) {
    let kind = node.kind();
    if is_attribute_name_kind(kind) {
        if let Ok(name) = node.utf8_text(source) {
            names.insert(name.to_string());
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_existing_attribute_names(child, source, names);
    }
}

pub fn existing_attribute_names(tag_node: tree_sitter::Node<'_>, source: &[u8]) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_existing_attribute_names(tag_node, source, &mut names);
    names
}

pub fn first_attribute_name_text(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let kind = node.kind();
    if is_attribute_name_kind(kind) {
        return node.utf8_text(source).ok().map(str::to_string);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(name) = first_attribute_name_text(child, source) {
            return Some(name);
        }
    }

    None
}

pub fn tag_element_name<'a>(tag_node: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let name_node = tag_node.child_by_field_name("name")?;
    name_node.utf8_text(source).ok()
}

pub fn enclosing_element_name<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> Option<&'a str> {
    let elem = find_ancestor_any(node, &["element", "svg_root_element"])?;
    let mut cursor = elem.walk();
    for child in elem.children(&mut cursor) {
        let kind = child.kind();
        if kind == "start_tag" || kind == "self_closing_tag" {
            return tag_element_name(child, source);
        }
    }
    None
}

fn attribute_value_inner_range(source: &[u8], value_node: tree_sitter::Node<'_>) -> Range {
    let text = value_node.utf8_text(source).unwrap_or_default();
    let quoted = text.len() >= 2
        && matches!(text.as_bytes().first().copied(), Some(b'"' | b'\''))
        && text.as_bytes().first() == text.as_bytes().last();

    let (start_byte, end_byte) = if quoted {
        (
            value_node.start_byte() + 1,
            value_node.end_byte().saturating_sub(1),
        )
    } else {
        (value_node.start_byte(), value_node.end_byte())
    };

    Range::new(
        position_for_byte_offset(source, start_byte),
        position_for_byte_offset(source, end_byte),
    )
}

fn href_value_completions(
    source: &[u8],
    tree: &tree_sitter::Tree,
    value_node: tree_sitter::Node<'_>,
) -> Vec<CompletionItem> {
    let replace_range = attribute_value_inner_range(source, value_node);
    let mut ids: Vec<String> = svg_references::collect_id_definitions(source, tree)
        .into_iter()
        .map(|definition| format!("#{}", definition.name))
        .collect();
    ids.sort();
    ids.dedup();

    ids.into_iter()
        .map(|fragment| {
            replace_completion_item(
                fragment.clone(),
                CompletionItemKind::REFERENCE,
                "In-document fragment reference",
                replace_range,
                fragment,
            )
        })
        .collect()
}

pub fn attribute_completion_items(
    elem_name: &str,
    existing: &HashSet<String>,
    compat: Option<&crate::compat::RuntimeCompat>,
) -> Vec<CompletionItem> {
    svg_data::attributes_for(elem_name)
        .into_iter()
        .filter(|attr| {
            let deprecated = compat
                .and_then(|c| c.attributes.get(attr.name))
                .map_or(attr.deprecated, |co| co.deprecated);
            !deprecated
        })
        .filter(|attr| !existing.contains(attr.name))
        .map(attribute_completion_item)
        .collect()
}

pub fn child_element_completion_items(
    elem_name: &str,
    compat: Option<&crate::compat::RuntimeCompat>,
) -> Vec<CompletionItem> {
    svg_data::allowed_children(elem_name)
        .into_iter()
        .filter_map(svg_data::element)
        .filter(|el| {
            let deprecated = compat
                .and_then(|c| c.elements.get(el.name))
                .map_or(el.deprecated, |co| co.deprecated);
            !deprecated
        })
        .map(element_completion_item)
        .collect()
}

pub fn root_element_completion_items() -> Vec<CompletionItem> {
    svg_data::element("svg")
        .into_iter()
        .map(element_completion_item)
        .collect()
}

pub fn value_completions(
    attr_name: &str,
    source: &[u8],
    tree: &tree_sitter::Tree,
    value_node: tree_sitter::Node<'_>,
) -> Vec<CompletionItem> {
    // Grammar-typed attribute values: dispatch on the tree node kind produced by
    // tree-sitter-svg rather than re-deriving the type from the catalog.
    match value_node.kind() {
        "href_attribute_value" => return href_value_completions(source, tree, value_node),
        "functional_iri_attribute_value" => {
            let mut items = href_value_completions(source, tree, value_node);
            items.push(completion_item("none", CompletionItemKind::KEYWORD));
            return items;
        }
        "paint_attribute_value" => {
            let mut items = href_value_completions(source, tree, value_node);
            items.extend(paint_value_completions());
            return items;
        }
        kind if kind.ends_with("_attribute_value") && kind != "quoted_attribute_value" => {
            if let Some(typed) = typed_value_completions(kind) {
                return typed;
            }
        }
        _ => {}
    }

    // Untyped fallback: href by name, then catalog-based completions.
    if matches!(attr_name, "href" | "xlink_href" | "xlink:href") {
        return href_value_completions(source, tree, value_node);
    }

    let Some(attr_def) = svg_data::attribute(attr_name) else {
        return Vec::new();
    };
    match &attr_def.values {
        AttributeValues::Enum(values) => values
            .iter()
            .map(|value| completion_item(value.to_string(), CompletionItemKind::VALUE))
            .collect(),
        AttributeValues::Transform(funcs) => funcs
            .iter()
            .map(|function| {
                snippet_completion_item(
                    function.to_string(),
                    CompletionItemKind::FUNCTION,
                    format!("{function}($0)"),
                )
            })
            .collect(),
        AttributeValues::PreserveAspectRatio {
            alignments,
            meet_or_slice,
        } => {
            let mut items: Vec<CompletionItem> = alignments
                .iter()
                .map(|alignment| {
                    completion_item(alignment.to_string(), CompletionItemKind::ENUM_MEMBER)
                })
                .collect();
            items.extend(
                meet_or_slice
                    .iter()
                    .map(|mode| completion_item(mode.to_string(), CompletionItemKind::ENUM_MEMBER)),
            );
            items
        }
        _ => Vec::new(),
    }
}

/// Dispatch completions for grammar-typed attribute value nodes.
fn typed_value_completions(value_kind: &str) -> Option<Vec<CompletionItem>> {
    Some(match value_kind {
        // Time / animation
        "duration_attribute_value" => duration_value_completions(),
        "repeat_count_attribute_value" => repeat_count_completions(),
        // Geometry
        "length_attribute_value" => length_value_completions(),
        "length_list_attribute_value" | "stroke_dasharray_attribute_value" => {
            let mut items = length_value_completions();
            items.push(completion_item("none", CompletionItemKind::KEYWORD));
            items.push(completion_item("inherit", CompletionItemKind::KEYWORD));
            items
        }
        "number_attribute_value" | "number_list_attribute_value" => number_completions(),
        "offset_attribute_value" | "opacity_attribute_value" => number_or_percentage_completions(),
        "viewbox_attribute_value" => viewbox_completions(),

        // Clipping
        "clip_attribute_value" => vec![
            completion_item("auto", CompletionItemKind::KEYWORD),
            completion_item("inherit", CompletionItemKind::KEYWORD),
            snippet_completion_item(
                "rect()",
                CompletionItemKind::FUNCTION,
                "rect($1, $2, $3, $4)",
            ),
        ],

        // Transform
        "transform_attribute_value" => transform_completions(),

        // Preserve aspect ratio
        "preserve_aspect_ratio_attribute_value" => preserve_aspect_ratio_completions(),

        // Enable-background (deprecated but grammar-typed)
        "enable_background_attribute_value" => vec![
            completion_item("accumulate", CompletionItemKind::KEYWORD),
            snippet_completion_item("new", CompletionItemKind::KEYWORD, "new $1 $2 $3 $4"),
        ],

        // Complex structured data — no simple value completions
        "d_attribute_value"
        | "points_attribute_value"
        | "key_splines_attribute_value"
        | "key_times_attribute_value"
        // Identity / class / style / events — no simple value completions
        | "id_attribute_value"
        | "class_attribute_value"
        | "style_attribute_value"
        | "event_attribute_value"
        | "xml_standalone_attribute_value" => {
            return None;
        }

        _ => return None,
    })
}

fn duration_value_completions() -> Vec<CompletionItem> {
    vec![
        detailed_completion_item("0s", CompletionItemKind::VALUE, "No duration"),
        detailed_completion_item("0.5s", CompletionItemKind::VALUE, "Half second"),
        detailed_completion_item("1s", CompletionItemKind::VALUE, "One second"),
        detailed_completion_item("2s", CompletionItemKind::VALUE, "Two seconds"),
        detailed_completion_item("200ms", CompletionItemKind::VALUE, "200 milliseconds"),
        completion_item("indefinite", CompletionItemKind::KEYWORD),
        completion_item("media", CompletionItemKind::KEYWORD),
    ]
}

fn repeat_count_completions() -> Vec<CompletionItem> {
    vec![
        detailed_completion_item("1", CompletionItemKind::VALUE, "Play once"),
        detailed_completion_item("2", CompletionItemKind::VALUE, "Play twice"),
        completion_item("indefinite", CompletionItemKind::KEYWORD),
    ]
}

fn paint_value_completions() -> Vec<CompletionItem> {
    vec![
        completion_item("none", CompletionItemKind::KEYWORD),
        completion_item("currentColor", CompletionItemKind::KEYWORD),
        completion_item("inherit", CompletionItemKind::KEYWORD),
        completion_item("context-fill", CompletionItemKind::KEYWORD),
        completion_item("context-stroke", CompletionItemKind::KEYWORD),
    ]
}

fn length_value_completions() -> Vec<CompletionItem> {
    ["px", "em", "rem", "%", "pt", "cm", "mm", "in"]
        .into_iter()
        .map(|unit| detailed_completion_item(unit, CompletionItemKind::UNIT, "SVG/CSS length unit"))
        .collect()
}

fn number_completions() -> Vec<CompletionItem> {
    vec![
        detailed_completion_item("0", CompletionItemKind::VALUE, "Zero"),
        detailed_completion_item("1", CompletionItemKind::VALUE, "One"),
    ]
}

fn number_or_percentage_completions() -> Vec<CompletionItem> {
    vec![
        detailed_completion_item("0", CompletionItemKind::VALUE, "Minimum"),
        detailed_completion_item("0.5", CompletionItemKind::VALUE, "Midpoint"),
        detailed_completion_item("1", CompletionItemKind::VALUE, "Maximum"),
        detailed_completion_item("50%", CompletionItemKind::VALUE, "Percentage midpoint"),
        detailed_completion_item("100%", CompletionItemKind::VALUE, "Full"),
    ]
}

fn viewbox_completions() -> Vec<CompletionItem> {
    vec![detailed_snippet_completion_item(
        "0 0 width height",
        CompletionItemKind::VALUE,
        "$1 $2 $3 $4",
        "min-x min-y width height",
    )]
}

fn transform_completions() -> Vec<CompletionItem> {
    [
        ("translate()", "translate($1, $2)"),
        ("scale()", "scale($1, $2)"),
        ("rotate()", "rotate($1)"),
        ("skewX()", "skewX($1)"),
        ("skewY()", "skewY($1)"),
        ("matrix()", "matrix($1, $2, $3, $4, $5, $6)"),
    ]
    .into_iter()
    .map(|(label, snippet)| snippet_completion_item(label, CompletionItemKind::FUNCTION, snippet))
    .collect()
}

fn preserve_aspect_ratio_completions() -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = [
        "none", "xMinYMin", "xMidYMin", "xMaxYMin", "xMinYMid", "xMidYMid", "xMaxYMid", "xMinYMax",
        "xMidYMax", "xMaxYMax",
    ]
    .into_iter()
    .map(|alignment| completion_item(alignment, CompletionItemKind::ENUM_MEMBER))
    .collect();
    items.push(completion_item("meet", CompletionItemKind::ENUM_MEMBER));
    items.push(completion_item("slice", CompletionItemKind::ENUM_MEMBER));
    items
}

fn attribute_completion_item(attr: &svg_data::AttributeDef) -> CompletionItem {
    detailed_snippet_completion_item(
        attr.name,
        CompletionItemKind::PROPERTY,
        format!("{}=\"$0\"", attr.name),
        attr.description,
    )
}

pub fn element_completion_item(el: &svg_data::ElementDef) -> CompletionItem {
    let insert_text = match el.content_model {
        ContentModel::Void => format!("{} />", el.name),
        _ => format!("{}>$0</{}>", el.name, el.name),
    };
    CompletionItem {
        label: el.name.to_string(),
        kind: Some(CompletionItemKind::PROPERTY),
        detail: Some(el.description.to_string()),
        deprecated: if el.deprecated { Some(true) } else { None },
        tags: if el.deprecated {
            Some(vec![CompletionItemTag::DEPRECATED])
        } else {
            None
        },
        insert_text: Some(insert_text),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}
