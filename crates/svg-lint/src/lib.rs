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
        assert!(
            !diags.iter().any(|d| d.code == DiagnosticCode::InvalidChild),
            "unknown elements should not also trigger invalid child: {diags:?}"
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

    #[test]
    fn valid_generic_attribute_does_not_depend_on_bcd_catalog() {
        let src = br#"<svg><filter><feColorMatrix type="matrix"/></filter></svg>"#;
        let diags = lint(src);
        let unknown = diags
            .iter()
            .any(|d| d.code == DiagnosticCode::UnknownAttribute);
        assert!(
            !unknown,
            "valid generic attributes should not trigger unknown diagnostics: {diags:?}"
        );
    }

    #[test]
    fn html_child_inside_foreign_object_is_allowed() {
        let src = br#"
            <svg>
                <foreignObject>
                    <p xmlns="http://www.w3.org/1999/xhtml">HTML inside SVG</p>
                </foreignObject>
            </svg>
        "#;
        let diags = lint(src);
        assert!(
            diags.is_empty(),
            "foreignObject should allow foreign-namespace subtrees without SVG diagnostics: {diags:?}"
        );
    }

    #[test]
    fn missing_local_reference_definition_warns_with_attribute_name() {
        let src = br#"<svg><rect clip-path="url(#myClip)" filter="url(#myFilter)"/></svg>"#;
        let diags = lint(src);

        assert!(
            diags.iter().any(|d| {
                d.code == DiagnosticCode::MissingReferenceDefinition
                    && d.message.contains("clip-path")
                    && d.message.contains("#myClip")
            }),
            "clip-path missing definition should warn: {diags:?}"
        );
        assert!(
            diags.iter().any(|d| {
                d.code == DiagnosticCode::MissingReferenceDefinition
                    && d.message.contains("filter")
                    && d.message.contains("#myFilter")
            }),
            "filter missing definition should warn: {diags:?}"
        );
    }

    #[test]
    fn existing_local_reference_definition_does_not_warn() {
        let src =
            br#"<svg><defs><clipPath id="myClip"/></defs><rect clip-path="url(#myClip)"/></svg>"#;
        let diags = lint(src);

        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::MissingReferenceDefinition),
            "defined references should not warn: {diags:?}"
        );
    }

    #[test]
    fn suppression_comment_disables_next_line_diagnostic() {
        let src = br#"<svg>
<!-- svg-lint-disable-next-line MissingReferenceDefinition -->
<rect clip-path="url(#myClip)"/>
</svg>"#;
        let diags = lint(src);

        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::MissingReferenceDefinition),
            "next-line suppression should suppress missing reference diagnostics: {diags:?}"
        );
    }

    #[test]
    fn suppression_comment_disables_file_diagnostic() {
        let src = br#"<!-- svg-lint-disable MissingReferenceDefinition -->
<svg><rect filter="url(#myFilter)"/></svg>"#;
        let diags = lint(src);

        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::MissingReferenceDefinition),
            "file suppression should suppress missing reference diagnostics: {diags:?}"
        );
    }
}
