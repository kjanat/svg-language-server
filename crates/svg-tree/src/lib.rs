//! Shared tree-sitter traversal and query helpers.
//!
//! This crate provides the small set of tree-sitter utility functions used by
//! multiple workspace crates (`svg-references`, `svg-lint`, `svg-color`,
//! `svg-language-server`).  Centralising them here eliminates duplicated
//! implementations and keeps the helpers in sync.
//!
//! # Examples
//!
//! ```rust
//! assert!(svg_tree::is_attribute_name_kind("attribute_name"));
//! assert!(svg_tree::is_attribute_name_kind("viewBox_attribute_name"));
//! assert!(!svg_tree::is_attribute_name_kind("element"));
//! ```

/// Iterative depth-first traversal of every node reachable from `cursor`.
///
/// Calls `f` on each node visited.  Uses `TreeCursor` internally for
/// efficiency (no per-node allocation).  Iterative traversal to avoid stack
/// overflow from deeply-nested SVG trees.
pub fn walk_tree(
    cursor: &mut tree_sitter::TreeCursor<'_>,
    f: &mut impl FnMut(tree_sitter::Node<'_>),
) {
    let start_depth = cursor.depth();
    loop {
        f(cursor.node());

        // Try descending into first child.
        if cursor.goto_first_child() {
            continue;
        }

        // No children — try next sibling, or walk up until we find one.
        // Never move to siblings at start_depth: that would leak into
        // adjacent subtrees outside the node we were given.
        loop {
            if cursor.depth() > start_depth && cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() || cursor.depth() < start_depth {
                return;
            }
        }
    }
}

/// Return the deepest (most specific) node that spans `byte_offset`.
///
/// Falls back to the tree root when no descendant covers the offset.
#[must_use]
pub fn deepest_node_at(tree: &tree_sitter::Tree, byte_offset: usize) -> tree_sitter::Node<'_> {
    tree.root_node()
        .descendant_for_byte_range(byte_offset, byte_offset)
        .unwrap_or_else(|| tree.root_node())
}

/// Walk up from `node` and return the first ancestor whose kind is in `kinds`.
///
/// The search includes `node` itself.
#[must_use]
pub fn find_ancestor_any<'a>(
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

/// Return `true` if any ancestor of `node` (exclusive) has the given `kind`.
#[must_use]
pub fn has_ancestor(node: tree_sitter::Node<'_>, kind: &str) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == kind {
            return true;
        }
        current = parent;
    }
    false
}

/// Return the first direct child of `node` whose kind matches `kind`.
#[must_use]
pub fn child_of_kind<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
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

/// Check whether a tree-sitter node kind represents an attribute name.
///
/// tree-sitter-svg emits `attribute_name` for plain attributes and
/// `*_attribute_name` variants (e.g. `viewBox_attribute_name`) as value
/// grammars expand.
#[must_use]
pub fn is_attribute_name_kind(kind: &str) -> bool {
    kind == "attribute_name" || kind.ends_with("_attribute_name")
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn parse_svg(src: &[u8]) -> Result<tree_sitter::Tree, Box<dyn std::error::Error>> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
        let tree = parser
            .parse(src, None)
            .ok_or_else(|| std::io::Error::other("parse failed"))?;
        Ok(tree)
    }

    #[test]
    fn walk_tree_visits_all_nodes() -> TestResult {
        let tree = parse_svg(br"<svg><rect/></svg>")?;
        let mut count = 0;
        let mut cursor = tree.root_node().walk();
        walk_tree(&mut cursor, &mut |_| {
            count += 1;
        });
        assert!(count > 3, "should visit multiple nodes, got {count}");
        Ok(())
    }

    #[test]
    fn deepest_node_at_finds_element() -> TestResult {
        let src = br#"<svg><rect x="1"/></svg>"#;
        let tree = parse_svg(src)?;
        let rect_pos = src.iter().position(|&b| b == b'r').ok_or("no r")?;
        let node = deepest_node_at(&tree, rect_pos);
        assert!(
            node.kind().contains("name") || node.kind().contains("rect"),
            "should find node near rect: {} at {}",
            node.kind(),
            rect_pos
        );
        Ok(())
    }

    #[test]
    fn deepest_node_at_falls_back_to_root() -> TestResult {
        let tree = parse_svg(br"<svg/>")?;
        let node = deepest_node_at(&tree, 9999);
        assert_eq!(node.kind(), tree.root_node().kind());
        Ok(())
    }

    #[test]
    fn find_ancestor_any_finds_matching_kind() -> TestResult {
        let src = br"<svg><g><rect/></g></svg>";
        let tree = parse_svg(src)?;
        let rect_pos = src.iter().position(|&b| b == b'r').ok_or("no r")?;
        let node = deepest_node_at(&tree, rect_pos);
        let ancestor = find_ancestor_any(node, &["element", "svg_root_element"]);
        assert!(ancestor.is_some(), "should find element ancestor");
        Ok(())
    }

    #[test]
    fn has_ancestor_detects_parent() -> TestResult {
        let src = br"<svg><g><rect/></g></svg>";
        let tree = parse_svg(src)?;
        let rect_pos = src.iter().position(|&b| b == b'r').ok_or("no r")?;
        let node = deepest_node_at(&tree, rect_pos);
        assert!(has_ancestor(node, "element") || has_ancestor(node, "svg_root_element"));
        Ok(())
    }

    #[test]
    fn is_attribute_name_kind_matches() {
        assert!(is_attribute_name_kind("attribute_name"));
        assert!(is_attribute_name_kind("viewBox_attribute_name"));
        assert!(!is_attribute_name_kind("attribute_value"));
        assert!(!is_attribute_name_kind("element"));
    }

    #[test]
    fn iri_reference_node_exists_in_url_attributes() -> TestResult {
        let src = br#"<svg><rect fill="url(#grad)"/></svg>"#;
        let tree = parse_svg(src)?;
        let mut found_iri = false;
        let mut cursor = tree.root_node().walk();
        walk_tree(&mut cursor, &mut |node| {
            if node.kind() == "iri_reference" {
                found_iri = true;
            }
        });
        assert!(
            found_iri,
            "grammar should produce iri_reference for url(#...)"
        );
        Ok(())
    }

    #[test]
    fn typed_attribute_value_kinds_are_produced() -> TestResult {
        let src = br#"<svg><animate dur="2s"/><line stroke-dasharray="10 5"/></svg>"#;
        let tree = parse_svg(src)?;
        let mut kinds = Vec::new();
        let mut cursor = tree.root_node().walk();
        walk_tree(&mut cursor, &mut |node| {
            let kind = node.kind();
            if kind.ends_with("_attribute_value") && kind != "quoted_attribute_value" {
                kinds.push(kind.to_owned());
            }
        });
        assert!(
            kinds.iter().any(|k| k == "duration_attribute_value"),
            "dur should produce duration_attribute_value: {kinds:?}"
        );
        assert!(
            kinds
                .iter()
                .any(|k| k == "stroke_dasharray_attribute_value"),
            "stroke-dasharray should produce stroke_dasharray_attribute_value: {kinds:?}"
        );
        Ok(())
    }
}
