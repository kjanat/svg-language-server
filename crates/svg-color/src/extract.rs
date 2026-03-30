mod property;
mod resolve;

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use svg_tree::walk_tree;
use tree_sitter::{Parser, Point, Tree, TreeCursor};

use crate::{
    parse,
    types::{ColorInfo, ColorKind},
};

type CustomProperties = HashMap<String, String>;
type Rgba = (f32, f32, f32, f32);
type ResolvedColor = (f32, f32, f32, f32, ColorKind);
type ColorStop = (Rgba, Option<f64>);

/// Extract all colors from SVG source text.
#[must_use]
pub fn colors(source: &[u8]) -> Vec<ColorInfo> {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_svg::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    colors_from_tree(source, &tree)
}

/// Extract colors from an already-parsed tree.
#[must_use]
pub fn colors_from_tree(source: &[u8], tree: &Tree) -> Vec<ColorInfo> {
    let mut css_parser = Parser::new();
    if css_parser
        .set_language(&tree_sitter_css::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }

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
            let (r, g, b, a) = parse::hex(text)?;
            (r, g, b, a, ColorKind::Hex)
        }
        "functional_color" => {
            let (r, g, b, a) = parse::functional(text)?;
            (r, g, b, a, ColorKind::Functional)
        }
        "named_color" => resolve::parse_named_color(text)?,
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
    if !property::is_style_raw_text(node, source) {
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
            let (r, g, b, a) = parse::hex(text)?;
            (r, g, b, a, ColorKind::Hex)
        }
        "call_expression" => {
            let function = property::css_function_name(node, css_source)?;
            if !matches!(
                function.to_ascii_lowercase().as_str(),
                "rgb" | "rgba" | "hsl" | "hsla" | "hwb" | "lab" | "lch" | "oklab" | "oklch"
            ) {
                return None;
            }
            let (r, g, b, a) = parse::functional(text)?;
            (r, g, b, a, ColorKind::Functional)
        }
        "plain_value" => {
            if !property::has_color_like_property(node, css_source) {
                return None;
            }
            resolve::parse_named_color(text)?
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

    let prop_name = property::declaration_property_name(node, css_source)?;
    if !property::is_color_like_property(prop_name) {
        return None;
    }

    let value_node = property::declaration_primary_value_node(node)?;
    let value_text = property::declaration_value_text(node, css_source)?;
    let (r, g, b, a, kind) =
        resolve::resolve_css_color(value_text, custom_properties, &mut HashSet::new())?;

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
    walk_tree(&mut cursor, &mut |node| {
        if node.kind() != "declaration" {
            return;
        }
        let Some(prop_name) = property::declaration_property_name(node, css_source) else {
            return;
        };
        if !prop_name.starts_with("--") {
            return;
        }
        let Some(value_text) = property::declaration_value_text(node, css_source) else {
            return;
        };
        properties.insert(prop_name.to_owned(), value_text.to_owned());
    });
    properties
}

const fn build_color_info(
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

const fn offset_range(range: Range<usize>, base_byte: usize) -> Range<usize> {
    (range.start + base_byte)..(range.end + base_byte)
}

const fn offset_point(point: Point, base: Point) -> Point {
    Point {
        row: point.row + base.row,
        column: if point.row == 0 {
            point.column + base.column
        } else {
            point.column
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{colors as extract_colors, *};

    #[test]
    fn hex_fill() {
        let src = b"<svg><rect fill=\"#ff0000\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert!((colors[0].r - 1.0).abs() < f32::EPSILON);
        assert!((colors[0].g - 0.0).abs() < f32::EPSILON);
        assert!((colors[0].b - 0.0).abs() < f32::EPSILON);
        assert_eq!(colors[0].kind, ColorKind::Hex);
    }

    #[test]
    fn named_stroke() {
        let src = b"<svg><circle stroke=\"red\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert!((colors[0].r - 1.0).abs() < f32::EPSILON);
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
        assert!((colors[0].g - 1.0).abs() < f32::EPSILON);
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
        assert!(colors[0].a.abs() < f32::EPSILON);
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
        assert!((colors[0].b - 1.0).abs() < f32::EPSILON); // blue, not the comment's red
    }

    #[test]
    fn stop_color() {
        let src = b"<svg><stop stop-color=\"#ff8800\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
    }

    #[test]
    fn byte_range_correct() -> Result<(), Box<dyn std::error::Error>> {
        let src = b"<svg><rect fill=\"#ff0000\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        let color_text = std::str::from_utf8(&src[colors[0].byte_range.clone()])?;
        assert_eq!(color_text, "#ff0000");
        Ok(())
    }

    #[test]
    fn empty_paint_value() {
        let src = b"<svg><rect fill=\"\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 0);
    }

    #[test]
    fn colors_inside_style_element() {
        let src = br"<svg><style>rect { fill: #ff0000; stroke: rgb(0 128 255 / 50%); color: red; background-color: oklch(0.627966 0.257704 29.2346); outline-color: oklab(62.7966% 0.22488 0.125859); border-color: hwb(120 0% 0%); text-decoration-color: lab(29.2345% 39.3825 20.0664); column-rule-color: lch(29.2345% 44.2 27); }</style></svg>";
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
    fn non_color_css_plain_values_are_ignored() -> Result<(), Box<dyn std::error::Error>> {
        let src =
            br"<svg><style>rect { display: block; animation-name: red; fill: red; }</style></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].kind, ColorKind::Named);
        let color_text = std::str::from_utf8(&src[colors[0].byte_range.clone()])?;
        assert_eq!(color_text, "red");
        Ok(())
    }

    #[test]
    fn style_color_byte_range_and_position_are_absolute() -> Result<(), Box<dyn std::error::Error>>
    {
        let src = b"<svg>\n<style>\nrect {\n  fill: red;\n}\n</style>\n</svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        let color = &colors[0];
        assert_eq!(std::str::from_utf8(&src[color.byte_range.clone()])?, "red");
        assert_eq!(color.start_row, 3);
        assert_eq!(color.start_col, 8);
        assert_eq!(color.end_row, 3);
        assert_eq!(color.end_col, 11);
        Ok(())
    }

    #[test]
    fn css_custom_properties_and_color_mix_are_resolved() -> Result<(), Box<dyn std::error::Error>>
    {
        let src = br"<svg><style>:root { --base: oklch(22.84% 0.038 283); --toolbar-bg: color-mix(in oklch, var(--base), white 8%); } rect { fill: var(--toolbar-bg); stroke: var(--base); }</style></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 4);

        let mut by_text = HashMap::new();
        for color in &colors {
            by_text.insert(std::str::from_utf8(&src[color.byte_range.clone()])?, color);
        }

        let base = parse::functional("oklch(22.84% 0.038 283)").ok_or("parse failed")?;
        let mixed = parse::mix_colors("oklch", base, 0.92, (1.0, 1.0, 1.0, 1.0), 0.08)
            .ok_or("mix failed")?;

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
        Ok(())
    }

    #[test]
    fn color_mix_with_transparent_preserves_base_hue() -> Result<(), Box<dyn std::error::Error>> {
        let src = br"<svg><style>:root { --base: oklch(22.84% 0.038 283); --panel-bg: color-mix(in oklch, var(--base) 96%, transparent); } rect { fill: var(--panel-bg); }</style></svg>";
        let colors = extract_colors(src);

        let fill_ref = colors
            .iter()
            .find(|color| {
                std::str::from_utf8(&src[color.byte_range.clone()])
                    .ok()
                    .is_some_and(|t| t == "var(--panel-bg)")
            })
            .ok_or("resolved panel color not found")?;
        let base = parse::functional("oklch(22.84% 0.038 283)").ok_or("parse failed")?;

        assert!((fill_ref.r - base.0).abs() < 0.03);
        assert!((fill_ref.g - base.1).abs() < 0.03);
        assert!((fill_ref.b - base.2).abs() < 0.03);
        assert!(fill_ref.a < 1.0);
        assert!(fill_ref.a > 0.9);
        Ok(())
    }
}
