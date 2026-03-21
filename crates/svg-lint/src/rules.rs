use crate::types::{DiagnosticCode, Severity, SvgDiagnostic};
use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Tree};

/// Run all lint checks on a parsed SVG tree.
pub fn check_all(source: &[u8], tree: &Tree) -> Vec<SvgDiagnostic> {
    let mut suppressions = collect_suppressions(source, tree);
    let defined_ids = collect_defined_ids(source, tree);
    let mut diagnostics = Vec::new();
    let mut seen_ids: HashMap<String, usize> = HashMap::new();
    walk_elements(
        source,
        tree.root_node(),
        &mut diagnostics,
        &mut suppressions,
        &defined_ids,
        &mut seen_ids,
        false,
    );
    diagnostics.extend(suppressions.unused_diagnostics());
    diagnostics
}

#[derive(Default)]
struct Suppressions {
    directives: Vec<SuppressionDirective>,
}

struct SuppressionDirective {
    scope: SuppressionScope,
    codes: HashSet<DiagnosticCode>,
    used_codes: HashSet<DiagnosticCode>,
    byte_range: std::ops::Range<usize>,
    start_row: usize,
    start_col: usize,
    end_row: usize,
    end_col: usize,
}

enum SuppressionScope {
    File,
    NextLine(usize),
}

impl Suppressions {
    fn suppresses(&mut self, row: usize, code: DiagnosticCode) -> bool {
        let mut suppressed = false;

        for directive in &mut self.directives {
            let applies = match directive.scope {
                SuppressionScope::File => true,
                SuppressionScope::NextLine(target_row) => target_row == row,
            };
            if applies && directive.codes.contains(&code) {
                directive.used_codes.insert(code);
                suppressed = true;
            }
        }

        suppressed
    }

    fn unused_diagnostics(&mut self) -> Vec<SvgDiagnostic> {
        let mut suppressed_unused = vec![false; self.directives.len()];

        for index in 0..self.directives.len() {
            let row = self.directives[index].start_row;

            for other_index in 0..self.directives.len() {
                if index == other_index {
                    continue;
                }

                let directive = &self.directives[other_index];
                let applies = match directive.scope {
                    SuppressionScope::File => true,
                    SuppressionScope::NextLine(target_row) => target_row == row,
                };
                if applies && directive.codes.contains(&DiagnosticCode::UnusedSuppression) {
                    suppressed_unused[index] = true;
                    self.directives[other_index]
                        .used_codes
                        .insert(DiagnosticCode::UnusedSuppression);
                }
            }
        }

        let mut diagnostics = Vec::new();

        for (index, directive) in self.directives.iter().enumerate() {
            let unused_codes: Vec<_> = directive
                .codes
                .difference(&directive.used_codes)
                .copied()
                .collect();
            if unused_codes.is_empty() {
                continue;
            }

            if suppressed_unused[index] {
                continue;
            }

            let unused_labels = unused_codes
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let message = if unused_codes.len() == 1 {
                format!("Unused suppression for {unused_labels}.")
            } else {
                format!("Unused suppressions for {unused_labels}.")
            };

            diagnostics.push(SvgDiagnostic {
                byte_range: directive.byte_range.clone(),
                start_row: directive.start_row,
                start_col: directive.start_col,
                end_row: directive.end_row,
                end_col: directive.end_col,
                severity: Severity::Warning,
                code: DiagnosticCode::UnusedSuppression,
                message,
            });
        }

        diagnostics
    }
}

fn walk_elements(
    source: &[u8],
    node: Node,
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    defined_ids: &HashSet<String>,
    seen_ids: &mut HashMap<String, usize>,
    in_foreign_content: bool,
) {
    let kind = node.kind();
    let mut child_in_foreign_content = in_foreign_content;
    if kind == "element" || kind == "svg_root_element" {
        child_in_foreign_content = check_element(
            source,
            node,
            diagnostics,
            suppressions,
            defined_ids,
            seen_ids,
            in_foreign_content,
        );
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_elements(
            source,
            child,
            diagnostics,
            suppressions,
            defined_ids,
            seen_ids,
            child_in_foreign_content,
        );
    }
}

fn check_element(
    source: &[u8],
    node: Node,
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    defined_ids: &HashSet<String>,
    seen_ids: &mut HashMap<String, usize>,
    in_foreign_content: bool,
) -> bool {
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
    let name_str = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");

    if in_foreign_content {
        return true;
    }

    // Check: Unknown element
    let Some(def) = svg_data::element(name_str) else {
        push_diag(
            diagnostics,
            suppressions,
            name_node,
            Severity::Warning,
            DiagnosticCode::UnknownElement,
            format!("Unknown SVG element: <{name_str}>"),
        );
        return false;
    };

    // Check: Deprecated element
    if def.deprecated {
        push_diag(
            diagnostics,
            suppressions,
            name_node,
            Severity::Warning,
            DiagnosticCode::DeprecatedElement,
            format!("<{name_str}> is deprecated"),
        );
    }

    // Check: Unknown/deprecated attributes
    check_attributes(source, tag, diagnostics, suppressions);

    // Check: Duplicate id
    check_duplicate_id(source, tag, diagnostics, suppressions, seen_ids);

    // Check: Missing local fragment reference definitions
    check_missing_reference_definitions(source, tag, diagnostics, suppressions, defined_ids);

    // Check: Invalid children
    check_children(source, node, name_str, diagnostics, suppressions);

    matches!(def.content_model, svg_data::ContentModel::Foreign)
}

/// Attribute name node kinds that carry the attribute name text.
const ATTR_NAME_KINDS: &[&str] = &[
    "attribute_name",
    "paint_attribute_name",
    "length_attribute_name",
    "transform_attribute_name",
    "viewbox_attribute_name",
    "preserve_aspect_ratio_attribute_name",
    "points_attribute_name",
    "d_attribute_name",
    "id_attribute_name",
    "href_attribute_name",
    "style_attribute_name",
    "functional_iri_attribute_name",
    "opacity_attribute_name",
    "class_attribute_name",
    "event_attribute_name",
];

/// XML infrastructure prefixes — skip these in attribute checks.
fn is_xml_infrastructure(name: &str) -> bool {
    name == "xmlns"
        || name.starts_with("xmlns:")
        || name.starts_with("xml:")
        || name.starts_with("xlink:")
}

fn check_attributes(
    source: &[u8],
    tag: Node,
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
) {
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
        let attr_name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
        if attr_name.is_empty() || is_xml_infrastructure(attr_name) {
            continue;
        }

        // Generic attribute names are a mixed bucket of valid SVG attributes and truly
        // unknown ones. Without a complete checked-in attribute catalog, treating a catalog
        // miss as "unknown" makes diagnostics depend on build-time BCD fetch state.
        if let Some(def) = svg_data::attribute(attr_name)
            && def.deprecated
        {
            push_diag(
                diagnostics,
                suppressions,
                name_node,
                Severity::Warning,
                DiagnosticCode::DeprecatedAttribute,
                format!("{attr_name} is deprecated"),
            );
        }
    }
}

/// Walk into an `attribute` node to find the name node.
fn find_attr_name(attr_node: Node) -> Option<Node> {
    let mut cursor = attr_node.walk();
    for child in attr_node.children(&mut cursor) {
        // Check if this child itself is a name node
        if ATTR_NAME_KINDS.contains(&child.kind()) {
            return Some(child);
        }
        // Check the child's children (typed attributes nest name inside)
        let mut inner_cursor = child.walk();
        for grandchild in child.children(&mut inner_cursor) {
            if ATTR_NAME_KINDS.contains(&grandchild.kind()) {
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
                    push_diag(
                        diagnostics,
                        suppressions,
                        v,
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

            push_diag(
                diagnostics,
                suppressions,
                node,
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

fn collect_suppressions(source: &[u8], tree: &Tree) -> Suppressions {
    let mut suppressions = Suppressions::default();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        if node.kind() != "comment" {
            return;
        }
        let Ok(text) = node.utf8_text(source) else {
            return;
        };
        let Some(comment) = strip_comment_delimiters(text) else {
            return;
        };

        if let Some(rest) = comment.strip_prefix("svg-lint-disable-next-line") {
            let codes = parse_suppression_codes(rest);
            if codes.is_empty() {
                return;
            }
            suppressions.directives.push(SuppressionDirective {
                scope: SuppressionScope::NextLine(node.end_position().row + 1),
                codes: codes.into_iter().collect(),
                used_codes: HashSet::new(),
                byte_range: node.byte_range(),
                start_row: node.start_position().row,
                start_col: node.start_position().column,
                end_row: node.end_position().row,
                end_col: node.end_position().column,
            });
            return;
        }

        if let Some(rest) = comment.strip_prefix("svg-lint-disable") {
            let codes = parse_suppression_codes(rest);
            if codes.is_empty() {
                return;
            }
            suppressions.directives.push(SuppressionDirective {
                scope: SuppressionScope::File,
                codes: codes.into_iter().collect(),
                used_codes: HashSet::new(),
                byte_range: node.byte_range(),
                start_row: node.start_position().row,
                start_col: node.start_position().column,
                end_row: node.end_position().row,
                end_col: node.end_position().column,
            });
        }
    });
    suppressions
}

fn strip_comment_delimiters(text: &str) -> Option<&str> {
    let text = text.trim();
    let text = text.strip_prefix("<!--")?;
    let text = text.strip_suffix("-->")?;
    Some(text.trim())
}

fn parse_suppression_codes(text: &str) -> Vec<DiagnosticCode> {
    let tokens: Vec<_> = text
        .split(|ch: char| ch == ',' || ch.is_ascii_whitespace())
        .filter(|token| !token.is_empty())
        .collect();

    if tokens.is_empty() || tokens.iter().any(|token| token.eq_ignore_ascii_case("all")) {
        return all_diagnostic_codes().to_vec();
    }

    tokens
        .into_iter()
        .filter_map(|token| token.parse().ok())
        .collect()
}

fn all_diagnostic_codes() -> &'static [DiagnosticCode] {
    &[
        DiagnosticCode::InvalidChild,
        DiagnosticCode::MissingRequiredAttr,
        DiagnosticCode::DeprecatedElement,
        DiagnosticCode::DeprecatedAttribute,
        DiagnosticCode::UnknownElement,
        DiagnosticCode::UnknownAttribute,
        DiagnosticCode::DuplicateId,
        DiagnosticCode::MissingReferenceDefinition,
        DiagnosticCode::UnusedSuppression,
    ]
}

fn walk_tree(cursor: &mut tree_sitter::TreeCursor<'_>, f: &mut impl FnMut(Node<'_>)) {
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

fn push_diag(
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    node: Node,
    severity: Severity,
    code: DiagnosticCode,
    message: String,
) {
    if suppressions.suppresses(node.start_position().row, code) {
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
