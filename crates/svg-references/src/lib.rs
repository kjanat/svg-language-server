//! Reference and definition helpers for SVG ids, CSS classes, and custom
//! properties.
//!
//! # Examples
//!
//! ```rust
//! let defs = svg_references::collect_class_definitions_from_stylesheet(
//!     ".icon { fill: red; }",
//!     0,
//!     0,
//! );
//! assert_eq!(defs.len(), 1);
//! assert_eq!(defs[0].name, "icon");
//! ```

use svg_tree::{child_of_kind, deepest_node_at, find_ancestor_any, has_ancestor, walk_tree};
use tree_sitter::{Parser, Tree};

/// A reference target that can be resolved to one or more definitions.
///
/// # Examples
///
/// ```rust
/// let target = svg_references::DefinitionTarget::Id("clip".to_owned());
/// assert_eq!(target, svg_references::DefinitionTarget::Id("clip".to_owned()));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionTarget {
    /// An `id` reference such as `url(#clip)` or `href="#id"`.
    Id(String),
    /// A CSS class selector reference.
    Class(String),
    /// A CSS custom-property reference such as `var(--accent)`.
    CustomProperty(String),
}

/// A zero-based source span.
///
/// # Examples
///
/// ```rust
/// let span = svg_references::Span {
///     start_row: 1,
///     start_col: 2,
///     end_row: 1,
///     end_col: 6,
/// };
/// assert_eq!(span.start_col, 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Start line.
    pub start_row: usize,
    /// Start column in bytes.
    pub start_col: usize,
    /// End line.
    pub end_row: usize,
    /// End column in bytes.
    pub end_col: usize,
}

/// A named symbol paired with its source span.
///
/// # Examples
///
/// ```rust
/// let symbol = svg_references::NamedSpan {
///     name: "icon".to_owned(),
///     span: svg_references::Span {
///         start_row: 0,
///         start_col: 9,
///         end_row: 0,
///         end_col: 13,
///     },
/// };
/// assert_eq!(symbol.name, "icon");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedSpan {
    /// Symbol name.
    pub name: String,
    /// Definition span.
    pub span: Span,
}

/// Inline stylesheet content extracted from an SVG document.
///
/// # Examples
///
/// ```rust
/// let stylesheet = svg_references::InlineStylesheet {
///     css: ".icon { fill: red; }".to_owned(),
///     start_byte: 12,
///     start_row: 0,
///     start_col: 12,
/// };
/// assert!(stylesheet.css.contains("fill"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineStylesheet {
    /// Raw CSS source.
    pub css: String,
    /// Byte offset where the stylesheet starts in the SVG source.
    pub start_byte: usize,
    /// Start line in the SVG source.
    pub start_row: usize,
    /// Start column in bytes within `start_row`.
    pub start_col: usize,
}

fn css_tree(css: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_css::LANGUAGE.into())
        .ok()?;
    parser.parse(css.as_bytes(), None)
}

#[must_use]
/// Resolve the definition target under the given SVG byte offset.
///
/// # Examples
///
/// ```rust
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = br##"<svg><clipPath id="clip"/><rect clip-path="url(#clip)"/></svg>"##;
/// let mut parser = tree_sitter::Parser::new();
/// parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
/// let tree = parser
///     .parse(source, None)
///     .ok_or_else(|| std::io::Error::other("parse failed"))?;
/// let offset = source.windows(5).position(|window| window == b"#clip").ok_or("missing ref")? + 1;
///
/// let target = svg_references::definition_target_at(source, &tree, offset);
/// assert_eq!(target, Some(svg_references::DefinitionTarget::Id("clip".to_owned())));
/// # Ok(())
/// # }
/// ```
pub fn definition_target_at(
    source: &[u8],
    tree: &Tree,
    byte_offset: usize,
) -> Option<DefinitionTarget> {
    let raw_node = deepest_node_at(tree, byte_offset);
    let node = if raw_node.is_named() {
        raw_node
    } else {
        raw_node.parent().unwrap_or(raw_node)
    };

    if let Some(class_name) = find_ancestor_any(node, &["class_name"])
        && has_ancestor(class_name, "class_attribute_value")
    {
        return Some(DefinitionTarget::Class(
            class_name.utf8_text(source).ok()?.to_owned(),
        ));
    }

    if let Some(iri) = find_ancestor_any(node, &["iri_reference"]) {
        let text = iri.utf8_text(source).ok()?;
        return text
            .strip_prefix('#')
            .map(|id| DefinitionTarget::Id(id.to_owned()));
    }

    if let Some(id_token) = find_ancestor_any(node, &["id_token"]) {
        return Some(DefinitionTarget::Id(
            id_token.utf8_text(source).ok()?.to_owned(),
        ));
    }

    if let Some(target) = paint_payload_reference_at(node, source, byte_offset) {
        return Some(target);
    }

    let stylesheet = inline_stylesheet_containing_offset(source, tree, byte_offset)?;
    definition_target_in_stylesheet(&stylesheet.css, byte_offset - stylesheet.start_byte).or_else(
        || custom_property_name_at_svg_node(node, source).map(DefinitionTarget::CustomProperty),
    )
}

fn paint_payload_reference_at(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    byte_offset: usize,
) -> Option<DefinitionTarget> {
    let payload = find_ancestor_any(node, &["paint_payload"])?;
    let relative_offset = byte_offset.checked_sub(payload.start_byte())?;
    let payload_source = source.get(payload.byte_range())?;
    if relative_offset > payload_source.len() {
        return None;
    }

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_svg_paint::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(payload_source, None)?;
    if tree.root_node().has_error() {
        return None;
    }

    let raw_node = deepest_node_at(&tree, relative_offset);
    let node = if raw_node.is_named() {
        raw_node
    } else {
        raw_node.parent().unwrap_or(raw_node)
    };

    let iri = find_ancestor_any(node, &["iri_reference"])?;
    let text = iri.utf8_text(payload_source).ok()?;
    text.strip_prefix('#')
        .map(|id| DefinitionTarget::Id(id.to_owned()))
}

fn custom_property_name_at_svg_node(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let property_name = find_ancestor_any(node, &["property_name"])?;
    let text = property_name.utf8_text(source).ok()?;
    text.starts_with("--").then(|| text.to_owned())
}

fn definition_target_in_stylesheet(css: &str, byte_offset: usize) -> Option<DefinitionTarget> {
    let tree = css_tree(css)?;
    let raw_node = deepest_node_at(&tree, byte_offset);
    let node = if raw_node.is_named() {
        raw_node
    } else {
        raw_node.parent().unwrap_or(raw_node)
    };

    if let Some(name) = css_custom_property_reference_name(node, css.as_bytes()) {
        return Some(DefinitionTarget::CustomProperty(name));
    }

    let property_name = find_ancestor_any(node, &["property_name"])?;
    let text = property_name.utf8_text(css.as_bytes()).ok()?;
    text.starts_with("--")
        .then(|| DefinitionTarget::CustomProperty(text.to_owned()))
}

fn css_custom_property_reference_name(
    node: tree_sitter::Node<'_>,
    source: &[u8],
) -> Option<String> {
    let plain_value = find_ancestor_any(node, &["plain_value"])?;
    let name = plain_value.utf8_text(source).ok()?;
    if !name.starts_with("--") {
        return None;
    }

    let call_expression = find_ancestor_any(plain_value, &["call_expression"])?;
    let function_name = child_of_kind(call_expression, "function_name")?;
    let function_name = function_name.utf8_text(source).ok()?;
    function_name
        .eq_ignore_ascii_case("var")
        .then(|| name.to_owned())
}

#[must_use]
/// Collect all `id` definitions from an SVG tree.
///
/// # Examples
///
/// ```rust
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = br#"<svg><symbol id="icon"/></svg>"#;
/// let mut parser = tree_sitter::Parser::new();
/// parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
/// let tree = parser
///     .parse(source, None)
///     .ok_or_else(|| std::io::Error::other("parse failed"))?;
///
/// let definitions = svg_references::collect_id_definitions(source, &tree);
/// assert_eq!(definitions[0].name, "icon");
/// # Ok(())
/// # }
/// ```
pub fn collect_id_definitions(source: &[u8], tree: &Tree) -> Vec<NamedSpan> {
    let mut results = Vec::new();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        if node.kind() != "id_token" {
            return;
        }
        let Some(text) = node.utf8_text(source).ok() else {
            return;
        };
        results.push(NamedSpan {
            name: text.to_owned(),
            span: span_from_node(node),
        });
    });
    results
}

#[must_use]
/// Collect inline `<style>` blocks from an SVG tree.
///
/// # Examples
///
/// ```rust
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = br"<svg><style>.icon { fill: red; }</style></svg>";
/// let mut parser = tree_sitter::Parser::new();
/// parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
/// let tree = parser
///     .parse(source, None)
///     .ok_or_else(|| std::io::Error::other("parse failed"))?;
///
/// let stylesheets = svg_references::collect_inline_stylesheets(source, &tree);
/// assert_eq!(stylesheets[0].css, ".icon { fill: red; }");
/// # Ok(())
/// # }
/// ```
pub fn collect_inline_stylesheets(source: &[u8], tree: &Tree) -> Vec<InlineStylesheet> {
    let mut stylesheets = Vec::new();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        if node.kind() != "element" {
            return;
        }
        let Some(start_tag) = child_of_kind(node, "start_tag") else {
            return;
        };
        let Some(name) = start_tag
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
        else {
            return;
        };
        if !is_style_name(name) {
            return;
        }

        collect_style_text_segments(source, node, &mut stylesheets);
    });
    stylesheets
}

#[must_use]
/// Collect CSS class definitions from a stylesheet, offset into SVG coordinates.
///
/// # Examples
///
/// ```rust
/// let definitions = svg_references::collect_class_definitions_from_stylesheet(
///     ".icon { fill: red; }",
///     4,
///     2,
/// );
/// assert_eq!(definitions[0].name, "icon");
/// assert_eq!(definitions[0].span.start_row, 4);
/// ```
pub fn collect_class_definitions_from_stylesheet(
    css: &str,
    start_row: usize,
    start_col: usize,
) -> Vec<NamedSpan> {
    let Some(tree) = css_tree(css) else {
        return Vec::new();
    };

    let mut results = Vec::new();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        if node.kind() != "class_selector" {
            return;
        }

        let mut child_cursor = node.walk();
        if !child_cursor.goto_first_child() {
            return;
        }

        loop {
            let child = child_cursor.node();
            if child.kind() == "class_name"
                && let Ok(name) = child.utf8_text(css.as_bytes())
            {
                results.push(NamedSpan {
                    name: name.to_owned(),
                    span: span_from_css_node(child, start_row, start_col),
                });
            }

            if !child_cursor.goto_next_sibling() {
                break;
            }
        }
    });
    results.sort_by_key(|definition| {
        (
            definition.span.start_row,
            definition.span.start_col,
            definition.span.end_row,
            definition.span.end_col,
        )
    });
    results
}

#[must_use]
/// Collect CSS custom-property definitions from a stylesheet, offset into SVG
/// coordinates.
///
/// # Examples
///
/// ```rust
/// let definitions = svg_references::collect_custom_property_definitions_from_stylesheet(
///     ":root { --accent: red; }",
///     2,
///     0,
/// );
/// assert_eq!(definitions[0].name, "--accent");
/// assert_eq!(definitions[0].span.start_row, 2);
/// ```
pub fn collect_custom_property_definitions_from_stylesheet(
    css: &str,
    start_row: usize,
    start_col: usize,
) -> Vec<NamedSpan> {
    let Some(tree) = css_tree(css) else {
        return Vec::new();
    };

    let mut results = Vec::new();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        if node.kind() != "declaration" {
            return;
        }
        let Some(property_name) = child_of_kind(node, "property_name") else {
            return;
        };
        let Ok(name) = property_name.utf8_text(css.as_bytes()) else {
            return;
        };
        if !name.starts_with("--") {
            return;
        }
        results.push(NamedSpan {
            name: name.to_owned(),
            span: span_from_css_node(property_name, start_row, start_col),
        });
    });
    results.sort_by_key(|definition| {
        (
            definition.span.start_row,
            definition.span.start_col,
            definition.span.end_row,
            definition.span.end_col,
        )
    });
    results
}

#[must_use]
/// Extract `href` values from `<?xml-stylesheet ...?>` processing instructions.
///
/// # Examples
///
/// ```rust
/// let hrefs = svg_references::extract_xml_stylesheet_hrefs(
///     br#"<?xml-stylesheet type="text/css" href="theme.css"?><svg/>"#,
/// );
/// assert_eq!(hrefs, ["theme.css"]);
/// ```
pub fn extract_xml_stylesheet_hrefs(source: &[u8]) -> Vec<String> {
    let mut hrefs = Vec::new();
    let Ok(text) = std::str::from_utf8(source) else {
        return hrefs;
    };

    let mut rest = text;
    while let Some(start) = rest.find("<?xml-stylesheet") {
        let after_start = &rest[start + "<?xml-stylesheet".len()..];
        let Some(end) = after_start.find("?>") else {
            break;
        };

        let attrs = parse_pi_attributes(&after_start[..end]);
        if attrs
            .get("type")
            .is_none_or(|kind| kind.eq_ignore_ascii_case("text/css"))
            && let Some(href) = attrs.get("href")
            && !href.is_empty()
        {
            hrefs.push(href.clone());
        }

        rest = &after_start[end + 2..];
    }
    hrefs
}

#[must_use]
/// Extract stylesheet `href` values from processing instructions and SVG/XHTML
/// `<link rel="stylesheet" ...>` elements.
///
/// # Examples
///
/// ```rust
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = br#"<svg><link rel="stylesheet" href="theme.css" /></svg>"#;
/// let mut parser = tree_sitter::Parser::new();
/// parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
/// let tree = parser
///     .parse(source, None)
///     .ok_or_else(|| std::io::Error::other("parse failed"))?;
///
/// assert_eq!(svg_references::extract_stylesheet_hrefs(source, &tree), ["theme.css"]);
/// # Ok(())
/// # }
/// ```
pub fn extract_stylesheet_hrefs(source: &[u8], tree: &Tree) -> Vec<String> {
    let mut hrefs = extract_xml_stylesheet_hrefs(source);
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        if !matches!(node.kind(), "start_tag" | "self_closing_tag") {
            return;
        }
        let Some(name) = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
        else {
            return;
        };
        if !is_link_name(name) {
            return;
        }
        if !tag_attribute_value(source, node, "rel").is_some_and(|rel| {
            rel.split_ascii_whitespace()
                .any(|item| item.eq_ignore_ascii_case("stylesheet"))
        }) {
            return;
        }
        if tag_attribute_value(source, node, "type")
            .is_some_and(|kind| !kind.eq_ignore_ascii_case("text/css"))
        {
            return;
        }
        if let Some(href) = tag_attribute_value(source, node, "href")
            .or_else(|| tag_attribute_value(source, node, "xlink:href"))
            && !href.is_empty()
        {
            hrefs.push(href);
        }
    });
    hrefs
}

fn inline_stylesheet_containing_offset(
    source: &[u8],
    tree: &Tree,
    byte_offset: usize,
) -> Option<InlineStylesheet> {
    collect_inline_stylesheets(source, tree)
        .into_iter()
        .find(|stylesheet| {
            let end = stylesheet.start_byte + stylesheet.css.len();
            (stylesheet.start_byte..end).contains(&byte_offset)
        })
}

fn is_style_name(name: &str) -> bool {
    name == "style" || name.ends_with(":style")
}

fn is_link_name(name: &str) -> bool {
    name == "link" || name.ends_with(":link")
}

fn collect_style_text_segments(
    source: &[u8],
    style_node: tree_sitter::Node<'_>,
    stylesheets: &mut Vec<InlineStylesheet>,
) {
    let mut cursor = style_node.walk();
    walk_tree(&mut cursor, &mut |node| {
        if !matches!(node.kind(), "raw_text" | "cdata_text") {
            return;
        }
        let Some(css) = std::str::from_utf8(&source[node.byte_range()]).ok() else {
            return;
        };
        stylesheets.push(InlineStylesheet {
            css: css.to_owned(),
            start_byte: node.start_byte(),
            start_row: node.start_position().row,
            start_col: node.start_position().column,
        });
    });
}

fn tag_attribute_value(
    source: &[u8],
    tag: tree_sitter::Node<'_>,
    target_name: &str,
) -> Option<String> {
    let mut cursor = tag.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        if let Some(value) = attribute_value(source, child, target_name) {
            return Some(value);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    None
}

fn attribute_value(
    source: &[u8],
    attribute: tree_sitter::Node<'_>,
    target_name: &str,
) -> Option<String> {
    let name = attribute
        .child_by_field_name("name")
        .and_then(|node| node.utf8_text(source).ok())?;
    if name != target_name {
        return None;
    }
    let text = attribute
        .child_by_field_name("value")
        .and_then(|node| node.utf8_text(source).ok())?;
    unquote_attribute_value(text).map(str::to_owned)
}

fn unquote_attribute_value(text: &str) -> Option<&str> {
    let quote = text.as_bytes().first().copied()?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    if text.as_bytes().last().copied()? != quote {
        return None;
    }
    text.get(1..text.len().checked_sub(1)?)
}

fn span_from_node(node: tree_sitter::Node<'_>) -> Span {
    Span {
        start_row: node.start_position().row,
        start_col: node.start_position().column,
        end_row: node.end_position().row,
        end_col: node.end_position().column,
    }
}

fn span_from_css_node(node: tree_sitter::Node<'_>, base_row: usize, base_col: usize) -> Span {
    Span {
        start_row: node.start_position().row + base_row,
        start_col: if node.start_position().row == 0 {
            node.start_position().column + base_col
        } else {
            node.start_position().column
        },
        end_row: node.end_position().row + base_row,
        end_col: if node.end_position().row == 0 {
            node.end_position().column + base_col
        } else {
            node.end_position().column
        },
    }
}

fn parse_pi_attributes(content: &str) -> std::collections::HashMap<String, String> {
    let mut attrs = std::collections::HashMap::new();
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let key_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let key = &content[key_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || !(bytes[i] == b'"' || bytes[i] == b'\'') {
            continue;
        }
        let quote = bytes[i];
        i += 1;
        let value_start = i;
        while i < bytes.len() && bytes[i] != quote {
            i += 1;
        }
        if i > value_start {
            attrs.insert(key.to_owned(), content[value_start..i].to_owned());
        }
        if i < bytes.len() {
            i += 1;
        }
    }

    attrs
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    fn parse_svg(source: &str) -> Result<Tree, Box<dyn Error>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_svg::LANGUAGE.into())
            .map_err(|e| format!("SVG grammar: {e}"))?;
        parser
            .parse(source, None)
            .ok_or_else(|| "parse returned None".into())
    }

    fn offset_of(source: &str, needle: &str) -> Result<usize, Box<dyn Error>> {
        source
            .find(needle)
            .ok_or_else(|| format!("needle {needle:?} not found").into())
    }

    #[test]
    fn definition_target_uses_clicked_class_only() -> Result<(), Box<dyn Error>> {
        let source = r#"<svg><rect class="a b c"/></svg>"#;
        let tree = parse_svg(source)?;
        let target = definition_target_at(source.as_bytes(), &tree, offset_of(source, "b c")?);
        assert_eq!(target, Some(DefinitionTarget::Class("b".into())));
        Ok(())
    }

    #[test]
    fn definition_target_ignores_class_whitespace() -> Result<(), Box<dyn Error>> {
        let source = r#"<svg><rect class="a b c"/></svg>"#;
        let tree = parse_svg(source)?;
        let offset = offset_of(source, "a b c")? + 1;
        assert_eq!(definition_target_at(source.as_bytes(), &tree, offset), None);
        Ok(())
    }

    #[test]
    fn collects_inline_styles_and_css_classes() -> Result<(), Box<dyn Error>> {
        let css_body = "{fill:red}";
        let source = format!("<svg><style>.a,.b:hover,.c.d{css_body}</style></svg>");
        let source = source.as_str();
        let tree = parse_svg(source)?;
        let styles = collect_inline_stylesheets(source.as_bytes(), &tree);
        assert_eq!(styles.len(), 1);
        let defs = collect_class_definitions_from_stylesheet(
            &styles[0].css,
            styles[0].start_row,
            styles[0].start_col,
        );
        let names: Vec<_> = defs.into_iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["a", "b", "c", "d"]);
        Ok(())
    }

    #[test]
    fn collects_cdata_inline_style_classes() -> Result<(), Box<dyn Error>> {
        let source = "<svg><style><![CDATA[.cdata { fill: red; }]]></style></svg>";
        let tree = parse_svg(source)?;
        let styles = collect_inline_stylesheets(source.as_bytes(), &tree);
        assert_eq!(styles.len(), 1);
        let defs = collect_class_definitions_from_stylesheet(
            &styles[0].css,
            styles[0].start_row,
            styles[0].start_col,
        );
        let names: Vec<_> = defs.into_iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["cdata"]);
        Ok(())
    }

    #[test]
    fn ignores_class_attribute_selector_as_definition() {
        let defs =
            collect_class_definitions_from_stylesheet("[class~='x'] .a { fill: red; }", 0, 0);
        let names: Vec<_> = defs.into_iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["a"]);
    }

    #[test]
    fn extracts_xml_stylesheet_href() {
        let source = "<?xml-stylesheet type=\"text/css\" href=\"style.css\"?><svg/>";
        assert_eq!(
            extract_xml_stylesheet_hrefs(source.as_bytes()),
            vec!["style.css".to_owned()]
        );
    }

    #[test]
    fn extracts_link_stylesheet_href() -> Result<(), Box<dyn Error>> {
        let source = "<svg><link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\" /></svg>";
        let tree = parse_svg(source)?;
        assert_eq!(
            extract_stylesheet_hrefs(source.as_bytes(), &tree),
            vec!["style.css".to_owned()]
        );
        Ok(())
    }

    #[test]
    fn definition_target_resolves_custom_property_reference_in_style() -> Result<(), Box<dyn Error>>
    {
        let source = r"<svg><style>:root { --panel-bg: red; } .var-alpha { fill: var(--panel-bg); }</style></svg>";
        let tree = parse_svg(source)?;
        let offset = offset_of(source, "--panel-bg);")? + 2;

        assert_eq!(
            definition_target_at(source.as_bytes(), &tree, offset),
            Some(DefinitionTarget::CustomProperty("--panel-bg".into()))
        );
        Ok(())
    }

    #[test]
    fn collects_custom_property_definitions_from_stylesheet() {
        let defs = collect_custom_property_definitions_from_stylesheet(
            ":root { --panel-bg: red; --base: blue; }",
            0,
            0,
        );
        let names: Vec<_> = defs.into_iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["--panel-bg", "--base"]);
    }

    #[test]
    fn definition_target_resolves_id_from_href() -> Result<(), Box<dyn Error>> {
        let source = r##"<svg><use href="#icon"/></svg>"##;
        let tree = parse_svg(source)?;
        let offset = offset_of(source, "#icon")?;
        assert_eq!(
            definition_target_at(source.as_bytes(), &tree, offset),
            Some(DefinitionTarget::Id("icon".into()))
        );
        Ok(())
    }

    #[test]
    fn definition_target_resolves_id_from_url() -> Result<(), Box<dyn Error>> {
        let source = r#"<svg><rect clip-path="url(#clip)"/></svg>"#;
        let tree = parse_svg(source)?;
        let offset = offset_of(source, "#clip")?;
        assert_eq!(
            definition_target_at(source.as_bytes(), &tree, offset),
            Some(DefinitionTarget::Id("clip".into()))
        );
        Ok(())
    }

    #[test]
    fn definition_target_resolves_id_from_paint_payload() -> Result<(), Box<dyn Error>> {
        let source = r#"<svg><rect fill="url(#paint) blue"/></svg>"#;
        let tree = parse_svg(source)?;
        let offset = offset_of(source, "#paint")?;
        assert_eq!(
            definition_target_at(source.as_bytes(), &tree, offset),
            Some(DefinitionTarget::Id("paint".into()))
        );
        Ok(())
    }

    #[test]
    fn definition_target_returns_none_outside_reference() -> Result<(), Box<dyn Error>> {
        let source = r#"<svg><rect width="10"/></svg>"#;
        let tree = parse_svg(source)?;
        let offset = offset_of(source, "10")?;
        assert_eq!(definition_target_at(source.as_bytes(), &tree, offset), None);
        Ok(())
    }

    #[test]
    fn collect_class_definitions_handles_empty_stylesheet() {
        let defs = collect_class_definitions_from_stylesheet("", 0, 0);
        assert!(defs.is_empty());
    }
}
