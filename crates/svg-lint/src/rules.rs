use crate::types::{DiagnosticCode, Severity, SvgDiagnostic};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Run all lint checks on a parsed SVG tree.
pub fn check_all(source: &[u8], tree: &Tree) -> Vec<SvgDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut id_map: HashMap<String, usize> = HashMap::new();
    walk_elements(source, tree.root_node(), &mut diagnostics, &mut id_map);
    diagnostics
}

fn walk_elements(
    source: &[u8],
    node: Node,
    diagnostics: &mut Vec<SvgDiagnostic>,
    id_map: &mut HashMap<String, usize>,
) {
    let kind = node.kind();
    if kind == "element" || kind == "svg_root_element" {
        check_element(source, node, diagnostics, id_map);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_elements(source, child, diagnostics, id_map);
    }
}

fn check_element(
    source: &[u8],
    node: Node,
    diagnostics: &mut Vec<SvgDiagnostic>,
    id_map: &mut HashMap<String, usize>,
) {
    // Find the opening tag node (start_tag or self_closing_tag)
    let mut tag_cursor = node.walk();
    let tag_node = node
        .children(&mut tag_cursor)
        .find(|c| c.kind() == "start_tag" || c.kind() == "self_closing_tag");
    let Some(tag) = tag_node else { return };

    // Extract element name from tag's `name` field
    let Some(name_node) = tag.child_by_field_name("name") else {
        return;
    };
    let name_str = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");

    // Check: Unknown element
    let Some(def) = svg_data::element(name_str) else {
        diagnostics.push(make_diag(
            name_node,
            Severity::Warning,
            DiagnosticCode::UnknownElement,
            format!("Unknown SVG element: <{name_str}>"),
        ));
        return;
    };

    // Check: Deprecated element
    if def.deprecated {
        diagnostics.push(make_diag(
            name_node,
            Severity::Warning,
            DiagnosticCode::DeprecatedElement,
            format!("<{name_str}> is deprecated"),
        ));
    }

    // Check: Duplicate id
    check_duplicate_id(source, tag, diagnostics, id_map);

    // Check: Invalid children
    check_children(source, node, name_str, diagnostics);
}

fn check_duplicate_id(
    source: &[u8],
    tag: Node,
    diagnostics: &mut Vec<SvgDiagnostic>,
    id_map: &mut HashMap<String, usize>,
) {
    let mut cursor = tag.walk();
    for attr_node in tag.children(&mut cursor) {
        if attr_node.kind() != "attribute" {
            continue;
        }
        let mut attr_cursor = attr_node.walk();
        for child in attr_node.children(&mut attr_cursor) {
            if child.kind() != "id_attribute" {
                continue;
            }
            let Some(value_node) = child.child_by_field_name("value") else {
                continue;
            };
            let mut vc = value_node.walk();
            for v in value_node.children(&mut vc) {
                if v.kind() != "id_token" {
                    continue;
                }
                let id_text = std::str::from_utf8(&source[v.byte_range()]).unwrap_or("");
                if let Some(&first_row) = id_map.get(id_text) {
                    diagnostics.push(make_diag(
                        v,
                        Severity::Warning,
                        DiagnosticCode::DuplicateId,
                        format!(
                            "Duplicate id \"{id_text}\" (first on line {})",
                            first_row + 1
                        ),
                    ));
                } else {
                    id_map.insert(id_text.to_string(), v.start_position().row);
                }
            }
        }
    }
}

fn check_children(
    source: &[u8],
    parent_node: Node,
    parent_name: &str,
    diagnostics: &mut Vec<SvgDiagnostic>,
) {
    let allowed = svg_data::allowed_children(parent_name);

    let mut cursor = parent_node.walk();
    for child in parent_node.children(&mut cursor) {
        if child.kind() != "element" {
            continue;
        }
        let mut child_cursor = child.walk();
        let child_tag = child
            .children(&mut child_cursor)
            .find(|c| c.kind() == "start_tag" || c.kind() == "self_closing_tag");
        let Some(ct) = child_tag else { continue };
        let Some(cn) = ct.child_by_field_name("name") else {
            continue;
        };
        let child_name = std::str::from_utf8(&source[cn.byte_range()]).unwrap_or("");

        if !allowed.contains(&child_name) {
            diagnostics.push(make_diag(
                cn,
                Severity::Error,
                DiagnosticCode::InvalidChild,
                format!("<{child_name}> is not allowed as a child of <{parent_name}>"),
            ));
        }
    }
}

fn make_diag(
    node: Node,
    severity: Severity,
    code: DiagnosticCode,
    message: String,
) -> SvgDiagnostic {
    SvgDiagnostic {
        byte_range: node.byte_range(),
        start_row: node.start_position().row,
        start_col: node.start_position().column,
        end_row: node.end_position().row,
        end_col: node.end_position().column,
        severity,
        code,
        message,
    }
}
