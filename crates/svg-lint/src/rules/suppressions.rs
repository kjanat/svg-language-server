use std::collections::HashSet;

use tree_sitter::Tree;

use crate::types::{DiagnosticCode, Severity, SvgDiagnostic};

#[derive(Default)]
pub struct Suppressions {
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
    pub fn suppresses(&mut self, row: usize, code: DiagnosticCode) -> bool {
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

    pub fn unused_diagnostics(&mut self) -> Vec<SvgDiagnostic> {
        let mut diagnostics = Vec::new();
        let rows: Vec<_> = self
            .directives
            .iter()
            .map(|directive| directive.start_row)
            .collect();
        let mut suppressed_unused = vec![false; self.directives.len()];

        for (index, suppressed_flag) in suppressed_unused.iter_mut().enumerate() {
            let row = rows[index];

            for (other_index, directive) in self.directives.iter().enumerate() {
                if index == other_index {
                    continue;
                }

                let applies = match directive.scope {
                    SuppressionScope::File => true,
                    SuppressionScope::NextLine(target_row) => target_row == row,
                };
                if applies && directive.codes.contains(&DiagnosticCode::UnusedSuppression) {
                    *suppressed_flag = true;
                    break;
                }
            }
        }

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

            for unused_code in unused_codes {
                diagnostics.push(SvgDiagnostic {
                    byte_range: directive.byte_range.clone(),
                    start_row: directive.start_row,
                    start_col: directive.start_col,
                    end_row: directive.end_row,
                    end_col: directive.end_col,
                    severity: Severity::Warning,
                    code: DiagnosticCode::UnusedSuppression,
                    message: format!("Unused suppression for {unused_code}."),
                });
            }
        }

        diagnostics
    }
}

pub fn collect_suppressions(source: &[u8], tree: &Tree) -> Suppressions {
    let mut suppressions = Suppressions::default();
    let mut cursor = tree.root_node().walk();
    super::walk_tree(&mut cursor, &mut |node| {
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
        DiagnosticCode::ExperimentalElement,
        DiagnosticCode::ExperimentalAttribute,
        DiagnosticCode::UnknownElement,
        DiagnosticCode::UnknownAttribute,
        DiagnosticCode::DuplicateId,
        DiagnosticCode::MissingReferenceDefinition,
        DiagnosticCode::UnusedSuppression,
    ]
}
