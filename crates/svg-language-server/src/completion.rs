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
) -> Vec<CompletionItem> {
    svg_data::attributes_for(elem_name)
        .into_iter()
        .filter(|attr| !attr.deprecated)
        .filter(|attr| !existing.contains(attr.name))
        .map(attribute_completion_item)
        .collect()
}

pub fn child_element_completion_items(elem_name: &str) -> Vec<CompletionItem> {
    svg_data::allowed_children(elem_name)
        .into_iter()
        .filter_map(svg_data::element)
        .filter(|el| !el.deprecated)
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
