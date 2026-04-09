use svg_tree::is_attribute_name_kind;
use tree_sitter::Node;

pub const SVG_NAMESPACE_URI: &str = "http://www.w3.org/2000/svg";
pub const XLINK_NAMESPACE_URI: &str = "http://www.w3.org/1999/xlink";

#[derive(Clone, Debug, Default)]
pub struct NamespaceScope<'a> {
    default_namespace: Option<&'a str>,
    prefixes: Vec<(&'a str, &'a str)>,
}

impl<'a> NamespaceScope<'a> {
    #[must_use]
    pub const fn default_namespace(&self) -> Option<&'a str> {
        self.default_namespace
    }

    pub const fn set_default_namespace(&mut self, namespace_uri: Option<&'a str>) {
        self.default_namespace = namespace_uri;
    }

    #[must_use]
    pub fn resolve_prefix(&self, prefix: &str) -> Option<&'a str> {
        self.prefixes
            .iter()
            .rev()
            .find_map(|(known_prefix, namespace_uri)| {
                (*known_prefix == prefix).then_some(*namespace_uri)
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpandedName<'a> {
    pub namespace_uri: Option<&'a str>,
    pub local_name: &'a str,
}

#[must_use]
pub fn scope_for_tag<'a>(
    source: &'a [u8],
    tag: Node,
    parent: &NamespaceScope<'a>,
) -> NamespaceScope<'a> {
    let mut scope = parent.clone();
    let mut cursor = tag.walk();
    for attr_node in tag.children(&mut cursor) {
        if attr_node.kind() != "attribute" {
            continue;
        }
        let Some(name_node) = find_attr_name(attr_node) else {
            continue;
        };
        let Ok(attr_name) = name_node.utf8_text(source) else {
            continue;
        };
        let Some(namespace_uri) = attr_value(attr_node, source) else {
            continue;
        };

        if attr_name == "xmlns" {
            scope.set_default_namespace(non_empty_namespace(namespace_uri));
            continue;
        }

        let Some(prefix) = attr_name.strip_prefix("xmlns:") else {
            continue;
        };
        if let Some(namespace_uri) = non_empty_namespace(namespace_uri) {
            scope.prefixes.push((prefix, namespace_uri));
        }
    }
    scope
}

#[must_use]
pub fn declares_default_namespace(source: &[u8], tag: Node) -> bool {
    let mut cursor = tag.walk();
    for attr_node in tag.children(&mut cursor) {
        if attr_node.kind() != "attribute" {
            continue;
        }
        let Some(name_node) = find_attr_name(attr_node) else {
            continue;
        };
        if name_node.utf8_text(source).ok() == Some("xmlns") {
            return true;
        }
    }
    false
}

#[must_use]
pub fn expand_element_name<'a>(raw_name: &'a str, scope: &NamespaceScope<'a>) -> ExpandedName<'a> {
    let (prefix, local_name) = split_qualified_name(raw_name);
    ExpandedName {
        namespace_uri: prefix
            .and_then(|qualified_prefix| scope.resolve_prefix(qualified_prefix))
            .or_else(|| scope.default_namespace()),
        local_name,
    }
}

#[must_use]
pub fn expand_attribute_name<'a>(
    raw_name: &'a str,
    scope: &NamespaceScope<'a>,
) -> ExpandedName<'a> {
    let (prefix, local_name) = split_qualified_name(raw_name);
    ExpandedName {
        namespace_uri: match prefix {
            Some("xlink") => scope.resolve_prefix("xlink").or(Some(XLINK_NAMESPACE_URI)),
            Some(other_prefix) => scope.resolve_prefix(other_prefix),
            None => None,
        },
        local_name,
    }
}

#[must_use]
pub fn split_qualified_name(raw_name: &str) -> (Option<&str>, &str) {
    match raw_name.split_once(':') {
        Some((prefix, local_name)) => (Some(prefix), local_name),
        None => (None, raw_name),
    }
}

fn find_attr_name(attr_node: Node) -> Option<Node> {
    let mut cursor = attr_node.walk();
    for child in attr_node.children(&mut cursor) {
        if is_attribute_name_kind(child.kind()) {
            return Some(child);
        }
        let mut inner_cursor = child.walk();
        for grandchild in child.children(&mut inner_cursor) {
            if is_attribute_name_kind(grandchild.kind()) {
                return Some(grandchild);
            }
        }
    }
    None
}

fn attr_value<'a>(attr_node: Node, source: &'a [u8]) -> Option<&'a str> {
    let raw_attr = attr_node.utf8_text(source).ok()?;
    let (_, raw_value) = raw_attr.split_once('=')?;
    Some(raw_value.trim_matches(|ch: char| ch == '"' || ch == '\'' || ch.is_ascii_whitespace()))
}

fn non_empty_namespace(namespace_uri: &str) -> Option<&str> {
    (!namespace_uri.is_empty()).then_some(namespace_uri)
}
