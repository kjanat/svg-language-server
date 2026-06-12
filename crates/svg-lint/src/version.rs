//! Resolve the effective spec profile for a document by reading the
//! profile-declaring attributes on the root `<svg>` element — primarily
//! `version`, with `baseProfile` (tiny/basic/full) narrowing the result.
//!
//! A document that says `<svg version="1.1">` is declaring the profile
//! it targets. Treating that declaration as the authoritative profile
//! (rather than erroring against a configured default) is the whole
//! point of this module. The LSP uses [`effective_profile`] at every
//! surface that depends on a profile — lint, hover, completion — so
//! they never disagree.
//!
//! Callers that want to ignore the document (e.g. user set
//! `svg.force_profile: true`) pass `force = true` and the configured
//! default is returned unchanged.
//!
//! # Limitations
//!
//! Only the catalogued snapshots resolve: SVG 1.0 and 1.1 collapse to
//! [`SpecSnapshotId::Svg11Rec20110816`]; SVG 2 values resolve to
//! [`SpecSnapshotId::Svg2EditorsDraft`].
//!
//! `baseProfile="tiny"` / `"basic"` name the SVG Tiny / Basic editions.
//! Those editions have no catalogued [`SpecSnapshotId`] yet, so a
//! constrained-profile declaration cannot resolve to a distinct snapshot
//! and falls through to the configured profile rather than silently
//! pretending the document targets the unconstrained full profile.
//! `baseProfile="full"` (and an absent `baseProfile`) leave the
//! version-derived snapshot unchanged.
//!
//! This module performs the `version` + `baseProfile` combination
//! itself. Once `svg-data` exposes a typed snapshot-from-(version,
//! baseProfile) mapping — including catalogued Tiny / Basic snapshots —
//! [`resolve_declared_profile`] should delegate to it so the Tiny /
//! Basic cases resolve to their own snapshots instead of falling
//! through. See finding #13 for the cross-crate dependency.
//!
//! Nested `<svg>` elements are ignored. Only the outermost
//! `svg_root_element` participates.

use svg_data::SpecSnapshotId;
use svg_tree::{child_of_kind, is_attribute_name_kind};
use tree_sitter::{Node, Tree};

/// Pick the profile to lint, hover, and complete against for this
/// document.
///
/// Resolution order:
/// 1. If `force` is true, `configured` wins unconditionally.
/// 2. Else, if the root `<svg>`'s `version` (narrowed by `baseProfile`)
///    resolves to a catalogued snapshot, that snapshot wins.
/// 3. Else, `configured`.
#[must_use]
pub fn effective_profile(
    tree: &Tree,
    source: &[u8],
    configured: SpecSnapshotId,
    force: bool,
) -> SpecSnapshotId {
    if force {
        return configured;
    }
    resolve_declared_profile(tree, source).unwrap_or(configured)
}

/// Combine the root `<svg>`'s declared `version` and `baseProfile` into a
/// catalogued snapshot, or `None` when the declaration doesn't pin one.
///
/// `baseProfile="tiny"`/`"basic"` are reductive *profiles of the same SVG
/// edition the `version` names* (e.g. SVG 1.1 for `version="1.1"`), so they
/// resolve to that edition's snapshot. Resolving to the base — rather than
/// returning `None` and letting the caller fall back to the *configured*
/// default, which can be an unrelated edition (e.g. an SVG 2 default mis-linting
/// a Tiny SVG 1.1 document) — is strictly more correct. The reductive
/// element/attribute removals Tiny/Basic impose are not yet modelled as a
/// constraint set; that needs the Tiny/Basic spec data vendored, after which
/// they can be enforced the way the SVG Native profile is.
fn resolve_declared_profile(tree: &Tree, source: &[u8]) -> Option<SpecSnapshotId> {
    let version_snapshot =
        extract_declared_version(tree, source).and_then(svg_data::snapshot_for_svg_version_attr)?;
    match extract_declared_base_profile(tree, source) {
        Some(BaseProfile::Tiny | BaseProfile::Basic | BaseProfile::Full) | None => {
            Some(version_snapshot)
        }
    }
}

/// The `baseProfile` declarations SVG 1.x defines. Unknown / malformed
/// values are intentionally absent so they parse to `None` and leave the
/// version-derived snapshot untouched rather than constraining it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BaseProfile {
    /// `baseProfile="full"` — the unconstrained profile.
    Full,
    /// `baseProfile="basic"` — the SVG Basic edition.
    Basic,
    /// `baseProfile="tiny"` — the SVG Tiny edition.
    Tiny,
}

impl BaseProfile {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "full" => Some(Self::Full),
            "basic" => Some(Self::Basic),
            "tiny" => Some(Self::Tiny),
            _ => None,
        }
    }
}

/// Return the parsed `baseProfile` declaration on the outermost
/// `svg_root_element`, or `None` when it is absent or unrecognised.
fn extract_declared_base_profile(tree: &Tree, source: &[u8]) -> Option<BaseProfile> {
    extract_root_attribute(tree, source, "baseProfile").and_then(BaseProfile::parse)
}

/// Return the raw text of the `version` attribute on the outermost
/// `svg_root_element`, or `None` when the root has no `version`
/// attribute (or the tree has no SVG root at all).
///
/// The returned slice still carries whatever surrounding whitespace
/// the source had; callers downstream are expected to trim.
#[must_use]
pub fn extract_declared_version<'a>(tree: &Tree, source: &'a [u8]) -> Option<&'a str> {
    extract_root_attribute(tree, source, "version")
}

/// Return the unquoted text of `target` on the outermost
/// `svg_root_element`, or `None` when the root lacks that attribute (or
/// the tree has no SVG root). Shared by `version` and `baseProfile`
/// detection so both honour the same "outermost root only" rule.
///
/// The returned slice carries whatever surrounding whitespace the source
/// had; callers downstream are expected to trim.
fn extract_root_attribute<'a>(tree: &Tree, source: &'a [u8], target: &str) -> Option<&'a str> {
    let root_svg = child_of_kind(tree.root_node(), "svg_root_element")?;
    let tag = opening_tag(root_svg)?;
    let value_node = find_attribute_value(tag, source, target)?;
    let raw = std::str::from_utf8(&source[value_node.byte_range()]).ok()?;
    Some(strip_attribute_quotes(raw))
}

fn opening_tag(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind() == "start_tag" || child.kind() == "self_closing_tag")
}

/// Find the value node for the attribute whose name matches `target`,
/// searching the direct children of a tag. Handles both generic
/// `attribute` nodes and the typed `*_attribute` wrappers emitted by
/// tree-sitter-svg (`version_attribute`, `id_attribute`, ...).
fn find_attribute_value<'a>(tag: Node<'a>, source: &[u8], target: &str) -> Option<Node<'a>> {
    let mut cursor = tag.walk();
    for attr_node in tag.children(&mut cursor) {
        if !is_attribute_like(attr_node.kind()) {
            continue;
        }
        // Skip (don't abort) attribute-like nodes that don't yield a clean
        // name+value pair: a single malformed attribute preceding `version`
        // must not silently disable profile detection for the whole tag.
        let Some((name_node, value_node)) = attribute_name_and_value(attr_node) else {
            continue;
        };
        let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()]) else {
            continue;
        };
        if name == target {
            return Some(value_node);
        }
    }
    None
}

fn is_attribute_like(kind: &str) -> bool {
    kind == "attribute" || kind.ends_with("_attribute")
}

/// Return (name, value) nodes for an `attribute` or typed `*_attribute`
/// wrapper. Typed wrappers nest the name/value one level deeper inside
/// the value grammar's typed node.
fn attribute_name_and_value(attr_node: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
    let mut cursor = attr_node.walk();
    let mut name_node = None;
    let mut value_node = attr_node.child_by_field_name("value");
    for child in attr_node.children(&mut cursor) {
        if is_attribute_name_kind(child.kind()) {
            name_node = Some(child);
            continue;
        }
        let mut inner = child.walk();
        for grandchild in child.children(&mut inner) {
            if is_attribute_name_kind(grandchild.kind()) {
                name_node = Some(grandchild);
            }
        }
        if value_node.is_none()
            && let Some(v) = child.child_by_field_name("value")
        {
            value_node = Some(v);
        }
    }
    Some((name_node?, value_node?))
}

/// Strip matching outer quotes if present. The tree-sitter `value`
/// field sometimes includes the quotes in its byte range and sometimes
/// excludes them (typed value grammars vs. the generic path); be
/// permissive.
fn strip_attribute_quotes(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &raw[1..raw.len() - 1];
        }
    }
    raw
}

#[cfg(test)]
mod tests {
    use tree_sitter::Parser;

    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn parse(src: &[u8]) -> Result<Tree, Box<dyn std::error::Error>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_svg::LANGUAGE.into()).ok();
        parser.parse(src, None).ok_or_else(|| "parse".into())
    }

    #[test]
    fn extracts_version_attr_from_root_svg() -> TestResult {
        let src = br#"<svg version="1.1" xmlns="http://www.w3.org/2000/svg"></svg>"#;
        let tree = parse(src)?;
        assert_eq!(extract_declared_version(&tree, src), Some("1.1"));
        Ok(())
    }

    #[test]
    fn extracts_version_attr_single_quoted() -> TestResult {
        let src = br"<svg version='2.0'></svg>";
        let tree = parse(src)?;
        assert_eq!(extract_declared_version(&tree, src), Some("2.0"));
        Ok(())
    }

    #[test]
    fn returns_none_when_version_attr_missing() -> TestResult {
        let src = br#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#;
        let tree = parse(src)?;
        assert_eq!(extract_declared_version(&tree, src), None);
        Ok(())
    }

    #[test]
    fn ignores_version_attr_on_nested_svg() -> TestResult {
        // Root declares no version; nested SVG declares 1.1. Only the
        // outermost one participates, so the result must be None.
        let src = br#"<svg xmlns="http://www.w3.org/2000/svg">
            <foreignObject>
                <svg version="1.1"></svg>
            </foreignObject>
        </svg>"#;
        let tree = parse(src)?;
        assert_eq!(extract_declared_version(&tree, src), None);
        Ok(())
    }

    #[test]
    fn root_version_wins_over_nested() -> TestResult {
        let src = br#"<svg version="2.0" xmlns="http://www.w3.org/2000/svg">
            <foreignObject>
                <svg version="1.1"></svg>
            </foreignObject>
        </svg>"#;
        let tree = parse(src)?;
        assert_eq!(extract_declared_version(&tree, src), Some("2.0"));
        Ok(())
    }

    #[test]
    fn self_closing_root_still_extracts_version() -> TestResult {
        let src = br#"<svg version="1.1"/>"#;
        let tree = parse(src)?;
        assert_eq!(extract_declared_version(&tree, src), Some("1.1"));
        Ok(())
    }

    #[test]
    fn effective_profile_returns_svg11_for_version_1_1() -> TestResult {
        let src = br#"<svg version="1.1"></svg>"#;
        let tree = parse(src)?;
        let configured = SpecSnapshotId::Svg2EditorsDraft;
        assert_eq!(
            effective_profile(&tree, src, configured, false),
            SpecSnapshotId::Svg11Rec20110816
        );
        Ok(())
    }

    #[test]
    fn effective_profile_honors_force_override() -> TestResult {
        let src = br#"<svg version="1.1"></svg>"#;
        let tree = parse(src)?;
        let configured = SpecSnapshotId::Svg2EditorsDraft;
        assert_eq!(effective_profile(&tree, src, configured, true), configured);
        Ok(())
    }

    #[test]
    fn effective_profile_resolves_uncatalogued_version_to_family_base() -> TestResult {
        // SVG Tiny 1.2 is an SVG 1.x document: it resolves to the SVG 1.1 base
        // edition, NOT the unrelated configured default (here SVG 2).
        let src = br#"<svg version="1.2"></svg>"#;
        let tree = parse(src)?;
        let configured = SpecSnapshotId::Svg2EditorsDraft;
        assert_eq!(
            effective_profile(&tree, src, configured, false),
            SpecSnapshotId::Svg11Rec20110816
        );
        Ok(())
    }

    #[test]
    fn effective_profile_falls_back_on_unrecognised_major() -> TestResult {
        // A version with no usable 1.x/2.x major carries no signal, so the
        // configured profile wins.
        let src = br#"<svg version="draft"></svg>"#;
        let tree = parse(src)?;
        let configured = SpecSnapshotId::Svg2Cr20181004;
        assert_eq!(effective_profile(&tree, src, configured, false), configured);
        Ok(())
    }

    #[test]
    fn effective_profile_falls_back_on_missing_version() -> TestResult {
        let src = br#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#;
        let tree = parse(src)?;
        let configured = SpecSnapshotId::Svg2Cr20181004;
        assert_eq!(effective_profile(&tree, src, configured, false), configured);
        Ok(())
    }

    #[test]
    fn effective_profile_trims_whitespace_in_value() -> TestResult {
        let src = br#"<svg version=" 1.1 "></svg>"#;
        let tree = parse(src)?;
        let configured = SpecSnapshotId::Svg2EditorsDraft;
        assert_eq!(
            effective_profile(&tree, src, configured, false),
            SpecSnapshotId::Svg11Rec20110816
        );
        Ok(())
    }

    #[test]
    fn base_profile_full_keeps_version_snapshot() -> TestResult {
        let src = br#"<svg version="1.1" baseProfile="full"></svg>"#;
        let tree = parse(src)?;
        let configured = SpecSnapshotId::Svg2EditorsDraft;
        assert_eq!(
            effective_profile(&tree, src, configured, false),
            SpecSnapshotId::Svg11Rec20110816
        );
        Ok(())
    }

    #[test]
    fn base_profile_tiny_resolves_to_base_edition() -> TestResult {
        // SVG Tiny is a reductive profile of the edition its `version` names,
        // so a `version="1.1" baseProfile="tiny"` document resolves to the
        // SVG 1.1 snapshot (its base) rather than falling through to an
        // unrelated configured default (here SVG 2).
        let src = br#"<svg version="1.1" baseProfile="tiny"></svg>"#;
        let tree = parse(src)?;
        let configured = SpecSnapshotId::Svg2EditorsDraft;
        assert_eq!(
            effective_profile(&tree, src, configured, false),
            SpecSnapshotId::Svg11Rec20110816
        );
        Ok(())
    }

    #[test]
    fn base_profile_basic_resolves_to_base_edition() -> TestResult {
        let src = br#"<svg version="1.1" baseProfile="basic"></svg>"#;
        let tree = parse(src)?;
        let configured = SpecSnapshotId::Svg2EditorsDraft;
        assert_eq!(
            effective_profile(&tree, src, configured, false),
            SpecSnapshotId::Svg11Rec20110816
        );
        Ok(())
    }

    #[test]
    fn unrecognised_base_profile_keeps_version_snapshot() -> TestResult {
        // A garbage `baseProfile` must not constrain the version mapping.
        let src = br#"<svg version="1.1" baseProfile="garbage"></svg>"#;
        let tree = parse(src)?;
        let configured = SpecSnapshotId::Svg2EditorsDraft;
        assert_eq!(
            effective_profile(&tree, src, configured, false),
            SpecSnapshotId::Svg11Rec20110816
        );
        Ok(())
    }

    #[test]
    fn base_profile_without_resolvable_version_falls_back() -> TestResult {
        // `baseProfile` alone (no catalogued `version`) cannot pin a
        // snapshot; the configured profile wins.
        let src = br#"<svg baseProfile="tiny"></svg>"#;
        let tree = parse(src)?;
        let configured = SpecSnapshotId::Svg2Cr20181004;
        assert_eq!(effective_profile(&tree, src, configured, false), configured);
        Ok(())
    }

    #[test]
    fn base_profile_ignored_on_nested_svg() -> TestResult {
        // Only the outermost root participates: a nested `baseProfile`
        // must not constrain the root's version-derived snapshot.
        let src = br#"<svg version="1.1">
            <foreignObject>
                <svg baseProfile="tiny"></svg>
            </foreignObject>
        </svg>"#;
        let tree = parse(src)?;
        let configured = SpecSnapshotId::Svg2EditorsDraft;
        assert_eq!(
            effective_profile(&tree, src, configured, false),
            SpecSnapshotId::Svg11Rec20110816
        );
        Ok(())
    }
}
