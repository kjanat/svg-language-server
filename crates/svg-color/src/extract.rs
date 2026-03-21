use crate::{
    named_colors, parse,
    types::{ColorInfo, ColorKind},
};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use tree_sitter::{Parser, Point, Tree, TreeCursor};

type CustomProperties = HashMap<String, String>;

/// Extract all colors from SVG source text.
pub fn extract_colors(source: &[u8]) -> Vec<ColorInfo> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_svg::LANGUAGE.into())
        .expect("failed to load SVG grammar");
    let tree = parser.parse(source, None).expect("failed to parse");
    extract_colors_from_tree(source, &tree)
}

/// Extract colors from an already-parsed tree.
pub fn extract_colors_from_tree(source: &[u8], tree: &Tree) -> Vec<ColorInfo> {
    let mut css_parser = Parser::new();
    css_parser
        .set_language(&tree_sitter_css::LANGUAGE.into())
        .expect("failed to load CSS grammar");

    let mut colors = Vec::new();
    let mut cursor = tree.root_node().walk();
    walk(&mut cursor, source, &mut css_parser, &mut colors);
    colors
}

fn walk(
    cursor: &mut TreeCursor<'_>,
    source: &[u8],
    css_parser: &mut Parser,
    out: &mut Vec<ColorInfo>,
) {
    loop {
        let node = cursor.node();

        if let Some(info) = try_extract_svg(node, source) {
            out.push(info);
            // Color leaf nodes have no meaningful children to descend into.
        } else if try_extract_style_colors(node, source, css_parser, out) {
            // Style text nodes are reparsed as CSS and handled separately.
        } else if cursor.goto_first_child() {
            walk(cursor, source, css_parser, out);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn try_extract_svg(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<ColorInfo> {
    let byte_range = node.byte_range();
    let text = std::str::from_utf8(&source[byte_range.clone()]).ok()?;

    let (r, g, b, a, kind) = match node.kind() {
        "hex_color" => {
            let (r, g, b, a) = parse::parse_hex(text)?;
            (r, g, b, a, ColorKind::Hex)
        }
        "functional_color" => {
            let (r, g, b, a) = parse::parse_functional(text)?;
            (r, g, b, a, ColorKind::Functional)
        }
        "named_color" => parse_named_color(text)?,
        _ => return None,
    };

    Some(build_color_info(
        (r, g, b, a),
        byte_range,
        node.start_position(),
        node.end_position(),
        kind,
    ))
}

fn try_extract_style_colors(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    css_parser: &mut Parser,
    out: &mut Vec<ColorInfo>,
) -> bool {
    if !is_style_raw_text(node, source) {
        return false;
    }

    let byte_range = node.byte_range();
    let Some(css_source) = source.get(byte_range.clone()) else {
        return true;
    };
    let Some(tree) = css_parser.parse(css_source, None) else {
        return true;
    };
    let custom_properties = collect_css_custom_properties(css_source, &tree);

    let mut cursor = tree.root_node().walk();
    walk_css(
        &mut cursor,
        css_source,
        byte_range.start,
        node.start_position(),
        &custom_properties,
        out,
    );
    true
}

fn walk_css(
    cursor: &mut TreeCursor<'_>,
    css_source: &[u8],
    base_byte: usize,
    base_start: Point,
    custom_properties: &CustomProperties,
    out: &mut Vec<ColorInfo>,
) {
    loop {
        let node = cursor.node();

        if let Some(info) =
            try_extract_css_declaration(node, css_source, base_byte, base_start, custom_properties)
        {
            out.push(info);
        } else if let Some(info) = try_extract_css_leaf(node, css_source, base_byte, base_start) {
            out.push(info);
        } else if cursor.goto_first_child() {
            walk_css(
                cursor,
                css_source,
                base_byte,
                base_start,
                custom_properties,
                out,
            );
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn try_extract_css_leaf(
    node: tree_sitter::Node<'_>,
    css_source: &[u8],
    base_byte: usize,
    base_start: Point,
) -> Option<ColorInfo> {
    let byte_range = node.byte_range();
    let text = std::str::from_utf8(&css_source[byte_range.clone()]).ok()?;

    let (r, g, b, a, kind) = match node.kind() {
        "color_value" => {
            let (r, g, b, a) = parse::parse_hex(text)?;
            (r, g, b, a, ColorKind::Hex)
        }
        "call_expression" => {
            let function = css_function_name(node, css_source)?;
            if !matches!(
                function.to_ascii_lowercase().as_str(),
                "rgb" | "rgba" | "hsl" | "hsla" | "hwb" | "lab" | "lch" | "oklab" | "oklch"
            ) {
                return None;
            }
            let (r, g, b, a) = parse::parse_functional(text)?;
            (r, g, b, a, ColorKind::Functional)
        }
        "plain_value" => {
            if !has_color_like_property(node, css_source) {
                return None;
            }
            parse_named_color(text)?
        }
        _ => return None,
    };

    Some(build_color_info(
        (r, g, b, a),
        offset_range(byte_range, base_byte),
        offset_point(node.start_position(), base_start),
        offset_point(node.end_position(), base_start),
        kind,
    ))
}

fn try_extract_css_declaration(
    node: tree_sitter::Node<'_>,
    css_source: &[u8],
    base_byte: usize,
    base_start: Point,
    custom_properties: &CustomProperties,
) -> Option<ColorInfo> {
    if node.kind() != "declaration" {
        return None;
    }

    let property_name = declaration_property_name(node, css_source)?;
    if !is_color_like_property(property_name) {
        return None;
    }

    let value_node = declaration_primary_value_node(node)?;
    let value_text = declaration_value_text(node, css_source)?;
    let (r, g, b, a, kind) = resolve_css_color(value_text, custom_properties, &mut HashSet::new())?;

    Some(build_color_info(
        (r, g, b, a),
        offset_range(value_node.byte_range(), base_byte),
        offset_point(value_node.start_position(), base_start),
        offset_point(value_node.end_position(), base_start),
        kind,
    ))
}

fn collect_css_custom_properties(css_source: &[u8], tree: &Tree) -> CustomProperties {
    let mut properties = HashMap::new();
    let mut cursor = tree.root_node().walk();
    walk_css_tree(&mut cursor, &mut |node| {
        if node.kind() != "declaration" {
            return;
        }
        let Some(property_name) = declaration_property_name(node, css_source) else {
            return;
        };
        if !property_name.starts_with("--") {
            return;
        }
        let Some(value_text) = declaration_value_text(node, css_source) else {
            return;
        };
        properties.insert(property_name.to_owned(), value_text.to_owned());
    });
    properties
}

fn resolve_css_color(
    text: &str,
    custom_properties: &CustomProperties,
    seen: &mut HashSet<String>,
) -> Option<(f32, f32, f32, f32, ColorKind)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    if let Some(color) = parse_literal_css_color(text) {
        return Some(color);
    }

    let (function, args) = parse_css_function_call(text)?;
    match function.as_str() {
        "var" => resolve_var_color(args, custom_properties, seen),
        "color-mix" => resolve_color_mix(args, custom_properties, seen),
        _ => None,
    }
}

fn parse_literal_css_color(text: &str) -> Option<(f32, f32, f32, f32, ColorKind)> {
    if let Some((r, g, b, a)) = parse::parse_hex(text) {
        return Some((r, g, b, a, ColorKind::Hex));
    }
    if let Some((r, g, b, a)) = parse::parse_functional(text) {
        return Some((r, g, b, a, ColorKind::Functional));
    }
    parse_named_color(text)
}

fn resolve_var_color(
    args: &str,
    custom_properties: &CustomProperties,
    seen: &mut HashSet<String>,
) -> Option<(f32, f32, f32, f32, ColorKind)> {
    let parts = split_top_level(args, ',');
    let name = parts.first()?.trim();
    if !name.starts_with("--") || !seen.insert(name.to_owned()) {
        return None;
    }

    let resolved = custom_properties
        .get(name)
        .and_then(|value| resolve_css_color(value, custom_properties, seen))
        .or_else(|| {
            parts
                .get(1)
                .and_then(|fallback| resolve_css_color(fallback.trim(), custom_properties, seen))
        });

    seen.remove(name);
    resolved
}

fn resolve_color_mix(
    args: &str,
    custom_properties: &CustomProperties,
    seen: &mut HashSet<String>,
) -> Option<(f32, f32, f32, f32, ColorKind)> {
    let parts = split_top_level(args, ',');
    let [space_part, left_stop, right_stop]: [&str; 3] = parts.try_into().ok()?;
    let space = space_part.trim().strip_prefix("in ")?.trim();
    let space = space.split_whitespace().next()?;

    let (left, left_pct) = parse_color_mix_stop(left_stop, custom_properties, seen)?;
    let (right, right_pct) = parse_color_mix_stop(right_stop, custom_properties, seen)?;
    let (left_weight, right_weight, alpha_scale) = resolve_mix_weights(left_pct, right_pct)?;

    let mut mixed = parse::mix_colors(space, left, left_weight as f32, right, right_weight as f32)?;
    mixed.3 = (mixed.3 * alpha_scale as f32).clamp(0.0, 1.0);
    Some((mixed.0, mixed.1, mixed.2, mixed.3, ColorKind::Functional))
}

fn parse_color_mix_stop(
    stop: &str,
    custom_properties: &CustomProperties,
    seen: &mut HashSet<String>,
) -> Option<((f32, f32, f32, f32), Option<f64>)> {
    let (color_text, percentage) = split_color_stop_percentage(stop.trim());
    let (r, g, b, a, _) = resolve_css_color(color_text, custom_properties, seen)?;
    Some(((r, g, b, a), percentage))
}

fn resolve_mix_weights(left_pct: Option<f64>, right_pct: Option<f64>) -> Option<(f64, f64, f64)> {
    let mut left = left_pct;
    let mut right = right_pct;

    match (left, right) {
        (Some(l), Some(r)) => {
            if l < 0.0 || r < 0.0 {
                return None;
            }
        }
        (Some(l), None) => {
            if !(0.0..=100.0).contains(&l) {
                return None;
            }
            right = Some(100.0 - l);
        }
        (None, Some(r)) => {
            if !(0.0..=100.0).contains(&r) {
                return None;
            }
            left = Some(100.0 - r);
        }
        (None, None) => {
            left = Some(50.0);
            right = Some(50.0);
        }
    }

    let left = left?;
    let right = right?;
    let total = left + right;
    if total <= 0.0 {
        return None;
    }

    Some((left / total, right / total, (total / 100.0).min(1.0)))
}

fn split_color_stop_percentage(stop: &str) -> (&str, Option<f64>) {
    let mut depth = 0usize;
    let mut last_space = None;

    for (idx, ch) in stop.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c.is_whitespace() && depth == 0 => last_space = Some(idx),
            _ => {}
        }
    }

    let Some(space_idx) = last_space else {
        return (stop, None);
    };
    let color = stop[..space_idx].trim();
    let candidate = stop[space_idx..].trim();
    if color.is_empty() {
        return (stop, None);
    }

    match parse_mix_percentage(candidate) {
        Some(percentage) => (color, Some(percentage)),
        None => (stop, None),
    }
}

fn parse_mix_percentage(text: &str) -> Option<f64> {
    let pct = text.strip_suffix('%')?.trim();
    let value: f64 = pct.parse().ok()?;
    if value.is_finite() { Some(value) } else { None }
}

fn parse_css_function_call(text: &str) -> Option<(String, &str)> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    if close != text.len() - 1 {
        return None;
    }
    let function = text[..open].trim().to_ascii_lowercase();
    if function.is_empty() {
        return None;
    }
    Some((function, text[open + 1..close].trim()))
}

fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c == separator && depth == 0 => {
                parts.push(text[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    parts.push(text[start..].trim());
    parts
}

fn walk_css_tree(cursor: &mut TreeCursor<'_>, f: &mut impl FnMut(tree_sitter::Node<'_>)) {
    loop {
        let node = cursor.node();
        f(node);

        if cursor.goto_first_child() {
            walk_css_tree(cursor, f);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn parse_named_color(text: &str) -> Option<(f32, f32, f32, f32, ColorKind)> {
    if text.eq_ignore_ascii_case("transparent") {
        return Some((0.0, 0.0, 0.0, 0.0, ColorKind::Named));
    }

    let (r, g, b) = named_colors::lookup(text)?;
    Some((r, g, b, 1.0, ColorKind::Named))
}

fn build_color_info(
    (r, g, b, a): (f32, f32, f32, f32),
    byte_range: Range<usize>,
    start: Point,
    end: Point,
    kind: ColorKind,
) -> ColorInfo {
    ColorInfo {
        r,
        g,
        b,
        a,
        byte_range,
        start_row: start.row,
        start_col: start.column,
        end_row: end.row,
        end_col: end.column,
        kind,
    }
}

fn offset_range(range: Range<usize>, base_byte: usize) -> Range<usize> {
    (range.start + base_byte)..(range.end + base_byte)
}

fn offset_point(point: Point, base: Point) -> Point {
    Point {
        row: point.row + base.row,
        column: if point.row == 0 {
            point.column + base.column
        } else {
            point.column
        },
    }
}

fn is_style_raw_text(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "raw_text" {
        return false;
    }

    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "element" {
        return false;
    }

    let mut cursor = parent.walk();
    if !cursor.goto_first_child() {
        return false;
    }

    loop {
        let child = cursor.node();
        if child.kind() == "start_tag" {
            return tag_name(child, source)
                .map(|name| name.eq_ignore_ascii_case("style"))
                .unwrap_or(false);
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    false
}

fn tag_name<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return None;
    }

    loop {
        let child = cursor.node();
        if child.kind() == "name" {
            return std::str::from_utf8(&source[child.byte_range()]).ok();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    None
}

fn css_function_name<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return None;
    }

    loop {
        let child = cursor.node();
        if child.kind() == "function_name" {
            return std::str::from_utf8(&source[child.byte_range()]).ok();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    None
}

fn has_color_like_property(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(name) = nearest_declaration_property_name(node, source) else {
        return false;
    };
    is_color_like_property(name)
}

fn nearest_declaration_property_name<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> Option<&'a str> {
    let mut current = Some(node);

    while let Some(node) = current {
        if node.kind() == "declaration" {
            return declaration_property_name(node, source);
        }
        current = node.parent();
    }

    None
}

fn declaration_primary_value_node(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return None;
    }

    loop {
        let child = cursor.node();
        if child.is_named() && child.kind() != "property_name" {
            return Some(child);
        }

        if !cursor.goto_next_sibling() {
            return None;
        }
    }
}

fn declaration_value_text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return None;
    }

    let mut start = None;
    let mut end = None;

    loop {
        let child = cursor.node();
        if child.is_named() && child.kind() != "property_name" {
            start.get_or_insert(child.start_byte());
            end = Some(child.end_byte());
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    let range = start?..end?;
    std::str::from_utf8(&source[range]).ok()
}

fn declaration_property_name<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return None;
    }

    loop {
        let child = cursor.node();
        if child.kind() == "property_name" {
            return std::str::from_utf8(&source[child.byte_range()]).ok();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    None
}

fn is_color_like_property(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "fill"
        || name == "stroke"
        || name == "color"
        || name.ends_with("color")
        || name.starts_with("--")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_fill() {
        let src = b"<svg><rect fill=\"#ff0000\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].r, 1.0);
        assert_eq!(colors[0].g, 0.0);
        assert_eq!(colors[0].b, 0.0);
        assert_eq!(colors[0].kind, ColorKind::Hex);
    }

    #[test]
    fn named_stroke() {
        let src = b"<svg><circle stroke=\"red\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].r, 1.0);
        assert_eq!(colors[0].kind, ColorKind::Named);
    }

    #[test]
    fn functional_fill() {
        let src = b"<svg><rect fill=\"rgb(0, 128, 255)\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].kind, ColorKind::Functional);
    }

    #[test]
    fn paint_server_fallback() {
        let src = b"<svg><rect fill=\"url(#grad) #00ff00\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].g, 1.0);
    }

    #[test]
    fn multiple_colors() {
        let src = b"<svg><rect fill=\"#ff0000\" stroke=\"#00ff00\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 2);
    }

    #[test]
    fn keywords_skipped() {
        let src = b"<svg><rect fill=\"none\" stroke=\"currentColor\" color=\"inherit\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 0);
    }

    #[test]
    fn transparent_named_color_is_included() {
        let src = b"<svg><rect fill=\"transparent\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].kind, ColorKind::Named);
        assert_eq!(colors[0].a, 0.0);
    }

    #[test]
    fn invalid_named_color_skipped() {
        let src = b"<svg><rect fill=\"banana\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 0);
    }

    #[test]
    fn color_in_comment_ignored() {
        let src = b"<svg><!-- fill=\"#ff0000\" --><rect fill=\"blue\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].b, 1.0); // blue, not the comment's red
    }

    #[test]
    fn stop_color() {
        let src = b"<svg><stop stop-color=\"#ff8800\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
    }

    #[test]
    fn byte_range_correct() {
        let src = b"<svg><rect fill=\"#ff0000\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        let color_text = std::str::from_utf8(&src[colors[0].byte_range.clone()]).unwrap();
        assert_eq!(color_text, "#ff0000");
    }

    #[test]
    fn empty_paint_value() {
        let src = b"<svg><rect fill=\"\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 0);
    }

    #[test]
    fn colors_inside_style_element() {
        let src = br#"<svg><style>rect { fill: #ff0000; stroke: rgb(0 128 255 / 50%); color: red; background-color: oklch(0.627966 0.257704 29.2346); outline-color: oklab(62.7966% 0.22488 0.125859); border-color: hwb(120 0% 0%); text-decoration-color: lab(29.2345% 39.3825 20.0664); column-rule-color: lch(29.2345% 44.2 27); }</style></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 8);
        assert_eq!(colors[0].kind, ColorKind::Hex);
        assert_eq!(colors[1].kind, ColorKind::Functional);
        assert_eq!(colors[2].kind, ColorKind::Named);
        assert_eq!(colors[3].kind, ColorKind::Functional);
        assert_eq!(colors[4].kind, ColorKind::Functional);
        assert_eq!(colors[5].kind, ColorKind::Functional);
        assert_eq!(colors[6].kind, ColorKind::Functional);
        assert_eq!(colors[7].kind, ColorKind::Functional);
    }

    #[test]
    fn non_color_css_plain_values_are_ignored() {
        let src =
            br#"<svg><style>rect { display: block; animation-name: red; fill: red; }</style></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].kind, ColorKind::Named);
        let color_text = std::str::from_utf8(&src[colors[0].byte_range.clone()]).unwrap();
        assert_eq!(color_text, "red");
    }

    #[test]
    fn style_color_byte_range_and_position_are_absolute() {
        let src = b"<svg>\n<style>\nrect {\n  fill: red;\n}\n</style>\n</svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        let color = &colors[0];
        assert_eq!(
            std::str::from_utf8(&src[color.byte_range.clone()]).unwrap(),
            "red"
        );
        assert_eq!(color.start_row, 3);
        assert_eq!(color.start_col, 8);
        assert_eq!(color.end_row, 3);
        assert_eq!(color.end_col, 11);
    }

    #[test]
    fn css_custom_properties_and_color_mix_are_resolved() {
        let src = br#"<svg><style>:root { --base: oklch(22.84% 0.038 283); --toolbar-bg: color-mix(in oklch, var(--base), white 8%); } rect { fill: var(--toolbar-bg); stroke: var(--base); }</style></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 4);

        let mut by_text = HashMap::new();
        for color in &colors {
            by_text.insert(
                std::str::from_utf8(&src[color.byte_range.clone()]).unwrap(),
                color,
            );
        }

        let base = parse::parse_functional("oklch(22.84% 0.038 283)").unwrap();
        let mixed = parse::mix_colors("oklch", base, 0.92, (1.0, 1.0, 1.0, 1.0), 0.08).unwrap();

        let base_decl = by_text["oklch(22.84% 0.038 283)"];
        assert!((base_decl.r - base.0).abs() < 0.02);
        assert_eq!(base_decl.kind, ColorKind::Functional);

        let mix_decl = by_text["color-mix(in oklch, var(--base), white 8%)"];
        assert!((mix_decl.r - mixed.0).abs() < 0.03);
        assert!((mix_decl.g - mixed.1).abs() < 0.03);
        assert!((mix_decl.b - mixed.2).abs() < 0.03);

        let fill_ref = by_text["var(--toolbar-bg)"];
        assert!((fill_ref.r - mixed.0).abs() < 0.03);
        assert!((fill_ref.g - mixed.1).abs() < 0.03);
        assert!((fill_ref.b - mixed.2).abs() < 0.03);

        let stroke_ref = by_text["var(--base)"];
        assert!((stroke_ref.r - base.0).abs() < 0.02);
        assert!((stroke_ref.g - base.1).abs() < 0.02);
        assert!((stroke_ref.b - base.2).abs() < 0.02);
    }

    #[test]
    fn color_mix_with_transparent_preserves_base_hue() {
        let src = br#"<svg><style>:root { --base: oklch(22.84% 0.038 283); --panel-bg: color-mix(in oklch, var(--base) 96%, transparent); } rect { fill: var(--panel-bg); }</style></svg>"#;
        let colors = extract_colors(src);

        let fill_ref = colors
            .iter()
            .find(|color| {
                std::str::from_utf8(&src[color.byte_range.clone()]).unwrap() == "var(--panel-bg)"
            })
            .expect("resolved panel color");
        let base = parse::parse_functional("oklch(22.84% 0.038 283)").unwrap();

        assert!((fill_ref.r - base.0).abs() < 0.03);
        assert!((fill_ref.g - base.1).abs() < 0.03);
        assert!((fill_ref.b - base.2).abs() < 0.03);
        assert!(fill_ref.a < 1.0);
        assert!(fill_ref.a > 0.9);
    }
}
