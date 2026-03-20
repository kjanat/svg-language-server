mod rules;
pub mod types;

pub use types::{DiagnosticCode, Severity, SvgDiagnostic};

use tree_sitter::Parser;

/// Parse source and lint.
pub fn lint(source: &[u8]) -> Vec<SvgDiagnostic> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_svg::LANGUAGE.into())
        .expect("SVG grammar");
    let tree = parser.parse(source, None).expect("parse");
    lint_tree(source, &tree)
}

/// Lint an already-parsed tree.
pub fn lint_tree(source: &[u8], tree: &tree_sitter::Tree) -> Vec<SvgDiagnostic> {
    rules::check_all(source, tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_svg_no_diagnostics() {
        let src = br#"<svg><rect x="0" y="0" width="10" height="10"/></svg>"#;
        let diags = lint(src);
        assert!(diags.is_empty(), "valid SVG: {diags:?}");
    }

    #[test]
    fn unknown_element() {
        let src = br#"<svg><banana/></svg>"#;
        let diags = lint(src);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::UnknownElement),
            "unknown element: {diags:?}"
        );
    }

    #[test]
    fn invalid_child_in_void_element() {
        // rect is Void — no children allowed
        let src = br#"<svg><rect><circle/></rect></svg>"#;
        let diags = lint(src);
        assert!(
            diags.iter().any(|d| d.code == DiagnosticCode::InvalidChild),
            "child in void element: {diags:?}"
        );
    }

    #[test]
    fn invalid_child_wrong_category() {
        // filter only allows FilterPrimitive + Descriptive children
        let src = br#"<svg><filter><rect/></filter></svg>"#;
        let diags = lint(src);
        assert!(
            diags.iter().any(|d| d.code == DiagnosticCode::InvalidChild),
            "rect in filter: {diags:?}"
        );
    }

    #[test]
    fn duplicate_id() {
        let src = br#"<svg><rect id="a"/><rect id="a"/></svg>"#;
        let diags = lint(src);
        assert!(
            diags.iter().any(|d| d.code == DiagnosticCode::DuplicateId),
            "duplicate ids: {diags:?}"
        );
    }

    #[test]
    fn rect_in_svg_is_valid() {
        let src = br#"<svg><rect/></svg>"#;
        let diags = lint(src);
        let invalid = diags.iter().any(|d| d.code == DiagnosticCode::InvalidChild);
        assert!(!invalid, "rect in svg should be valid: {diags:?}");
    }

    #[test]
    fn nested_valid_structure() {
        let src = br#"<svg><g><rect/><circle/></g></svg>"#;
        let diags = lint(src);
        assert!(diags.is_empty(), "valid nested: {diags:?}");
    }

    #[test]
    fn unique_ids_no_diagnostic() {
        let src = br#"<svg><rect id="a"/><rect id="b"/></svg>"#;
        let diags = lint(src);
        let dup = diags.iter().any(|d| d.code == DiagnosticCode::DuplicateId);
        assert!(!dup, "unique ids should not trigger: {diags:?}");
    }
}
