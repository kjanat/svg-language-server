# svg-tree

Shared tree-sitter traversal and query helpers used by the SVG workspace crates.

## API

```rust
use svg_tree::{walk_tree, deepest_node_at, find_ancestor_any, has_ancestor, child_of_kind, is_attribute_name_kind};

// Depth-first traversal of every node
let mut cursor = tree.root_node().walk();
walk_tree(&mut cursor, &mut |node| { /* visit */ });

// Find the most specific node at a byte offset
let node = deepest_node_at(&tree, byte_offset);

// Walk up the tree looking for a matching ancestor
let element = find_ancestor_any(node, &["element", "svg_root_element"]);

// Check for a specific ancestor kind
let inside_style = has_ancestor(node, "style_element");

// Find a direct child by kind
let name_node = child_of_kind(tag_node, "name");

// Check attribute name node kinds (handles typed variants)
let is_attr = is_attribute_name_kind(node.kind());
```

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg-language-server
