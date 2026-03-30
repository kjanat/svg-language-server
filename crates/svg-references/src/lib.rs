//! Reference and definition helpers for SVG ids, CSS classes, and custom
//! properties.

use tree_sitter::{Parser, Tree, TreeCursor};

/// A reference target that can be resolved to one or more definitions.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedSpan {
    /// Symbol name.
    pub name: String,
    /// Definition span.
    pub span: Span,
}

/// Inline stylesheet content extracted from an SVG document.
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

    let stylesheet = inline_stylesheet_containing_offset(source, tree, byte_offset)?;
    definition_target_in_stylesheet(&stylesheet.css, byte_offset - stylesheet.start_byte).or_else(
        || custom_property_name_at_svg_node(node, source).map(DefinitionTarget::CustomProperty),
    )
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
pub fn collect_inline_stylesheets(source: &[u8], tree: &Tree) -> Vec<InlineStylesheet> {
    let mut stylesheets = Vec::new();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        if node.kind() != "element" {
            return;
        }
        let Some(raw_text) = child_of_kind(node, "raw_text") else {
            return;
        };
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

        let Some(css) = std::str::from_utf8(&source[raw_text.byte_range()]).ok() else {
            return;
        };
        stylesheets.push(InlineStylesheet {
            css: css.to_owned(),
            start_byte: raw_text.start_byte(),
            start_row: raw_text.start_position().row,
            start_col: raw_text.start_position().column,
        });
    });
    stylesheets
}

#[must_use]
/// Collect CSS class definitions from a stylesheet, offset into SVG coordinates.
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

fn walk_tree(cursor: &mut TreeCursor<'_>, f: &mut impl FnMut(tree_sitter::Node<'_>)) {
    loop {
        let node = cursor.node();
        f(node);

        if cursor.goto_first_child() {
            walk_tree(cursor, f);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
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

fn deepest_node_at(tree: &Tree, byte_offset: usize) -> tree_sitter::Node<'_> {
    tree.root_node()
        .descendant_for_byte_range(byte_offset, byte_offset)
        .unwrap_or_else(|| tree.root_node())
}

fn find_ancestor_any<'a>(
    node: tree_sitter::Node<'a>,
    kinds: &[&str],
) -> Option<tree_sitter::Node<'a>> {
    let mut current = node;
    loop {
        if kinds.contains(&current.kind()) {
            return Some(current);
        }
        current = current.parent()?;
    }
}

fn has_ancestor(node: tree_sitter::Node<'_>, kind: &str) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == kind {
            return true;
        }
        current = parent;
    }
    false
}

fn child_of_kind<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        if child.kind() == kind {
            return Some(child);
        }
        if !cursor.goto_next_sibling() {
            return None;
        }
    }
}

fn is_style_name(name: &str) -> bool {
    name == "style" || name.ends_with(":style")
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
    use super::*;

    fn parse_svg(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_svg::LANGUAGE.into())
            .expect("SVG grammar");
        parser.parse(source, None).expect("tree")
    }

    fn offset_of(source: &str, needle: &str) -> usize {
        source.find(needle).expect("needle present")
    }

    #[test]
    fn definition_target_uses_clicked_class_only() {
        let source = r#"<svg><rect class="a b c"/></svg>"#;
        let tree = parse_svg(source);
        let target = definition_target_at(source.as_bytes(), &tree, offset_of(source, "b c"));
        assert_eq!(target, Some(DefinitionTarget::Class("b".into())));
    }

    #[test]
    fn definition_target_ignores_class_whitespace() {
        let source = r#"<svg><rect class="a b c"/></svg>"#;
        let tree = parse_svg(source);
        let offset = offset_of(source, "a b c") + 1;
        assert_eq!(definition_target_at(source.as_bytes(), &tree, offset), None);
    }

    #[test]
    fn collects_inline_styles_and_css_classes() {
        let source = r#"<svg><style>.a,.b:hover,.c.d{fill:red}</style></svg>"#;
        let tree = parse_svg(source);
        let styles = collect_inline_stylesheets(source.as_bytes(), &tree);
        assert_eq!(styles.len(), 1);
        let defs = collect_class_definitions_from_stylesheet(
            &styles[0].css,
            styles[0].start_row,
            styles[0].start_col,
        );
        let names: Vec<_> = defs.into_iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["a", "b", "c", "d"]);
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
    fn definition_target_resolves_custom_property_reference_in_style() {
        let source = r#"<svg><style>:root { --panel-bg: red; } .var-alpha { fill: var(--panel-bg); }</style></svg>"#;
        let tree = parse_svg(source);
        let offset = offset_of(source, "--panel-bg);") + 2;

        assert_eq!(
            definition_target_at(source.as_bytes(), &tree, offset),
            Some(DefinitionTarget::CustomProperty("--panel-bg".into()))
        );
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
}
