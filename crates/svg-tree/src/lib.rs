//! Shared tree-sitter traversal and query helpers.
//!
//! This crate provides the small set of tree-sitter utility functions used by
//! multiple workspace crates (`svg-references`, `svg-lint`, `svg-color`,
//! `svg-language-server`).  Centralising them here eliminates duplicated
//! implementations and keeps the helpers in sync.

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
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            // No more siblings — go up. Stop if we've returned to the
            // starting depth (finished the subtree we were given).
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
