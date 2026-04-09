//! Generated SVG catalog and browser-compat lookup APIs.
//!
//! This crate exposes the baked element and attribute metadata consumed by the
//! language server, linter, and other workspace crates.
//!
//! # Examples
//!
//! ```rust
//! let rect = svg_data::element("rect").expect("rect should exist");
//! assert_eq!(rect.name, "rect");
//!
//! let attrs = svg_data::attributes_for("rect");
//! assert!(attrs.iter().any(|a| a.name == "width"));
//! ```

/// Browser-compat-data model types used by the generated SVG compatibility
/// catalog.
pub mod bcd;
mod catalog;
/// Category-based helpers for allowed-child and grouping queries.
pub mod categories;
/// Shared BCD JSON parsing helpers for runtime compat overlays.
pub mod compat_parse;
/// Public catalog type definitions.
pub mod types;
/// `XLink` attribute name canonicalization (BCD underscore form to colon form).
pub mod xlink;

use std::{collections::HashMap, sync::LazyLock};

use catalog::{ATTRIBUTES, ELEMENTS};
pub use types::{
    AttributeDef, AttributeValues, BaselineStatus, BrowserSupport, BrowserVersion, ContentModel,
    ElementCategory, ElementDef, ProfileLookup, ProfiledAttribute, ProfiledElement, SpecLifecycle,
    SpecSnapshotId, SpecSnapshotMetadata,
};

const SVG11_REC_20030114_ALIASES: &[&str] = &[
    "Svg11Rec20030114",
    "Svg11FirstEdition",
    "SVG 1.1 First Edition",
    "SVG 1.1 Recommendation 2003-01-14",
];

const SVG11_REC_20110816_ALIASES: &[&str] = &[
    "Svg1",
    "Svg11",
    "Svg11Rec20110816",
    "Svg11SecondEdition",
    "SVG 1",
    "SVG 1.1",
    "SVG 1.1 Second Edition",
    "SVG 1.1 Recommendation 2011-08-16",
];

const SVG2_CR_20181004_ALIASES: &[&str] = &[
    "Svg2",
    "Svg2Cr20181004",
    "Svg2CandidateRecommendation",
    "SVG 2",
    "SVG 2 CR",
    "SVG 2 Candidate Recommendation",
    "SVG 2 Candidate Recommendation 2018-10-04",
];

const SVG2_EDITORS_DRAFT_20250914_ALIASES: &[&str] = &[
    "Svg2Draft",
    "Svg2EditorsDraft20250914",
    "Svg2EditorsDraft",
    "SVG 2 Draft",
    "SVG 2 Editor's Draft",
    "SVG 2 Editors Draft",
    "SVG 2 Editor's Draft 2025-09-14",
];

const ALL_SPEC_SNAPSHOTS: &[SpecSnapshotId] = &[
    SpecSnapshotId::Svg11Rec20030114,
    SpecSnapshotId::Svg11Rec20110816,
    SpecSnapshotId::Svg2Cr20181004,
    SpecSnapshotId::Svg2EditorsDraft20250914,
];

const SVG11_SNAPSHOTS: &[SpecSnapshotId] = &[
    SpecSnapshotId::Svg11Rec20030114,
    SpecSnapshotId::Svg11Rec20110816,
];

const SVG2_SNAPSHOTS: &[SpecSnapshotId] = &[
    SpecSnapshotId::Svg2Cr20181004,
    SpecSnapshotId::Svg2EditorsDraft20250914,
];

const SVG11_REC_20030114_METADATA: SpecSnapshotMetadata = SpecSnapshotMetadata {
    canonical_id: SpecSnapshotId::Svg11Rec20030114,
    aliases: SVG11_REC_20030114_ALIASES,
    source_url: "https://www.w3.org/TR/2003/REC-SVG11-20030114/",
    snapshot_date: "2003-01-14",
    stable_base: None,
    errata_folded: false,
};

const SVG11_REC_20110816_METADATA: SpecSnapshotMetadata = SpecSnapshotMetadata {
    canonical_id: SpecSnapshotId::Svg11Rec20110816,
    aliases: SVG11_REC_20110816_ALIASES,
    source_url: "https://www.w3.org/TR/2011/REC-SVG11-20110816/",
    snapshot_date: "2011-08-16",
    stable_base: None,
    errata_folded: true,
};

const SVG2_CR_20181004_METADATA: SpecSnapshotMetadata = SpecSnapshotMetadata {
    canonical_id: SpecSnapshotId::Svg2Cr20181004,
    aliases: SVG2_CR_20181004_ALIASES,
    source_url: "https://www.w3.org/TR/2018/CR-SVG2-20181004/",
    snapshot_date: "2018-10-04",
    stable_base: None,
    errata_folded: false,
};

const SVG2_EDITORS_DRAFT_20250914_METADATA: SpecSnapshotMetadata = SpecSnapshotMetadata {
    canonical_id: SpecSnapshotId::Svg2EditorsDraft20250914,
    aliases: SVG2_EDITORS_DRAFT_20250914_ALIASES,
    source_url: "https://svgwg.org/svg2-draft/",
    snapshot_date: "2025-09-14",
    stable_base: Some(SpecSnapshotId::Svg2Cr20181004),
    errata_folded: false,
};

static ELEMENT_MAP: LazyLock<HashMap<&'static str, &'static ElementDef>> =
    LazyLock::new(|| ELEMENTS.iter().map(|e| (e.name, e)).collect());

static ATTRIBUTE_MAP: LazyLock<HashMap<&'static str, &'static AttributeDef>> =
    LazyLock::new(|| ATTRIBUTES.iter().map(|a| (a.name, a)).collect());

/// Look up a single SVG element definition by tag name.
#[must_use]
pub fn element(name: &str) -> Option<&'static ElementDef> {
    ELEMENT_MAP.get(name).copied()
}

/// Look up a single SVG attribute definition by attribute name.
#[must_use]
pub fn attribute(name: &str) -> Option<&'static AttributeDef> {
    ATTRIBUTE_MAP
        .get(name)
        .or_else(|| {
            let canonical_name = xlink::canonical_svg_attribute_name(name);
            (canonical_name.as_ref() != name)
                .then(|| ATTRIBUTE_MAP.get(canonical_name.as_ref()))
                .flatten()
        })
        .copied()
}

/// Return the supported SVG spec snapshots in canonical order.
#[must_use]
pub const fn spec_snapshots() -> &'static [SpecSnapshotId] {
    ALL_SPEC_SNAPSHOTS
}

/// Return pinned metadata for a canonical SVG spec snapshot id.
#[must_use]
pub const fn snapshot_metadata(snapshot: SpecSnapshotId) -> &'static SpecSnapshotMetadata {
    match snapshot {
        SpecSnapshotId::Svg11Rec20030114 => &SVG11_REC_20030114_METADATA,
        SpecSnapshotId::Svg11Rec20110816 => &SVG11_REC_20110816_METADATA,
        SpecSnapshotId::Svg2Cr20181004 => &SVG2_CR_20181004_METADATA,
        SpecSnapshotId::Svg2EditorsDraft20250914 => &SVG2_EDITORS_DRAFT_20250914_METADATA,
    }
}

/// Resolve a user-facing profile id, alias, or long-form synonym.
#[must_use]
pub fn resolve_profile_id(input: &str) -> Option<SpecSnapshotId> {
    let normalized_input = normalize_profile_key(input);
    if normalized_input.is_empty() {
        return None;
    }

    spec_snapshots()
        .iter()
        .copied()
        .find(|snapshot| profile_key_matches(*snapshot, &normalized_input))
}

/// Return the full generated SVG element catalog.
#[must_use]
pub fn elements() -> &'static [ElementDef] {
    ELEMENTS
}

/// Return the full generated SVG attribute catalog.
#[must_use]
pub fn attributes() -> &'static [AttributeDef] {
    ATTRIBUTES
}

/// Look up a single SVG element definition against a selected profile.
#[must_use]
pub fn element_for_profile(
    profile: SpecSnapshotId,
    name: &str,
) -> ProfileLookup<&'static ElementDef> {
    let Some(element) = element(name) else {
        return ProfileLookup::Unknown;
    };
    lookup_for_profile(profile, element, known_element_snapshots(element.name))
}

/// Look up a single SVG attribute definition against a selected profile.
#[must_use]
pub fn attribute_for_profile(
    profile: SpecSnapshotId,
    name: &str,
) -> ProfileLookup<&'static AttributeDef> {
    let Some(attribute) = attribute(name) else {
        return ProfileLookup::Unknown;
    };
    lookup_for_profile(
        profile,
        attribute,
        known_attribute_snapshots(attribute.name),
    )
}

/// Return all SVG elements available in the selected profile.
#[must_use]
pub fn elements_with_profile(profile: SpecSnapshotId) -> Vec<ProfiledElement> {
    ELEMENTS
        .iter()
        .filter_map(|element| match element_for_profile(profile, element.name) {
            ProfileLookup::Present { value, lifecycle } => Some(ProfiledElement {
                element: value,
                lifecycle,
            }),
            ProfileLookup::UnsupportedInProfile { .. } | ProfileLookup::Unknown => None,
        })
        .collect()
}

/// Return the concrete child element names allowed under `parent`.
#[must_use]
pub fn allowed_children(parent: &str) -> Vec<&'static str> {
    categories::allowed_children(parent)
}

/// Return the child element defs allowed under `parent` in the selected profile.
#[must_use]
pub fn allowed_children_with_profile(
    profile: SpecSnapshotId,
    parent: &str,
) -> Vec<ProfiledElement> {
    let ProfileLookup::Present {
        value: parent_element,
        ..
    } = element_for_profile(profile, parent)
    else {
        return Vec::new();
    };

    allowed_children(parent_element.name)
        .into_iter()
        .filter_map(|child| match element_for_profile(profile, child) {
            ProfileLookup::Present { value, lifecycle } => Some(ProfiledElement {
                element: value,
                lifecycle,
            }),
            ProfileLookup::UnsupportedInProfile { .. } | ProfileLookup::Unknown => None,
        })
        .collect()
}

/// Return whether `parent` accepts foreign-namespace children.
#[must_use]
pub fn allows_foreign_children(parent: &str) -> bool {
    element(parent).is_some_and(|el| matches!(el.content_model, ContentModel::Foreign))
}

fn attribute_applies_to(attr: &AttributeDef, element_name: &str) -> bool {
    // Empty elements list means global — current codegen uses "*" but older
    // build artifacts may still emit an empty list.
    attr.elements.is_empty()
        || attr.elements.contains(&"*")
        || attr.elements.contains(&element_name)
}

/// Return all attributes that apply to `element_name`, including global ones.
#[must_use]
pub fn attributes_for(element_name: &str) -> Vec<&'static AttributeDef> {
    let Some(el) = element(element_name) else {
        return Vec::new();
    };
    let mut result: Vec<&'static AttributeDef> = Vec::new();
    for attr in ATTRIBUTES {
        if attribute_applies_to(attr, el.name) {
            result.push(attr);
        }
    }
    result
}

/// Return all attributes that apply to `element_name` in the selected profile.
#[must_use]
pub fn attributes_for_with_profile(
    profile: SpecSnapshotId,
    element_name: &str,
) -> Vec<ProfiledAttribute> {
    let ProfileLookup::Present { value: element, .. } = element_for_profile(profile, element_name)
    else {
        return Vec::new();
    };

    ATTRIBUTES
        .iter()
        .filter(|attribute| attribute_applies_to(attribute, element.name))
        .filter_map(
            |attribute| match attribute_for_profile(profile, attribute.name) {
                ProfileLookup::Present { value, lifecycle } => Some(ProfiledAttribute {
                    attribute: value,
                    lifecycle,
                }),
                ProfileLookup::UnsupportedInProfile { .. } | ProfileLookup::Unknown => None,
            },
        )
        .collect()
}

/// Return all element names belonging to the given catalog category.
#[must_use]
pub const fn elements_in_category(cat: ElementCategory) -> &'static [&'static str] {
    categories::elements_in_category(cat)
}

fn profile_key_matches(snapshot: SpecSnapshotId, normalized_input: &str) -> bool {
    let metadata = snapshot_metadata(snapshot);
    normalize_profile_key(metadata.canonical_id.as_str()) == normalized_input
        || metadata
            .aliases
            .iter()
            .copied()
            .any(|alias| normalize_profile_key(alias) == normalized_input)
}

const fn known_element_snapshots(_name: &str) -> &'static [SpecSnapshotId] {
    ALL_SPEC_SNAPSHOTS
}

fn known_attribute_snapshots(name: &str) -> &'static [SpecSnapshotId] {
    match name {
        "href" => SVG2_SNAPSHOTS,
        "xlink:actuate" | "xlink:arcrole" | "xlink:href" | "xlink:role" | "xlink:show"
        | "xlink:title" | "xlink:type" => SVG11_SNAPSHOTS,
        _ => ALL_SPEC_SNAPSHOTS,
    }
}

fn lookup_for_profile<T: Copy + HasSpecLifecycle>(
    profile: SpecSnapshotId,
    value: T,
    known_in: &'static [SpecSnapshotId],
) -> ProfileLookup<T> {
    if !known_in.contains(&profile) {
        return ProfileLookup::UnsupportedInProfile { known_in };
    }

    ProfileLookup::Present {
        value,
        lifecycle: lifecycle_for_profile(profile, known_in, value.spec_lifecycle()),
    }
}

fn lifecycle_for_profile(
    profile: SpecSnapshotId,
    known_in: &'static [SpecSnapshotId],
    union_lifecycle: SpecLifecycle,
) -> SpecLifecycle {
    if union_lifecycle != SpecLifecycle::Stable {
        return union_lifecycle;
    }

    let Some(stable_base) = snapshot_metadata(profile).stable_base else {
        return SpecLifecycle::Stable;
    };

    if known_in.contains(&stable_base) {
        SpecLifecycle::Stable
    } else {
        SpecLifecycle::Experimental
    }
}

trait HasSpecLifecycle {
    fn spec_lifecycle(self) -> SpecLifecycle;
}

impl HasSpecLifecycle for &'static ElementDef {
    fn spec_lifecycle(self) -> SpecLifecycle {
        self.spec_lifecycle
    }
}

impl HasSpecLifecycle for &'static AttributeDef {
    fn spec_lifecycle(self) -> SpecLifecycle {
        self.spec_lifecycle
    }
}

fn normalize_profile_key(input: &str) -> String {
    input
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    const XLINK_ATTRIBUTE_NAMES: &[(&str, &str)] = &[
        ("xlink_actuate", "xlink:actuate"),
        ("xlink_arcrole", "xlink:arcrole"),
        ("xlink_href", "xlink:href"),
        ("xlink_role", "xlink:role"),
        ("xlink_show", "xlink:show"),
        ("xlink_title", "xlink:title"),
        ("xlink_type", "xlink:type"),
    ];
    const EMITTED_XLINK_ATTRIBUTE_NAMES: &[(&str, &str)] = &[
        ("xlink_actuate", "xlink:actuate"),
        ("xlink_href", "xlink:href"),
        ("xlink_show", "xlink:show"),
        ("xlink_title", "xlink:title"),
    ];

    #[test]
    fn element_lookup() -> Result<(), Box<dyn Error>> {
        let rect = element("rect").ok_or("rect should exist")?;
        assert_eq!(rect.name, "rect");
        assert!(!rect.deprecated);
        assert!(matches!(rect.content_model, ContentModel::Void));
        Ok(())
    }

    #[test]
    fn linear_gradient_description_uses_lead_paragraph() -> Result<(), Box<dyn Error>> {
        let linear = element("linearGradient").ok_or("linearGradient should exist")?;
        assert!(
            linear
                .description
                .starts_with("Linear gradients are defined by a 'linearGradient' element."),
            "unexpected linearGradient description: {}",
            linear.description
        );
        Ok(())
    }

    #[test]
    fn rect_description_is_not_interface_object_prose() -> Result<(), Box<dyn Error>> {
        let rect = element("rect").ok_or("rect should exist")?;
        assert!(
            !rect
                .description
                .to_ascii_lowercase()
                .contains("object represents"),
            "rect description should not use interface-object prose: {}",
            rect.description
        );
        Ok(())
    }

    #[test]
    fn element_not_found() {
        assert!(element("notanelement").is_none());
    }

    #[test]
    fn text_content_model() -> Result<(), Box<dyn Error>> {
        let text = element("text").ok_or("text should exist")?;
        assert!(matches!(text.content_model, ContentModel::Children(_)));
        Ok(())
    }

    #[test]
    fn foreign_object_content_model() -> Result<(), Box<dyn Error>> {
        let foreign_object = element("foreignObject").ok_or("foreignObject should exist")?;
        assert!(matches!(
            foreign_object.content_model,
            ContentModel::Foreign
        ));
        assert!(allows_foreign_children("foreignObject"));
        Ok(())
    }

    #[test]
    fn allowed_children_text() {
        let children = allowed_children("text");
        assert!(children.contains(&"tspan"), "text should allow tspan");
        assert!(!children.contains(&"rect"), "text should not allow rect");
    }

    #[test]
    fn allowed_children_void() {
        let children = allowed_children("rect");
        assert!(children.is_empty(), "void element should have no children");
    }

    #[test]
    fn attribute_lookup() -> Result<(), Box<dyn Error>> {
        let fill = attribute("fill").ok_or("fill should exist")?;
        assert!(matches!(fill.values, AttributeValues::Color));
        Ok(())
    }

    #[test]
    fn attribute_d_on_path() -> Result<(), Box<dyn Error>> {
        let d = attribute("d").ok_or("d should exist")?;
        assert!(d.elements.contains(&"path"));
        assert!(matches!(d.values, AttributeValues::PathData));
        Ok(())
    }

    #[test]
    fn attributes_for_rect() {
        let attrs = attributes_for("rect");
        let names: Vec<&str> = attrs.iter().map(|a| a.name).collect();
        assert!(names.contains(&"fill"), "rect should accept fill");
        assert!(names.contains(&"x"), "rect should accept x");
        assert!(!names.contains(&"d"), "rect should not accept d");
    }

    #[test]
    fn xlink_alias_helper_canonicalizes_known_legacy_names() {
        for &(legacy_name, canonical_name) in XLINK_ATTRIBUTE_NAMES {
            assert_eq!(
                super::xlink::canonical_svg_attribute_name(legacy_name).as_ref(),
                canonical_name
            );
            assert_eq!(
                super::xlink::canonical_svg_attribute_name(canonical_name).as_ref(),
                canonical_name
            );
        }
    }

    #[test]
    fn xlink_attribute_lookup_is_canonical_and_backward_compatible() -> Result<(), Box<dyn Error>> {
        for &(legacy_name, canonical_name) in EMITTED_XLINK_ATTRIBUTE_NAMES {
            let canonical = attribute(canonical_name)
                .ok_or_else(|| format!("{canonical_name} should exist"))?;
            let legacy =
                attribute(legacy_name).ok_or_else(|| format!("{legacy_name} should alias"))?;
            assert!(std::ptr::eq(canonical, legacy));
            assert_eq!(canonical.name, canonical_name);
            assert!(
                canonical.deprecated,
                "{canonical_name} should be deprecated"
            );
        }

        let href = attribute("xlink:href").ok_or("xlink:href should exist")?;
        assert_eq!(
            href.mdn_url,
            "https://developer.mozilla.org/docs/Web/SVG/Attribute/xlink:href"
        );
        Ok(())
    }

    #[test]
    fn public_xlink_attribute_names_are_canonical() {
        let xlink_names: Vec<&str> = attributes()
            .iter()
            .filter(|attribute| attribute.name.starts_with("xlink"))
            .map(|attribute| attribute.name)
            .collect();

        assert!(
            !xlink_names.is_empty(),
            "catalog must include deprecated xlink attributes for backwards compatibility"
        );
        assert!(
            xlink_names.iter().all(|name| name.contains(':')),
            "public xlink names should use canonical colon syntax: {xlink_names:?}"
        );
    }

    #[test]
    fn attributes_for_use_only_exposes_canonical_xlink_names() {
        let attrs = attributes_for("use");
        let names: Vec<&str> = attrs.iter().map(|a| a.name).collect();
        assert!(names.contains(&"xlink:href"));
        assert!(!names.contains(&"xlink_href"));
    }

    #[test]
    fn empty_elements_list_is_treated_as_global() {
        let attr = AttributeDef {
            name: "legacy-global",
            description: "",
            mdn_url: "",
            spec_lifecycle: SpecLifecycle::Stable,
            deprecated: false,
            experimental: false,
            spec_url: None,
            baseline: None,
            browser_support: None,
            values: AttributeValues::FreeText,
            elements: &[],
        };

        assert!(attribute_applies_to(&attr, "rect"));
        assert!(attribute_applies_to(&attr, "svg"));
    }

    #[test]
    fn elements_in_shape_category() {
        let shapes = elements_in_category(ElementCategory::Shape);
        assert!(shapes.contains(&"rect"));
        assert!(shapes.contains(&"circle"));
        assert!(shapes.contains(&"path"));
        assert!(!shapes.contains(&"g"));
    }

    #[test]
    fn all_elements_have_mdn_url() {
        for el in elements() {
            assert!(
                el.mdn_url.starts_with("https://developer.mozilla.org/"),
                "element {} missing MDN URL",
                el.name
            );
        }
    }

    #[test]
    fn profile_resolution_accepts_aliases_case_insensitively() {
        assert_eq!(
            resolve_profile_id("Svg2Draft"),
            Some(SpecSnapshotId::Svg2EditorsDraft20250914)
        );
        assert_eq!(
            resolve_profile_id("svg1"),
            Some(SpecSnapshotId::Svg11Rec20110816)
        );
        assert_eq!(
            resolve_profile_id("svg11rec20110816"),
            Some(SpecSnapshotId::Svg11Rec20110816)
        );
    }

    #[test]
    fn profile_resolution_accepts_long_form_synonyms() {
        assert_eq!(
            resolve_profile_id("SVG 1.1 Second Edition"),
            Some(SpecSnapshotId::Svg11Rec20110816)
        );
        assert_eq!(
            resolve_profile_id("SVG 2 Editor's Draft"),
            Some(SpecSnapshotId::Svg2EditorsDraft20250914)
        );
    }

    #[test]
    fn friendly_aliases_resolve_to_pinned_snapshots() {
        assert_eq!(
            resolve_profile_id("Svg1"),
            Some(SpecSnapshotId::Svg11Rec20110816)
        );
        assert_eq!(
            resolve_profile_id("Svg2Draft"),
            Some(SpecSnapshotId::Svg2EditorsDraft20250914)
        );
    }

    #[test]
    fn snapshot_metadata_tracks_stable_base_and_errata() {
        let svg11 = snapshot_metadata(SpecSnapshotId::Svg11Rec20110816);
        assert!(svg11.errata_folded);
        assert_eq!(svg11.stable_base, None);

        let draft = snapshot_metadata(SpecSnapshotId::Svg2EditorsDraft20250914);
        assert_eq!(draft.stable_base, Some(SpecSnapshotId::Svg2Cr20181004));
        assert!(!draft.errata_folded);
    }

    #[test]
    fn draft_only_membership_derives_experimental_lifecycle() {
        assert_eq!(
            lifecycle_for_profile(
                SpecSnapshotId::Svg2EditorsDraft20250914,
                &[SpecSnapshotId::Svg2EditorsDraft20250914],
                SpecLifecycle::Stable,
            ),
            SpecLifecycle::Experimental
        );
    }

    #[test]
    fn spec_lifecycle_is_separate_from_compat_deprecation() -> Result<(), Box<dyn Error>> {
        let href = attribute("xlink:href").ok_or("xlink:href should exist")?;
        assert_eq!(href.spec_lifecycle, SpecLifecycle::Stable);
        assert!(href.deprecated, "compat deprecation should remain visible");
        Ok(())
    }

    #[test]
    fn element_profile_lookup_returns_present_for_stable_union_entries()
    -> Result<(), Box<dyn Error>> {
        let lookup = element_for_profile(SpecSnapshotId::Svg11Rec20030114, "rect");
        let ProfileLookup::Present { value, lifecycle } = lookup else {
            return Err("rect should be available in SVG 1.1".into());
        };
        assert_eq!(value.name, "rect");
        assert_eq!(lifecycle, SpecLifecycle::Stable);
        Ok(())
    }

    #[test]
    fn attribute_profile_lookup_distinguishes_href_and_xlink_href() -> Result<(), Box<dyn Error>> {
        let svg11_href = attribute_for_profile(SpecSnapshotId::Svg11Rec20110816, "href");
        let ProfileLookup::UnsupportedInProfile { known_in } = svg11_href else {
            return Err("href should be unsupported in SVG 1.1".into());
        };
        assert_eq!(known_in, SVG2_SNAPSHOTS);

        let svg2_xlink = attribute_for_profile(SpecSnapshotId::Svg2Cr20181004, "xlink_href");
        let ProfileLookup::UnsupportedInProfile { known_in } = svg2_xlink else {
            return Err("xlink:href should be unsupported in SVG 2".into());
        };
        assert_eq!(known_in, SVG11_SNAPSHOTS);

        let svg2_href = attribute_for_profile(SpecSnapshotId::Svg2Cr20181004, "href");
        let ProfileLookup::Present { value, lifecycle } = svg2_href else {
            return Err("href should be available in SVG 2".into());
        };
        assert_eq!(value.name, "href");
        assert_eq!(lifecycle, SpecLifecycle::Stable);
        Ok(())
    }

    #[test]
    fn attributes_for_profile_swaps_href_forms_by_snapshot() {
        let svg11_names: Vec<&str> =
            attributes_for_with_profile(SpecSnapshotId::Svg11Rec20110816, "use")
                .iter()
                .map(|attribute| attribute.attribute.name)
                .collect();
        assert!(svg11_names.contains(&"xlink:href"));
        assert!(!svg11_names.contains(&"href"));

        let svg2_names: Vec<&str> =
            attributes_for_with_profile(SpecSnapshotId::Svg2Cr20181004, "use")
                .iter()
                .map(|attribute| attribute.attribute.name)
                .collect();
        assert!(svg2_names.contains(&"href"));
        assert!(!svg2_names.contains(&"xlink:href"));
    }

    #[test]
    fn unknown_profile_lookup_stays_distinct_from_unsupported() {
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::Svg2Cr20181004, "not-an-attribute"),
            ProfileLookup::Unknown
        ));
    }
}
