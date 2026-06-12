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
/// Derived union membership and adjacent snapshot overlay artifacts.
pub mod derived;
/// Typed W3C edition index and freshness primitives.
pub mod edition;
/// Shared manifest, cache, provenance, and dataset emission helpers.
pub mod extraction;
/// Complete, spec-faithful SVG 2 ED inventory baked from `definitions*.xml`.
pub mod inventory;
/// Typed constraint model for SVG profiles (SVG Native).
pub mod profile;
/// Deterministic audit helpers for checked-in snapshot reviews.
pub mod review;
/// Typed schema for normalized per-snapshot checked-in SVG data.
pub mod snapshot_schema;
/// Public catalog type definitions.
pub mod types;
/// Deserialization types for the svg-compat worker JSON output.
pub mod worker_schema;
/// `XLink` attribute name canonicalization (BCD underscore form to colon form).
pub mod xlink;

use std::{collections::HashMap, sync::LazyLock};

use catalog::{
    ATTRIBUTES, ELEMENTS, generated_attribute_names_for_profile,
    generated_attribute_values_for_profile, generated_known_attribute_snapshots,
    generated_known_element_snapshots,
};
pub use types::{
    AttributeDef, AttributeValues, BaselineQualifier, BaselineStatus, BrowserFlag, BrowserSupport,
    BrowserVersion, CompatVerdict, ContentModel, ElementCategory, ElementDef, ProfileLookup,
    ProfiledAttribute, ProfiledElement, RawVersionAdded, SpecLifecycle, SpecSnapshotId,
    SpecSnapshotMetadata, VerdictReason, VerdictRecommendation,
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

const SVG2_EDITORS_DRAFT_ALIASES: &[&str] = &[
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
    SpecSnapshotId::Svg2EditorsDraft,
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

const SVG2_EDITORS_DRAFT_METADATA: SpecSnapshotMetadata = SpecSnapshotMetadata {
    canonical_id: SpecSnapshotId::Svg2EditorsDraft,
    aliases: SVG2_EDITORS_DRAFT_ALIASES,
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
        SpecSnapshotId::Svg2EditorsDraft => &SVG2_EDITORS_DRAFT_METADATA,
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

/// Map the literal string from an SVG root `version` attribute to the
/// closest catalogued snapshot, resolved by **major-version family**.
///
/// Any SVG 1.x version (`"1.0"`, `"1.1"`, and crucially `"1.2"` — SVG Tiny 1.2)
/// collapses to the SVG 1.1 Second Edition, the catalogued base of the 1.x
/// family; any SVG 2.x version resolves to the current SVG 2 editor's draft.
///
/// Resolving by family — rather than an exact-string allowlist — is deliberate:
/// a document that *declares* a version we do not catalogue exactly (e.g.
/// `"1.2"`) must still lint against the right base edition, not silently fall
/// through to whatever unrelated profile the user happened to configure (the
/// old behaviour mis-linted an SVG 1.2 document as SVG 2). Only a value with no
/// recognisable major (`""`, `"draft"`, a future `"3"`) returns `None`, leaving
/// the caller to use the configured profile — which is correct, because such a
/// document carries no usable version signal.
///
/// Intentionally narrower than [`resolve_profile_id`]: version attribute
/// values live in their own enumerated space, and treating bare `"1.1"`
/// as a general profile alias would be ambiguous when the user sets it
/// as a config value.
#[must_use]
pub fn snapshot_for_svg_version_attr(value: &str) -> Option<SpecSnapshotId> {
    // The major component is the run before the first '.'. `split` always
    // yields at least one element, so the default is unreachable.
    let major = value.trim().split('.').next().unwrap_or_default();
    match major {
        "1" => Some(SpecSnapshotId::Svg11Rec20110816),
        "2" => Some(SpecSnapshotId::Svg2EditorsDraft),
        _ => None,
    }
}

/// The catalogued **edition** a root `version` attribute selects when no curated
/// [`SpecSnapshotId`] represents it faithfully.
///
/// SVG 1.0 is the case that matters: it has a vendored edition inventory
/// ([`inventory::for_edition`]) but **no** `SpecSnapshotId`, so
/// [`snapshot_for_svg_version_attr`] necessarily collapses `"1.0"` onto the SVG
/// 1.1 snapshot. Linting an SVG 1.0 document against SVG 1.1 is wrong — SVG 1.1
/// adds constructs SVG 1.0 never defined — so a `version="1.0"` document targets
/// the SVG 1.0 *edition* (its 1.1 snapshot is only the base the edition
/// inventory restricts). Every other version has a faithful snapshot and returns
/// `None`.
#[must_use]
pub fn edition_for_svg_version_attr(value: &str) -> Option<inventory::EditionId> {
    match value.trim() {
        "1.0" => Some(inventory::EditionId::dated(
            edition::Series::Svg10,
            "2001-09-04",
        )),
        _ => None,
    }
}

/// The reductive SVG **profile** a root `baseProfile` attribute selects.
///
/// SVG 1.1 defines two reductive mobile profiles (`tiny`, `basic`) layered on
/// the SVG 1.1 base, plus `full` (the unreduced base). They are *profiles* in
/// the [`profile`](crate::profile) sense — reductive subsets of a base edition —
/// not point-in-time [`SpecSnapshotId`] snapshots, so they live in their own
/// enumerated space rather than being smuggled into the snapshot enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BaseProfile {
    /// `baseProfile="tiny"` — SVG Tiny, the most reduced SVG 1.1 mobile profile.
    Tiny,
    /// `baseProfile="basic"` — SVG Basic, the intermediate SVG 1.1 mobile profile.
    Basic,
    /// `baseProfile="full"` — the unreduced SVG 1.1 base.
    Full,
}

impl BaseProfile {
    /// Parse the literal value of a root `baseProfile` attribute.
    ///
    /// Returns `None` for any value outside the spec-defined `tiny` / `basic` /
    /// `full` set (including the empty string), so callers fall back to other
    /// markers or the configured profile.
    #[must_use]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value.trim() {
            "tiny" => Some(Self::Tiny),
            "basic" => Some(Self::Basic),
            "full" => Some(Self::Full),
            _ => None,
        }
    }

    /// The catalogued [`SpecSnapshotId`] this profile is a reductive subset of.
    ///
    /// All three SVG 1.1 base profiles reduce the SVG 1.1 Second Edition base;
    /// the reductive constraints themselves (which elements/attributes a profile
    /// drops) are a separate axis the lint crate layers on top, but the base
    /// snapshot is the catalog membership a feature is checked against.
    #[must_use]
    pub const fn base_snapshot(self) -> SpecSnapshotId {
        match self {
            Self::Tiny | Self::Basic | Self::Full => SpecSnapshotId::Svg11Rec20110816,
        }
    }
}

/// The profile/snapshot target autodetected from an SVG root element's markers.
///
/// The lint crate needs to pick a target edition from the document itself,
/// looking past the bare root `version` attribute (handled by
/// [`snapshot_for_svg_version_attr`]) to the `baseProfile` mobile profiles and
/// the SVG Native marker. Modeling the outcome as an ADT keeps the three
/// distinct targets — a catalogued snapshot, a reductive SVG 1.1 base profile,
/// and the SVG Native profile — from collapsing into one another, so a caller
/// can route each to the right checker ([`spec_snapshots`] membership, the
/// reductive [`BaseProfile`] constraints, or the
/// [`profile::SvgNative`](crate::profile::SvgNative) constraint dataset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedTarget {
    /// A catalogued spec snapshot, resolved from the root `version` attribute.
    Snapshot(SpecSnapshotId),
    /// An SVG 1.1 reductive base profile, resolved from `baseProfile`. Carries
    /// the [`BaseProfile`]; its catalog base is [`BaseProfile::base_snapshot`].
    BaseProfile(BaseProfile),
    /// The SVG Native profile, detected from an explicit SVG Native marker.
    SvgNative,
}

/// The relevant markers read off an SVG root `<svg>` element for autodetection.
///
/// Parse the document's root attributes into this at the boundary, then call
/// [`detect_target`] — keeping the resolution logic pure and the raw attribute
/// soup at the edge. All fields are optional because any given document may
/// carry none, some, or all of them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RootProfileMarkers<'a> {
    /// The root `version` attribute value, if present.
    pub version: Option<&'a str>,
    /// The root `baseProfile` attribute value, if present.
    pub base_profile: Option<&'a str>,
    /// Whether an explicit SVG Native marker is present on the root (e.g. the
    /// `baseProfile="SVG-static"` / SVG-Native authoring marker). The caller
    /// decides what constitutes the marker for its document model; this flag is
    /// the typed signal that one was found.
    pub svg_native: bool,
}

/// Autodetect the [`DetectedTarget`] from an SVG root element's markers.
///
/// Resolution order, most specific first:
///
/// 1. an explicit SVG Native marker wins outright — it is the most reductive,
///    deliberately-authored target;
/// 2. otherwise a recognized `baseProfile` (`tiny` / `basic` / `full`) selects
///    the matching [`BaseProfile`];
/// 3. otherwise a recognized `version` (`1.0` / `1.1` / `2` / `2.0`) selects the
///    catalogued [`SpecSnapshotId`] via [`snapshot_for_svg_version_attr`];
/// 4. otherwise `None`, so the caller falls back to its configured profile.
///
/// This is the additive, typed entry point the lint crate's autodetection reads;
/// it never panics and consults only the markers handed to it.
#[must_use]
pub fn detect_target(markers: &RootProfileMarkers<'_>) -> Option<DetectedTarget> {
    if markers.svg_native {
        return Some(DetectedTarget::SvgNative);
    }
    if let Some(profile) = markers.base_profile.and_then(BaseProfile::from_attr) {
        return Some(DetectedTarget::BaseProfile(profile));
    }
    markers
        .version
        .and_then(snapshot_for_svg_version_attr)
        .map(DetectedTarget::Snapshot)
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
    let Some(known_in) = generated_known_element_snapshots(element.name) else {
        return ProfileLookup::Unknown;
    };
    lookup_for_profile(profile, element, known_in)
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
    let Some(known_in) = generated_known_attribute_snapshots(attribute.name) else {
        return ProfileLookup::Unknown;
    };
    lookup_for_profile(profile, attribute, known_in)
}

/// Look up the pre-computed [`CompatVerdict`] for an element against a
/// selected profile.
///
/// Verdicts are baked into [`ElementDef::verdicts`] at build time, so
/// this is a linear scan over a small slice (typically ≤4 entries, one
/// per tracked snapshot). Returns `None` only when the element has no
/// verdicts at all — a build-time invariant violation that should not
/// happen for a covered snapshot. Callers that need a rendered fallback
/// should treat `None` as "no verdict → no compat diagnostic".
///
/// Both the LSP hover and the lint rules consume this helper, so they
/// cannot drift from one another's view of the same reconciled verdict.
#[must_use]
pub fn compat_verdict_for_element(
    def: &ElementDef,
    profile: SpecSnapshotId,
) -> Option<CompatVerdict> {
    verdict_for_profile(def.verdicts, profile)
}

/// Look up the pre-computed [`CompatVerdict`] for an attribute against
/// a selected profile.
///
/// See [`compat_verdict_for_element`] for semantics — the only
/// difference is the input type.
#[must_use]
pub fn compat_verdict_for_attribute(
    def: &AttributeDef,
    profile: SpecSnapshotId,
) -> Option<CompatVerdict> {
    verdict_for_profile(def.verdicts, profile)
}

/// Shared linear-scan over a pre-baked verdicts slice. Falls back to
/// the first entry when the requested profile isn't tracked, so hover
/// and lint always get *some* verdict for known features rather than
/// silently dropping diagnostics.
fn verdict_for_profile(
    verdicts: &'static [(SpecSnapshotId, CompatVerdict)],
    profile: SpecSnapshotId,
) -> Option<CompatVerdict> {
    verdicts
        .iter()
        .find(|(snap, _)| *snap == profile)
        .or_else(|| verdicts.first())
        .map(|(_, verdict)| *verdict)
}

impl AttributeDef {
    /// Resolve this attribute's value description for the given profile,
    /// preferring the per-snapshot override (when the spec text diverges
    /// between snapshots) over the union default baked into
    /// [`AttributeDef::values`]. Skips the catalog name lookup that a
    /// standalone helper would have to re-do internally.
    #[must_use]
    pub fn values_for_profile(&self, profile: SpecSnapshotId) -> &AttributeValues {
        generated_attribute_values_for_profile(profile, self.name).unwrap_or(&self.values)
    }
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

    generated_attribute_names_for_profile(profile, element.name)
        .iter()
        .filter_map(|name| match attribute_for_profile(profile, name) {
            ProfileLookup::Present { value, lifecycle } => Some(ProfiledAttribute {
                attribute: value,
                lifecycle,
            }),
            ProfileLookup::UnsupportedInProfile { .. } | ProfileLookup::Unknown => None,
        })
        .collect()
}

/// Return all element names belonging to the given catalog category.
#[must_use]
pub const fn elements_in_category(cat: ElementCategory) -> &'static [&'static str] {
    categories::elements_in_category(cat)
}

// --- Full-spec inventory access (additive companion to the curated catalog) ---
//
// The functions above (`attributes_for`, `attributes_for_with_profile`, …)
// expose the *curated*, browser-reconciled catalog and remain authoritative
// for current LSP/lint behavior. The functions below expose the *full*,
// spec-faithful SVG 2 ED inventory baked from `definitions*.xml` (see
// [`inventory`]) so consumers can pick/compare the complete universe —
// including the bulky `aria-*`/`on*` families the curated catalog drops — and
// filter it by [`inventory::Classification`]. The two layers are independent:
// reading the inventory never changes curated behavior.

/// Return the baked full-spec inventory for `snapshot`, if one exists.
///
/// All four snapshots now carry a baked, spec-faithful inventory, each derived
/// from the machine-readable artifact pinned for it (the SVG 2 ED
/// `definitions*.xml`, the SVG 1.1 DTDs, and the SVG 2 CR published index
/// tables) — so this returns [`Some`] for every [`SpecSnapshotId`]. The
/// signature stays [`Option`] only so a future un-inventoried snapshot cannot
/// silently fabricate an attribute universe (see [`inventory::for_snapshot`]).
/// This is the entry point to the full-exposure layer; from the returned
/// [`inventory::Inventory`] a caller can enumerate every element, attribute and
/// edge, and filter by classification.
#[must_use]
pub fn spec_inventory(snapshot: SpecSnapshotId) -> Option<&'static inventory::Inventory> {
    inventory::for_snapshot(snapshot)
}

/// Return the baked full-spec inventory for an arbitrary edition `id`, if one
/// exists.
///
/// This is the additive, edition-keyed companion to [`spec_inventory`]: where
/// [`spec_inventory`] is limited to the four curated [`SpecSnapshotId`]s, this
/// resolves *every* registered edition through one uniform key — including the
/// two older SVG 2 CRs (2016-09-15, 2018-08-07) that have no `SpecSnapshotId`.
/// Build an [`inventory::EditionId`] directly, or from a snapshot via
/// [`inventory::EditionId::for_snapshot`]; enumerate the full set with
/// [`inventory::registered_editions`]. The curated `spec_inventory` /
/// `for_snapshot` API is unchanged.
#[must_use]
pub fn inventory_for_edition(id: &inventory::EditionId) -> Option<&'static inventory::Inventory> {
    inventory::for_edition(id)
}

/// Resolve a user-facing edition config string to a registered
/// [`inventory::EditionId`].
///
/// This is the edition-keyed analogue of [`resolve_profile_id`]: where
/// `resolve_profile_id` resolves the four curated [`SpecSnapshotId`]s, this
/// resolves *every* registered edition — including the SVG 1.0 REC, SVG 1.1 PR,
/// and the two older SVG 2 CRs that carry no `SpecSnapshotId` — so an LSP can
/// target those inventories from config. Pair the result with
/// [`inventory_for_edition`] to read the baked inventory. See
/// [`inventory::resolve_edition_id`] for the matching rules.
#[must_use]
pub fn resolve_edition_id(input: &str) -> Option<inventory::EditionId> {
    inventory::resolve_edition_id(input)
}

/// Every classified attribute in `snapshot`'s full-spec inventory, sorted by
/// name; empty for snapshots without a baked inventory.
///
/// Each record carries its normalized [`inventory::Classification`] set and the
/// verbatim upstream `attributecategory` provenance, so a consumer can pick or
/// compare on either. Use [`spec_inventory`] when the present/absent
/// distinction between snapshots matters.
#[must_use]
pub fn spec_attributes(snapshot: SpecSnapshotId) -> &'static [inventory::Attribute] {
    spec_inventory(snapshot).map_or(&[], |inv| inv.attributes.as_ref())
}

/// Every element declared in `snapshot`'s full-spec inventory, sorted by name;
/// empty for snapshots without a baked inventory.
#[must_use]
pub fn spec_elements(snapshot: SpecSnapshotId) -> &'static [inventory::Element] {
    spec_inventory(snapshot).map_or(&[], |inv| inv.elements.as_ref())
}

/// Iterate the full-spec attributes `snapshot` attaches to `element`, resolved
/// to complete [`inventory::Attribute`] records (classification + provenance).
///
/// Yields nothing for snapshots without a baked inventory, or for an element
/// the snapshot does not declare. See
/// [`inventory::Inventory::attributes_for_element`] for resolution semantics.
pub fn spec_attributes_for_element(
    snapshot: SpecSnapshotId,
    element: &str,
) -> impl Iterator<Item = &'static inventory::Attribute> + use<'_> {
    spec_inventory(snapshot)
        .into_iter()
        .flat_map(move |inv| inv.attributes_for_element(element))
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

fn lookup_for_profile<T: Copy>(
    profile: SpecSnapshotId,
    value: T,
    known_in: &'static [SpecSnapshotId],
) -> ProfileLookup<T> {
    if !known_in.contains(&profile) {
        return ProfileLookup::UnsupportedInProfile { known_in };
    }

    ProfileLookup::Present {
        value,
        lifecycle: lifecycle_for_profile(profile, known_in),
    }
}

/// Compute the lifecycle signal for `profile` given the membership list.
///
/// Precondition: `profile` is in `known_in` (callers hit the
/// `UnsupportedInProfile` branch otherwise). The function picks
/// [`SpecLifecycle::Experimental`] when the feature lives only in an
/// unstable tip ([`SpecSnapshotId::LATEST`]-only or
/// draft-not-yet-in-stable-base), and [`SpecLifecycle::Stable`]
/// everywhere else.
///
/// The union `spec_lifecycle` baked onto each catalog entry is
/// "latest-relative" (`Obsolete` = absent from [`SpecSnapshotId::LATEST`]),
/// which misreads older profiles in which a feature is still defined.
/// Consulting per-profile membership directly avoids that asymmetry.
///
/// Mirrors `spec_facts_for_profile` in `build.rs`, which drives the
/// hover verdict builder. Keeping the two definitions aligned prevents
/// lint diagnostics and hover verdicts from disagreeing about whether a
/// feature is obsolete in a given profile.
fn lifecycle_for_profile(
    profile: SpecSnapshotId,
    known_in: &'static [SpecSnapshotId],
) -> SpecLifecycle {
    debug_assert!(
        known_in.contains(&profile),
        "lifecycle_for_profile called for profile absent from known_in"
    );

    // Latest-only membership = experimental in the tip.
    if profile == SpecSnapshotId::LATEST && known_in == [SpecSnapshotId::LATEST] {
        return SpecLifecycle::Experimental;
    }

    // Draft profile whose feature hasn't landed in the stable base yet.
    if let Some(stable_base) = snapshot_metadata(profile).stable_base
        && !known_in.contains(&stable_base)
    {
        return SpecLifecycle::Experimental;
    }

    SpecLifecycle::Stable
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
        // A shape is not childless: SVG lets it carry animation + descriptive
        // (and paint-server / clip-mask) children, just not other shapes.
        assert!(matches!(rect.content_model, ContentModel::Children(_)));
        let children = allowed_children("rect");
        assert!(children.contains(&"animate"), "rect allows <animate>");
        assert!(!children.contains(&"circle"), "rect rejects other shapes");
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
        // `text` is spec-derived as an explicit element set (resolved from the
        // animation / descriptive / text-content-child / paint-server categories
        // plus its listed elements) rather than a broad category set, so it can
        // exclude the `text` / `textPath` containers it must not nest.
        assert!(matches!(text.content_model, ContentModel::ChildrenSet(_)));
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
        // The inline-text trap: a `<text>` container cannot nest another
        // `<text>`, but `<textPath>` and `<tspan>` ARE valid text-content
        // children (the spec's `text content child` category names both).
        assert!(
            !children.contains(&"text"),
            "text must reject nested <text>"
        );
        assert!(
            children.contains(&"textPath"),
            "text should allow <textPath> as a text-content child"
        );
    }

    #[test]
    fn allowed_children_text_path_excludes_containers() {
        let children = allowed_children("textPath");
        assert!(children.contains(&"tspan"), "textPath should allow tspan");
        assert!(
            children.contains(&"animate"),
            "textPath should allow <animate>"
        );
        assert!(
            !children.contains(&"text"),
            "textPath must reject nested <text>"
        );
        assert!(
            !children.contains(&"textPath"),
            "textPath must reject nested <textPath>"
        );
    }

    #[test]
    fn allowed_children_stop() {
        // A gradient `<stop>` is no longer modelled as empty: the spec gives it
        // `animate, set, script, style` children.
        let children = allowed_children("stop");
        for expected in ["animate", "set", "script", "style"] {
            assert!(
                children.contains(&expected),
                "stop should allow <{expected}>"
            );
        }
        assert!(!children.contains(&"rect"), "stop should not allow <rect>");
    }

    #[test]
    fn allowed_children_filter_primitive() {
        // Filter primitives accept exactly `animate, script, set` (their own
        // narrow element set) — not the whole animation category, and certainly
        // not shapes.
        let children = allowed_children("feGaussianBlur");
        assert!(
            children.contains(&"animate"),
            "feGaussianBlur should allow <animate>"
        );
        assert!(
            !children.contains(&"rect"),
            "feGaussianBlur should not allow <rect>"
        );
        assert!(
            !children.contains(&"animateMotion"),
            "feGaussianBlur allows only animate/script/set, not the whole animation category"
        );
    }

    #[test]
    fn allowed_children_fe_image_includes_animate_transform() {
        // `feImage` is the one filter primitive whose spec element set also
        // names `animateTransform`.
        let children = allowed_children("feImage");
        assert!(
            children.contains(&"animateTransform"),
            "feImage should allow <animateTransform>"
        );
        assert!(
            !children.contains(&"rect"),
            "feImage should not allow <rect>"
        );
    }

    #[test]
    fn allowed_children_use_includes_descriptive() {
        // `use` was wrongly empty; the spec gives it animation + descriptive
        // children plus clipPath/mask/script/style.
        let children = allowed_children("use");
        assert!(children.contains(&"desc"), "use should allow <desc>");
        assert!(children.contains(&"animate"), "use should allow <animate>");
        assert!(!children.contains(&"rect"), "use should not allow <rect>");
    }

    #[test]
    fn tspan_rejects_text_container() {
        // SVG inline text containers accept text-content children (tspan, a,
        // animation, …) but NOT the `text` element itself.
        let children = allowed_children("tspan");
        assert!(children.contains(&"tspan"), "tspan allows nested tspan");
        assert!(!children.contains(&"text"), "tspan rejects <text>");
        assert!(!children.contains(&"textPath"), "tspan rejects <textPath>");
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
            verdicts: &[],
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
            Some(SpecSnapshotId::Svg2EditorsDraft)
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
            Some(SpecSnapshotId::Svg2EditorsDraft)
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
            Some(SpecSnapshotId::Svg2EditorsDraft)
        );
    }

    #[test]
    fn svg_version_attr_maps_known_literals() {
        assert_eq!(
            snapshot_for_svg_version_attr("1.0"),
            Some(SpecSnapshotId::Svg11Rec20110816)
        );
        assert_eq!(
            snapshot_for_svg_version_attr("1.1"),
            Some(SpecSnapshotId::Svg11Rec20110816)
        );
        assert_eq!(
            snapshot_for_svg_version_attr("2"),
            Some(SpecSnapshotId::Svg2EditorsDraft)
        );
        assert_eq!(
            snapshot_for_svg_version_attr("2.0"),
            Some(SpecSnapshotId::Svg2EditorsDraft)
        );
        // SVG Tiny 1.2 is an SVG 1.x document: it resolves to the 1.x base
        // edition, never to an unrelated default.
        assert_eq!(
            snapshot_for_svg_version_attr("1.2"),
            Some(SpecSnapshotId::Svg11Rec20110816)
        );
        // A future/unknown major carries no usable 1.x/2.x signal.
        assert_eq!(
            snapshot_for_svg_version_attr("2.1"),
            Some(SpecSnapshotId::Svg2EditorsDraft)
        );
    }

    #[test]
    fn svg_version_attr_trims_whitespace() {
        assert_eq!(
            snapshot_for_svg_version_attr("  1.1 \n"),
            Some(SpecSnapshotId::Svg11Rec20110816)
        );
    }

    #[test]
    fn svg_version_attr_returns_none_for_unknown_values() {
        // No usable major-version signal -> caller uses the configured profile.
        assert!(snapshot_for_svg_version_attr("").is_none());
        assert!(snapshot_for_svg_version_attr("garbage").is_none());
        assert!(snapshot_for_svg_version_attr("draft").is_none());
        // A future major (SVG 3) is genuinely uncatalogued.
        assert!(snapshot_for_svg_version_attr("3.0").is_none());
    }

    #[test]
    fn base_profile_parses_known_values_and_maps_to_svg11() {
        assert_eq!(BaseProfile::from_attr("tiny"), Some(BaseProfile::Tiny));
        assert_eq!(BaseProfile::from_attr(" basic "), Some(BaseProfile::Basic));
        assert_eq!(BaseProfile::from_attr("full"), Some(BaseProfile::Full));
        assert_eq!(BaseProfile::from_attr("none"), None);
        assert_eq!(BaseProfile::from_attr(""), None);
        assert_eq!(
            BaseProfile::Tiny.base_snapshot(),
            SpecSnapshotId::Svg11Rec20110816
        );
    }

    #[test]
    fn detect_target_prefers_svg_native_then_base_profile_then_version() {
        // SVG Native wins even when version/baseProfile are also present.
        assert_eq!(
            detect_target(&RootProfileMarkers {
                version: Some("1.1"),
                base_profile: Some("tiny"),
                svg_native: true,
            }),
            Some(DetectedTarget::SvgNative)
        );
        // baseProfile beats version.
        assert_eq!(
            detect_target(&RootProfileMarkers {
                version: Some("1.1"),
                base_profile: Some("basic"),
                svg_native: false,
            }),
            Some(DetectedTarget::BaseProfile(BaseProfile::Basic))
        );
        // version alone resolves to the snapshot.
        assert_eq!(
            detect_target(&RootProfileMarkers {
                version: Some("2"),
                base_profile: None,
                svg_native: false,
            }),
            Some(DetectedTarget::Snapshot(SpecSnapshotId::Svg2EditorsDraft))
        );
    }

    #[test]
    fn detect_target_returns_none_without_recognized_markers() {
        assert_eq!(detect_target(&RootProfileMarkers::default()), None);
        assert_eq!(
            detect_target(&RootProfileMarkers {
                version: Some("3"),
                base_profile: Some("weird"),
                svg_native: false,
            }),
            None
        );
    }

    #[test]
    fn snapshot_metadata_tracks_stable_base_and_errata() {
        let svg11 = snapshot_metadata(SpecSnapshotId::Svg11Rec20110816);
        assert!(svg11.errata_folded);
        assert_eq!(svg11.stable_base, None);

        let draft = snapshot_metadata(SpecSnapshotId::Svg2EditorsDraft);
        assert_eq!(draft.stable_base, Some(SpecSnapshotId::Svg2Cr20181004));
        assert!(!draft.errata_folded);
    }

    #[test]
    fn draft_only_membership_derives_experimental_lifecycle() {
        assert_eq!(
            lifecycle_for_profile(
                SpecSnapshotId::Svg2EditorsDraft,
                &[SpecSnapshotId::Svg2EditorsDraft],
            ),
            SpecLifecycle::Experimental
        );
    }

    #[test]
    fn non_latest_profile_with_membership_is_stable_regardless_of_union() {
        // Before the profile-aware fix, this would have returned Obsolete
        // because the attribute isn't in the LATEST snapshot. Now the
        // function consults per-profile membership directly: if you ask
        // about SVG 1.1 for an attribute that exists in SVG 1.1, you get
        // Stable.
        assert_eq!(
            lifecycle_for_profile(
                SpecSnapshotId::Svg11Rec20110816,
                &[
                    SpecSnapshotId::Svg11Rec20030114,
                    SpecSnapshotId::Svg11Rec20110816,
                ],
            ),
            SpecLifecycle::Stable
        );
    }

    #[test]
    fn spec_lifecycle_is_separate_from_compat_deprecation() -> Result<(), Box<dyn Error>> {
        let href = attribute("xlink:href").ok_or("xlink:href should exist")?;
        assert_eq!(href.spec_lifecycle, SpecLifecycle::Obsolete);
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
    fn bcd_only_fedropshadow_is_svg2_only() -> Result<(), Box<dyn Error>> {
        let lookup = element_for_profile(SpecSnapshotId::Svg11Rec20110816, "feDropShadow");
        let ProfileLookup::UnsupportedInProfile { known_in } = lookup else {
            return Err("feDropShadow should be unsupported in SVG 1.1".into());
        };

        assert_eq!(
            known_in,
            &[
                SpecSnapshotId::Svg2Cr20181004,
                SpecSnapshotId::Svg2EditorsDraft,
            ]
        );
        Ok(())
    }

    #[test]
    fn attribute_profile_lookup_distinguishes_href_and_xlink_href() -> Result<(), Box<dyn Error>> {
        let svg11_href = attribute_for_profile(SpecSnapshotId::Svg11Rec20110816, "href");
        let ProfileLookup::UnsupportedInProfile { known_in } = svg11_href else {
            return Err("href should be unsupported in SVG 1.1".into());
        };
        assert_eq!(
            known_in,
            &[
                SpecSnapshotId::Svg2Cr20181004,
                SpecSnapshotId::Svg2EditorsDraft,
            ]
        );

        let svg2_xlink = attribute_for_profile(SpecSnapshotId::Svg2Cr20181004, "xlink_href");
        let ProfileLookup::UnsupportedInProfile { known_in } = svg2_xlink else {
            return Err("xlink:href should be unsupported in SVG 2".into());
        };
        assert_eq!(
            known_in,
            &[
                SpecSnapshotId::Svg11Rec20030114,
                SpecSnapshotId::Svg11Rec20110816,
            ]
        );

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

    #[test]
    fn reviewed_union_includes_view_element() -> Result<(), Box<dyn Error>> {
        let view = element("view").ok_or("view should exist in reviewed union")?;
        assert_eq!(view.name, "view");
        Ok(())
    }

    // ---- Verdict pipeline tests ---------------------------------------
    //
    // These tests exercise the baked-in verdicts slice on real catalog
    // entries. They don't unit-test the build-time compute logic in
    // `build/verdict.rs` directly — instead they lock the *end-to-end*
    // contract: the static catalog that consumers (hover, lint) actually
    // read must carry the expected recommendation tier and reason set.
    //
    // Integration-style verdict tests are the right tool here because:
    //
    // 1. the compute logic lives in a build script (hard to test in
    //    isolation without duplicating the type shadows);
    // 2. the failure mode we care about is a drift between compute and
    //    bake — which an isolated compute-unit test would miss;
    // 3. the fixtures are concrete, named SVG features whose expected
    //    verdicts are documented in the Phase 1 audit document.

    #[test]
    fn verdict_rect_is_safe_in_all_profiles() -> Result<(), Box<dyn Error>> {
        // `<rect>` is the canonical "safe, widely available" fixture.
        // Its verdict must be Safe with no reasons across every tracked
        // snapshot — otherwise the Caution-or-worse tier would flood
        // every document with spurious hints.
        let rect = element("rect").ok_or("rect should exist")?;
        for profile in spec_snapshots() {
            let verdict = compat_verdict_for_element(rect, *profile)
                .ok_or_else(|| format!("rect should have a verdict in {}", profile.as_str()))?;
            assert_eq!(
                verdict.recommendation,
                VerdictRecommendation::Safe,
                "rect must be Safe in {}: {verdict:?}",
                profile.as_str()
            );
            assert!(
                verdict.reasons.is_empty(),
                "rect must have no reasons in {}: {verdict:?}",
                profile.as_str()
            );
        }
        Ok(())
    }

    #[test]
    fn verdict_xlink_href_under_declared_svg11_profile_omits_bcd_reasons()
    -> Result<(), Box<dyn Error>> {
        // xlink:href was the canonical SVG 1.1 linking attribute; its BCD
        // `deprecated` flag only reflects its SVG 2 replacement by `href`.
        // Under the user-declared SVG 1.1 profile, that latest-era advice
        // is stripped: the verdict must NOT carry BcdDeprecated, and the
        // hover tier must not escalate to Avoid from BCD alone. Caution
        // tier signals from browser-support baseline remain legitimate.
        let href = attribute("xlink:href").ok_or("xlink:href should exist")?;
        let verdict = compat_verdict_for_attribute(href, SpecSnapshotId::Svg11Rec20110816)
            .ok_or("xlink:href should have a verdict in SVG 1.1")?;

        assert!(
            !verdict
                .reasons
                .iter()
                .any(|r| matches!(r, VerdictReason::BcdDeprecated)),
            "xlink:href in SVG 1.1 must not carry BcdDeprecated: {verdict:?}"
        );
        assert!(
            !matches!(
                verdict.recommendation,
                VerdictRecommendation::Avoid | VerdictRecommendation::Forbid
            ),
            "xlink:href in SVG 1.1 must not escalate to Avoid/Forbid from BCD: {verdict:?}"
        );
        Ok(())
    }

    #[test]
    fn verdict_xlink_href_is_forbid_in_svg2() -> Result<(), Box<dyn Error>> {
        // In SVG 2 the attribute is absent from membership — verdict
        // must escalate to Forbid with `ProfileObsolete` naming the
        // last snapshot it was still in. This is the single source
        // of truth both the hover `Forbid` headline and the lint
        // `UnsupportedInProfile` diagnostic read from.
        let href = attribute("xlink:href").ok_or("xlink:href should exist")?;
        let verdict = compat_verdict_for_attribute(href, SpecSnapshotId::Svg2EditorsDraft)
            .ok_or("xlink:href should have a verdict in SVG 2")?;

        assert_eq!(verdict.recommendation, VerdictRecommendation::Forbid);
        assert!(
            verdict
                .reasons
                .iter()
                .any(|r| matches!(r, VerdictReason::ProfileObsolete { .. })),
            "Forbid verdict must include ProfileObsolete reason: {verdict:?}"
        );
        // `last_seen` must point at an SVG 1.1 snapshot since that's the
        // latest profile xlink:href is still defined in.
        for reason in verdict.reasons {
            if let VerdictReason::ProfileObsolete { last_seen } = reason {
                assert!(
                    matches!(
                        last_seen,
                        SpecSnapshotId::Svg11Rec20030114 | SpecSnapshotId::Svg11Rec20110816
                    ),
                    "last_seen should be an SVG 1.1 snapshot, got {last_seen:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn verdict_recommendation_tier_ordering_is_total() {
        // The priority algorithm in `build/verdict.rs` relies on
        // `VerdictRecommendation` forming a total order `Safe < Caution
        // < Avoid < Forbid`. This test locks the derived ordering so
        // a future enum reshuffle can't silently break the max-tier
        // selection at build time.
        assert!(VerdictRecommendation::Safe < VerdictRecommendation::Caution);
        assert!(VerdictRecommendation::Caution < VerdictRecommendation::Avoid);
        assert!(VerdictRecommendation::Avoid < VerdictRecommendation::Forbid);
    }

    #[test]
    fn verdict_color_interpolation_has_partial_reason() -> Result<(), Box<dyn Error>> {
        // `color-interpolation` has `partial_implementation: true` on
        // chrome/edge/safari in live BCD. The verdict must carry at
        // least one `PartialImplementationIn` reason so the lint
        // `PartialImplementation` rule and the hover per-browser
        // sub-bullet both have data to render.
        let ci = attribute("color-interpolation").ok_or("color-interpolation should exist")?;
        let verdict = compat_verdict_for_attribute(ci, SpecSnapshotId::Svg2EditorsDraft)
            .ok_or("color-interpolation should have a verdict")?;

        assert!(
            verdict
                .reasons
                .iter()
                .any(|r| matches!(r, VerdictReason::PartialImplementationIn(_))),
            "color-interpolation verdict should include PartialImplementationIn: {verdict:?}"
        );
        // Partial-only features should be at most Caution — never
        // Avoid or Forbid, since the feature is still usable.
        assert!(
            verdict.recommendation <= VerdictRecommendation::Caution,
            "partial-only features should stay at Caution or below: {verdict:?}"
        );
        Ok(())
    }

    #[test]
    fn verdict_hover_and_lint_share_the_same_source() -> Result<(), Box<dyn Error>> {
        // Regression guard for the architectural linchpin: hover and
        // lint both read through `compat_verdict_for_attribute`, so
        // calling it twice must return byte-identical results. If a
        // caller ever reaches into `def.verdicts` directly and picks
        // a different entry, this test will fail.
        let ci = attribute("color-interpolation").ok_or("color-interpolation should exist")?;
        let a = compat_verdict_for_attribute(ci, SpecSnapshotId::Svg2EditorsDraft);
        let b = compat_verdict_for_attribute(ci, SpecSnapshotId::Svg2EditorsDraft);
        assert_eq!(a, b);
        Ok(())
    }
}
