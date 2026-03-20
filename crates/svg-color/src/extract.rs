use crate::{
    named_colors, parse,
    types::{ColorInfo, ColorKind},
};
use tree_sitter::{Parser, Tree, TreeCursor};

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
    let mut colors = Vec::new();
    let mut cursor = tree.root_node().walk();
    walk(&mut cursor, source, &mut colors);
    colors
}

fn walk(cursor: &mut TreeCursor<'_>, source: &[u8], out: &mut Vec<ColorInfo>) {
    loop {
        let node = cursor.node();
        let kind = node.kind();

        if let Some(info) = try_extract(kind, node, source) {
            out.push(info);
            // Color leaf nodes have no meaningful children to descend into.
        } else if cursor.goto_first_child() {
            walk(cursor, source, out);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn try_extract(kind: &str, node: tree_sitter::Node<'_>, source: &[u8]) -> Option<ColorInfo> {
    let byte_range = node.byte_range();
    let text = std::str::from_utf8(&source[byte_range.clone()]).ok()?;
    let start = node.start_position();
    let end = node.end_position();

    let (r, g, b, a, color_kind) = match kind {
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

    Some(ColorInfo {
        r,
        g,
        b,
        a,
        byte_range,
        start_row: start.row,
        start_col: start.column,
        end_row: end.row,
        end_col: end.column,
        kind: color_kind,
    })
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
}
