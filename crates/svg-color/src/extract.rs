mod property;
mod resolve;

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::OnceLock,
};

use svg_tree::walk_tree;
use tree_sitter::{Parser, Point, Tree, TreeCursor};

use crate::{
    parse,
    types::{ColorInfo, ColorKind},
};

type CustomProperties<'a> = HashMap<&'a str, &'a str>;
type CustomPropertyScopes<'a> = HashMap<CssScopeKey, CustomProperties<'a>>;
type Rgba = (f32, f32, f32, f32);
type ResolvedColor = (f32, f32, f32, f32, ColorKind);
type ColorStop = (Rgba, Option<f64>);

/// Identifies the scope a CSS custom property is declared in.
///
/// # Scope model (static syntactic heuristic, no DOM)
///
/// This crate resolves a `var()` reference purely from the parsed stylesheet
/// text; there is no document tree to consult. A reference is resolved against
/// just two property sources:
///
/// 1. the rule block the declaration physically lives in ([`Block`]), and
/// 2. the `:root` globals ([`Root`]), which fill any gaps the block leaves.
///
/// Block-local declarations win over `:root` on conflict. That is the entire
/// model. It deliberately does **not** emulate the real CSS cascade: it ignores
/// descendant inheritance (a property set on an ancestor selector is invisible
/// to a descendant rule), selector specificity, source order across rules, and
/// any cross-rule cascade between two non-`:root` blocks. The goal is a useful
/// swatch for the common `:root` + local-override pattern, not a conformant
/// resolver.
///
/// [`Root`]: CssScopeKey::Root
/// [`Block`]: CssScopeKey::Block
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum CssScopeKey {
    Root,
    Block(usize),
}

/// Extract all colors from SVG source text.
///
/// # Panics
///
/// Panics if the compiled tree-sitter SVG grammar cannot be loaded.
#[must_use]
pub fn colors(source: &[u8]) -> Vec<ColorInfo> {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_svg::LANGUAGE.into())
        .is_err()
    {
        panic!("SVG grammar ABI mismatch: rebuild tree-sitter-svg");
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    colors_from_tree(source, &tree)
}

/// Extract colors from an already-parsed tree.
///
/// # Panics
///
/// Panics if the compiled tree-sitter CSS or SVG paint grammar cannot be loaded.
#[must_use]
pub fn colors_from_tree(source: &[u8], tree: &Tree) -> Vec<ColorInfo> {
    let mut css_parser = Parser::new();
    if css_parser
        .set_language(&tree_sitter_css::LANGUAGE.into())
        .is_err()
    {
        panic!("CSS grammar ABI mismatch: rebuild tree-sitter-css");
    }

    let mut paint_parser = Parser::new();
    if paint_parser
        .set_language(&tree_sitter_svg_paint::LANGUAGE.into())
        .is_err()
    {
        panic!("SVG paint grammar ABI mismatch: rebuild tree-sitter-svg-paint");
    }

    let mut colors = Vec::new();
    let mut cursor = tree.root_node().walk();
    walk(
        &mut cursor,
        source,
        &mut css_parser,
        &mut paint_parser,
        &mut colors,
    );
    colors
}

fn walk(
    cursor: &mut TreeCursor<'_>,
    source: &[u8],
    css_parser: &mut Parser,
    paint_parser: &mut Parser,
    out: &mut Vec<ColorInfo>,
) {
    loop {
        let node = cursor.node();

        if try_extract_paint_payload(node, source, paint_parser, out) {
            // Paint attribute values are an opaque `paint_payload` token in the
            // host grammar; reparse with the injected `svg_paint` grammar.
        } else if try_extract_style_colors(node, source, css_parser, out) {
            // Style text nodes are reparsed as CSS and handled separately.
        } else if cursor.goto_first_child() {
            walk(cursor, source, css_parser, paint_parser, out);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Reparse an opaque host `paint_payload` token (the value of a paint
/// attribute such as `fill`/`stroke`/`stop-color`) with the injected
/// `svg_paint` grammar and extract any literal colors, translating byte
/// offsets and points back to host-document coordinates.
fn try_extract_paint_payload(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    paint_parser: &mut Parser,
    out: &mut Vec<ColorInfo>,
) -> bool {
    if node.kind() != "paint_payload" {
        return false;
    }

    let byte_range = node.byte_range();
    let Some(paint_source) = source.get(byte_range.clone()) else {
        return true;
    };
    let Some(tree) = paint_parser.parse(paint_source, None) else {
        return true;
    };
    if tree.root_node().has_error() {
        // The value does not cleanly parse as paint/color — e.g. `var()`
        // custom-property refs, which the paint grammar does not model and
        // recovers from by emitting stray `named_color` tokens. Surfacing
        // those would paint misleading swatches, so leave it unresolved
        // (matching the intentional opacity of attribute-level `var()`).
        return true;
    }

    let mut cursor = tree.root_node().walk();
    walk_paint(
        &mut cursor,
        paint_source,
        byte_range.start,
        node.start_position(),
        out,
    );
    true
}

fn walk_paint(
    cursor: &mut TreeCursor<'_>,
    paint_source: &[u8],
    base_byte: usize,
    base_start: Point,
    out: &mut Vec<ColorInfo>,
) {
    loop {
        let node = cursor.node();

        if let Some(info) = try_extract_paint_color(node, paint_source, base_byte, base_start) {
            out.push(info);
            // Color leaf nodes have no meaningful children to descend into.
        } else if node.kind() != "functional_color" && cursor.goto_first_child() {
            // Descend into containers (`paint_value`, `paint_server`, …) but
            // NOT into a `functional_color`: an unresolved function such as a
            // `var()` custom-property reference must not surface its nested
            // argument colors (e.g. the fallback `red`) as standalone swatches.
            walk_paint(cursor, paint_source, base_byte, base_start, out);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn try_extract_paint_color(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    base_byte: usize,
    base_start: Point,
) -> Option<ColorInfo> {
    let byte_range = node.byte_range();
    let text = std::str::from_utf8(&source[byte_range.clone()]).ok()?;

    let (r, g, b, a, kind) = match node.kind() {
        "hex_color" => {
            let (r, g, b, a) = parse::hex(text)?;
            (r, g, b, a, ColorKind::Hex)
        }
        "functional_color" => {
            // Delegate to the CSS-aware resolver so `color-mix(...)` with
            // literal operands works in attribute values, matching how the
            // `svg_paint` grammar structurally recognizes CSS Color 4/5
            // functions. Custom-property refs (`var(--x)`) intentionally fail
            // here — they only resolve inside `<style>` with a property map.
            let (r, g, b, a, _) = resolve::resolve_literal_color(text)?;
            (r, g, b, a, ColorKind::Functional)
        }
        "named_color" => resolve::parse_named_color(text)?,
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
    let scopes = collect_css_custom_properties(css_source, &tree);
    let resolved_scopes = merge_scopes(&scopes);

    let mut cursor = tree.root_node().walk();
    walk_css(
        &mut cursor,
        css_source,
        byte_range.start,
        node.start_position(),
        &resolved_scopes,
        out,
    );
    true
}

fn walk_css(
    cursor: &mut TreeCursor<'_>,
    css_source: &[u8],
    base_byte: usize,
    base_start: Point,
    resolved_scopes: &CustomPropertyScopes<'_>,
    out: &mut Vec<ColorInfo>,
) {
    loop {
        let node = cursor.node();

        if let Some(info) =
            try_extract_css_declaration(node, css_source, base_byte, base_start, resolved_scopes)
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
                resolved_scopes,
                out,
            );
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// CSS color-producing functions recognized in `<style>` leaf values. Compared
/// case-insensitively, so no per-node lowercase allocation is needed.
const COLOR_FUNCTIONS: [&str; 9] = [
    "rgb", "rgba", "hsl", "hsla", "hwb", "lab", "lch", "oklab", "oklch",
];

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
            if !COLOR_FUNCTIONS
                .iter()
                .any(|name| function.eq_ignore_ascii_case(name))
            {
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

/// Shared empty property map for declarations whose scope (and `:root`) carry no
/// custom properties. Initialized at most once; avoids allocating a throwaway
/// `HashMap` on every color declaration.
static EMPTY_CUSTOM_PROPERTIES: OnceLock<CustomProperties<'static>> = OnceLock::new();

fn try_extract_css_declaration(
    node: tree_sitter::Node<'_>,
    css_source: &[u8],
    base_byte: usize,
    base_start: Point,
    resolved_scopes: &CustomPropertyScopes<'_>,
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
    let scope = css_scope_key(node);
    // `resolved_scopes` already holds the fully merged property map for each
    // scope (block-local props overlaid on `:root` globals, local winning), so
    // this is a single borrow with no per-declaration cloning. A block that
    // declares no custom properties of its own is absent from the map; it still
    // sees `:root` globals by falling back to the `:root` scope's map.
    let scoped_properties = resolved_scopes
        .get(&scope)
        .or_else(|| resolved_scopes.get(&CssScopeKey::Root))
        .unwrap_or_else(|| EMPTY_CUSTOM_PROPERTIES.get_or_init(HashMap::new));
    let (r, g, b, a, kind) =
        resolve::resolve_css_color(value_text, scoped_properties, &mut HashSet::new())?;

    Some(build_color_info(
        (r, g, b, a),
        offset_range(value_node.byte_range(), base_byte),
        offset_point(value_node.start_position(), base_start),
        offset_point(value_node.end_position(), base_start),
        kind,
    ))
}

fn collect_css_custom_properties<'a>(
    css_source: &'a [u8],
    tree: &Tree,
) -> CustomPropertyScopes<'a> {
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
        let scope_block = css_scope_block(node);
        let scope = scope_block.map_or(CssScopeKey::Root, |block| {
            CssScopeKey::Block(block.start_byte())
        });
        properties
            .entry(scope)
            .or_insert_with(HashMap::new)
            .insert(prop_name, value_text);
        if scope != CssScopeKey::Root
            && let Some(block) = scope_block
            && block_is_root(block, css_source)
        {
            properties
                .entry(CssScopeKey::Root)
                .or_insert_with(HashMap::new)
                .insert(prop_name, value_text);
        }
    });
    properties
}

/// Precompute, once per stylesheet, the fully merged custom-property map for
/// every scope.
///
/// For each non-`:root` block scope the result is the `:root` globals overlaid
/// with that block's own declarations, with the block-local value winning on
/// conflict and `:root` filling the gaps. The `:root` scope itself resolves
/// against only its own globals. Doing this here means the per-declaration
/// resolution path is a single map lookup with no cloning.
fn merge_scopes<'a>(scopes: &CustomPropertyScopes<'a>) -> CustomPropertyScopes<'a> {
    let empty = CustomProperties::new();
    let root = scopes.get(&CssScopeKey::Root).unwrap_or(&empty);
    let mut merged = CustomPropertyScopes::with_capacity(scopes.len());
    for (&scope, local) in scopes {
        if scope == CssScopeKey::Root || root.is_empty() {
            merged.insert(scope, local.clone());
            continue;
        }
        // Start from `:root` globals, then overlay block-local props so the
        // local value wins on conflict while `:root` fills the gaps. Keys and
        // values are `&str` borrowed from the stylesheet, so these clones copy
        // references, not owned strings.
        let mut combined = root.clone();
        for (&name, &value) in local {
            combined.insert(name, value);
        }
        merged.insert(scope, combined);
    }
    merged
}

fn css_scope_key(node: tree_sitter::Node<'_>) -> CssScopeKey {
    css_scope_block(node).map_or(CssScopeKey::Root, |block| {
        CssScopeKey::Block(block.start_byte())
    })
}

fn css_scope_block(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut current = Some(node);
    while let Some(node) = current {
        if node.kind() == "block" {
            return Some(node);
        }
        current = node.parent();
    }
    None
}

/// Detect whether `block`'s owning rule set targets the document root via a
/// real `:root` pseudo-class selector.
///
/// This walks the parsed tree-sitter-css AST rather than scanning selector
/// text, so the literal `:root` inside an attribute selector
/// (`[data-x=":root"]`), a string, a comment, or a `:not(:root)` argument is
/// not mistaken for a root rule. A compound such as `:root:hover` or
/// `html:root` is intentionally *not* treated as the unconditional document
/// root: only a top-level selector that is exactly `:root` qualifies, because
/// the heuristic treats `:root` blocks as the source of global custom
/// properties.
fn block_is_root(block: tree_sitter::Node<'_>, css_source: &[u8]) -> bool {
    let Some(parent) = block.parent() else {
        return false;
    };
    if parent.kind() != "rule_set" {
        return false;
    }
    let mut cursor = parent.walk();
    if !cursor.goto_first_child() {
        return false;
    }
    loop {
        let child = cursor.node();
        if child.kind() == "selectors" {
            return selectors_target_root(child, css_source);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    false
}

/// Returns `true` when any top-level (comma-separated) selector in a
/// `selectors` node is exactly the `:root` pseudo-class.
fn selectors_target_root(selectors: tree_sitter::Node<'_>, css_source: &[u8]) -> bool {
    let mut cursor = selectors.walk();
    if !cursor.goto_first_child() {
        return false;
    }
    loop {
        let child = cursor.node();
        if child.kind() == "pseudo_class_selector" && is_bare_root_pseudo(child, css_source) {
            return true;
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    false
}

/// Returns `true` when `node` is a `pseudo_class_selector` consisting solely of
/// `:root` — a single `class_name` child whose text is `root`
/// (case-insensitive), with no leading tag/class part, nested pseudo-class, or
/// argument list. This rejects compounds like `:root:hover`, `html:root`, and
/// `:not(:root)`.
fn is_bare_root_pseudo(node: tree_sitter::Node<'_>, css_source: &[u8]) -> bool {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return false;
    }
    let mut class_name: Option<tree_sitter::Node<'_>> = None;
    loop {
        let child = cursor.node();
        if child.is_named() {
            if child.kind() != "class_name" || class_name.is_some() {
                // A tag_name, nested pseudo_class_selector, arguments node, or a
                // second named child means this is a compound, not a bare :root.
                return false;
            }
            class_name = Some(child);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    class_name
        .and_then(|name| std::str::from_utf8(&css_source[name.byte_range()]).ok())
        .is_some_and(|text| text.eq_ignore_ascii_case("root"))
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
    fn color_mix_in_attribute_with_literal_operands() {
        // CSS Color 5 color-mix() directly in an SVG attribute. Both operands
        // are literal colors (no `var()` refs), so no custom-property context
        // is needed. The underlying grammar parses this as a single
        // functional_color node since 85121c6.
        let src = b"<svg><rect fill=\"color-mix(in srgb, red, blue)\"/></svg>";
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1, "expected 1 color, got {colors:?}");
        assert_eq!(colors[0].kind, ColorKind::Functional);
        // 50/50 mix of red and blue in sRGB is (0.5, 0, 0.5).
        assert!((colors[0].r - 0.5).abs() < 1e-3);
        assert!(colors[0].g.abs() < 1e-3);
        assert!((colors[0].b - 0.5).abs() < 1e-3);
    }

    #[test]
    fn color_var_in_attribute_not_resolved() {
        // `var(--brand, red)` directly in an SVG attribute has no
        // custom-property scope: the rendered color depends on whether
        // `--brand` is defined in a stylesheet the attribute can't see.
        // Resolving it to the literal `red` fallback would paint a
        // misleading swatch, so the extractor must leave it unresolved.
        // (var() only resolves inside `<style>` with a full property map.)
        let src = b"<svg><rect fill=\"var(--brand, red)\"/></svg>";
        let colors = extract_colors(src);
        assert!(
            colors.is_empty(),
            "var() in an attribute must not resolve to its fallback: {colors:?}"
        );
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
    fn css_custom_properties_are_scoped_by_rule() {
        let src = br"<svg><style>.a { --color: red; fill: var(--color); } .b { --color: blue; fill: var(--color); }</style></svg>";
        let colors = extract_colors(src);

        let mut var_refs: Vec<_> = colors
            .iter()
            .filter(|color| {
                std::str::from_utf8(&src[color.byte_range.clone()])
                    .ok()
                    .is_some_and(|text| text == "var(--color)")
            })
            .collect();
        var_refs.sort_by_key(|color| color.byte_range.start);
        assert_eq!(var_refs.len(), 2);

        let first = var_refs[0];
        assert!((first.r - 1.0).abs() < f32::EPSILON);
        assert!(first.g.abs() < f32::EPSILON);
        assert!(first.b.abs() < f32::EPSILON);
        assert_eq!(first.kind, ColorKind::Named);

        let second = var_refs[1];
        assert!(second.r.abs() < f32::EPSILON);
        assert!(second.g.abs() < f32::EPSILON);
        assert!((second.b - 1.0).abs() < f32::EPSILON);
        assert_eq!(second.kind, ColorKind::Named);
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

    #[test]
    fn local_property_overrides_root_and_root_fills_gaps() {
        // `.a` redefines `--c` locally (green) and also references `--g`, which
        // only `:root` defines (blue). The local `--c` must win over the `:root`
        // red, and the `:root` `--g` must still be visible to fill the gap.
        let src = br"<svg><style>:root { --c: red; --g: blue; } .a { --c: green; fill: var(--c); stroke: var(--g); }</style></svg>";
        let colors = extract_colors(src);

        let mut by_text = HashMap::new();
        for color in &colors {
            if let Ok(text) = std::str::from_utf8(&src[color.byte_range.clone()]) {
                by_text.insert(text, color);
            }
        }

        let fill = by_text["var(--c)"];
        // green = (0, 0.5019.., 0): local override wins over :root red.
        assert!(
            fill.r.abs() < f32::EPSILON,
            "local --c must win, got {fill:?}"
        );
        assert!(fill.g > 0.4);
        assert!(fill.b.abs() < f32::EPSILON);

        let stroke = by_text["var(--g)"];
        // blue from :root fills the gap the local block leaves.
        assert!(stroke.r.abs() < f32::EPSILON);
        assert!(stroke.g.abs() < f32::EPSILON);
        assert!(
            (stroke.b - 1.0).abs() < f32::EPSILON,
            "root --g must fill gap, got {stroke:?}"
        );
    }

    #[test]
    fn attribute_selector_containing_root_text_is_not_root() {
        // The literal `:root` lives only inside an attribute-selector string, so
        // this rule does NOT define a `:root` global. A different, genuinely
        // non-`:root` rule that references `--c` must therefore NOT resolve it.
        // The old `windows(5)` substring scan over the selector text matched the
        // `:root` inside `[data-x=":root"]`, so it leaked `--c` into the global
        // scope and this `fill` resolved to red — this test pins the AST-based
        // behavior that rejects it.
        let src =
            br#"<svg><style>[data-x=":root"] { --c: red; } .b { fill: var(--c); }</style></svg>"#;
        let colors = extract_colors(src);

        let leaked = colors.iter().any(|color| {
            std::str::from_utf8(&src[color.byte_range.clone()])
                .ok()
                .is_some_and(|text| text == "var(--c)")
        });
        assert!(
            !leaked,
            ":root inside an attribute string must not create a global; got {colors:?}"
        );
    }

    #[test]
    fn cross_rule_non_root_var_does_not_resolve() {
        // Known limitation, codified: `--c` is defined in `.a` but referenced
        // from a *different* non-`:root` rule `.b`. The heuristic only looks at
        // the reference's own block plus `:root`, so this must NOT resolve.
        let src = br"<svg><style>.a { --c: red; } .b { fill: var(--c); }</style></svg>";
        let colors = extract_colors(src);

        let resolved = colors.iter().any(|color| {
            std::str::from_utf8(&src[color.byte_range.clone()])
                .ok()
                .is_some_and(|text| text == "var(--c)")
        });
        assert!(
            !resolved,
            "cross-rule non-:root var() must stay unresolved; got {colors:?}"
        );
    }
}
