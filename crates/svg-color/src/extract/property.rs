pub(super) fn is_style_raw_text(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
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
            return tag_name(child, source).is_some_and(|name| name.eq_ignore_ascii_case("style"));
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

pub(super) fn css_function_name<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> Option<&'a str> {
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

pub(super) fn has_color_like_property(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
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

pub(super) fn declaration_primary_value_node(
    node: tree_sitter::Node<'_>,
) -> Option<tree_sitter::Node<'_>> {
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

pub(super) fn declaration_value_text<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> Option<&'a str> {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return None;
    }

    let mut start = None;
    let mut end = None;

    loop {
        let child = cursor.node();
        if child.is_named() && child.kind() != "property_name" {
            start.get_or_insert_with(|| child.start_byte());
            end = Some(child.end_byte());
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    let range = start?..end?;
    std::str::from_utf8(&source[range]).ok()
}

pub(super) fn declaration_property_name<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> Option<&'a str> {
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

pub(super) fn is_color_like_property(name: &str) -> bool {
    let has_color_suffix = name.len() >= "color".len()
        && name[name.len() - "color".len()..].eq_ignore_ascii_case("color");

    name.eq_ignore_ascii_case("fill")
        || name.eq_ignore_ascii_case("stroke")
        || name.eq_ignore_ascii_case("color")
        || has_color_suffix
        || name.starts_with("--")
}
