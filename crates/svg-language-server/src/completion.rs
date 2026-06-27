use std::collections::HashSet;

use svg_data::{
    AttributeValues, ContentModel, ProfiledAttribute, ProfiledElement, SpecLifecycle,
    SpecSnapshotId,
};
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
    match css_completion_context(css, byte_offset) {
        CssCompletionContext::Selector => css_selector_completions(),
        CssCompletionContext::Property => css_property_completions(),
        CssCompletionContext::Value => {
            // Only the value context consumes custom properties; defer the
            // stylesheet scan so selector/property completions don't pay for it.
            let custom_properties =
                svg_references::collect_custom_property_definitions_from_stylesheet(css, 0, 0);
            css_value_completions(&custom_properties)
        }
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
    let mut items = vec![css_value_keyword("none")];
    items.extend(color_value_completions());
    items.extend([
        css_value_function("var()", "var(--$0)", "CSS custom property reference"),
        css_value_function("url()", "url(#$0)", "SVG fragment reference"),
    ]);

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

fn color_value_completions() -> Vec<CompletionItem> {
    let mut items = vec![
        css_value_keyword("currentColor"),
        css_value_keyword("transparent"),
        css_value_keyword("inherit"),
        css_value_function("rgb()", "rgb($0)", "RGB color"),
        css_value_function("hsl()", "hsl($0)", "HSL color"),
        css_value_function("hwb()", "hwb($0)", "HWB color"),
        css_value_function("lab()", "lab($0)", "Lab color"),
        css_value_function("lch()", "lch($0)", "LCH color"),
        css_value_function("oklab()", "oklab($0)", "Oklab color"),
        css_value_function("oklch()", "oklch($0)", "Oklch color"),
        css_value_function("color()", "color($0)", "Color function"),
        css_value_function(
            "color-mix()",
            "color-mix(in oklch, $1, $2)",
            "Mixed color expression",
        ),
        css_value_function(
            "contrast-color()",
            "contrast-color($0)",
            "Contrasting color",
        ),
        css_value_function("device-cmyk()", "device-cmyk($0)", "Device CMYK color"),
        css_value_function(
            "light-dark()",
            "light-dark($1, $2)",
            "Light/dark color pair",
        ),
    ];

    items.extend(
        svg_color::NAMED_COLOR_NAMES.iter().map(|name| {
            detailed_completion_item(*name, CompletionItemKind::COLOR, "CSS named color")
        }),
    );
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
    fragment_reference_completions(source, tree, value_node, FragmentReferenceSyntax::Bare)
}

fn url_value_completions(
    source: &[u8],
    tree: &tree_sitter::Tree,
    value_node: tree_sitter::Node<'_>,
) -> Vec<CompletionItem> {
    fragment_reference_completions(
        source,
        tree,
        value_node,
        FragmentReferenceSyntax::UrlFunction,
    )
}

#[derive(Clone, Copy)]
enum FragmentReferenceSyntax {
    Bare,
    UrlFunction,
}

fn fragment_reference_completions(
    source: &[u8],
    tree: &tree_sitter::Tree,
    value_node: tree_sitter::Node<'_>,
    syntax: FragmentReferenceSyntax,
) -> Vec<CompletionItem> {
    let replace_range = attribute_value_inner_range(source, value_node);
    let mut ids: Vec<String> = svg_references::collect_id_definitions(source, tree)
        .into_iter()
        .map(|definition| definition.name)
        .collect();
    ids.sort();
    ids.dedup();

    ids.into_iter()
        .map(|id| {
            let replacement = match syntax {
                FragmentReferenceSyntax::Bare => format!("#{id}"),
                FragmentReferenceSyntax::UrlFunction => format!("url(#{id})"),
            };
            replace_completion_item(
                replacement.clone(),
                CompletionItemKind::REFERENCE,
                "In-document fragment reference",
                replace_range,
                replacement,
            )
        })
        .collect()
}

pub fn attribute_completion_items(
    elem_name: &str,
    existing: &HashSet<String>,
    profile: svg_data::SpecSnapshotId,
) -> Vec<CompletionItem> {
    svg_data::attributes_for_with_profile(profile, elem_name)
        .into_iter()
        .filter(|attr| !existing.contains(attr.name))
        .map(attribute_completion_item)
        .collect()
}

pub fn child_element_completion_items(
    elem_name: &str,
    profile: svg_data::SpecSnapshotId,
) -> Vec<CompletionItem> {
    svg_data::allowed_children_with_profile(profile, elem_name)
        .into_iter()
        .map(element_completion_item)
        .collect()
}

pub fn root_element_completion_items(profile: svg_data::SpecSnapshotId) -> Vec<CompletionItem> {
    match svg_data::element_for_profile(profile, "svg") {
        svg_data::ProfileLookup::Present { value, lifecycle } => {
            vec![element_completion_item(ProfiledElement {
                element: value,
                lifecycle,
            })]
        }
        svg_data::ProfileLookup::UnsupportedInProfile { .. } | svg_data::ProfileLookup::Unknown => {
            Vec::new()
        }
    }
}

/// Build completion items for an attribute value position.
///
/// Dispatches grammar-typed `*_attribute_value` node kinds first (typed
/// completions like `length`, `paint`, `transform`), then falls back to the
/// `svg-data` catalog and resolves the active profile's value list via
/// [`svg_data::AttributeDef::values_for_profile`] so SVG 1.1-only keywords
/// (e.g. `display` keeps `run-in`/`compact`/`marker`) surface for the active
/// profile and disappear from SVG 2.
pub fn value_completions(
    attr_name: &str,
    source: &[u8],
    tree: &tree_sitter::Tree,
    value_node: tree_sitter::Node<'_>,
    profile: SpecSnapshotId,
) -> Vec<CompletionItem> {
    // Grammar-typed attribute values: dispatch on the tree node kind produced by
    // tree-sitter-svg rather than re-deriving the type from the catalog.
    match value_node.kind() {
        "href_attribute_value" => return href_value_completions(source, tree, value_node),
        "functional_iri_attribute_value" => {
            let mut items = url_value_completions(source, tree, value_node);
            items.push(completion_item("none", CompletionItemKind::KEYWORD));
            return items;
        }
        "paint_attribute_value" => {
            if paint_attribute_allows_fragment_reference(attr_name) {
                let mut items = url_value_completions(source, tree, value_node);
                items.extend(paint_value_completions());
                return items;
            }
            return color_value_completions();
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
    // Profile overrides only reach the catalog-driven arms below; grammar-typed
    // values are dispatched above. SVG 1.1 `display`, for instance, keeps the
    // CSS2 `run-in`/`compact`/`marker` keywords that the union default drops.
    let values = attr_def.values_for_profile(profile);
    match values {
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
        AttributeValues::CssGrammar { graph, .. } => css_grammar_value_completions(graph),
        AttributeValues::Color
        | AttributeValues::Length
        | AttributeValues::Url
        | AttributeValues::Boolean
        | AttributeValues::TokenList
        | AttributeValues::CommaTokenList
        | AttributeValues::UrlTokenList
        | AttributeValues::LanguageTag
        | AttributeValues::Integer
        | AttributeValues::MediaType
        | AttributeValues::MediaQueryList
        | AttributeValues::CssDeclarationList
        | AttributeValues::Id
        | AttributeValues::ReferrerPolicy
        | AttributeValues::SuggestedFileName
        | AttributeValues::PathData
        | AttributeValues::SemicolonNumberList
        | AttributeValues::CoordinatePair
        | AttributeValues::CoordinatePairList
        | AttributeValues::NumberOrPercentage
        | AttributeValues::FreeText => Vec::new(),
    }
}

fn paint_attribute_allows_fragment_reference(attr_name: &str) -> bool {
    matches!(attr_name, "fill" | "stroke")
}

fn css_grammar_value_completions(graph: &svg_data::CssGrammarGraph) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    for node in graph.nodes {
        let Some(text) = node.text else {
            continue;
        };
        match node.kind {
            svg_data::CssGrammarNodeKind::Keyword => {
                items.push(completion_item(text.to_owned(), CompletionItemKind::VALUE));
            }
            svg_data::CssGrammarNodeKind::Function => {
                items.push(snippet_completion_item(
                    text.to_owned(),
                    CompletionItemKind::FUNCTION,
                    format!("{text}($0)"),
                ));
            }
            svg_data::CssGrammarNodeKind::Root
            | svg_data::CssGrammarNodeKind::Group
            | svg_data::CssGrammarNodeKind::Type
            | svg_data::CssGrammarNodeKind::Operator => {}
        }
    }
    items.sort_by(|left, right| left.label.cmp(&right.label));
    items.dedup_by(|left, right| left.label == right.label);
    items
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
        "rotate_attribute_value" => {
            let mut items = number_completions();
            items.push(completion_item("auto", CompletionItemKind::KEYWORD));
            items.push(completion_item("auto-reverse", CompletionItemKind::KEYWORD));
            items
        }
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
    let mut items = vec![
        completion_item("none", CompletionItemKind::KEYWORD),
        completion_item("context-fill", CompletionItemKind::KEYWORD),
        completion_item("context-stroke", CompletionItemKind::KEYWORD),
    ];
    items.extend(color_value_completions());
    items
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

fn lifecycle_completion_detail(description: &str, lifecycle: SpecLifecycle) -> String {
    match lifecycle {
        SpecLifecycle::Stable => description.to_owned(),
        SpecLifecycle::Experimental => format!("{description} [Experimental]"),
        SpecLifecycle::Deprecated => format!("{description} [Deprecated]"),
        SpecLifecycle::Obsolete => format!("{description} [Obsolete]"),
    }
}

fn attribute_completion_item(attr: ProfiledAttribute) -> CompletionItem {
    detailed_snippet_completion_item(
        attr.name,
        CompletionItemKind::PROPERTY,
        format!("{}=\"$0\"", attr.name),
        lifecycle_completion_detail(attr.attribute.description, attr.lifecycle),
    )
}

pub fn element_completion_item(el: ProfiledElement) -> CompletionItem {
    let insert_text = match el.element.content_model {
        ContentModel::Void => format!("{} />", el.element.name),
        _ => format!("{}>$0</{}>", el.element.name, el.element.name),
    };
    let is_deprecated = matches!(
        el.lifecycle,
        SpecLifecycle::Deprecated | SpecLifecycle::Obsolete
    );
    CompletionItem {
        label: el.element.name.to_string(),
        kind: Some(CompletionItemKind::PROPERTY),
        detail: Some(lifecycle_completion_detail(
            el.element.description,
            el.lifecycle,
        )),
        deprecated: is_deprecated.then_some(true),
        tags: if is_deprecated {
            Some(vec![CompletionItemTag::DEPRECATED])
        } else {
            None
        },
        insert_text: Some(insert_text),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ELEMENT: svg_data::ElementDef = svg_data::ElementDef {
        name: "demo",
        description: "Demo element.",
        mdn_url: "https://example.com/demo",
        spec_url: None,
        deprecated: false,
        experimental: false,
        standard_track: None,
        baseline: None,
        browser_support: None,
        content_model: ContentModel::Void,
        attrs: &[],
        global_attrs: false,
    };

    const TEST_ATTRIBUTE: svg_data::AttributeDef = svg_data::AttributeDef {
        name: "demo-attr",
        description: "Demo attribute.",
        mdn_url: "https://example.com/demo-attr",
        spec_url: None,
        deprecated: false,
        experimental: false,
        standard_track: None,
        animatable: false,
        presentation_attribute: None,
        baseline: None,
        browser_support: None,
        element_compat: &[],
        element_values: &[],
        values: AttributeValues::FreeText,
        value_overrides: &[],
        applicability: svg_data::AttributeApplicability::Global,
    };

    #[test]
    fn deprecated_element_completion_is_annotated_and_tagged() {
        let item = element_completion_item(ProfiledElement {
            element: &TEST_ELEMENT,
            lifecycle: SpecLifecycle::Deprecated,
        });

        assert_eq!(item.detail.as_deref(), Some("Demo element. [Deprecated]"));
        assert_eq!(item.deprecated, Some(true));
        assert_eq!(item.tags, Some(vec![CompletionItemTag::DEPRECATED]));
    }

    #[test]
    fn experimental_attribute_completion_is_annotated() {
        let item = attribute_completion_item(ProfiledAttribute {
            name: TEST_ATTRIBUTE.name,
            attribute: &TEST_ATTRIBUTE,
            lifecycle: SpecLifecycle::Experimental,
        });

        assert_eq!(
            item.detail.as_deref(),
            Some("Demo attribute. [Experimental]")
        );
        assert_eq!(item.deprecated, None);
        assert_eq!(item.tags, None);
    }
}
