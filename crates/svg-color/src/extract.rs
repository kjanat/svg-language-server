use crate::{
    named_colors, parse,
    types::{ColorInfo, ColorKind},
};
use std::ops::Range;
use tree_sitter::{Parser, Point, Tree, TreeCursor};

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
        "named_color" => {
            let (r, g, b) = named_colors::lookup(text)?;
            (r, g, b, 1.0, ColorKind::Named)
        }
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

    let mut cursor = tree.root_node().walk();
    walk_css(
        &mut cursor,
        css_source,
        byte_range.start,
        node.start_position(),
        out,
    );
    true
}

fn walk_css(
    cursor: &mut TreeCursor<'_>,
    css_source: &[u8],
    base_byte: usize,
    base_start: Point,
    out: &mut Vec<ColorInfo>,
) {
    loop {
        let node = cursor.node();

        if let Some(info) = try_extract_css(node, css_source, base_byte, base_start) {
            out.push(info);
        } else if cursor.goto_first_child() {
            walk_css(cursor, css_source, base_byte, base_start, out);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn try_extract_css(
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
                "rgb" | "rgba" | "hsl" | "hsla" | "oklab" | "oklch"
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
            let (r, g, b) = named_colors::lookup(text)?;
            (r, g, b, 1.0, ColorKind::Named)
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
        let src = br#"<svg><style>rect { fill: #ff0000; stroke: rgb(0, 128, 255); color: red; background-color: oklch(0.627966 0.257704 29.2346); outline-color: oklab(62.7966% 0.22488 0.125859); }</style></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 5);
        assert_eq!(colors[0].kind, ColorKind::Hex);
        assert_eq!(colors[1].kind, ColorKind::Functional);
        assert_eq!(colors[2].kind, ColorKind::Named);
        assert_eq!(colors[3].kind, ColorKind::Functional);
        assert_eq!(colors[4].kind, ColorKind::Functional);
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
}
