//! Structural SVG lint rules and suppression handling.
//!
//! This crate validates SVG trees against the generated catalog and returns
//! transport-agnostic diagnostics. A convenience `lint()` entry point handles
//! parsing internally.
//!
//! # Examples
//!
//! ```rust
//! let diagnostics = svg_lint::lint(br"<svg><banana/></svg>");
//! assert!(diagnostics.iter().any(|d| d.code == svg_lint::DiagnosticCode::UnknownElement));
//! ```

mod namespaces;
mod rules;
/// Public diagnostic data structures returned by the linter.
pub mod types;

use tree_sitter::Parser;
pub use types::{CompatFlags, DiagnosticCode, LintOptions, LintOverrides, Severity, SvgDiagnostic};

/// Parse source and lint.
///
/// # Panics
///
/// Panics if the compiled tree-sitter SVG grammar cannot be loaded.
#[must_use]
pub fn lint(source: &[u8]) -> Vec<SvgDiagnostic> {
    lint_with_options(source, LintOptions::default())
}

/// Parse source and lint with an explicit profile.
///
/// # Panics
///
/// Panics if the compiled tree-sitter SVG grammar cannot be loaded.
#[must_use]
pub fn lint_with_options(source: &[u8], options: LintOptions) -> Vec<SvgDiagnostic> {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_svg::LANGUAGE.into())
        .is_err()
    {
        panic!("SVG grammar ABI mismatch: rebuild tree-sitter-svg");
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    lint_tree_with_options(source, &tree, options, None)
}

/// Lint an already-parsed tree with optional runtime compat overrides.
#[must_use]
pub fn lint_tree(
    source: &[u8],
    tree: &tree_sitter::Tree,
    overrides: Option<&LintOverrides>,
) -> Vec<SvgDiagnostic> {
    lint_tree_with_options(source, tree, LintOptions::default(), overrides)
}

/// Lint an already-parsed tree with explicit profile options.
#[must_use]
pub fn lint_tree_with_options(
    source: &[u8],
    tree: &tree_sitter::Tree,
    options: LintOptions,
    overrides: Option<&LintOverrides>,
) -> Vec<SvgDiagnostic> {
    rules::check_all(source, tree, options, overrides)
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
        let src = br"<svg><banana/></svg>";
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
        let src = br"<svg><rect><circle/></rect></svg>";
        let diags = lint(src);
        assert!(
            diags.iter().any(|d| d.code == DiagnosticCode::InvalidChild),
            "child in void element: {diags:?}"
        );
    }

    #[test]
    fn invalid_child_wrong_category() {
        // filter only allows FilterPrimitive + Descriptive children
        let src = br"<svg><filter><rect/></filter></svg>";
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
        let src = br"<svg><rect/></svg>";
        let diags = lint(src);
        let invalid = diags.iter().any(|d| d.code == DiagnosticCode::InvalidChild);
        assert!(!invalid, "rect in svg should be valid: {diags:?}");
    }

    #[test]
    fn nested_valid_structure() {
        let src = br"<svg><g><rect/><circle/></g></svg>";
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
    fn prefixed_svg_elements_are_linted() {
        let src = br#"<svg:svg xmlns:svg="http://www.w3.org/2000/svg"><svg:banana/></svg:svg>"#;
        let diags = lint(src);

        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::UnknownElement),
            "prefixed svg elements should still be linted: {diags:?}"
        );
    }

    #[test]
    fn namespace_reset_stops_svg_linting() {
        let src = br#"<svg xmlns="http://www.w3.org/2000/svg"><g xmlns=""><rect><banana/></rect></g></svg>"#;
        let diags = lint(src);

        assert!(
            diags.is_empty(),
            "elements outside the svg namespace should be skipped: {diags:?}"
        );
    }

    #[test]
    fn nested_svg_reenters_linting_inside_foreign_content() {
        let src = br#"
            <svg>
                <foreignObject>
                    <svg xmlns="http://www.w3.org/2000/svg"><banana/></svg>
                </foreignObject>
            </svg>
        "#;
        let diags = lint(src);

        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::UnknownElement),
            "nested svg should re-enter linting: {diags:?}"
        );
    }

    #[test]
    fn prefixed_svg_elements_still_use_svg_validation_rules() {
        let src = br#"
            <svg xmlns:svg="http://www.w3.org/2000/svg">
                <svg:rect/>
                <svg:animate attributeName="width" dur="1s"/>
            </svg>
        "#;
        let diags = lint(src);

        assert!(
            diags.is_empty(),
            "prefixed svg:rect/svg:animate should lint like SVG elements: {diags:?}"
        );
    }

    #[test]
    fn unsupported_attribute_is_distinct_from_unknown() {
        let src = br##"<svg><use href="#icon" banana="1"/></svg>"##;
        let diags = lint_with_options(
            src,
            LintOptions {
                profile: svg_data::SpecSnapshotId::Svg11Rec20110816,
            },
        );

        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::UnsupportedInProfile
                    && d.message.contains("href")),
            "href should be flagged as unsupported in svg 1.1: {diags:?}"
        );
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::UnknownAttribute),
            "unknown generic attrs should stay separate from unsupported: {diags:?}"
        );
    }

    #[test]
    fn unsupported_attribute_does_not_also_emit_deprecated() {
        let src = br##"<svg><use xlink:href="#icon"/></svg>"##;
        let diags = lint_with_options(
            src,
            LintOptions {
                profile: svg_data::SpecSnapshotId::Svg2Cr20181004,
            },
        );

        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::UnsupportedInProfile),
            "xlink:href should be unsupported in svg 2: {diags:?}"
        );
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::DeprecatedAttribute),
            "unsupported attrs should not also report deprecated: {diags:?}"
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
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::UnusedSuppression),
            "used next-line suppression should not warn as unused: {diags:?}"
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
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::UnusedSuppression),
            "used file suppression should not warn as unused: {diags:?}"
        );
    }

    #[test]
    fn unused_next_line_suppression_warns() {
        let src = br#"<svg>
<!-- svg-lint-disable-next-line MissingReferenceDefinition -->
<rect fill="red"/>
</svg>"#;
        let diags = lint(src);

        assert!(
            diags.iter().any(|d| {
                d.code == DiagnosticCode::UnusedSuppression
                    && d.message.contains("MissingReferenceDefinition")
                    && d.start_row == 1
            }),
            "unused next-line suppression should warn on the comment line: {diags:?}"
        );
    }

    #[test]
    fn unused_file_suppression_warns() {
        let src = br#"<!-- svg-lint-disable MissingReferenceDefinition -->
<svg><rect fill="red"/></svg>"#;
        let diags = lint(src);

        assert!(
            diags.iter().any(|d| {
                d.code == DiagnosticCode::UnusedSuppression
                    && d.message.contains("MissingReferenceDefinition")
                    && d.start_row == 0
            }),
            "unused file suppression should warn on the comment line: {diags:?}"
        );
    }

    #[test]
    fn unused_suppression_warning_can_be_suppressed_on_next_line() {
        let src = br#"<svg>
<!-- svg-lint-disable-next-line UnusedSuppression -->
<!-- svg-lint-disable-next-line MissingReferenceDefinition -->
<rect fill="red"/>
</svg>"#;
        let diags = lint(src);

        assert!(
            diags.iter().any(|d| {
                d.code == DiagnosticCode::UnusedSuppression
                    && d.message == "Unused suppression for UnusedSuppression."
                    && d.start_row == 1
            }),
            "the first UnusedSuppression in a run should still be reported as unused: {diags:?}"
        );
        assert!(
            !diags.iter().any(|d| {
                d.code == DiagnosticCode::UnusedSuppression
                    && d.message == "Unused suppression for MissingReferenceDefinition."
                    && d.start_row == 2
            }),
            "next-line UnusedSuppression should suppress the following directive's unused warning: {diags:?}"
        );
    }

    #[test]
    fn unused_suppression_is_reported_per_unused_entry() {
        let src = br#"<svg>
<!-- svg-lint-disable-next-line UnusedSuppression -->
<!-- svg-lint-disable-next-line DuplicateId, InvalidChild, UnusedSuppression -->
<filter id="bad-filter">
  <!-- svg-lint-disable-next-line InvalidChild, UnusedSuppression -->
  <rect x="0" y="0" width="10" height="10"/>
</filter>
</svg>"#;
        let diags = lint(src);

        assert!(
            diags.iter().any(|d| {
                d.code == DiagnosticCode::UnusedSuppression
                    && d.message == "Unused suppression for UnusedSuppression."
                    && d.start_row == 1
            }),
            "the first UnusedSuppression in the run should be reported: {diags:?}"
        );
        assert!(
            !diags
                .iter()
                .any(|d| { d.code == DiagnosticCode::UnusedSuppression && d.start_row == 2 }),
            "the outer directive's unused warning should be fully suppressed by the previous comment: {diags:?}"
        );
        assert!(
            diags.iter().any(|d| {
                d.code == DiagnosticCode::UnusedSuppression
                    && d.message == "Unused suppression for UnusedSuppression."
                    && d.start_row == 4
            }),
            "the nested directive should still report its own unused UnusedSuppression entry: {diags:?}"
        );
    }

    #[test]
    fn multiline_tag_suppression_covers_attr_on_later_line() {
        let src = br##"<svg>
<defs><g id="icon"/></defs>
<!-- svg-lint-disable-next-line UnsupportedInProfile -->
<use
	href="#icon"/>
</svg>"##;
        let diags = lint_with_options(
            src,
            LintOptions {
                profile: svg_data::SpecSnapshotId::Svg11Rec20110816,
            },
        );

        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::UnsupportedInProfile),
            "directive before multiline tag should suppress attribute diagnostics on later rows: {diags:?}"
        );
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::UnusedSuppression),
            "used multiline suppression should not be reported as unused: {diags:?}"
        );
    }

    #[test]
    fn multiline_tag_suppression_does_not_reach_next_element() {
        let src = br##"<svg>
<defs><g id="icon"/></defs>
<!-- svg-lint-disable-next-line UnsupportedInProfile -->
<rect x="0" y="0" width="10" height="10"/>
<use href="#icon"/>
</svg>"##;
        let diags = lint_with_options(
            src,
            LintOptions {
                profile: svg_data::SpecSnapshotId::Svg11Rec20110816,
            },
        );

        assert!(
            diags.iter().any(|d| {
                d.code == DiagnosticCode::UnsupportedInProfile && d.message.contains("href")
            }),
            "directive should not suppress diagnostics on a different element: {diags:?}"
        );
    }

    #[test]
    fn single_line_tag_suppression_still_works() {
        let src = br#"<svg>
<!-- svg-lint-disable-next-line DeprecatedAttribute -->
<text clip="rect(0,100,100,0)">deprecated</text>
</svg>"#;
        let diags = lint(src);

        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::DeprecatedAttribute),
            "single-line tag suppression should still work: {diags:?}"
        );
    }

    #[test]
    fn multiline_tag_suppression_multiple_deprecated_attrs() {
        let src = br##"<svg>
<!-- svg-lint-disable-next-line DeprecatedAttribute -->
<text x="10" y="260"
	clip="rect(0,100,100,0)"
	font-stretch="condensed"
	xlink:href="#foo">multiple deprecated</text>
</svg>"##;
        let diags = lint(src);

        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::DeprecatedAttribute),
            "all deprecated attrs in multiline tag should be suppressed: {diags:?}"
        );
    }

    #[test]
    fn empty_overrides_preserve_catalog_behavior() -> Result<(), Box<dyn std::error::Error>> {
        let src = br"<svg><rect/></svg>";
        let without = lint(src);

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_svg::LANGUAGE.into()).ok();
        let tree = parser.parse(src, None).ok_or("parse")?;

        let overrides = LintOverrides {
            elements: std::collections::HashMap::new(),
            attributes: std::collections::HashMap::new(),
        };
        let with = lint_tree(src, &tree, Some(&overrides));
        assert_eq!(without, with, "empty overrides should match catalog");
        Ok(())
    }

    #[test]
    fn compat_deprecated_attribute_emits_diagnostic() {
        let src = br#"<svg><text clip="rect(0,100,100,0)">deprecated</text></svg>"#;
        let diags = lint(src);

        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::DeprecatedAttribute),
            "compat deprecated attrs should warn: {diags:?}"
        );
    }

    #[test]
    fn overrides_can_clear_compat_deprecation() -> Result<(), Box<dyn std::error::Error>> {
        let src = br#"<svg><text clip="rect(0,100,100,0)">deprecated</text></svg>"#;

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_svg::LANGUAGE.into()).ok();
        let tree = parser.parse(src, None).ok_or("parse")?;

        let mut attributes = std::collections::HashMap::new();
        attributes.insert(
            "clip".to_string(),
            CompatFlags {
                deprecated: false,
                experimental: false,
            },
        );
        let overrides = LintOverrides {
            elements: std::collections::HashMap::new(),
            attributes,
        };

        let diags = lint_tree(src, &tree, Some(&overrides));
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::DeprecatedAttribute),
            "runtime overrides should replace compat deprecation flags: {diags:?}"
        );
        Ok(())
    }

    #[test]
    fn overrides_can_mark_stable_elements_experimental() -> Result<(), Box<dyn std::error::Error>> {
        let src = br"<svg><rect/></svg>";

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_svg::LANGUAGE.into()).ok();
        let tree = parser.parse(src, None).ok_or("parse")?;

        let mut elements = std::collections::HashMap::new();
        elements.insert(
            "rect".to_string(),
            CompatFlags {
                deprecated: false,
                experimental: true,
            },
        );
        let overrides = LintOverrides {
            elements,
            attributes: std::collections::HashMap::new(),
        };

        let diags = lint_tree(src, &tree, Some(&overrides));
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::ExperimentalElement),
            "runtime overrides should add experimental element diagnostics: {diags:?}"
        );
        Ok(())
    }
}
