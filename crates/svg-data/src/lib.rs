//! Structured SVG specification data.
//!
//! The catalog is generated at build time from structured data extracted from
//! the canonical SVG specification (fetched fresh by the regeneration step —
//! never from a local checkout). This crate exposes a typed, profile-aware view
//! of that data for the SVG language server and linter: element/attribute
//! lookups, content models, compatibility verdicts, and spec permalinks.

pub mod compat_parse;
pub mod edition;
pub mod inventory;
pub mod profile;
pub mod xlink;

mod catalog;
pub mod types;

pub use types::{
    AttributeApplicability, AttributeDef, AttributeElementCompat, AttributeElementValues,
    AttributeValues, BaselineQualifier, BaselineStatus, BrowserFlag, BrowserSupport,
    BrowserVersion, CatalogGraph, CatalogGraphEdge, CatalogGraphEdgeKind, CatalogGraphNode,
    CatalogGraphNodeKind, CompatFacts, CompatSubfeature, CompatSubfeatureKind, CompatVerdict,
    ContentModel, CssGrammarEdge, CssGrammarEdgeKind, CssGrammarGraph, CssGrammarNode,
    CssGrammarNodeKind, ElementCategory, ElementDef, FeatureLifecycle, ProfileLookup,
    ProfiledAttribute, ProfiledElement, SnapshotLifecycle, SnapshotMetadata, SpecLifecycle,
    SpecSnapshotId, VerdictReason, VerdictRecommendation,
};

use catalog::{
    ATTRIBUTES, CATALOG_GRAPH, COMPAT_SUBFEATURES, ELEMENTS, LIFECYCLE_OVERLAYS, SNAPSHOT_METADATA,
};

/// All snapshots the catalog tracks, oldest first.
///
/// # Examples
///
/// ```rust
/// assert!(svg_data::spec_snapshots().contains(&svg_data::SpecSnapshotId::Svg2EditorsDraft));
/// ```
#[must_use]
pub const fn spec_snapshots() -> &'static [SpecSnapshotId] {
    &[
        SpecSnapshotId::Svg11Rec20030114,
        SpecSnapshotId::Svg11Rec20110816,
        SpecSnapshotId::Svg2Cr20181004,
        SpecSnapshotId::Svg2EditorsDraft,
    ]
}

/// Look up an element definition by tag name.
///
/// # Examples
///
/// ```rust
/// let svg = svg_data::element("svg").expect("svg element");
/// assert_eq!(svg.name, "svg");
/// ```
#[must_use]
pub fn element(name: &str) -> Option<&'static ElementDef> {
    ELEMENTS.iter().find(|element| element.name == name)
}

/// Look up an attribute definition by (canonical) name.
///
/// # Examples
///
/// ```rust
/// let fill = svg_data::attribute("fill").expect("fill attribute");
/// assert_eq!(fill.name, "fill");
/// ```
#[must_use]
pub fn attribute(name: &str) -> Option<&'static AttributeDef> {
    let canonical = xlink::canonical_svg_attribute_name(name);
    attribute_by_catalog_name(canonical.as_ref())
}

fn attribute_by_catalog_name(name: &str) -> Option<&'static AttributeDef> {
    ATTRIBUTES.iter().find(|attribute| attribute.name == name)
}

/// All element definitions in the union catalog.
///
/// # Examples
///
/// ```rust
/// assert!(svg_data::elements().iter().any(|element| element.name == "svg"));
/// ```
#[must_use]
pub const fn elements() -> &'static [ElementDef] {
    ELEMENTS
}

/// All attribute definitions in the union catalog.
///
/// # Examples
///
/// ```rust
/// assert!(svg_data::attributes().iter().any(|attribute| attribute.name == "fill"));
/// ```
#[must_use]
pub const fn attributes() -> &'static [AttributeDef] {
    ATTRIBUTES
}

/// Generated per-snapshot inventories.
///
/// # Examples
///
/// ```rust
/// let _inventories = svg_data::inventories();
/// ```
#[must_use]
pub const fn inventories() -> &'static [inventory::Inventory] {
    inventory::generated()
}

/// Generated per-snapshot lifecycle overlays.
///
/// # Examples
///
/// ```rust
/// let _overlays = svg_data::lifecycle_overlays();
/// ```
#[must_use]
pub const fn lifecycle_overlays() -> &'static [SnapshotLifecycle] {
    LIFECYCLE_OVERLAYS
}

/// Derived graph view over the catalog.
///
/// # Examples
///
/// ```rust
/// let graph = svg_data::catalog_graph();
/// assert!(!graph.nodes.is_empty());
/// ```
#[must_use]
pub const fn catalog_graph() -> &'static CatalogGraph {
    &CATALOG_GRAPH
}

/// BCD subfeatures retained for behavior/value-specific diagnostics.
///
/// # Examples
///
/// ```rust
/// let _subfeatures = svg_data::compat_subfeatures();
/// ```
#[must_use]
pub const fn compat_subfeatures() -> &'static [CompatSubfeature] {
    COMPAT_SUBFEATURES
}

/// Look up one retained BCD subfeature by full compat key.
///
/// # Examples
///
/// ```rust
/// let _feature = svg_data::compat_subfeature("svg.elements.use.data_uri");
/// ```
#[must_use]
pub fn compat_subfeature(compat_key: &str) -> Option<&'static CompatSubfeature> {
    COMPAT_SUBFEATURES
        .iter()
        .find(|feature| feature.compat_key == compat_key)
}

/// Profile-aware element lookup.
///
/// # Examples
///
/// ```rust
/// let lookup = svg_data::element_for_profile(svg_data::SpecSnapshotId::Svg2EditorsDraft, "svg");
/// assert!(matches!(lookup, svg_data::ProfileLookup::Present { .. }));
/// ```
#[must_use]
pub fn element_for_profile(profile: SpecSnapshotId, name: &str) -> ProfileLookup<ElementDef> {
    if let Some(lifecycle) = element_lifecycle_for_profile(profile, name) {
        if !lifecycle.present {
            return ProfileLookup::UnsupportedInProfile {
                known_in: lifecycle.known_in,
            };
        }
        return element(name).map_or(ProfileLookup::Unknown, |value| ProfileLookup::Present {
            value,
            lifecycle: lifecycle.lifecycle,
        });
    }
    element(name).map_or(ProfileLookup::Unknown, |value| ProfileLookup::Present {
        value,
        lifecycle: SpecLifecycle::Stable,
    })
}

/// Profile-aware attribute lookup.
///
/// # Examples
///
/// ```rust
/// let lookup = svg_data::attribute_for_profile(svg_data::SpecSnapshotId::Svg2EditorsDraft, "fill");
/// assert!(matches!(lookup, svg_data::ProfileLookup::Present { .. }));
/// ```
#[must_use]
pub fn attribute_for_profile(profile: SpecSnapshotId, name: &str) -> ProfileLookup<AttributeDef> {
    if let Some(lifecycle) = attribute_lifecycle_for_profile(profile, name) {
        if !lifecycle.present {
            return ProfileLookup::UnsupportedInProfile {
                known_in: lifecycle.known_in,
            };
        }
        let catalog_name = lifecycle.catalog_name.unwrap_or(lifecycle.name);
        return attribute_by_catalog_name(catalog_name).map_or(ProfileLookup::Unknown, |value| {
            attribute_lookup_present_with_lifecycle(value, lifecycle.lifecycle)
        });
    }
    attribute(name).map_or(ProfileLookup::Unknown, attribute_lookup_present)
}

const fn attribute_lookup_present(value: &'static AttributeDef) -> ProfileLookup<AttributeDef> {
    attribute_lookup_present_with_lifecycle(value, SpecLifecycle::Stable)
}

const fn attribute_lookup_present_with_lifecycle(
    value: &'static AttributeDef,
    lifecycle: SpecLifecycle,
) -> ProfileLookup<AttributeDef> {
    if matches!(value.applicability, AttributeApplicability::None) {
        return ProfileLookup::Unknown;
    }
    ProfileLookup::Present { value, lifecycle }
}

/// Attributes that apply to `elem_name` in `profile`.
///
/// # Examples
///
/// ```rust
/// let attrs = svg_data::attributes_for_with_profile(svg_data::SpecSnapshotId::Svg2EditorsDraft, "rect");
/// assert!(attrs.iter().any(|attr| attr.name == "fill"));
/// ```
#[must_use]
pub fn attributes_for_with_profile(
    profile: SpecSnapshotId,
    elem_name: &str,
) -> Vec<ProfiledAttribute> {
    let Some(element) = element(elem_name) else {
        return Vec::new();
    };
    ATTRIBUTES
        .iter()
        .filter(|attribute| {
            attribute
                .applicability
                .includes(elem_name, element.global_attrs)
        })
        .filter_map(|attribute| {
            let (name, lifecycle) = attribute_profile_name_and_lifecycle(profile, attribute.name)?;
            Some(ProfiledAttribute {
                name,
                attribute,
                lifecycle,
            })
        })
        .collect()
}

/// Concrete child elements allowed inside `parent` in `profile`.
///
/// # Examples
///
/// ```rust
/// let children = svg_data::allowed_children_with_profile(svg_data::SpecSnapshotId::Svg2EditorsDraft, "svg");
/// assert!(children.iter().any(|child| child.element.name == "g"));
/// ```
#[must_use]
pub fn allowed_children_with_profile(
    profile: SpecSnapshotId,
    parent_name: &str,
) -> Vec<ProfiledElement> {
    let _ = profile;
    let Some(parent) = element(parent_name) else {
        return Vec::new();
    };
    allowed_child_names(&parent.content_model)
        .into_iter()
        .filter(|name| {
            element_lifecycle_for_profile(profile, name).is_none_or(|lifecycle| lifecycle.present)
        })
        .filter_map(element)
        .map(|element| ProfiledElement {
            element,
            lifecycle: element_lifecycle_for_profile(profile, element.name)
                .map_or(SpecLifecycle::Stable, |lifecycle| lifecycle.lifecycle),
        })
        .collect()
}

fn element_lifecycle_for_profile(
    profile: SpecSnapshotId,
    name: &str,
) -> Option<&'static FeatureLifecycle> {
    lifecycle_overlay(profile)?
        .elements
        .iter()
        .find(|entry| entry.name == name)
}

fn attribute_lifecycle_for_profile(
    profile: SpecSnapshotId,
    name: &str,
) -> Option<&'static FeatureLifecycle> {
    lifecycle_overlay(profile)?.attributes.iter().find(|entry| {
        entry.name == name
            || entry
                .catalog_name
                .is_some_and(|catalog_name| catalog_name == name && entry.present)
    })
}

fn attribute_profile_name_and_lifecycle(
    profile: SpecSnapshotId,
    catalog_name: &'static str,
) -> Option<(&'static str, SpecLifecycle)> {
    if let Some(entry) = lifecycle_overlay(profile).and_then(|overlay| {
        overlay.attributes.iter().find(|entry| {
            entry.present
                && (entry.name == catalog_name || entry.catalog_name == Some(catalog_name))
        })
    }) {
        return Some((entry.name, entry.lifecycle));
    }
    if lifecycle_overlay(profile).is_some_and(|overlay| {
        overlay
            .attributes
            .iter()
            .any(|entry| !entry.present && entry.name == catalog_name)
    }) {
        return None;
    }
    Some((catalog_name, SpecLifecycle::Stable))
}

fn lifecycle_overlay(profile: SpecSnapshotId) -> Option<&'static SnapshotLifecycle> {
    LIFECYCLE_OVERLAYS
        .iter()
        .find(|overlay| overlay.snapshot == profile)
}

/// Whether `parent` hosts foreign-namespace (e.g. HTML) children.
///
/// # Examples
///
/// ```rust
/// assert!(svg_data::allows_foreign_children("foreignObject"));
/// ```
#[must_use]
pub fn allows_foreign_children(parent_name: &str) -> bool {
    element(parent_name)
        .is_some_and(|element| matches!(element.content_model, ContentModel::Foreign))
}

/// The compat verdict for an element in a profile, when one was derived.
///
/// # Examples
///
/// ```rust
/// let svg = svg_data::element("svg").expect("svg element");
/// let _verdict = svg_data::compat_verdict_for_element(svg, svg_data::SpecSnapshotId::Svg2EditorsDraft);
/// ```
#[must_use]
pub fn compat_verdict_for_element(
    element: &ElementDef,
    profile: SpecSnapshotId,
) -> Option<CompatVerdict> {
    let _ = profile;
    compat_verdict_from_facts(&CompatFacts {
        deprecated: element.deprecated,
        experimental: element.experimental,
        standard_track: element.standard_track,
        baseline: element.baseline,
        browser_support: element.browser_support,
    })
}

/// The compat verdict for an attribute in a profile, when one was derived.
///
/// # Examples
///
/// ```rust
/// let fill = svg_data::attribute("fill").expect("fill attribute");
/// let _verdict = svg_data::compat_verdict_for_attribute(fill, svg_data::SpecSnapshotId::Svg2EditorsDraft);
/// ```
#[must_use]
pub fn compat_verdict_for_attribute(
    attribute: &AttributeDef,
    profile: SpecSnapshotId,
) -> Option<CompatVerdict> {
    compat_verdict_for_attribute_on_element(attribute, None, profile)
}

/// The compat verdict for an attribute on a concrete element, when one was derived.
///
/// # Examples
///
/// ```rust
/// let href = svg_data::attribute("href").expect("href attribute");
/// let _verdict = svg_data::compat_verdict_for_attribute_on_element(
///     href,
///     Some("use"),
///     svg_data::SpecSnapshotId::Svg2EditorsDraft,
/// );
/// ```
#[must_use]
pub fn compat_verdict_for_attribute_on_element(
    attribute: &AttributeDef,
    element_name: Option<&str>,
    profile: SpecSnapshotId,
) -> Option<CompatVerdict> {
    let _ = profile;
    compat_verdict_from_facts(&attribute.compat_facts_for_element(element_name))
}

/// The compat verdict for a retained behavior/value subfeature.
///
/// # Examples
///
/// ```rust
/// if let Some(subfeature) = svg_data::compat_subfeatures().first() {
///     let _verdict = svg_data::compat_verdict_for_subfeature(subfeature, svg_data::SpecSnapshotId::Svg2EditorsDraft);
/// }
/// ```
#[must_use]
pub fn compat_verdict_for_subfeature(
    subfeature: &CompatSubfeature,
    profile: SpecSnapshotId,
) -> Option<CompatVerdict> {
    let _ = profile;
    compat_verdict_from_facts(&subfeature.facts)
}

/// Resolve a `version="…"` attribute value to a snapshot by major family.
///
/// # Examples
///
/// ```rust
/// assert_eq!(
///     svg_data::snapshot_for_svg_version_attr("2.0"),
///     Some(svg_data::SpecSnapshotId::Svg2EditorsDraft),
/// );
/// ```
#[must_use]
pub fn snapshot_for_svg_version_attr(version: &str) -> Option<SpecSnapshotId> {
    match version.trim().split('.').next().unwrap_or_default() {
        "1" => Some(SpecSnapshotId::Svg11Rec20110816),
        "2" => Some(SpecSnapshotId::Svg2EditorsDraft),
        _ => None,
    }
}

/// Resolve a `version="…"` attribute value to an edition id.
///
/// # Examples
///
/// ```rust
/// let id = svg_data::edition_for_svg_version_attr("2.0").expect("SVG 2 edition");
/// assert_eq!(id.series, svg_data::edition::Series::Svg2);
/// ```
#[must_use]
pub fn edition_for_svg_version_attr(version: &str) -> Option<inventory::EditionId> {
    snapshot_for_svg_version_attr(version).map(inventory::EditionId::for_snapshot)
}

/// Metadata (aliases, …) for a snapshot.
///
/// # Examples
///
/// ```rust
/// let metadata = svg_data::snapshot_metadata(svg_data::SpecSnapshotId::Svg2EditorsDraft);
/// assert_eq!(metadata.snapshot, svg_data::SpecSnapshotId::Svg2EditorsDraft);
/// ```
#[must_use]
pub fn snapshot_metadata(snapshot: SpecSnapshotId) -> SnapshotMetadata {
    SNAPSHOT_METADATA
        .iter()
        .find(|metadata| metadata.snapshot == snapshot)
        .cloned()
        .unwrap_or(SnapshotMetadata {
            snapshot,
            aliases: &[],
        })
}

/// Resolve a requested profile string (id or alias) to a snapshot.
///
/// # Examples
///
/// ```rust
/// assert_eq!(
///     svg_data::resolve_profile_id("svg2draft"),
///     Some(svg_data::SpecSnapshotId::Svg2EditorsDraft),
/// );
/// ```
#[must_use]
pub fn resolve_profile_id(requested: &str) -> Option<SpecSnapshotId> {
    let requested = requested.trim();
    spec_snapshots().iter().copied().find(|snapshot| {
        snapshot.as_str().eq_ignore_ascii_case(requested)
            || snapshot_metadata(*snapshot)
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(requested))
    })
}

/// Resolve a requested edition string to an edition id.
///
/// # Examples
///
/// ```rust
/// let id = svg_data::resolve_edition_id("svg2draft").expect("SVG 2 draft edition");
/// assert_eq!(id.series, svg_data::edition::Series::Svg2);
/// ```
#[must_use]
pub fn resolve_edition_id(requested: &str) -> Option<inventory::EditionId> {
    resolve_profile_id(requested).map(inventory::EditionId::for_snapshot)
}

fn compat_verdict_from_facts(facts: &CompatFacts) -> Option<CompatVerdict> {
    let mut reasons = Vec::new();
    if facts.deprecated {
        reasons.push(VerdictReason::BcdDeprecated);
    }
    if facts.experimental {
        reasons.push(VerdictReason::BcdExperimental);
    }
    if facts.standard_track == Some(false) {
        reasons.push(VerdictReason::BcdNonStandard);
    }
    match facts.baseline {
        Some(BaselineStatus::Limited) => reasons.push(VerdictReason::BaselineLimited),
        Some(BaselineStatus::Newly { since, qualifier }) => {
            reasons.push(VerdictReason::BaselineNewly { since, qualifier });
        }
        Some(BaselineStatus::Widely { .. }) | None => {}
    }
    if let Some(support) = facts.browser_support.as_ref() {
        collect_browser_reasons(&mut reasons, "chrome", support.chrome);
        collect_browser_reasons(&mut reasons, "edge", support.edge);
        collect_browser_reasons(&mut reasons, "firefox", support.firefox);
        collect_browser_reasons(&mut reasons, "safari", support.safari);
    }
    if reasons.is_empty() {
        return None;
    }
    let recommendation = recommendation_for_reasons(&reasons);
    Some(CompatVerdict {
        recommendation,
        headline_template: headline_for_recommendation(recommendation),
        reasons,
    })
}

fn collect_browser_reasons(
    reasons: &mut Vec<VerdictReason>,
    browser: &'static str,
    version: Option<BrowserVersion>,
) {
    let Some(version) = version else {
        return;
    };
    if version.supported == Some(false) {
        reasons.push(VerdictReason::UnsupportedIn(browser));
    }
    if version.partial_implementation {
        reasons.push(VerdictReason::PartialImplementationIn(browser));
    }
    if let Some(prefix) = version.prefix {
        reasons.push(VerdictReason::PrefixRequiredIn { browser, prefix });
    }
    if !version.flags.is_empty() {
        reasons.push(VerdictReason::BehindFlagIn(browser));
    }
    if let Some(version_removed) = version.version_removed {
        reasons.push(VerdictReason::RemovedIn {
            browser,
            version: version_removed,
            qualifier: version.version_removed_qualifier,
        });
    }
}

fn recommendation_for_reasons(reasons: &[VerdictReason]) -> VerdictRecommendation {
    if reasons
        .iter()
        .any(|reason| matches!(reason, VerdictReason::ProfileObsolete { .. }))
    {
        return VerdictRecommendation::Forbid;
    }
    if reasons.iter().any(|reason| {
        matches!(
            reason,
            VerdictReason::BcdDeprecated
                | VerdictReason::BcdNonStandard
                | VerdictReason::RemovedIn { .. }
        )
    }) {
        return VerdictRecommendation::Avoid;
    }
    VerdictRecommendation::Caution
}

const fn headline_for_recommendation(recommendation: VerdictRecommendation) -> &'static str {
    match recommendation {
        VerdictRecommendation::Safe => "safe to use",
        VerdictRecommendation::Caution => "use with care",
        VerdictRecommendation::Avoid => "avoid in new work",
        VerdictRecommendation::Forbid => "do not use",
    }
}

fn allowed_child_names(content_model: &ContentModel) -> Vec<&'static str> {
    match content_model {
        ContentModel::Children {
            categories,
            elements,
        } => {
            let mut names: Vec<&'static str> = categories
                .iter()
                .flat_map(|category| elements_in_category(*category))
                .copied()
                .chain(elements.iter().copied())
                .collect();
            names.sort_unstable();
            names.dedup();
            names
        }
        ContentModel::ChildrenSet(names) => {
            let mut names: Vec<&'static str> = (*names).to_vec();
            names.sort_unstable();
            names.dedup();
            names
        }
        ContentModel::AnySvg => ELEMENTS.iter().map(|element| element.name).collect(),
        ContentModel::Foreign | ContentModel::Void | ContentModel::Text => Vec::new(),
    }
}

const fn elements_in_category(category: ElementCategory) -> &'static [&'static str] {
    let _ = category;
    // Category membership is part of the extracted data; empty until it lands.
    &[]
}

#[cfg(test)]
mod catalog_tests {
    use super::*;

    #[test]
    fn circle_is_catalogued_with_real_content_model() {
        let Some(circle) = element("circle") else {
            panic!("circle missing from catalog");
        };
        assert!(circle.global_attrs, "circle carries core attributes");
        assert!(
            circle.attrs.contains(&"pathLength"),
            "circle has pathLength"
        );
        assert!(circle.spec_url.is_some(), "circle has a spec permalink");

        // The flattened content model resolves to real child elements.
        let children = allowed_children_with_profile(SpecSnapshotId::LATEST, "circle");
        let names: Vec<&str> = children.iter().map(|child| child.element.name).collect();
        assert!(names.contains(&"animate"), "animation members are allowed");
        assert!(names.contains(&"desc"), "descriptive members are allowed");
        assert!(names.contains(&"clipPath"), "explicit children are allowed");
    }

    #[test]
    fn catalog_is_non_empty() {
        assert!(elements().len() >= 60, "the element catalog is populated");
    }

    #[test]
    fn catalog_graph_exposes_derived_relationships() {
        let graph = catalog_graph();
        assert!(graph.nodes.len() > elements().len());
        assert!(graph.edges.len() > attributes().len());
        assert!(graph.nodes.iter().any(|node| {
            node.id == "element:circle"
                && node.name == "circle"
                && node.kind == CatalogGraphNodeKind::Element
        }));
        assert!(graph.nodes.iter().any(|node| {
            node.id == "value:fill"
                && node.name == "fill (color)"
                && node.kind == CatalogGraphNodeKind::ValueGrammar
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.from == "attribute:fill"
                && edge.to == "css-property:fill"
                && edge.kind == CatalogGraphEdgeKind::UsesCssProperty
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.from == "attribute:fill"
                && edge.to == "value:fill"
                && edge.kind == CatalogGraphEdgeKind::HasValueGrammar
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.from == "attribute:fill"
                && edge.to == "element:circle"
                && edge.kind == CatalogGraphEdgeKind::AppliesTo
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.from == "element:circle"
                && edge.to == "attribute-category:global"
                && edge.kind == CatalogGraphEdgeKind::AcceptsGlobalAttributes
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.from == "element:font"
                && edge.to == "profile:Svg11Rec20110816"
                && edge.kind == CatalogGraphEdgeKind::PresentIn
        }));
        assert!(!graph.edges.iter().any(|edge| {
            edge.from == "element:font"
                && edge.to == "profile:Svg2EditorsDraft"
                && edge.kind == CatalogGraphEdgeKind::PresentIn
        }));
        assert!(!graph.edges.iter().any(|edge| {
            edge.from == "attribute:fetchpriority" && edge.kind == CatalogGraphEdgeKind::PresentIn
        }));
    }

    #[test]
    fn generated_inventories_track_snapshot_presence() {
        assert_eq!(inventories().len(), spec_snapshots().len());
        let Some(svg11) = inventory::for_edition(&inventory::EditionId::for_snapshot(
            SpecSnapshotId::Svg11Rec20110816,
        )) else {
            panic!("SVG 1.1 inventory is generated");
        };
        assert!(svg11.elements.iter().any(|element| element.name == "font"));
        assert!(
            svg11
                .attributes_for_element("a")
                .any(|attribute| attribute.name == "xlink:href")
        );

        let Some(svg2) = inventory::for_edition(&inventory::EditionId::for_snapshot(
            SpecSnapshotId::Svg2EditorsDraft,
        )) else {
            panic!("SVG 2 editor's draft inventory is generated");
        };
        assert!(!svg2.elements.iter().any(|element| element.name == "font"));
        assert!(
            svg2.attributes_for_element("a")
                .any(|attribute| attribute.name == "href")
        );
    }

    #[test]
    fn lifecycle_overlays_drive_profile_lookup() {
        assert_eq!(lifecycle_overlays().len(), spec_snapshots().len());

        let Some(latest) = lifecycle_overlays()
            .iter()
            .find(|overlay| overlay.snapshot == SpecSnapshotId::Svg2EditorsDraft)
        else {
            panic!("latest lifecycle overlay");
        };
        assert!(latest.elements.iter().any(|entry| {
            entry.name == "font"
                && !entry.present
                && entry.lifecycle == SpecLifecycle::Obsolete
                && entry.known_in
                    == [
                        SpecSnapshotId::Svg11Rec20030114,
                        SpecSnapshotId::Svg11Rec20110816,
                    ]
        }));
        assert!(latest.attributes.iter().any(|entry| {
            entry.name == "mask-type"
                && entry.present
                && entry.lifecycle == SpecLifecycle::Experimental
                && entry.known_in == [SpecSnapshotId::Svg2EditorsDraft]
        }));

        assert!(matches!(
            element_for_profile(SpecSnapshotId::Svg2EditorsDraft, "font"),
            ProfileLookup::UnsupportedInProfile { known_in }
                if known_in == [SpecSnapshotId::Svg11Rec20030114, SpecSnapshotId::Svg11Rec20110816]
        ));
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::Svg2EditorsDraft, "mask-type"),
            ProfileLookup::Present { value, lifecycle: SpecLifecycle::Experimental }
                if value.name == "mask-type"
        ));
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::Svg2EditorsDraft, "baseProfile"),
            ProfileLookup::UnsupportedInProfile { known_in }
                if known_in == [SpecSnapshotId::Svg11Rec20030114, SpecSnapshotId::Svg11Rec20110816]
        ));
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::Svg11Rec20110816, "mask-type"),
            ProfileLookup::UnsupportedInProfile { known_in }
                if known_in == [SpecSnapshotId::Svg2EditorsDraft]
        ));
    }

    #[test]
    fn profile_aliases_resolve_from_generated_snapshot_metadata() {
        assert!(
            snapshot_metadata(SpecSnapshotId::Svg11Rec20110816)
                .aliases
                .contains(&"svg11")
        );
        assert!(
            snapshot_metadata(SpecSnapshotId::Svg2EditorsDraft)
                .aliases
                .contains(&"latest")
        );
        assert_eq!(
            resolve_profile_id("svg11"),
            Some(SpecSnapshotId::Svg11Rec20110816)
        );
        assert_eq!(
            resolve_profile_id("Svg2Draft"),
            Some(SpecSnapshotId::Svg2EditorsDraft)
        );
        assert_eq!(
            resolve_profile_id("Svg11Rec20030114"),
            Some(SpecSnapshotId::Svg11Rec20030114)
        );
    }

    #[test]
    fn attribute_catalog_distinguishes_global_scoped_and_geometry_attrs() {
        let Some(id) = attribute("id") else {
            panic!("id missing from catalog");
        };
        assert_eq!(id.applicability, AttributeApplicability::Global);

        let Some(href) = attribute("xlink:href") else {
            panic!("href missing from catalog");
        };
        assert_eq!(href.name, "href");
        assert!(matches!(
            href.applicability,
            AttributeApplicability::Elements(elements)
                if elements.contains(&"a") && elements.contains(&"use")
        ));

        let Some(cx) = attribute("cx") else {
            panic!("cx missing from catalog");
        };
        assert_eq!(cx.presentation_attribute, None);
        assert!(matches!(
            cx.applicability,
            AttributeApplicability::Elements(elements)
                if elements.contains(&"circle") && !elements.contains(&"rect")
        ));

        let circle_attrs = attributes_for_with_profile(SpecSnapshotId::LATEST, "circle");
        assert!(
            circle_attrs
                .iter()
                .any(|profiled| profiled.attribute.name == "id")
        );
        assert!(
            circle_attrs
                .iter()
                .any(|profiled| profiled.attribute.name == "cx")
        );

        let rect_attrs = attributes_for_with_profile(SpecSnapshotId::LATEST, "rect");
        assert!(
            !rect_attrs
                .iter()
                .any(|profiled| profiled.attribute.name == "cx")
        );
    }

    #[test]
    fn href_alias_tracks_svg11_and_svg2_profiles() {
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::Svg11Rec20110816, "href"),
            ProfileLookup::UnsupportedInProfile { known_in }
                if known_in == [SpecSnapshotId::Svg2Cr20181004, SpecSnapshotId::Svg2EditorsDraft]
        ));
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::Svg11Rec20110816, "xlink:href"),
            ProfileLookup::Present { value, lifecycle: SpecLifecycle::Stable }
                if value.name == "href"
        ));
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::Svg2EditorsDraft, "xlink:href"),
            ProfileLookup::UnsupportedInProfile { known_in }
                if known_in == [SpecSnapshotId::Svg11Rec20030114, SpecSnapshotId::Svg11Rec20110816]
        ));
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::Svg2EditorsDraft, "href"),
            ProfileLookup::Present { value, lifecycle: SpecLifecycle::Stable }
                if value.name == "href"
        ));

        let svg11_use_attrs = attributes_for_with_profile(SpecSnapshotId::Svg11Rec20110816, "use");
        assert!(
            svg11_use_attrs
                .iter()
                .any(|profiled| profiled.name == "xlink:href")
        );
        assert!(
            !svg11_use_attrs
                .iter()
                .any(|profiled| profiled.name == "href")
        );
    }

    #[test]
    fn svg11_legacy_property_index_supplies_keyword_overrides() {
        let override_count = attributes()
            .iter()
            .filter(|attribute| !attribute.value_overrides.is_empty())
            .count();
        assert!(
            override_count >= 20,
            "expected broad SVG 1.1 value override coverage, got {override_count}"
        );

        let Some(display) = attribute("display") else {
            panic!("display missing from catalog");
        };
        assert!(matches!(
            &display.values,
            AttributeValues::Enum(values)
                if values.contains(&"inline-block") && !values.contains(&"run-in")
        ));
        assert!(matches!(
            display.values_for_profile(SpecSnapshotId::Svg11Rec20110816),
            AttributeValues::Enum(values)
                if values.contains(&"run-in") && !values.contains(&"inline-block")
        ));
    }

    #[test]
    fn external_css_property_definitions_supply_latest_value_spaces() {
        let Some(clip_rule) = attribute("clip-rule") else {
            panic!("clip-rule missing from catalog");
        };
        assert!(matches!(
            &clip_rule.values,
            AttributeValues::Enum(values)
                if values.contains(&"evenodd") && values.contains(&"nonzero")
        ));

        let Some(font_style) = attribute("font-style") else {
            panic!("font-style missing from catalog");
        };
        assert!(matches!(
            &font_style.values,
            AttributeValues::Enum(values)
                if values.contains(&"normal")
                    && values.contains(&"italic")
                    && values.contains(&"oblique")
        ));

        let Some(display) = attribute("display") else {
            panic!("display missing from catalog");
        };
        assert!(matches!(
            &display.values,
            AttributeValues::Enum(values)
                if values.contains(&"inline")
                    && values.contains(&"inline-block")
                    && values.contains(&"none")
                    && !values.contains(&"run-in")
        ));

        let Some(unicode_bidi) = attribute("unicode-bidi") else {
            panic!("unicode-bidi missing from catalog");
        };
        assert!(matches!(
            &unicode_bidi.values,
            AttributeValues::Enum(values)
                if values.contains(&"normal")
                    && values.contains(&"embed")
                    && values.contains(&"bidi-override")
                    && values.contains(&"plaintext")
        ));

        let Some(clip_path) = attribute("clip-path") else {
            panic!("clip-path missing from catalog");
        };
        assert!(matches!(
            &clip_path.values,
            AttributeValues::CssGrammar { grammar, graph }
                if grammar.contains("<clip-source>")
                    && graph.nodes.iter().any(|node|
                        node.kind == CssGrammarNodeKind::Type && node.text == Some("<clip-source>")
                    )
                    && graph.nodes.iter().any(|node|
                        node.kind == CssGrammarNodeKind::Type && node.text == Some("<basic-shape>")
                    )
                    && graph.nodes.iter().any(|node|
                        node.kind == CssGrammarNodeKind::Group
                    )
        ));
    }

    #[test]
    fn prose_and_referenced_values_get_semantic_shapes() {
        let cases = [
            ("id", "id"),
            ("class", "token_list"),
            ("lang", "language_tag"),
            ("tabindex", "integer"),
            ("style", "css_declaration_list"),
            ("referrerpolicy", "referrer_policy"),
            ("download", "suggested_file_name"),
            ("calcMode", "enum"),
            ("keyPoints", "semicolon_number_list"),
            ("origin", "enum"),
            ("path", "path_data"),
        ];
        for (name, expected) in cases {
            let Some(attribute) = attribute(name) else {
                panic!("{name} missing from catalog");
            };
            let actual = match &attribute.values {
                AttributeValues::TokenList => "token_list",
                AttributeValues::LanguageTag => "language_tag",
                AttributeValues::Integer => "integer",
                AttributeValues::CssDeclarationList => "css_declaration_list",
                AttributeValues::Id => "id",
                AttributeValues::ReferrerPolicy => "referrer_policy",
                AttributeValues::SuggestedFileName => "suggested_file_name",
                AttributeValues::PathData => "path_data",
                AttributeValues::SemicolonNumberList => "semicolon_number_list",
                AttributeValues::Enum(_) => "enum",
                _ => "other",
            };
            assert_eq!(actual, expected, "{name} value shape");
        }

        let Some(xml_space) = attribute("xml:space") else {
            panic!("xml:space missing from catalog");
        };
        assert!(matches!(
            &xml_space.values,
            AttributeValues::Enum(values)
                if *values == ["default", "preserve"]
        ));
    }

    #[test]
    fn animate_motion_uses_motion_specific_value_shapes() {
        let Some(animate_motion) = element("animateMotion") else {
            panic!("animateMotion missing from catalog");
        };
        assert!(matches!(
            &animate_motion.content_model,
            ContentModel::ChildrenSet(elements)
                if *elements == ["desc", "metadata", "mpath", "script", "title"]
        ));

        let cases = [
            ("by", "coordinate_pair"),
            ("from", "coordinate_pair"),
            ("to", "coordinate_pair"),
            ("values", "coordinate_pair_list"),
        ];
        for (name, expected) in cases {
            let Some(attribute) = attribute(name) else {
                panic!("{name} missing from catalog");
            };
            let Some(scoped) = attribute
                .element_values
                .iter()
                .find(|values| values.element == "animateMotion")
            else {
                panic!("{name} missing animateMotion scoped values");
            };
            let actual = match &scoped.values {
                AttributeValues::CoordinatePair => "coordinate_pair",
                AttributeValues::CoordinatePairList => "coordinate_pair_list",
                _ => "other",
            };
            assert_eq!(actual, expected, "{name} animateMotion value shape");
        }
    }

    #[test]
    fn nowhere_supported_attributes_are_not_present_without_element_context() {
        let Some(xlink_title) = attribute("xlink:title") else {
            panic!("xlink:title missing from catalog");
        };
        assert_eq!(xlink_title.applicability, AttributeApplicability::None);
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::LATEST, "xlink:title"),
            ProfileLookup::Unknown
        ));
    }

    #[test]
    fn compat_verdict_is_derived_from_objective_attribute_facts() {
        let attribute = AttributeDef {
            name: "demo",
            description: "",
            mdn_url: "",
            spec_url: None,
            deprecated: true,
            experimental: false,
            standard_track: Some(false),
            animatable: false,
            presentation_attribute: None,
            baseline: Some(BaselineStatus::Limited),
            browser_support: Some(BrowserSupport {
                chrome: Some(BrowserVersion {
                    partial_implementation: true,
                    ..BrowserVersion::EMPTY
                }),
                edge: None,
                firefox: None,
                safari: Some(BrowserVersion {
                    prefix: Some("-webkit-"),
                    ..BrowserVersion::EMPTY
                }),
            }),
            element_compat: &[],
            element_values: &[],
            values: AttributeValues::FreeText,
            value_overrides: &[],
            applicability: AttributeApplicability::Global,
        };

        let Some(verdict) = compat_verdict_for_attribute(&attribute, SpecSnapshotId::LATEST) else {
            panic!("objective facts should produce a verdict");
        };

        assert_eq!(verdict.recommendation, VerdictRecommendation::Avoid);
        assert!(verdict.reasons.contains(&VerdictReason::BcdDeprecated));
        assert!(verdict.reasons.contains(&VerdictReason::BcdNonStandard));
        assert!(verdict.reasons.contains(&VerdictReason::BaselineLimited));
        assert!(
            verdict
                .reasons
                .contains(&VerdictReason::PartialImplementationIn("chrome"))
        );
        assert!(verdict.reasons.contains(&VerdictReason::PrefixRequiredIn {
            browser: "safari",
            prefix: "-webkit-"
        }));
    }

    #[test]
    fn compat_verdict_is_absent_without_objective_caveats() {
        let attribute = AttributeDef {
            name: "demo",
            description: "",
            mdn_url: "",
            spec_url: None,
            deprecated: false,
            experimental: false,
            standard_track: Some(true),
            animatable: false,
            presentation_attribute: None,
            baseline: Some(BaselineStatus::Widely {
                since: 2020,
                qualifier: None,
            }),
            browser_support: None,
            element_compat: &[],
            element_values: &[],
            values: AttributeValues::FreeText,
            value_overrides: &[],
            applicability: AttributeApplicability::Global,
        };

        assert!(compat_verdict_for_attribute(&attribute, SpecSnapshotId::LATEST).is_none());
    }

    #[test]
    fn fetchpriority_is_catalogued_from_bcd_only_data() {
        let Some(fetchpriority) = attribute("fetchpriority") else {
            panic!("fetchpriority missing from catalog");
        };

        assert_eq!(fetchpriority.spec_url, None);
        assert_eq!(fetchpriority.standard_track, Some(false));
        assert!(fetchpriority.experimental);
        assert!(matches!(
            fetchpriority.baseline,
            Some(BaselineStatus::Limited)
        ));
        assert!(matches!(
            fetchpriority.applicability,
            AttributeApplicability::Elements(elements)
                if elements == ["feImage", "image", "script"]
        ));

        let Some(verdict) = compat_verdict_for_attribute(fetchpriority, SpecSnapshotId::LATEST)
        else {
            panic!("fetchpriority objective facts should produce a verdict");
        };
        assert!(verdict.reasons.contains(&VerdictReason::BcdExperimental));
        assert!(verdict.reasons.contains(&VerdictReason::BcdNonStandard));
        assert!(verdict.reasons.contains(&VerdictReason::BaselineLimited));
    }

    #[test]
    fn element_scoped_attribute_compat_does_not_flatten_by_name() {
        let Some(path) = attribute("path") else {
            panic!("path missing from catalog");
        };

        assert!(!path.experimental);
        assert!(compat_verdict_for_attribute(path, SpecSnapshotId::LATEST).is_none());
        assert!(
            compat_verdict_for_attribute_on_element(
                path,
                Some("animateMotion"),
                SpecSnapshotId::LATEST
            )
            .is_none()
        );

        let Some(text_path_verdict) =
            compat_verdict_for_attribute_on_element(path, Some("textPath"), SpecSnapshotId::LATEST)
        else {
            panic!("textPath path facts should produce a verdict");
        };
        assert!(
            text_path_verdict
                .reasons
                .contains(&VerdictReason::BcdExperimental)
        );
        assert!(
            text_path_verdict
                .reasons
                .contains(&VerdictReason::UnsupportedIn("chrome"))
        );
    }

    #[test]
    fn bcd_behavior_subfeatures_are_retained_for_future_lints() {
        let Some(data_uri) = compat_subfeature("svg.elements.use.data_uri") else {
            panic!("use.data_uri subfeature missing");
        };
        assert_eq!(data_uri.kind, CompatSubfeatureKind::Behavior);
        assert_eq!(data_uri.element, "use");
        assert_eq!(data_uri.name, "data_uri");

        let Some(verdict) = compat_verdict_for_subfeature(data_uri, SpecSnapshotId::LATEST) else {
            panic!("use.data_uri facts should produce a verdict");
        };
        assert!(verdict.reasons.contains(&VerdictReason::BaselineLimited));
        assert!(
            verdict
                .reasons
                .contains(&VerdictReason::UnsupportedIn("safari"))
        );
        assert!(verdict.reasons.iter().any(|reason| matches!(
            reason,
            VerdictReason::RemovedIn {
                browser: "chrome",
                version: "120",
                ..
            }
        )));

        let Some(xlink_href) = compat_subfeature("svg.elements.use.xlink_href") else {
            panic!("use.xlink_href subfeature missing");
        };
        assert_eq!(xlink_href.kind, CompatSubfeatureKind::LegacyXlinkAlias);
    }

    #[test]
    fn foreign_object_hosts_foreign_content() {
        let Some(foreign_object) = element("foreignObject") else {
            panic!("foreignObject missing from catalog");
        };
        assert!(
            matches!(foreign_object.content_model, ContentModel::Foreign),
            "the spec's `any` content model maps to Foreign"
        );
        assert!(allows_foreign_children("foreignObject"));
        // A regular element is not a foreign host.
        assert!(!allows_foreign_children("circle"));
    }
}
