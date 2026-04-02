mod suppressions;

use std::collections::{HashMap, HashSet};

use suppressions::Suppressions;
use svg_tree::{is_attribute_name_kind, walk_tree};
use tree_sitter::{Node, Tree};

use crate::types::{DiagnosticCode, LintOverrides, Severity, SvgDiagnostic};

struct LintContext<'a> {
    source: &'a [u8],
    diagnostics: Vec<SvgDiagnostic>,
    suppressions: Suppressions,
    defined_ids: HashSet<String>,
    seen_ids: HashMap<String, usize>,
    overrides: Option<&'a LintOverrides>,
}

/// Run all lint checks on a parsed SVG tree.
pub fn check_all(
    source: &[u8],
    tree: &Tree,
    overrides: Option<&LintOverrides>,
) -> Vec<SvgDiagnostic> {
    let mut ctx = LintContext {
        source,
        diagnostics: Vec::new(),
        suppressions: suppressions::collect_suppressions(source, tree),
        defined_ids: collect_defined_ids(source, tree),
        seen_ids: HashMap::new(),
        overrides,
    };
    walk_elements(&mut ctx, tree.root_node(), false);
    ctx.diagnostics
        .extend(ctx.suppressions.unused_diagnostics());
    ctx.diagnostics
}

fn walk_elements(ctx: &mut LintContext<'_>, node: Node, in_foreign_content: bool) {
    let kind = node.kind();
    let child_in_foreign_content = if kind == "element" || kind == "svg_root_element" {
        check_element(ctx, node, in_foreign_content)
    } else {
        in_foreign_content
    };
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_elements(ctx, child, child_in_foreign_content);
    }
}

fn check_element(ctx: &mut LintContext<'_>, node: Node, in_foreign_content: bool) -> bool {
    // Find the opening tag node (start_tag or self_closing_tag)
    let mut tag_cursor = node.walk();
    let tag_node = node
        .children(&mut tag_cursor)
        .find(|c| c.kind() == "start_tag" || c.kind() == "self_closing_tag");
    let Some(tag) = tag_node else {
        return in_foreign_content;
    };

    // Extract element name from tag's `name` field
    let Some(name_node) = tag.child_by_field_name("name") else {
        return in_foreign_content;
    };
    let name_str = std::str::from_utf8(&ctx.source[name_node.byte_range()]).unwrap_or("");

    if in_foreign_content {
        return true;
    }

    // Check: Unknown element
    let Some(def) = svg_data::element(name_str) else {
        push_diag(
            &mut ctx.diagnostics,
            &mut ctx.suppressions,
            name_node,
            Severity::Warning,
            DiagnosticCode::UnknownElement,
            format!("Unknown SVG element: <{name_str}>"),
        );
        return false;
    };

    // Check: Deprecated element
    let el_flags = ctx.overrides.and_then(|o| o.elements.get(name_str));
    let deprecated = el_flags.map_or(def.deprecated, |f| f.deprecated);
    if deprecated {
        push_diag(
            &mut ctx.diagnostics,
            &mut ctx.suppressions,
            name_node,
            Severity::Warning,
            DiagnosticCode::DeprecatedElement,
            format!("<{name_str}> is deprecated"),
        );
    }

    // Check: Experimental element
    let experimental = el_flags.map_or(def.experimental, |f| f.experimental);
    if experimental && !deprecated {
        push_diag(
            &mut ctx.diagnostics,
            &mut ctx.suppressions,
            name_node,
            Severity::Hint,
            DiagnosticCode::ExperimentalElement,
            format!("<{name_str}> is experimental"),
        );
    }

    // Check: Unknown/deprecated/experimental attributes
    check_attributes(ctx, tag);

    // Check: Duplicate id
    check_duplicate_id(
        ctx.source,
        tag,
        &mut ctx.diagnostics,
        &mut ctx.suppressions,
        &mut ctx.seen_ids,
    );

    // Check: Missing local fragment reference definitions
    check_missing_reference_definitions(
        ctx.source,
        tag,
        &mut ctx.diagnostics,
        &mut ctx.suppressions,
        &ctx.defined_ids,
    );

    // Check: Invalid children
    check_children(
        ctx.source,
        node,
        name_str,
        &mut ctx.diagnostics,
        &mut ctx.suppressions,
    );

    matches!(def.content_model, svg_data::ContentModel::Foreign)
}

/// XML infrastructure prefixes — skip these in attribute checks.
fn is_xml_infrastructure(name: &str) -> bool {
    name == "xmlns" || name.starts_with("xmlns:") || name.starts_with("xml:")
}

fn check_attributes(ctx: &mut LintContext<'_>, tag: Node) {
    let tag_start = tag.start_position().row;
    let mut cursor = tag.walk();
    for attr_node in tag.children(&mut cursor) {
        if attr_node.kind() != "attribute" {
            continue;
        }
        // Find the attribute name node inside the (possibly typed) attribute
        let name_node = find_attr_name(attr_node);
        let Some(name_node) = name_node else {
            continue;
        };
        let attr_name = std::str::from_utf8(&ctx.source[name_node.byte_range()]).unwrap_or("");
        if attr_name.is_empty() || is_xml_infrastructure(attr_name) {
            continue;
        }

        // All xlink: attributes are deprecated in SVG2.
        if attr_name.starts_with("xlink:") {
            let msg = if attr_name == "xlink:href" {
                "xlink:href is deprecated; use href instead".to_string()
            } else {
                format!("{attr_name} is deprecated")
            };
            push_diag_in_tag(
                &mut ctx.diagnostics,
                &mut ctx.suppressions,
                name_node,
                Some(tag_start),
                Severity::Warning,
                DiagnosticCode::DeprecatedAttribute,
                msg,
            );
            continue;
        }

        // Generic attribute names are a mixed bucket of valid SVG attributes and truly
        // unknown ones. Without a complete checked-in attribute catalog, treating a catalog
        // miss as "unknown" makes diagnostics depend on build-time BCD fetch state.
        if let Some(def) = svg_data::attribute(attr_name) {
            let attr_flags = ctx.overrides.and_then(|o| o.attributes.get(attr_name));
            let deprecated = attr_flags.map_or(def.deprecated, |f| f.deprecated);
            let experimental = attr_flags.map_or(def.experimental, |f| f.experimental);
            if deprecated {
                push_diag_in_tag(
                    &mut ctx.diagnostics,
                    &mut ctx.suppressions,
                    name_node,
                    Some(tag_start),
                    Severity::Warning,
                    DiagnosticCode::DeprecatedAttribute,
                    format!("{attr_name} is deprecated"),
                );
            } else if experimental {
                push_diag_in_tag(
                    &mut ctx.diagnostics,
                    &mut ctx.suppressions,
                    name_node,
                    Some(tag_start),
                    Severity::Hint,
                    DiagnosticCode::ExperimentalAttribute,
                    format!("{attr_name} is experimental"),
                );
            }
        }
    }
}

/// Walk into an `attribute` node to find the name node.
fn find_attr_name(attr_node: Node) -> Option<Node> {
    let mut cursor = attr_node.walk();
    for child in attr_node.children(&mut cursor) {
        // Check if this child itself is a name node
        if is_attribute_name_kind(child.kind()) {
            return Some(child);
        }
        // Check the child's children (typed attributes nest name inside)
        let mut inner_cursor = child.walk();
        for grandchild in child.children(&mut inner_cursor) {
            if is_attribute_name_kind(grandchild.kind()) {
                return Some(grandchild);
            }
        }
    }
    None
}

fn check_duplicate_id(
    source: &[u8],
    tag: Node,
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    seen_ids: &mut HashMap<String, usize>,
) {
    let tag_start = tag.start_position().row;
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
                if let Some(&first_row) = seen_ids.get(id_text) {
                    push_diag_in_tag(
                        diagnostics,
                        suppressions,
                        v,
                        Some(tag_start),
                        Severity::Warning,
                        DiagnosticCode::DuplicateId,
                        format!(
                            "Duplicate id \"{id_text}\" (first on line {})",
                            first_row + 1
                        ),
                    );
                } else {
                    seen_ids.insert(id_text.to_string(), v.start_position().row);
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
    suppressions: &mut Suppressions,
) {
    if svg_data::allows_foreign_children(parent_name) {
        return;
    }

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

        if svg_data::element(child_name).is_none() {
            continue;
        }

        if !allowed.contains(&child_name) {
            push_diag(
                diagnostics,
                suppressions,
                cn,
                Severity::Error,
                DiagnosticCode::InvalidChild,
                format!("<{child_name}> is not allowed as a child of <{parent_name}>"),
            );
        }
    }
}

fn check_missing_reference_definitions(
    source: &[u8],
    tag: Node,
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    defined_ids: &HashSet<String>,
) {
    let tag_start = tag.start_position().row;
    let mut cursor = tag.walk();
    for attr_node in tag.children(&mut cursor) {
        if attr_node.kind() != "attribute" {
            continue;
        }

        let Some(name_node) = find_attr_name(attr_node) else {
            continue;
        };
        let attr_name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
        if attr_name.is_empty() || is_xml_infrastructure(attr_name) {
            continue;
        }

        let mut attr_cursor = attr_node.walk();
        walk_tree(&mut attr_cursor, &mut |node| {
            if node.kind() != "iri_reference" {
                return;
            }

            let Ok(reference_text) = node.utf8_text(source) else {
                return;
            };
            let Some(id) = reference_text.strip_prefix('#') else {
                return;
            };
            if defined_ids.contains(id) {
                return;
            }

            push_diag_in_tag(
                diagnostics,
                suppressions,
                node,
                Some(tag_start),
                Severity::Warning,
                DiagnosticCode::MissingReferenceDefinition,
                format!(
                    "{attr_name} references #{id}, but no element with id=\"{id}\" exists in this SVG.\nDefine one or remove the reference."
                ),
            );
        });
    }
}

fn collect_defined_ids(source: &[u8], tree: &Tree) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        if node.kind() != "id_token" {
            return;
        }
        if let Ok(id) = node.utf8_text(source) {
            ids.insert(id.to_owned());
        }
    });
    ids
}

fn push_diag(
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    node: Node,
    severity: Severity,
    code: DiagnosticCode,
    message: String,
) {
    push_diag_in_tag(
        diagnostics,
        suppressions,
        node,
        None,
        severity,
        code,
        message,
    );
}

/// Like `push_diag` but accepts the enclosing tag's start row so that a
/// `disable-next-line` before a multiline opening tag covers all attributes.
fn push_diag_in_tag(
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    node: Node,
    tag_start_row: Option<usize>,
    severity: Severity,
    code: DiagnosticCode,
    message: String,
) {
    if suppressions.suppresses(node.start_position().row, code, tag_start_row) {
        return;
    }
    diagnostics.push(make_diag(node, severity, code, message));
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

#[cfg(test)]
mod tests {
    use std::error::Error;

    use tree_sitter::{Parser, Tree};

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

    fn first_attribute_node(tree: &Tree) -> Result<Node<'_>, Box<dyn Error>> {
        fn visit(node: Node<'_>) -> Option<Node<'_>> {
            if node.kind() == "attribute" {
                return Some(node);
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(found) = visit(child) {
                    return Some(found);
                }
            }
            None
        }

        visit(tree.root_node()).ok_or_else(|| "expected an attribute node".into())
    }

    #[test]
    fn find_attr_name_matches_new_duration_attribute_kind() -> Result<(), Box<dyn Error>> {
        let tree = parse_svg(r#"<svg><animate dur="2s" /></svg>"#)?;
        let attr = first_attribute_node(&tree)?;
        let name = find_attr_name(attr).ok_or("duration attribute name")?;
        assert_eq!(name.kind(), "duration_attribute_name");
        Ok(())
    }

    #[test]
    fn find_attr_name_matches_new_stroke_dasharray_attribute_kind() -> Result<(), Box<dyn Error>> {
        let tree = parse_svg(r#"<svg><line stroke-dasharray="10 5" /></svg>"#)?;
        let attr = first_attribute_node(&tree)?;
        let name = find_attr_name(attr).ok_or("stroke-dasharray attribute name")?;
        assert_eq!(name.kind(), "stroke_dasharray_attribute_name");
        Ok(())
    }
}
