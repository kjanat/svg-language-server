//! Map the extracted spec data into the committed catalog JSON that
//! `svg-data`'s `build.rs` turns into static `ElementDef` arrays.
//!
//! The extraction types mirror the spec's own shapes; this module reshapes them
//! into the runtime catalog's vocabulary. `catalog.core.json` is therefore not
//! a raw spec dump: it is extracted input plus semantic projection that the
//! runtime consumes directly. Policy that invents or over-approximates meaning
//! (for example value-shape classification, content-model fallbacks, and legacy
//! name canonicalization) must be explicit in this module rather than hidden in
//! fetch/parse code. The structural content model is flattened here (categories
//! resolved to their member elements, unioned with the explicit allowed
//! elements) so the runtime never depends on a category enum staying in sync
//! with the spec's taxonomy. The output is sorted, so the same upstream commit
//! always yields byte-identical JSON.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::Serialize;

use crate::{
    chapter::{AnchorDescription, PropertyValueDef},
    compat::CompatCatalog,
    extract::{AttributeRef, ContentModelKind, Definitions, PropertyDef},
    util::{boxed, is_keyword_token, normalize_ws},
};

/// The full derived catalog kept in memory while writing split JSON files.
#[derive(Debug)]
pub struct Catalog {
    /// Version of the JSON catalog/schema contract.
    pub schema_version: u16,
    /// The upstream commit this catalog was derived from.
    pub commit: String,
    /// Browser-compat data sources used for objective compat facts.
    pub compat: Option<CatalogCompatProvenance>,
    /// Authoritative legacy-profile sources used for snapshot-specific data.
    pub legacy_sources: Vec<CatalogLegacySource>,
    /// Version-specific overlay documents, sorted by profile.
    pub snapshots: Vec<CatalogSnapshotRef>,
    /// Element definitions, sorted by name.
    pub elements: Vec<CatalogElement>,
    /// Attribute definitions, sorted by canonical name.
    pub attributes: Vec<CatalogAttribute>,
    /// Derived graph view over the catalog.
    pub graph: CatalogGraph,
    /// Tree-sitter parser projection derived from catalog value facts.
    pub tree_sitter: CatalogTreeSitterDocument,
}

/// Root manifest written to `svg-data/data/catalog.json`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CatalogManifest {
    /// Version of the JSON catalog/schema contract.
    pub schema_version: u16,
    /// The upstream commit this catalog was derived from.
    pub commit: String,
    /// Canonical latest element/attribute catalog document.
    pub core: CatalogFileRef,
    /// Browser-compat provenance and retained compat subfeatures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compat: Option<CatalogFileRef>,
    /// Derived relationship graph document.
    pub graph: CatalogFileRef,
    /// Tree-sitter parser projection document.
    pub tree_sitter: CatalogFileRef,
    /// Version-specific overlay documents, sorted by profile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snapshots: Vec<CatalogSnapshotRef>,
}

/// Relative reference from one catalog document to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogFileRef {
    /// Relative path from `catalog.json` to the JSON file.
    pub href: String,
}

/// Canonical latest catalog data after semantic projection.
///
/// This document is the runtime-facing catalog, not a raw upstream XML/HTML mirror.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CatalogCore<'a> {
    /// Version of the JSON catalog/schema contract.
    pub schema_version: u16,
    /// Element definitions, sorted by name.
    pub elements: &'a [CatalogElement],
    /// Attribute definitions, sorted by canonical name.
    pub attributes: &'a [CatalogAttribute],
}

/// Browser-compat provenance and retained non-catalogued compat features.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CatalogCompatDocument<'a> {
    /// Version of the JSON catalog/schema contract.
    pub schema_version: u16,
    /// MDN browser-compat-data package source.
    pub browser_compat_data: &'a CatalogPackageSource,
    /// web-features package source.
    pub web_features: &'a CatalogPackageSource,
    /// BCD features intentionally not modeled as elements/attributes.
    #[serde(default, skip_serializing_if = "is_empty_slice")]
    pub unmodeled_features: &'a [CatalogCompatSubfeature],
}

/// Derived graph document.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CatalogGraphDocument<'a> {
    /// Version of the JSON catalog/schema contract.
    pub schema_version: u16,
    /// Graph nodes, sorted by id.
    pub nodes: &'a [CatalogGraphNode],
    /// Directed graph edges, sorted by `(from, to, kind)`.
    pub edges: &'a [CatalogGraphEdge],
}

/// Tree-sitter-facing projection of parser facts from the canonical catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogTreeSitterDocument {
    /// Version of the JSON catalog/schema contract.
    pub schema_version: u16,
    /// Sources used to derive non-SVG grammar tokens.
    pub sources: CatalogTreeSitterSources,
    /// Attribute-name buckets consumed by `grammars/tree-sitter-svg`.
    pub attribute_buckets: CatalogTreeSitterAttributeBuckets,
    /// Shared token sets consumed by `grammars/tree-sitter-svg`.
    pub tokens: CatalogTreeSitterTokens,
}

/// Source provenance for generated grammar facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogTreeSitterSources {
    /// CSS definitions source package.
    pub webref_css: CatalogPackageSource,
    /// CSS Values URLs used for unit extraction.
    pub css_unit_pages: Vec<String>,
    /// SVG Animations URL used for SVG clock units.
    pub svg_clock_value_syntax: String,
    /// Pinned SVGWG `paths.html` URL used for path-data grammar facts.
    pub paths_html: String,
}

/// Attribute name groups that share a parser value shape.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, JsonSchema)]
pub struct CatalogTreeSitterAttributeBuckets {
    /// Attribute values represented as bare keyword tokens.
    pub keyword: Vec<String>,
    /// Attribute values represented by the color parser.
    pub color: Vec<String>,
    /// Attribute values represented by a single length/percentage parser.
    pub length: Vec<String>,
    /// Attribute values represented by a length/percentage list parser.
    pub length_list: Vec<String>,
    /// Attribute values represented by `none` or a length/percentage list.
    pub length_list_or_none: Vec<String>,
    /// Attribute values represented by a bare number parser.
    pub number: Vec<String>,
    /// Attribute values represented by one or two whitespace-separated numbers
    /// (CSS `<number-optional-number>`, e.g. `stdDeviation`, `baseFrequency`).
    pub number_optional_number: Vec<String>,
    /// Attribute values represented by a list of bare numbers.
    pub number_list: Vec<String>,
    /// Attribute values represented by number or percentage.
    pub number_or_percentage: Vec<String>,
    /// Attribute values represented by coordinate pairs.
    pub coordinate_pair_list: Vec<String>,
    /// Attribute values represented by SVG path data.
    pub path_data: Vec<String>,
    /// Attribute values represented by an SVG viewBox.
    pub view_box: Vec<String>,
    /// Attribute values that can contain a functional IRI / URL reference.
    pub functional_iri: Vec<String>,
    /// Remaining structured CSS grammars that should be parsed as CSS text.
    pub css_text: Vec<String>,
}

/// Token sets shared by multiple generated grammar rules.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, JsonSchema)]
pub struct CatalogTreeSitterTokens {
    /// CSS length units from CSS Values.
    pub length_units: Vec<String>,
    /// CSS angle units from CSS Values.
    pub angle_units: Vec<String>,
    /// SVG clock value metric units from SVG Animations.
    pub time_units: Vec<String>,
    /// Predefined color spaces accepted by CSS `color()`.
    pub color_spaces: Vec<String>,
    /// Color interpolation spaces accepted by CSS `color-mix()`.
    pub color_interpolation_spaces: Vec<String>,
    /// Hue interpolation direction keywords.
    pub hue_interpolation_methods: Vec<String>,
    /// Single-letter path command tokens from SVGWG `paths.html#PathDataBNF`.
    pub path_command_letters: Vec<String>,
}

/// One element's spec-derived catalog entry.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CatalogElement {
    /// Element tag name.
    pub name: String,
    /// Short human-readable description from spec prose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MDN reference URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mdn_url: Option<String>,
    /// Resolved spec permalink (the module's anchor base joined with the href).
    pub spec_url: Option<String>,
    /// Whether compat data marks the element deprecated.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub deprecated: bool,
    /// Whether compat data marks the element experimental.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub experimental: bool,
    /// Whether compat data marks the element as standards-track.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard_track: Option<bool>,
    /// Web-platform baseline status, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<CatalogBaselineStatus>,
    /// Per-browser support data, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_support: Option<CatalogBrowserSupport>,
    /// Structural child-content model.
    pub content_model: CatalogContentModel,
    /// Element-specific attribute names (sorted, deduped).
    pub attrs: Vec<String>,
    /// Whether the element carries the SVG global (`core`) attributes.
    pub global_attrs: bool,
}

/// One attribute's spec-derived catalog entry.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CatalogAttribute {
    /// Canonical attribute name (`xlink:href` collapses to `href`).
    pub name: String,
    /// Short human-readable description from spec prose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MDN reference URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mdn_url: Option<String>,
    /// Resolved spec permalink.
    pub spec_url: Option<String>,
    /// Whether compat data marks the attribute deprecated.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub deprecated: bool,
    /// Whether compat data marks the attribute experimental.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub experimental: bool,
    /// Whether compat data marks the attribute as standards-track.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard_track: Option<bool>,
    /// Whether the spec marks the attribute animatable.
    pub animatable: bool,
    /// Backing CSS property name when this is a presentation attribute.
    pub presentation_attribute: Option<String>,
    /// Web-platform baseline status, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<CatalogBaselineStatus>,
    /// Per-browser support data, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_support: Option<CatalogBrowserSupport>,
    /// Element-scoped compat facts for attributes whose BCD data differs by bearer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub element_compat: Vec<CatalogAttributeElementCompat>,
    /// Element-scoped value-space overrides for attributes whose value grammar
    /// genuinely differs by bearer element within the same profile (e.g.
    /// `operator` on `feComposite` vs `feMorphology`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub element_values: Vec<CatalogAttributeElementValues>,
    /// Per-profile value-space overrides, when older snapshots accepted a
    /// different grammar. Consumed when building per-snapshot artifacts; never
    /// serialized into the core catalog, so it is also omitted from the schema.
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub value_overrides: Vec<CatalogAttributeValueOverride>,
    /// Attribute value space.
    pub values: CatalogAttributeValues,
    /// Which elements accept the attribute.
    pub applicability: CatalogAttributeApplicability,
}

/// Browser-compat package provenance for objective catalog facts.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CatalogCompatProvenance {
    /// MDN browser-compat-data package source.
    pub browser_compat_data: CatalogPackageSource,
    /// web-features package source.
    pub web_features: CatalogPackageSource,
    /// BCD features intentionally not modeled as elements/attributes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unmodeled_features: Vec<CatalogCompatSubfeature>,
}

/// One npm package source used during regeneration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogPackageSource {
    /// Package name.
    pub name: String,
    /// Resolved package version.
    pub version: String,
    /// Exact data URL fetched.
    pub url: String,
}

/// One legacy SVG profile source used to derive snapshot-specific facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogLegacySource {
    /// Human-readable source label.
    pub name: String,
    /// Snapshot this source represents.
    pub profile: CatalogSpecSnapshotId,
    /// Exact source URL fetched.
    pub url: String,
}

/// Attribute compat facts scoped to one element bearer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogAttributeElementCompat {
    /// Element name this compat record applies to.
    pub element: String,
    /// Objective compat facts for this attribute on `element`.
    #[serde(flatten)]
    pub facts: CatalogCompatFacts,
}

/// One element-scoped value-space override for an attribute whose value grammar
/// genuinely differs by bearer element within the same profile.
///
/// The canonical [`CatalogAttribute::values`] holds the value space of the
/// first bearer (by sorted element name); each diverging bearer is recorded
/// here so no spec-defined value space is lost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogAttributeElementValues {
    /// Element this value-space override applies to.
    pub element: String,
    /// Value space when the attribute is borne by `element`.
    pub values: CatalogAttributeValues,
}

/// One per-profile value-space override for an attribute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogAttributeValueOverride {
    /// Profile snapshot where this value grammar applies.
    pub profile: CatalogSpecSnapshotId,
    /// Value space for that profile.
    pub values: CatalogAttributeValues,
}

/// Reference from the root catalog to a version-specific overlay document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogSnapshotRef {
    /// Profile snapshot this overlay describes.
    pub profile: CatalogSpecSnapshotId,
    /// Relative path from `catalog.json` to the overlay JSON file.
    pub href: String,
}

/// Version-specific overlay document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogSnapshot {
    /// Version of the JSON catalog/schema contract.
    pub schema_version: u16,
    /// Profile snapshot this overlay describes.
    pub profile: CatalogSpecSnapshotId,
    /// Accepted profile id aliases for this snapshot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Exact source URLs used to derive this snapshot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    /// Per-snapshot element/attribute inventory.
    pub inventory: CatalogSnapshotInventory,
    /// Profile lifecycle facts derived from snapshot membership.
    #[serde(default, skip_serializing_if = "CatalogSnapshotLifecycle::is_empty")]
    pub lifecycle: CatalogSnapshotLifecycle,
    /// Per-profile value-space overrides for this snapshot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub value_overrides: Vec<CatalogSnapshotValueOverride>,
}

/// Per-snapshot lifecycle overlay.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, JsonSchema)]
pub struct CatalogSnapshotLifecycle {
    /// Element lifecycle facts that differ from ordinary stable presence.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<CatalogLifecycleEntry>,
    /// Attribute lifecycle facts that differ from ordinary stable presence.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<CatalogLifecycleEntry>,
}

impl CatalogSnapshotLifecycle {
    const fn is_empty(&self) -> bool {
        self.elements.is_empty() && self.attributes.is_empty()
    }
}

/// One feature lifecycle fact in a snapshot overlay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogLifecycleEntry {
    /// Feature name as written in this profile family (`xlink:href` stays
    /// distinct from `href`).
    pub name: String,
    /// Canonical catalog attribute name, when different from `name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_name: Option<String>,
    /// Whether the feature is present in the containing snapshot.
    pub present: bool,
    /// Lifecycle status for this snapshot.
    pub lifecycle: CatalogLifecycleStatus,
    /// Snapshots where this feature is present.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_in: Vec<CatalogSpecSnapshotId>,
}

/// Lifecycle statuses the snapshot overlay can derive from profile membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CatalogLifecycleStatus {
    /// Present and stable, included only when the overlay must carry profile
    /// spelling metadata.
    Stable,
    /// Present only in a draft snapshot.
    Experimental,
    /// Known in earlier snapshots, absent from this one.
    Obsolete,
    /// Known in later snapshots, absent from this one.
    NotYetIntroduced,
}

/// Per-snapshot element/attribute inventory payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogSnapshotInventory {
    /// Elements present in this profile.
    pub elements: Vec<CatalogInventoryElement>,
    /// Attributes present anywhere in this profile.
    pub attributes: Vec<String>,
}

/// One value-space override scoped by the containing snapshot document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogSnapshotValueOverride {
    /// Attribute/property name whose value space is overridden.
    pub attribute: String,
    /// Value space for this snapshot.
    pub values: CatalogAttributeValues,
}

/// Per-snapshot element/attribute inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogInventory {
    /// Profile snapshot this inventory describes.
    pub profile: CatalogSpecSnapshotId,
    /// Exact source URLs used to derive this inventory.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    /// Elements present in this profile.
    pub elements: Vec<CatalogInventoryElement>,
    /// Attributes present anywhere in this profile.
    pub attributes: Vec<String>,
}

/// One element's per-snapshot attribute inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogInventoryElement {
    /// Element name.
    pub name: String,
    /// Attribute names this profile lists for the element.
    pub attributes: Vec<String>,
}

/// SVG specification snapshots understood by the catalog contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
pub enum CatalogSpecSnapshotId {
    /// SVG 1.1 First Edition (W3C REC 2003-01-14).
    Svg11Rec20030114,
    /// SVG 1.1 Second Edition (W3C REC 2011-08-16).
    Svg11Rec20110816,
    /// SVG 2 Candidate Recommendation (2018-10-04).
    Svg2Cr20181004,
    /// SVG 2 Editor's Draft (rolling).
    Svg2EditorsDraft,
}

impl CatalogSnapshot {
    /// Convert the in-memory inventory shape into its committed overlay file.
    #[must_use]
    pub fn from_inventory(
        inventory: &CatalogInventory,
        inventories: &[CatalogInventory],
        attributes: &[CatalogAttribute],
        legacy_sources: &[CatalogLegacySource],
    ) -> Self {
        let sources = inventory
            .sources
            .iter()
            .cloned()
            .chain(
                legacy_sources
                    .iter()
                    .filter(|source| source.profile == inventory.profile)
                    .map(|source| source.url.clone()),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let value_overrides = attributes
            .iter()
            .flat_map(|attribute| {
                attribute
                    .value_overrides
                    .iter()
                    .filter(move |override_| override_.profile == inventory.profile)
                    .map(|override_| CatalogSnapshotValueOverride {
                        attribute: attribute.name.clone(),
                        values: override_.values.clone(),
                    })
            })
            .collect();
        Self {
            schema_version: crate::schema::CATALOG_SCHEMA_VERSION,
            profile: inventory.profile,
            aliases: catalog_snapshot_aliases(inventory.profile)
                .iter()
                .map(|alias| (*alias).to_owned())
                .collect(),
            sources,
            inventory: CatalogSnapshotInventory {
                elements: inventory.elements.clone(),
                attributes: inventory.attributes.clone(),
            },
            lifecycle: derive_snapshot_lifecycle(inventory.profile, inventories, attributes),
            value_overrides,
        }
    }
}

impl Catalog {
    /// Root manifest for the split catalog.
    #[must_use]
    pub fn manifest(&self) -> CatalogManifest {
        CatalogManifest {
            schema_version: self.schema_version,
            commit: self.commit.clone(),
            core: CatalogFileRef {
                href: CATALOG_CORE_HREF.to_owned(),
            },
            compat: self.compat.as_ref().map(|_| CatalogFileRef {
                href: CATALOG_COMPAT_HREF.to_owned(),
            }),
            graph: CatalogFileRef {
                href: CATALOG_GRAPH_HREF.to_owned(),
            },
            tree_sitter: CatalogFileRef {
                href: CATALOG_TREE_SITTER_HREF.to_owned(),
            },
            snapshots: self.snapshots.clone(),
        }
    }

    /// Canonical latest core document.
    #[must_use]
    pub fn core_document(&self) -> CatalogCore<'_> {
        CatalogCore {
            schema_version: self.schema_version,
            elements: &self.elements,
            attributes: &self.attributes,
        }
    }

    /// Browser-compat companion document, when compat data was available.
    #[must_use]
    pub fn compat_document(&self) -> Option<CatalogCompatDocument<'_>> {
        self.compat.as_ref().map(|compat| CatalogCompatDocument {
            schema_version: self.schema_version,
            browser_compat_data: &compat.browser_compat_data,
            web_features: &compat.web_features,
            unmodeled_features: &compat.unmodeled_features,
        })
    }

    /// Derived graph companion document.
    #[must_use]
    pub fn graph_document(&self) -> CatalogGraphDocument<'_> {
        CatalogGraphDocument {
            schema_version: self.schema_version,
            nodes: &self.graph.nodes,
            edges: &self.graph.edges,
        }
    }

    /// Tree-sitter parser projection companion document.
    #[must_use]
    pub const fn tree_sitter_document(&self) -> &CatalogTreeSitterDocument {
        &self.tree_sitter
    }
}

/// Relative JSON file path for canonical latest catalog data.
pub const CATALOG_CORE_HREF: &str = "catalog.core.json";
/// Relative JSON file path for browser-compat companion data.
pub const CATALOG_COMPAT_HREF: &str = "catalog.compat.json";
/// Relative JSON file path for the derived relationship graph.
pub const CATALOG_GRAPH_HREF: &str = "catalog.graph.json";
/// Relative JSON file path for the tree-sitter parser projection.
pub const CATALOG_TREE_SITTER_HREF: &str = "catalog.tree-sitter.json";

/// Relative JSON file path for the snapshot overlay for `profile`.
#[must_use]
pub const fn catalog_snapshot_href(profile: CatalogSpecSnapshotId) -> &'static str {
    match profile {
        CatalogSpecSnapshotId::Svg11Rec20030114 => "snapshots/svg11-rec-20030114.json",
        CatalogSpecSnapshotId::Svg11Rec20110816 => "snapshots/svg11-rec-20110816.json",
        CatalogSpecSnapshotId::Svg2Cr20181004 => "snapshots/svg2-cr-20181004.json",
        CatalogSpecSnapshotId::Svg2EditorsDraft => "snapshots/svg2-editors-draft.json",
    }
}

/// Accepted profile id aliases for `profile`.
#[must_use]
pub const fn catalog_snapshot_aliases(profile: CatalogSpecSnapshotId) -> &'static [&'static str] {
    match profile {
        CatalogSpecSnapshotId::Svg11Rec20030114 => {
            &["svg11rec20030114", "svg11-20030114", "svg1.1-20030114"]
        }
        CatalogSpecSnapshotId::Svg11Rec20110816 => &[
            "svg11",
            "svg1.1",
            "1.1",
            "svg11rec20110816",
            "svg11-20110816",
        ],
        CatalogSpecSnapshotId::Svg2Cr20181004 => {
            &["svg2cr", "svg2-cr", "svg2cr20181004", "svg2-20181004"]
        }
        CatalogSpecSnapshotId::Svg2EditorsDraft => &[
            "svg2",
            "svg2.0",
            "2",
            "2.0",
            "svg2draft",
            "svg2-draft",
            "latest",
        ],
    }
}

fn derive_snapshot_lifecycle(
    profile: CatalogSpecSnapshotId,
    inventories: &[CatalogInventory],
    attributes: &[CatalogAttribute],
) -> CatalogSnapshotLifecycle {
    let catalog_attribute_names: BTreeSet<&str> = attributes
        .iter()
        .map(|attribute| attribute.name.as_str())
        .collect();
    let element_presence = collect_element_presence(inventories);
    let attribute_presence = collect_attribute_presence(inventories, &catalog_attribute_names);
    CatalogSnapshotLifecycle {
        elements: lifecycle_entries_for_profile(profile, &element_presence, false),
        attributes: lifecycle_entries_for_profile(profile, &attribute_presence, true),
    }
}

fn collect_element_presence(
    inventories: &[CatalogInventory],
) -> BTreeMap<String, Vec<CatalogSpecSnapshotId>> {
    let mut presence: BTreeMap<String, BTreeSet<CatalogSpecSnapshotId>> = BTreeMap::new();
    for inventory in inventories {
        for element in &inventory.elements {
            presence
                .entry(element.name.clone())
                .or_default()
                .insert(inventory.profile);
        }
    }
    presence
        .into_iter()
        .map(|(name, profiles)| (name, profiles.into_iter().collect()))
        .collect()
}

fn collect_attribute_presence(
    inventories: &[CatalogInventory],
    catalog_attribute_names: &BTreeSet<&str>,
) -> BTreeMap<String, Vec<CatalogSpecSnapshotId>> {
    let mut presence: BTreeMap<String, BTreeSet<CatalogSpecSnapshotId>> = BTreeMap::new();
    for inventory in inventories {
        for attribute in &inventory.attributes {
            let Some(attribute) =
                lifecycle_attribute_name(inventory.profile, attribute, catalog_attribute_names)
            else {
                continue;
            };
            presence
                .entry(attribute)
                .or_default()
                .insert(inventory.profile);
        }
    }
    presence
        .into_iter()
        .map(|(name, profiles)| (name, profiles.into_iter().collect()))
        .collect()
}

fn lifecycle_entries_for_profile(
    profile: CatalogSpecSnapshotId,
    presence: &BTreeMap<String, Vec<CatalogSpecSnapshotId>>,
    attributes: bool,
) -> Vec<CatalogLifecycleEntry> {
    let mut entries = Vec::new();
    for (name, known_in) in presence {
        let present = known_in.contains(&profile);
        let catalog_name = attributes
            .then(|| canonical_attribute_name(name))
            .and_then(|canonical| (canonical.as_ref() != name).then(|| canonical.into_owned()));
        let lifecycle = if present {
            if is_draft_only(profile, known_in) {
                Some(CatalogLifecycleStatus::Experimental)
            } else if catalog_name.is_some() {
                Some(CatalogLifecycleStatus::Stable)
            } else {
                None
            }
        } else if known_before(profile, known_in) {
            Some(CatalogLifecycleStatus::Obsolete)
        } else if known_after(profile, known_in) {
            Some(CatalogLifecycleStatus::NotYetIntroduced)
        } else {
            None
        };
        let Some(lifecycle) = lifecycle else {
            continue;
        };
        entries.push(CatalogLifecycleEntry {
            name: name.clone(),
            catalog_name,
            present,
            lifecycle,
            known_in: known_in.clone(),
        });
    }
    entries
}

fn lifecycle_attribute_name(
    profile: CatalogSpecSnapshotId,
    attribute: &str,
    catalog_attribute_names: &BTreeSet<&str>,
) -> Option<String> {
    if attribute == "xlink:href" && !is_svg11_profile(profile) {
        return None;
    }
    let canonical = canonical_attribute_name(attribute);
    (attribute == "xlink:href" || catalog_attribute_names.contains(canonical.as_ref()))
        .then(|| attribute.to_owned())
}

fn is_draft_only(profile: CatalogSpecSnapshotId, known_in: &[CatalogSpecSnapshotId]) -> bool {
    profile == CatalogSpecSnapshotId::Svg2EditorsDraft
        && known_in == [CatalogSpecSnapshotId::Svg2EditorsDraft]
}

fn known_before(profile: CatalogSpecSnapshotId, known_in: &[CatalogSpecSnapshotId]) -> bool {
    known_in.iter().any(|known| *known < profile)
}

fn known_after(profile: CatalogSpecSnapshotId, known_in: &[CatalogSpecSnapshotId]) -> bool {
    known_in.iter().any(|known| *known > profile)
}

const fn is_svg11_profile(profile: CatalogSpecSnapshotId) -> bool {
    matches!(
        profile,
        CatalogSpecSnapshotId::Svg11Rec20030114 | CatalogSpecSnapshotId::Svg11Rec20110816
    )
}

/// A BCD feature below an SVG element that is not an element or attribute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogCompatSubfeature {
    /// Full BCD compat key, e.g. `svg.elements.use.data_uri`.
    pub compat_key: String,
    /// Why this was not modeled as a normal attribute/element.
    pub kind: CatalogCompatSubfeatureKind,
    /// Owning SVG element name.
    pub element: String,
    /// BCD child feature name.
    pub name: String,
    /// Objective compat facts for the subfeature.
    #[serde(flatten)]
    pub facts: CatalogCompatFacts,
}

/// Why a BCD child feature is kept out of the attribute catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CatalogCompatSubfeatureKind {
    /// A behavior or value-shape feature, not an attribute name.
    Behavior,
    /// A legacy `xlink:*` alias that needs profile-scoped alias modeling.
    LegacyXlinkAlias,
}

/// Web-platform baseline status of a feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CatalogBaselineStatus {
    /// Widely available across engines.
    Widely {
        /// Year it reached widely-available baseline.
        since: u16,
        /// Qualifier when the upstream date was inexact.
        qualifier: Option<CatalogBaselineQualifier>,
    },
    /// Newly available, not yet widely available.
    Newly {
        /// Year it reached newly-available baseline.
        since: u16,
        /// Qualifier when the upstream date was inexact.
        qualifier: Option<CatalogBaselineQualifier>,
    },
    /// Limited availability.
    Limited,
}

/// Inexactness qualifier on a baseline / version date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CatalogBaselineQualifier {
    /// The date is an "on or before" upper bound.
    Before,
    /// The date is an "on or after" lower bound.
    After,
    /// The date is approximate.
    Approximately,
}

/// Per-browser support across the four tracked engines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogBrowserSupport {
    /// Chrome support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chrome: Option<CatalogBrowserVersion>,
    /// Edge support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<CatalogBrowserVersion>,
    /// Firefox support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firefox: Option<CatalogBrowserVersion>,
    /// Safari support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safari: Option<CatalogBrowserVersion>,
}

/// Baked support detail for one browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogBrowserVersion {
    /// Explicit support flag, when the data states one (`false` = unsupported).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supported: Option<bool>,
    /// Whether support is partial.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub partial_implementation: bool,
    /// Upstream notes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Vendor prefix required, when any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    /// Alternative name the browser ships under, when any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternative_name: Option<String>,
    /// Runtime flags gating the feature.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<CatalogBrowserFlag>,
    /// First version (`"15"`, `"<=37"`), when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_added: Option<String>,
    /// Qualifier on the added version's date inexactness.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_qualifier: Option<CatalogBaselineQualifier>,
    /// Version support was removed in, when any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_removed: Option<String>,
    /// Qualifier on the removed version's date inexactness.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_removed_qualifier: Option<CatalogBaselineQualifier>,
}

impl CatalogBrowserSupport {
    /// Whether all tracked browser entries are absent.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.chrome.is_none()
            && self.edge.is_none()
            && self.firefox.is_none()
            && self.safari.is_none()
    }
}

/// A runtime flag a browser gates a feature behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogBrowserFlag {
    /// Flag/preference name.
    pub name: String,
}

/// Objective browser-compat facts for one catalog entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogCompatFacts {
    /// MDN reference URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mdn_url: Option<String>,
    /// Whether compat data marks the feature deprecated.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub deprecated: bool,
    /// Whether compat data marks the feature experimental.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub experimental: bool,
    /// Whether compat data marks the feature as standards-track.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard_track: Option<bool>,
    /// Web-platform baseline status, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<CatalogBaselineStatus>,
    /// Per-browser support data, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_support: Option<CatalogBrowserSupport>,
}

const fn is_empty_slice<T>(slice: &&[T]) -> bool {
    slice.is_empty()
}

/// The runtime value-space shapes the catalog emits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CatalogAttributeValues {
    /// One of the listed keyword values.
    Enum {
        /// Allowed keyword values.
        values: Vec<String>,
    },
    /// A transform list; functions are completion hints.
    Transform {
        /// Transform function names.
        functions: Vec<String>,
    },
    /// A CSS/SVG color value.
    Color,
    /// A length or length-percentage value.
    Length,
    /// A URL / fragment reference.
    Url,
    /// A boolean attribute as defined by HTML.
    Boolean,
    /// A space-separated token list.
    TokenList,
    /// A comma-separated token list.
    CommaTokenList,
    /// A space-separated list of URL tokens.
    UrlTokenList,
    /// A BCP 47 / ABNF language tag.
    LanguageTag,
    /// An integer value.
    Integer,
    /// A MIME media type.
    MediaType,
    /// A CSS media query list.
    MediaQueryList,
    /// A CSS declaration list, as used by inline `style`.
    CssDeclarationList,
    /// An SVG element ID value.
    Id,
    /// A referrer policy string.
    ReferrerPolicy,
    /// A suggested download file name.
    SuggestedFileName,
    /// SVG path data.
    PathData,
    /// A semicolon-separated list of numbers.
    SemicolonNumberList,
    /// One motion-animation coordinate pair.
    CoordinatePair,
    /// A semicolon-separated list of motion-animation coordinate pairs.
    CoordinatePairList,
    /// A number or percentage.
    NumberOrPercentage,
    /// A structured CSS value grammar that is richer than the runtime's
    /// specialized variants.
    CssGrammar {
        /// Raw grammar text from the defining spec.
        grammar: String,
        /// Token graph extracted from the grammar.
        graph: CatalogCssGrammarGraph,
    },
    /// Free-form text with no constrained grammar.
    FreeText,
}

/// Graph representation of a CSS value grammar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogCssGrammarGraph {
    /// Root node id.
    pub root: u16,
    /// Grammar nodes.
    pub nodes: Vec<CatalogCssGrammarNode>,
    /// Grammar edges.
    pub edges: Vec<CatalogCssGrammarEdge>,
}

/// One node in a CSS grammar graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogCssGrammarNode {
    /// Stable node id within the graph.
    pub id: u16,
    /// Node kind.
    pub kind: CatalogCssGrammarNodeKind,
    /// Token text, when the node carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// CSS grammar node kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CatalogCssGrammarNodeKind {
    /// Synthetic root.
    Root,
    /// Bracketed group.
    Group,
    /// CSS keyword token.
    Keyword,
    /// CSS type token, e.g. `<length>`.
    Type,
    /// Functional notation token, e.g. `url()`.
    Function,
    /// CSS grammar operator or multiplier.
    Operator,
}

/// One directed edge in a CSS grammar graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogCssGrammarEdge {
    /// Source node id.
    pub from: u16,
    /// Target node id.
    pub to: u16,
    /// Edge kind.
    pub kind: CatalogCssGrammarEdgeKind,
}

/// CSS grammar edge kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CatalogCssGrammarEdgeKind {
    /// Parent/group containment.
    Contains,
    /// Sibling order within a parent/group.
    Next,
}

/// Which elements an attribute can appear on.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CatalogAttributeApplicability {
    /// Applies to every element that accepts global SVG attributes.
    Global,
    /// Applies only to the listed element names.
    Elements {
        /// Bearer element names.
        elements: Vec<String>,
    },
    /// Known attribute that applies to no elements in this catalog.
    None,
}

/// Derived graph view over the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogGraph {
    /// Graph nodes, sorted by id.
    pub nodes: Vec<CatalogGraphNode>,
    /// Directed graph edges, sorted by `(from, to, kind)`.
    pub edges: Vec<CatalogGraphEdge>,
}

/// One node in the derived catalog graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogGraphNode {
    /// Stable node id, namespaced by node kind.
    pub id: String,
    /// Node kind.
    pub kind: CatalogGraphNodeKind,
    /// Human-readable node name.
    pub name: String,
}

/// Catalog graph node kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CatalogGraphNodeKind {
    /// SVG element definition.
    Element,
    /// SVG attribute definition.
    Attribute,
    /// SVG element category from the spec taxonomy.
    ElementCategory,
    /// SVG attribute category from the spec taxonomy.
    AttributeCategory,
    /// SVG specification profile/snapshot.
    Profile,
    /// CSS property backing a presentation attribute.
    CssProperty,
    /// Attribute value grammar node.
    ValueGrammar,
    /// Retained browser-compat subfeature not modeled as a first-class element/attribute.
    CompatFeature,
}

/// One directed edge in the derived catalog graph.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
pub struct CatalogGraphEdge {
    /// Source node id.
    pub from: String,
    /// Target node id.
    pub to: String,
    /// Edge kind.
    pub kind: CatalogGraphEdgeKind,
}

/// Catalog graph edge kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CatalogGraphEdgeKind {
    /// Parent element accepts child element.
    AllowsChild,
    /// Element has a direct/specific attribute declaration.
    HasAttribute,
    /// Attribute applies to an element after global/scoped applicability is resolved.
    AppliesTo,
    /// Feature is a member of a spec category.
    MemberOf,
    /// Element accepts the SVG global attribute set.
    AcceptsGlobalAttributes,
    /// Presentation attribute is backed by a CSS property.
    UsesCssProperty,
    /// Attribute points at a value grammar node.
    HasValueGrammar,
    /// Value grammar override applies in the target profile.
    OverridesValueInProfile,
    /// Compat subfeature describes the target element/attribute.
    Describes,
    /// Feature is present in the target profile.
    PresentIn,
}

/// The runtime content-model shapes the catalog emits. The spec's category
/// taxonomy is already flattened into [`Self::ChildrenSet`].
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CatalogContentModel {
    /// Accepts exactly the listed child element names.
    ChildrenSet {
        /// Allowed child element names (sorted, deduped).
        elements: Vec<String>,
    },
    /// Accepts any element from the SVG namespace.
    AnySvg,
    /// Hosts arbitrary foreign-namespace content (the spec's `any` model:
    /// "any elements or character data"), e.g. HTML in `foreignObject`.
    Foreign,
    /// Primarily character data.
    Text,
}

/// Legacy-profile inputs derived from dated authoritative sources.
#[derive(Clone, Copy)]
pub struct CatalogLegacyInputs<'a> {
    /// Source records to persist in `catalog.json`.
    pub sources: &'a [CatalogLegacySource],
    /// Attribute value overrides keyed by attribute/property name.
    pub value_overrides: &'a BTreeMap<String, Vec<CatalogAttributeValueOverride>>,
    /// Per-snapshot element/attribute inventories.
    pub inventories: &'a [CatalogInventory],
    /// External inputs for tree-sitter grammar projection, when enabled.
    pub grammar_inputs: Option<&'a crate::treesitter::GrammarProjectionInputs>,
}

/// Build the catalog from every definitions module's extracted entities.
///
/// `editors_draft_base` is the SVG 2 editor's-draft base URL (from
/// `publish.xml`'s `<cvs>`), used to resolve permalinks for the modules defined
/// within `svgwg` itself; modules carrying their own external `anchor_base`
/// (CSS drafts) resolve against that instead.
/// # Errors
/// Returns an error when tree-sitter grammar projection validation fails.
pub fn build_catalog(
    modules: &[Definitions],
    properties: &[PropertyValueDef],
    descriptions: &[AnchorDescription],
    editors_draft_base: &str,
    commit: &str,
    compat: Option<&CompatCatalog>,
    legacy: CatalogLegacyInputs<'_>,
) -> Result<Catalog, Box<dyn std::error::Error>> {
    let members = category_members(modules);
    let descriptions_by_id = descriptions_by_id(descriptions);
    let mut elements = Vec::new();
    for module in modules {
        let base = module.anchor_base.as_deref().unwrap_or(editors_draft_base);
        for element in &module.elements {
            elements.push(build_element(
                element,
                base,
                &members,
                &descriptions_by_id,
                compat.and_then(|compat| compat.elements.get(&element.name)),
            ));
        }
    }
    elements.sort_by(|a, b| a.name.cmp(&b.name));
    let attributes = build_attributes(
        modules,
        properties,
        &descriptions_by_id,
        editors_draft_base,
        compat,
        legacy.value_overrides,
        legacy.grammar_inputs,
    );
    if legacy.grammar_inputs.is_some() {
        validate_required_extracted_facts(&elements, &attributes)?;
    }
    let inventories = legacy.inventories.to_vec();
    let graph = build_catalog_graph(
        modules,
        properties,
        compat,
        &elements,
        &attributes,
        &inventories,
    );
    let tree_sitter = match legacy.grammar_inputs {
        None => CatalogTreeSitterDocument {
            schema_version: crate::schema::CATALOG_SCHEMA_VERSION,
            sources: CatalogTreeSitterSources {
                webref_css: CatalogPackageSource {
                    name: String::new(),
                    version: String::new(),
                    url: String::new(),
                },
                css_unit_pages: Vec::new(),
                svg_clock_value_syntax: String::new(),
                paths_html: String::new(),
            },
            attribute_buckets: CatalogTreeSitterAttributeBuckets::default(),
            tokens: CatalogTreeSitterTokens::default(),
        },
        // Real catalog builds carry the full attribute set, so run the
        // completeness validation; the `None` arm above is only hit by unit
        // tests that never reach this call.
        Some(inputs) => crate::treesitter::build_tree_sitter_document(&attributes, inputs, true)?,
    };
    let snapshots = inventories
        .iter()
        .map(|inventory| CatalogSnapshotRef {
            profile: inventory.profile,
            href: catalog_snapshot_href(inventory.profile).to_owned(),
        })
        .collect();
    Ok(Catalog {
        schema_version: crate::schema::CATALOG_SCHEMA_VERSION,
        commit: commit.to_owned(),
        compat: compat.map(|compat| compat.provenance.clone()),
        legacy_sources: legacy.sources.to_vec(),
        snapshots,
        elements,
        attributes,
        graph,
        tree_sitter,
    })
}

fn validate_required_extracted_facts(
    elements: &[CatalogElement],
    attributes: &[CatalogAttribute],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut errors = Vec::new();

    for (element_name, expected) in known_content_models() {
        let Some(element) = elements.iter().find(|element| element.name == element_name) else {
            errors.push(format!("missing required element `{element_name}`"));
            continue;
        };
        if element.content_model != expected {
            errors.push(format!(
                "extraction invariant failed for <{element_name}> content model: expected \
                 {expected:?}, got {:?}",
                element.content_model,
            ));
        }
    }

    for (attribute_name, expected) in known_attribute_values() {
        let Some(attribute) = attributes
            .iter()
            .find(|attribute| attribute.name == attribute_name)
        else {
            errors.push(format!("missing required attribute `{attribute_name}`"));
            continue;
        };
        if attribute.values != expected {
            errors.push(format!(
                "extraction invariant failed for `{attribute_name}` values: expected \
                 {expected:?}, got {:?}",
                attribute.values,
            ));
        }
    }

    for (attribute_name, element_name, expected) in known_element_values() {
        let Some(attribute) = attributes
            .iter()
            .find(|attribute| attribute.name == attribute_name)
        else {
            errors.push(format!("missing required attribute `{attribute_name}`"));
            continue;
        };
        let Some(actual) = attribute
            .element_values
            .iter()
            .find(|values| values.element == element_name)
        else {
            errors.push(format!(
                "extraction invariant failed for `{attribute_name}` on <{element_name}>: missing \
                 element-scoped values",
            ));
            continue;
        };
        if actual.values != expected {
            errors.push(format!(
                "extraction invariant failed for `{attribute_name}` on <{element_name}>: expected \
                 {expected:?}, got {:?}",
                actual.values,
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(boxed(format!(
            "required extraction invariant(s) failed:\n- {}",
            errors.join("\n- ")
        )))
    }
}

fn known_content_models() -> Vec<(&'static str, CatalogContentModel)> {
    vec![(
        "animateMotion",
        CatalogContentModel::ChildrenSet {
            // SVG Animations permits descriptive children, script, and mpath. The
            // current catalog shape models allowed names, not mpath cardinality.
            elements: ["desc", "metadata", "mpath", "script", "title"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        },
    )]
}

fn known_attribute_values() -> Vec<(&'static str, CatalogAttributeValues)> {
    vec![
        (
            "calcMode",
            CatalogAttributeValues::Enum {
                values: ["discrete", "linear", "paced", "spline"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            },
        ),
        ("keyPoints", CatalogAttributeValues::SemicolonNumberList),
        (
            "origin",
            CatalogAttributeValues::Enum {
                values: vec!["default".to_owned()],
            },
        ),
        ("path", CatalogAttributeValues::PathData),
    ]
}

fn known_element_values() -> Vec<(&'static str, &'static str, CatalogAttributeValues)> {
    let animate_transform_type = CatalogAttributeValues::Enum {
        values: ["rotate", "scale", "skewX", "skewY", "translate"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    };
    let animate_motion_rotate = {
        let grammar = "<number> | auto | auto-reverse".to_owned();
        CatalogAttributeValues::CssGrammar {
            graph: css_grammar_graph(&grammar),
            grammar,
        }
    };

    vec![
        ("type", "animateTransform", animate_transform_type),
        ("rotate", "animateMotion", animate_motion_rotate),
        (
            "by",
            "animateMotion",
            CatalogAttributeValues::CoordinatePair,
        ),
        (
            "from",
            "animateMotion",
            CatalogAttributeValues::CoordinatePair,
        ),
        (
            "to",
            "animateMotion",
            CatalogAttributeValues::CoordinatePair,
        ),
        (
            "values",
            "animateMotion",
            CatalogAttributeValues::CoordinatePairList,
        ),
    ]
}

fn build_catalog_graph(
    modules: &[Definitions],
    properties: &[PropertyValueDef],
    compat: Option<&CompatCatalog>,
    elements: &[CatalogElement],
    attributes: &[CatalogAttribute],
    inventories: &[CatalogInventory],
) -> CatalogGraph {
    let mut builder = CatalogGraphBuilder::default();
    add_graph_profile_nodes(&mut builder);
    add_graph_element_nodes(&mut builder, elements);
    add_graph_attribute_nodes(&mut builder, attributes);
    add_graph_css_property_nodes(&mut builder, modules, properties);
    add_graph_category_edges(&mut builder, modules);
    add_graph_element_edges(&mut builder, elements);
    add_graph_attribute_edges(&mut builder, elements, attributes);
    add_graph_value_edges(&mut builder, attributes);
    add_graph_inventory_edges(&mut builder, inventories);
    add_graph_compat_edges(&mut builder, compat);
    builder.finish()
}

#[derive(Default)]
struct CatalogGraphBuilder {
    nodes: BTreeMap<String, CatalogGraphNode>,
    edges: BTreeSet<CatalogGraphEdge>,
}

impl CatalogGraphBuilder {
    fn node(&mut self, kind: CatalogGraphNodeKind, name: impl Into<String>) -> String {
        let name = name.into();
        let id = catalog_graph_node_id(kind, &name);
        self.node_with_id(kind, id, name)
    }

    fn node_with_id(
        &mut self,
        kind: CatalogGraphNodeKind,
        id: impl Into<String>,
        name: impl Into<String>,
    ) -> String {
        let id = id.into();
        let name = name.into();
        self.nodes
            .entry(id.clone())
            .or_insert_with(|| CatalogGraphNode {
                id: id.clone(),
                kind,
                name,
            });
        id
    }

    fn edge(&mut self, from: &str, to: &str, kind: CatalogGraphEdgeKind) {
        self.edges.insert(CatalogGraphEdge {
            from: from.to_owned(),
            to: to.to_owned(),
            kind,
        });
    }

    fn finish(self) -> CatalogGraph {
        CatalogGraph {
            nodes: self.nodes.into_values().collect(),
            edges: self.edges.into_iter().collect(),
        }
    }
}

fn catalog_graph_node_id(kind: CatalogGraphNodeKind, name: &str) -> String {
    format!("{}:{name}", catalog_graph_node_prefix(kind))
}

const fn catalog_graph_node_prefix(kind: CatalogGraphNodeKind) -> &'static str {
    match kind {
        CatalogGraphNodeKind::Element => "element",
        CatalogGraphNodeKind::Attribute => "attribute",
        CatalogGraphNodeKind::ElementCategory => "element-category",
        CatalogGraphNodeKind::AttributeCategory => "attribute-category",
        CatalogGraphNodeKind::Profile => "profile",
        CatalogGraphNodeKind::CssProperty => "css-property",
        CatalogGraphNodeKind::ValueGrammar => "value",
        CatalogGraphNodeKind::CompatFeature => "compat",
    }
}

fn add_graph_profile_nodes(builder: &mut CatalogGraphBuilder) {
    for profile in [
        CatalogSpecSnapshotId::Svg11Rec20030114,
        CatalogSpecSnapshotId::Svg11Rec20110816,
        CatalogSpecSnapshotId::Svg2Cr20181004,
        CatalogSpecSnapshotId::Svg2EditorsDraft,
    ] {
        builder.node(CatalogGraphNodeKind::Profile, catalog_profile_name(profile));
    }
}

const fn catalog_profile_name(profile: CatalogSpecSnapshotId) -> &'static str {
    match profile {
        CatalogSpecSnapshotId::Svg11Rec20030114 => "Svg11Rec20030114",
        CatalogSpecSnapshotId::Svg11Rec20110816 => "Svg11Rec20110816",
        CatalogSpecSnapshotId::Svg2Cr20181004 => "Svg2Cr20181004",
        CatalogSpecSnapshotId::Svg2EditorsDraft => "Svg2EditorsDraft",
    }
}

fn add_graph_element_nodes(builder: &mut CatalogGraphBuilder, elements: &[CatalogElement]) {
    for element in elements {
        builder.node(CatalogGraphNodeKind::Element, &element.name);
    }
}

fn add_graph_attribute_nodes(builder: &mut CatalogGraphBuilder, attributes: &[CatalogAttribute]) {
    for attribute in attributes {
        builder.node(CatalogGraphNodeKind::Attribute, &attribute.name);
    }
}

fn add_graph_css_property_nodes(
    builder: &mut CatalogGraphBuilder,
    modules: &[Definitions],
    properties: &[PropertyValueDef],
) {
    for property in properties {
        builder.node(CatalogGraphNodeKind::CssProperty, &property.name);
    }
    for module in modules {
        for property in &module.properties {
            builder.node(CatalogGraphNodeKind::CssProperty, &property.name);
        }
    }
}

fn add_graph_category_edges(builder: &mut CatalogGraphBuilder, modules: &[Definitions]) {
    let global_category = builder.node(CatalogGraphNodeKind::AttributeCategory, "global");
    for module in modules {
        for attribute in &module.global_attributes {
            let attribute_id = builder.node(
                CatalogGraphNodeKind::Attribute,
                canonical_attribute_name(&attribute.name).as_ref(),
            );
            builder.edge(
                &attribute_id,
                &global_category,
                CatalogGraphEdgeKind::MemberOf,
            );
        }
        add_graph_element_category_edges(builder, module);
        add_graph_attribute_category_edges(builder, module);
    }
}

fn add_graph_element_category_edges(builder: &mut CatalogGraphBuilder, module: &Definitions) {
    for category in &module.element_categories {
        let category_id = builder.node(CatalogGraphNodeKind::ElementCategory, &category.name);
        for element in &category.elements {
            let element_id = builder.node(CatalogGraphNodeKind::Element, element);
            builder.edge(&element_id, &category_id, CatalogGraphEdgeKind::MemberOf);
        }
    }
}

fn add_graph_attribute_category_edges(builder: &mut CatalogGraphBuilder, module: &Definitions) {
    for category in &module.attribute_categories {
        let category_id = builder.node(CatalogGraphNodeKind::AttributeCategory, &category.name);
        for attribute in &category.attributes {
            let attribute_id = builder.node(
                CatalogGraphNodeKind::Attribute,
                canonical_attribute_name(&attribute.name).as_ref(),
            );
            builder.edge(&attribute_id, &category_id, CatalogGraphEdgeKind::MemberOf);
        }
        for attribute in &category.presentation_attributes {
            let attribute_id = builder.node(
                CatalogGraphNodeKind::Attribute,
                canonical_attribute_name(attribute).as_ref(),
            );
            builder.edge(&attribute_id, &category_id, CatalogGraphEdgeKind::MemberOf);
        }
    }
}

fn add_graph_element_edges(builder: &mut CatalogGraphBuilder, elements: &[CatalogElement]) {
    let element_names: Vec<&str> = elements
        .iter()
        .map(|element| element.name.as_str())
        .collect();
    let global_category = builder.node(CatalogGraphNodeKind::AttributeCategory, "global");
    for element in elements {
        let element_id = builder.node(CatalogGraphNodeKind::Element, &element.name);
        if element.global_attrs {
            builder.edge(
                &element_id,
                &global_category,
                CatalogGraphEdgeKind::AcceptsGlobalAttributes,
            );
        }
        match &element.content_model {
            CatalogContentModel::ChildrenSet { elements } => {
                for child in elements {
                    let child_id = builder.node(CatalogGraphNodeKind::Element, child);
                    builder.edge(&element_id, &child_id, CatalogGraphEdgeKind::AllowsChild);
                }
            }
            CatalogContentModel::AnySvg => {
                for child in &element_names {
                    let child_id = builder.node(CatalogGraphNodeKind::Element, *child);
                    builder.edge(&element_id, &child_id, CatalogGraphEdgeKind::AllowsChild);
                }
            }
            CatalogContentModel::Foreign | CatalogContentModel::Text => {}
        }
    }
}

fn add_graph_attribute_edges(
    builder: &mut CatalogGraphBuilder,
    elements: &[CatalogElement],
    attributes: &[CatalogAttribute],
) {
    for element in elements {
        let element_id = builder.node(CatalogGraphNodeKind::Element, &element.name);
        for attribute in &element.attrs {
            let attribute_id = builder.node(CatalogGraphNodeKind::Attribute, attribute);
            builder.edge(
                &element_id,
                &attribute_id,
                CatalogGraphEdgeKind::HasAttribute,
            );
        }
    }
    for attribute in attributes {
        let attribute_id = builder.node(CatalogGraphNodeKind::Attribute, &attribute.name);
        add_graph_attribute_applicability_edges(builder, &attribute_id, attribute, elements);
        if let Some(property) = attribute.presentation_attribute.as_deref() {
            let property_id = builder.node(CatalogGraphNodeKind::CssProperty, property);
            builder.edge(
                &attribute_id,
                &property_id,
                CatalogGraphEdgeKind::UsesCssProperty,
            );
        }
    }
}

fn add_graph_attribute_applicability_edges(
    builder: &mut CatalogGraphBuilder,
    attribute_id: &str,
    attribute: &CatalogAttribute,
    elements: &[CatalogElement],
) {
    match &attribute.applicability {
        CatalogAttributeApplicability::Global => {
            for element in elements.iter().filter(|element| element.global_attrs) {
                let element_id = builder.node(CatalogGraphNodeKind::Element, &element.name);
                builder.edge(attribute_id, &element_id, CatalogGraphEdgeKind::AppliesTo);
            }
        }
        CatalogAttributeApplicability::Elements { elements } => {
            for element in elements {
                let element_id = builder.node(CatalogGraphNodeKind::Element, element);
                builder.edge(attribute_id, &element_id, CatalogGraphEdgeKind::AppliesTo);
            }
        }
        CatalogAttributeApplicability::None => {}
    }
}

fn add_graph_value_edges(builder: &mut CatalogGraphBuilder, attributes: &[CatalogAttribute]) {
    for attribute in attributes {
        let attribute_id = builder.node(CatalogGraphNodeKind::Attribute, &attribute.name);
        let value_id = builder.node_with_id(
            CatalogGraphNodeKind::ValueGrammar,
            catalog_graph_node_id(CatalogGraphNodeKind::ValueGrammar, &attribute.name),
            format!(
                "{} ({})",
                attribute.name,
                catalog_attribute_values_kind(&attribute.values)
            ),
        );
        builder.edge(
            &attribute_id,
            &value_id,
            CatalogGraphEdgeKind::HasValueGrammar,
        );
        for override_ in &attribute.value_overrides {
            let profile = catalog_profile_name(override_.profile);
            let override_key = format!("{}@{profile}", attribute.name);
            let override_value_id = builder.node_with_id(
                CatalogGraphNodeKind::ValueGrammar,
                catalog_graph_node_id(CatalogGraphNodeKind::ValueGrammar, &override_key),
                format!(
                    "{}@{} ({})",
                    attribute.name,
                    profile,
                    catalog_attribute_values_kind(&override_.values)
                ),
            );
            let profile_id = builder.node(CatalogGraphNodeKind::Profile, profile);
            builder.edge(
                &attribute_id,
                &override_value_id,
                CatalogGraphEdgeKind::HasValueGrammar,
            );
            builder.edge(
                &override_value_id,
                &profile_id,
                CatalogGraphEdgeKind::OverridesValueInProfile,
            );
        }
    }
}

const fn catalog_attribute_values_kind(values: &CatalogAttributeValues) -> &'static str {
    match values {
        CatalogAttributeValues::Enum { .. } => "enum",
        CatalogAttributeValues::Transform { .. } => "transform",
        CatalogAttributeValues::Color => "color",
        CatalogAttributeValues::Length => "length",
        CatalogAttributeValues::Url => "url",
        CatalogAttributeValues::Boolean => "boolean",
        CatalogAttributeValues::TokenList => "token_list",
        CatalogAttributeValues::CommaTokenList => "comma_token_list",
        CatalogAttributeValues::UrlTokenList => "url_token_list",
        CatalogAttributeValues::LanguageTag => "language_tag",
        CatalogAttributeValues::Integer => "integer",
        CatalogAttributeValues::MediaType => "media_type",
        CatalogAttributeValues::MediaQueryList => "media_query_list",
        CatalogAttributeValues::CssDeclarationList => "css_declaration_list",
        CatalogAttributeValues::Id => "id",
        CatalogAttributeValues::ReferrerPolicy => "referrer_policy",
        CatalogAttributeValues::SuggestedFileName => "suggested_file_name",
        CatalogAttributeValues::PathData => "path_data",
        CatalogAttributeValues::SemicolonNumberList => "semicolon_number_list",
        CatalogAttributeValues::CoordinatePair => "coordinate_pair",
        CatalogAttributeValues::CoordinatePairList => "coordinate_pair_list",
        CatalogAttributeValues::NumberOrPercentage => "number_or_percentage",
        CatalogAttributeValues::CssGrammar { .. } => "css_grammar",
        CatalogAttributeValues::FreeText => "free_text",
    }
}

fn add_graph_compat_edges(builder: &mut CatalogGraphBuilder, compat: Option<&CompatCatalog>) {
    let Some(compat) = compat else {
        return;
    };
    for feature in &compat.provenance.unmodeled_features {
        let feature_id = builder.node(CatalogGraphNodeKind::CompatFeature, &feature.compat_key);
        let element_id = builder.node(CatalogGraphNodeKind::Element, &feature.element);
        builder.edge(&feature_id, &element_id, CatalogGraphEdgeKind::Describes);
        if !feature.name.is_empty() {
            let attribute_id = builder.node(CatalogGraphNodeKind::Attribute, &feature.name);
            builder.edge(&feature_id, &attribute_id, CatalogGraphEdgeKind::Describes);
        }
    }
}

fn add_graph_inventory_edges(builder: &mut CatalogGraphBuilder, inventories: &[CatalogInventory]) {
    for inventory in inventories {
        let profile_id = builder.node(
            CatalogGraphNodeKind::Profile,
            catalog_profile_name(inventory.profile),
        );
        for element in &inventory.elements {
            let element_id = builder.node(CatalogGraphNodeKind::Element, &element.name);
            builder.edge(&element_id, &profile_id, CatalogGraphEdgeKind::PresentIn);
        }
        for attribute in &inventory.attributes {
            let attribute_id = builder.node(
                CatalogGraphNodeKind::Attribute,
                canonical_attribute_name(attribute).as_ref(),
            );
            builder.edge(&attribute_id, &profile_id, CatalogGraphEdgeKind::PresentIn);
        }
    }
}

fn descriptions_by_id(descriptions: &[AnchorDescription]) -> BTreeMap<&str, &str> {
    let mut by_id = BTreeMap::new();
    for description in descriptions {
        by_id
            .entry(description.id.as_str())
            .or_insert(description.description.as_str());
    }
    by_id
}

fn description_for_href(
    href: Option<&str>,
    descriptions_by_id: &BTreeMap<&str, &str>,
) -> Option<String> {
    let fragment = href?.rsplit_once('#')?.1;
    descriptions_by_id
        .get(fragment)
        .map(|description| (*description).to_owned())
}

fn description_derived_content_model(
    element: &crate::extract::ElementDef,
    members: &BTreeMap<&str, Vec<&str>>,
) -> Option<CatalogContentModel> {
    // Semantic projection policy: some SVG definitions describe the allowed
    // children only in prose. We derive the explicit child names from that
    // fetched definitions.xml prose instead of hard-appending known names.
    let description = element.content_model_description.as_deref()?;
    let lower = description.to_ascii_lowercase();
    let mut elements: Vec<String> = Vec::new();
    if lower.contains("descriptive elements") {
        elements.extend(
            members
                .get("descriptive")
                .into_iter()
                .flatten()
                .map(|name| (*name).to_owned()),
        );
    }
    elements.extend(quoted_keyword_values(description));
    if elements.is_empty() {
        return None;
    }
    elements.sort();
    elements.dedup();
    Some(CatalogContentModel::ChildrenSet { elements })
}

/// Element-category membership across all modules: category name to its member
/// element names.
fn category_members(modules: &[Definitions]) -> BTreeMap<&str, Vec<&str>> {
    let mut members: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for module in modules {
        for category in &module.element_categories {
            let entry = members.entry(category.name.as_str()).or_default();
            entry.extend(category.elements.iter().map(String::as_str));
        }
    }
    members
}

/// Reshape one extracted element into its catalog entry.
fn build_element(
    element: &crate::extract::ElementDef,
    base: &str,
    members: &BTreeMap<&str, Vec<&str>>,
    descriptions_by_id: &BTreeMap<&str, &str>,
    compat: Option<&CatalogCompatFacts>,
) -> CatalogElement {
    let content_model = match element.content_model {
        // The spec's `any` ("any elements or character data") hosts foreign
        // content (`foreignObject`, `desc`, `title`, `metadata`); its children
        // are not SVG and must not be validated as such.
        Some(ContentModelKind::Any) => CatalogContentModel::Foreign,
        Some(ContentModelKind::Text) => CatalogContentModel::Text,
        Some(ContentModelKind::AnyOf | ContentModelKind::TextOrAnyOf) => {
            CatalogContentModel::ChildrenSet {
                elements: flatten_children(element, members),
            }
        }
        // Description-only models (e.g. `a`, whose children mirror the parent):
        // over-approximate as "any SVG element" so valid children never trip a
        // false "invalid child" diagnostic.
        None => description_derived_content_model(element, members)
            .unwrap_or(CatalogContentModel::AnySvg),
    };

    let mut attrs: Vec<String> = element
        .attributes
        .iter()
        .map(|attribute| canonical_attribute_name(&attribute.name).into_owned())
        .chain(
            element
                .common_attributes
                .iter()
                .map(|attribute| canonical_attribute_name(attribute).into_owned()),
        )
        .chain(
            element
                .geometry_properties
                .iter()
                .map(|attribute| canonical_attribute_name(attribute).into_owned()),
        )
        .collect();
    attrs.sort();
    attrs.dedup();

    CatalogElement {
        name: element.name.clone(),
        description: description_for_href(element.href.as_deref(), descriptions_by_id),
        mdn_url: compat.and_then(|facts| facts.mdn_url.clone()),
        spec_url: element.href.as_ref().map(|href| format!("{base}{href}")),
        deprecated: compat.is_some_and(|facts| facts.deprecated),
        experimental: compat.is_some_and(|facts| facts.experimental),
        standard_track: compat.and_then(|facts| facts.standard_track),
        baseline: compat.and_then(|facts| facts.baseline),
        browser_support: compat.and_then(|facts| facts.browser_support.clone()),
        content_model,
        attrs,
        global_attrs: element
            .attribute_categories
            .iter()
            .any(|category| category == "core"),
    }
}

/// Build the canonical attribute catalog from globals, categories, and element
/// bearer declarations.
fn build_attributes(
    modules: &[Definitions],
    properties: &[PropertyValueDef],
    descriptions_by_id: &BTreeMap<&str, &str>,
    editors_draft_base: &str,
    compat: Option<&CompatCatalog>,
    legacy_value_overrides: &BTreeMap<String, Vec<CatalogAttributeValueOverride>>,
    grammar_inputs: Option<&crate::treesitter::GrammarProjectionInputs>,
) -> Vec<CatalogAttribute> {
    let all_elements: BTreeSet<String> = modules
        .iter()
        .flat_map(|module| module.elements.iter().map(|element| element.name.clone()))
        .collect();
    // Resolve cross-references (`in2 = "(see in attribute)"`) before grouping so
    // every definition carries a concrete value space; `properties_by_name`
    // borrows from this owned, rewritten vec, which outlives it.
    let resolved = resolve_see_references(properties);
    let resolved = augment_properties_with_direct_attribute_scopes(&resolved, modules);
    let mut properties_by_name: BTreeMap<&str, Vec<&PropertyValueDef>> = BTreeMap::new();
    for property in &resolved {
        properties_by_name
            .entry(property.name.as_str())
            .or_default()
            .push(property);
    }
    let module_properties = module_properties(modules, editors_draft_base);
    let presentation_attributes =
        presentation_attributes(modules, &module_properties, editors_draft_base);
    let category_attributes = attribute_categories(modules, editors_draft_base);
    let mut attributes: BTreeMap<String, AttributeAccumulator> = BTreeMap::new();

    seed_attribute_metadata(
        modules,
        editors_draft_base,
        descriptions_by_id,
        &mut attributes,
    );
    seed_presentation_attribute_metadata(
        &presentation_attributes,
        descriptions_by_id,
        &mut attributes,
    );
    collect_element_attribute_bearers(
        modules,
        editors_draft_base,
        &presentation_attributes,
        &category_attributes,
        descriptions_by_id,
        &mut attributes,
    );

    let mut attributes: Vec<CatalogAttribute> = attributes
        .into_values()
        .map(|attribute| {
            let mut attribute = attribute.finish(
                &all_elements,
                &properties_by_name,
                legacy_value_overrides,
                descriptions_by_id,
                grammar_inputs,
            );
            if let Some(attribute_compat) =
                compat.and_then(|compat| compat.attributes.get(&attribute.name))
            {
                if let Some(facts) = attribute_compat.common_facts() {
                    attribute.apply_compat(facts);
                }
                attribute.apply_element_compat(attribute_compat);
            }
            attribute
        })
        .collect();
    if let Some(compat) = compat {
        append_compat_only_attributes(&mut attributes, compat);
    }
    attributes.sort_by(|a, b| a.name.cmp(&b.name));
    attributes
}

fn append_compat_only_attributes(attributes: &mut Vec<CatalogAttribute>, compat: &CompatCatalog) {
    let existing: BTreeSet<String> = attributes
        .iter()
        .map(|attribute| attribute.name.clone())
        .collect();
    for (name, attribute_compat) in &compat.attributes {
        if existing.contains(name) {
            continue;
        }
        let mut attribute = CatalogAttribute {
            name: name.clone(),
            description: None,
            mdn_url: None,
            spec_url: None,
            deprecated: false,
            experimental: false,
            standard_track: None,
            animatable: false,
            presentation_attribute: None,
            baseline: None,
            browser_support: None,
            element_compat: Vec::new(),
            element_values: Vec::new(),
            value_overrides: Vec::new(),
            values: CatalogAttributeValues::FreeText,
            applicability: compat_attribute_applicability(attribute_compat),
        };
        if let Some(facts) = attribute_compat.common_facts() {
            attribute.apply_compat(facts);
        }
        attribute.apply_element_compat(attribute_compat);
        attributes.push(attribute);
    }
}

fn compat_attribute_applicability(
    attribute: &crate::compat::CompatAttribute,
) -> CatalogAttributeApplicability {
    if attribute.is_global() {
        CatalogAttributeApplicability::Global
    } else if attribute.element_facts.is_empty() {
        CatalogAttributeApplicability::None
    } else {
        CatalogAttributeApplicability::Elements {
            elements: attribute.bearers().cloned().collect(),
        }
    }
}

fn seed_attribute_metadata(
    modules: &[Definitions],
    editors_draft_base: &str,
    descriptions_by_id: &BTreeMap<&str, &str>,
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    for module in modules {
        let base = module.anchor_base.as_deref().unwrap_or(editors_draft_base);
        for property in &module.properties {
            accumulator_for(attributes, &property.name).merge_property(
                property,
                base,
                descriptions_by_id,
            );
        }
        for attribute in &module.global_attributes {
            let entry = accumulator_for(attributes, &attribute.name);
            entry.merge_ref(attribute, base, descriptions_by_id);
            if !attribute.elements.is_empty() {
                entry.bearers.extend(attribute.elements.iter().cloned());
            }
        }
        for category in &module.attribute_categories {
            for attribute in &category.attributes {
                let entry = accumulator_for(attributes, &attribute.name);
                entry.merge_ref(attribute, base, descriptions_by_id);
                if category.name == "presentation" {
                    entry.presentation_attribute = Some(entry.name.clone());
                }
            }
        }
    }
}

fn seed_presentation_attribute_metadata(
    presentation_attributes: &[PresentationAttribute],
    descriptions_by_id: &BTreeMap<&str, &str>,
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    for presentation in presentation_attributes {
        accumulator_for(attributes, &presentation.name).merge_presentation_href(
            presentation.href.as_deref(),
            &presentation.base,
            descriptions_by_id,
        );
    }
}

fn collect_element_attribute_bearers(
    modules: &[Definitions],
    editors_draft_base: &str,
    presentation_attributes: &[PresentationAttribute],
    category_attributes: &BTreeMap<String, Vec<CategorizedAttribute>>,
    descriptions_by_id: &BTreeMap<&str, &str>,
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    for module in modules {
        let base = module.anchor_base.as_deref().unwrap_or(editors_draft_base);
        for element in &module.elements {
            collect_direct_attribute_bearers(element, base, descriptions_by_id, attributes);
            collect_category_attribute_bearers(
                element,
                presentation_attributes,
                category_attributes,
                descriptions_by_id,
                attributes,
            );
        }
    }
}

fn collect_direct_attribute_bearers(
    element: &crate::extract::ElementDef,
    base: &str,
    descriptions_by_id: &BTreeMap<&str, &str>,
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    for attribute in &element.attributes {
        let entry = accumulator_for(attributes, &attribute.name);
        entry.merge_ref(attribute, base, descriptions_by_id);
        if attribute.elements.is_empty() {
            entry.bearers.insert(element.name.clone());
        } else {
            entry.bearers.extend(attribute.elements.iter().cloned());
        }
    }
    for attribute_name in element
        .common_attributes
        .iter()
        .chain(&element.geometry_properties)
    {
        accumulator_for(attributes, attribute_name)
            .bearers
            .insert(element.name.clone());
    }
}

fn collect_category_attribute_bearers(
    element: &crate::extract::ElementDef,
    presentation_attributes: &[PresentationAttribute],
    category_attributes: &BTreeMap<String, Vec<CategorizedAttribute>>,
    descriptions_by_id: &BTreeMap<&str, &str>,
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    for category_name in &element.attribute_categories {
        if category_name == "presentation" {
            collect_presentation_attribute_bearers(
                element,
                presentation_attributes,
                descriptions_by_id,
                attributes,
            );
        }
        if category_name == "deprecated xlink" {
            continue;
        }
        let Some(category) = category_attributes.get(category_name.as_str()) else {
            continue;
        };
        for attribute in category {
            collect_one_category_attribute_bearer(
                element,
                attribute,
                descriptions_by_id,
                attributes,
            );
        }
    }
}

fn collect_presentation_attribute_bearers(
    element: &crate::extract::ElementDef,
    presentation_attributes: &[PresentationAttribute],
    descriptions_by_id: &BTreeMap<&str, &str>,
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    for presentation in presentation_attributes {
        let entry = accumulator_for(attributes, &presentation.name);
        entry.merge_presentation_href(
            presentation.href.as_deref(),
            &presentation.base,
            descriptions_by_id,
        );
        entry.bearers.insert(element.name.clone());
    }
}

fn collect_one_category_attribute_bearer(
    element: &crate::extract::ElementDef,
    attribute: &CategorizedAttribute,
    descriptions_by_id: &BTreeMap<&str, &str>,
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    let entry = accumulator_for(attributes, &attribute.attribute.name);
    entry.merge_ref(&attribute.attribute, &attribute.base, descriptions_by_id);
    if attribute.presentation {
        entry.presentation_attribute = Some(entry.name.clone());
    }
    if attribute.attribute.elements.is_empty() {
        entry.bearers.insert(element.name.clone());
    } else {
        entry
            .bearers
            .extend(attribute.attribute.elements.iter().cloned());
    }
}

/// Module property declarations paired with the base URL needed to resolve
/// each property href.
fn module_properties<'a>(
    modules: &'a [Definitions],
    editors_draft_base: &str,
) -> Vec<CategorizedProperty<'a>> {
    let mut properties = Vec::new();
    for module in modules {
        let base = module
            .anchor_base
            .as_deref()
            .unwrap_or(editors_draft_base)
            .to_owned();
        properties.extend(
            module
                .properties
                .iter()
                .map(|property| CategorizedProperty {
                    property,
                    base: base.clone(),
                }),
        );
    }
    properties
}

/// A property paired with the base URL needed to resolve its href.
struct CategorizedProperty<'a> {
    property: &'a PropertyDef,
    base: String,
}

/// Presentation attribute declarations from the spec's
/// `presentationattributes` category field, paired with property href metadata.
fn presentation_attributes(
    modules: &[Definitions],
    module_properties: &[CategorizedProperty<'_>],
    editors_draft_base: &str,
) -> Vec<PresentationAttribute> {
    let property_sources: BTreeMap<&str, (&PropertyDef, &str)> = module_properties
        .iter()
        .map(|property| {
            (
                property.property.name.as_str(),
                (property.property, property.base.as_str()),
            )
        })
        .collect();
    let mut presentations: BTreeMap<String, PresentationAttribute> = BTreeMap::new();
    for module in modules {
        let base = module
            .anchor_base
            .as_deref()
            .unwrap_or(editors_draft_base)
            .to_owned();
        for category in &module.attribute_categories {
            if category.name != "presentation" {
                continue;
            }
            for name in &category.presentation_attributes {
                let canonical = canonical_attribute_name(name).into_owned();
                let (href, source_base) = property_sources.get(canonical.as_str()).map_or_else(
                    || (None, base.clone()),
                    |(property, property_base)| {
                        (property.href.clone(), (*property_base).to_owned())
                    },
                );
                presentations
                    .entry(canonical.clone())
                    .or_insert(PresentationAttribute {
                        name: canonical,
                        href,
                        base: source_base,
                    });
            }
        }
    }
    presentations.into_values().collect()
}

/// A presentation attribute name with its property permalink metadata.
struct PresentationAttribute {
    name: String,
    href: Option<String>,
    base: String,
}

/// Attribute-category membership across all modules, preserving each member's
/// own permalink base.
fn attribute_categories(
    modules: &[Definitions],
    editors_draft_base: &str,
) -> BTreeMap<String, Vec<CategorizedAttribute>> {
    let mut categories: BTreeMap<String, Vec<CategorizedAttribute>> = BTreeMap::new();
    for module in modules {
        let base = module
            .anchor_base
            .as_deref()
            .unwrap_or(editors_draft_base)
            .to_owned();
        for category in &module.attribute_categories {
            let entry = categories.entry(category.name.clone()).or_default();
            entry.extend(category.attributes.iter().cloned().map(|attribute| {
                CategorizedAttribute {
                    attribute,
                    base: base.clone(),
                    presentation: category.name == "presentation",
                }
            }));
        }
    }
    categories
}

/// A category member paired with the base URL needed to resolve its href.
struct CategorizedAttribute {
    attribute: AttributeRef,
    base: String,
    presentation: bool,
}

/// Mutable aggregation for duplicate declarations of one canonical attribute.
#[derive(Debug)]
struct AttributeAccumulator {
    name: String,
    description: Option<String>,
    spec_url: Option<String>,
    metadata_priority: Option<MetadataPriority>,
    animatable: Option<bool>,
    presentation_attribute: Option<String>,
    bearers: BTreeSet<String>,
    global: bool,
}

impl AttributeAccumulator {
    /// Start tracking one canonical attribute name.
    const fn new(name: String) -> Self {
        Self {
            name,
            description: None,
            spec_url: None,
            metadata_priority: None,
            animatable: None,
            presentation_attribute: None,
            bearers: BTreeSet::new(),
            global: false,
        }
    }

    /// Merge href/animatability metadata from one spec attribute reference.
    fn merge_ref(
        &mut self,
        attribute: &AttributeRef,
        base: &str,
        descriptions_by_id: &BTreeMap<&str, &str>,
    ) {
        self.merge_href(
            attribute.href.as_deref(),
            base,
            descriptions_by_id,
            metadata_priority(attribute.name.as_str()),
        );
        match attribute.animatable {
            Some(true) => self.animatable = Some(true),
            Some(false) if self.animatable.is_none() => self.animatable = Some(false),
            _ => {}
        }
    }

    /// Merge href metadata from a CSS/SVG property.
    fn merge_property(
        &mut self,
        property: &PropertyDef,
        base: &str,
        descriptions_by_id: &BTreeMap<&str, &str>,
    ) {
        self.merge_href(
            property.href.as_deref(),
            base,
            descriptions_by_id,
            MetadataPriority::Normal,
        );
    }

    /// Merge href metadata and mark this attribute as a presentation attribute.
    fn merge_presentation_href(
        &mut self,
        href: Option<&str>,
        base: &str,
        descriptions_by_id: &BTreeMap<&str, &str>,
    ) {
        self.merge_href(href, base, descriptions_by_id, MetadataPriority::Normal);
        self.presentation_attribute = Some(self.name.clone());
    }

    /// Set the spec URL, preferring canonical declarations over legacy aliases.
    fn merge_href(
        &mut self,
        href: Option<&str>,
        base: &str,
        descriptions_by_id: &BTreeMap<&str, &str>,
        priority: MetadataPriority,
    ) {
        let Some(href) = href else {
            return;
        };
        let description = description_for_href(Some(href), descriptions_by_id);
        let should_replace = self
            .metadata_priority
            .is_none_or(|current| priority > current);
        if should_replace {
            self.spec_url = Some(resolve_url(base, href));
            self.description = description;
            self.metadata_priority = Some(priority);
        } else if self.description.is_none()
            && description.is_some()
            && self
                .metadata_priority
                .is_some_and(|current| priority == current)
        {
            self.spec_url = Some(resolve_url(base, href));
            self.description = description;
        }
    }

    /// Convert the accumulator into its serialized catalog entry.
    fn finish(
        self,
        all_elements: &BTreeSet<String>,
        properties_by_name: &BTreeMap<&str, Vec<&PropertyValueDef>>,
        legacy_value_overrides: &BTreeMap<String, Vec<CatalogAttributeValueOverride>>,
        descriptions_by_id: &BTreeMap<&str, &str>,
        grammar_inputs: Option<&crate::treesitter::GrammarProjectionInputs>,
    ) -> CatalogAttribute {
        let property_group = properties_by_name
            .get(self.name.as_str())
            .map_or(&[][..], Vec::as_slice);
        let property = property_group.first().copied();
        let (values, element_values) =
            resolve_property_values(property_group, descriptions_by_id, grammar_inputs);
        let value_overrides = legacy_value_overrides
            .get(&self.name)
            .cloned()
            .unwrap_or_default();
        let animatable = self
            .animatable
            .unwrap_or_else(|| property.is_some_and(property_is_animatable));
        let applicability = if self.global || self.bearers == *all_elements {
            CatalogAttributeApplicability::Global
        } else if self.bearers.is_empty() {
            CatalogAttributeApplicability::None
        } else {
            CatalogAttributeApplicability::Elements {
                elements: self.bearers.into_iter().collect(),
            }
        };
        CatalogAttribute {
            name: self.name,
            description: self.description,
            mdn_url: None,
            spec_url: self.spec_url,
            deprecated: false,
            experimental: false,
            standard_track: None,
            animatable,
            presentation_attribute: self.presentation_attribute,
            baseline: None,
            browser_support: None,
            element_compat: Vec::new(),
            element_values,
            value_overrides,
            values,
            applicability,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MetadataPriority {
    LegacyAlias,
    Normal,
}

fn metadata_priority(attribute_name: &str) -> MetadataPriority {
    if attribute_name.starts_with("xlink:") {
        MetadataPriority::LegacyAlias
    } else {
        MetadataPriority::Normal
    }
}

impl CatalogAttribute {
    fn apply_compat(&mut self, facts: &CatalogCompatFacts) {
        self.mdn_url.clone_from(&facts.mdn_url);
        self.deprecated = facts.deprecated;
        self.experimental = facts.experimental;
        self.standard_track = facts.standard_track;
        self.baseline = facts.baseline;
        self.browser_support.clone_from(&facts.browser_support);
    }

    fn apply_element_compat(&mut self, attribute: &crate::compat::CompatAttribute) {
        self.element_compat = attribute
            .element_facts
            .iter()
            .map(|(element, facts)| CatalogAttributeElementCompat {
                element: element.clone(),
                facts: facts.clone(),
            })
            .collect();
    }
}

/// Get or insert the accumulator for a raw attribute name.
fn accumulator_for<'a>(
    attributes: &'a mut BTreeMap<String, AttributeAccumulator>,
    raw_name: &str,
) -> &'a mut AttributeAccumulator {
    let canonical = canonical_attribute_name(raw_name).into_owned();
    attributes
        .entry(canonical.clone())
        .or_insert_with(|| AttributeAccumulator::new(canonical))
}

/// Resolve the canonical value space for an attribute and any element-scoped
/// divergences across the property definitions that share its name.
///
/// Most attributes have a single defining table, so the common path returns
/// that table's value space with no element overrides. Some attribute names are
/// defined on multiple bearer elements with genuinely different value grammars
/// (e.g. `operator` on `feComposite` vs `feMorphology`, or `offset`, which is
/// `<number> | <percentage>` on `stop` but bare `<number>` on the `feFunc*`
/// transfer functions). Every distinct value space is preserved:
///
/// * The **canonical** [`CatalogAttribute::values`] is the first *unscoped*
///   definition (an authoritative property/propdef table, which carries no
///   `data-dfn-for`), preserving the historical first-wins grammar. When no
///   unscoped definition exists (e.g. `operator`), the bearer-scoped definition
///   whose element name sorts first becomes canonical.
/// * When an unscoped definition exists, bearer-scoped definitions are emitted
///   only when they differ from that canonical grammar.
/// * When all definitions are bearer-scoped, every scoped definition is emitted,
///   including the canonical one, so element-specific facts remain explicit.
fn resolve_property_values(
    properties: &[&PropertyValueDef],
    descriptions_by_id: &BTreeMap<&str, &str>,
    grammar_inputs: Option<&crate::treesitter::GrammarProjectionInputs>,
) -> (CatalogAttributeValues, Vec<CatalogAttributeElementValues>) {
    // Bearer-scoped definitions (those carrying `data-dfn-for`), sorted by their
    // bearer element name for deterministic, documented selection.
    let mut scoped: Vec<(&str, &PropertyValueDef)> = properties
        .iter()
        .filter_map(|property| {
            property
                .dfn_for
                .as_deref()
                .map(|element| (element, *property))
        })
        .collect();
    scoped.sort_by_key(|(element, _)| *element);

    // Prefer an authoritative unscoped definition for the canonical value space;
    // otherwise fall back to the first bearer-scoped definition.
    let canonical_def = properties
        .iter()
        .find(|property| property.dfn_for.is_none())
        .copied()
        .or_else(|| scoped.first().map(|(_, def)| *def));
    let Some(canonical_def) = canonical_def else {
        return (CatalogAttributeValues::FreeText, Vec::new());
    };
    let canonical_values = values_for_property(canonical_def, descriptions_by_id, grammar_inputs);
    let canonical_is_unscoped = canonical_def.dfn_for.is_none();

    let mut element_values: Vec<CatalogAttributeElementValues> = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (element, def) in &scoped {
        if !seen.insert(element) {
            continue;
        }
        let values = values_for_property(def, descriptions_by_id, grammar_inputs);
        if values != canonical_values || !canonical_is_unscoped {
            element_values.push(CatalogAttributeElementValues {
                element: (*element).to_owned(),
                values,
            });
        }
    }
    (canonical_values, element_values)
}

fn augment_properties_with_direct_attribute_scopes(
    properties: &[PropertyValueDef],
    modules: &[Definitions],
) -> Vec<PropertyValueDef> {
    let direct_scopes = direct_attribute_scopes(modules);
    let mut scoped = properties.to_vec();
    for property in properties {
        let Some(id) = property.id.as_deref() else {
            continue;
        };
        let key = (property.name.clone(), id.to_owned());
        let Some(elements) = direct_scopes.get(&key) else {
            continue;
        };
        for element in elements {
            if property.dfn_for.as_deref() == Some(element.as_str()) {
                continue;
            }
            let mut property = property.clone();
            property.dfn_for = Some(element.clone());
            scoped.push(property);
        }
    }
    scoped
}

fn direct_attribute_scopes(modules: &[Definitions]) -> BTreeMap<(String, String), Vec<String>> {
    let mut scopes: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for module in modules {
        for element in &module.elements {
            for attribute in &element.attributes {
                let Some(fragment) = attribute.href.as_deref().and_then(attribute_href_fragment)
                else {
                    continue;
                };
                scopes
                    .entry((
                        canonical_attribute_name(&attribute.name).into_owned(),
                        fragment.to_owned(),
                    ))
                    .or_default()
                    .push(element.name.clone());
            }
        }
    }
    for elements in scopes.values_mut() {
        elements.sort();
        elements.dedup();
    }
    scopes
}

fn attribute_href_fragment(href: &str) -> Option<&str> {
    href.rsplit_once('#')
        .map_or(Some(href), |(_, fragment)| Some(fragment))
}

/// Parse a spec "see-reference" value (`(see <attr> attribute)`) into the
/// referenced attribute name.
///
/// Some `<dfn>`s define a value space by deferring to another attribute's
/// grammar in prose, e.g. `in2 = "(see in attribute)"`. The extraction in
/// [`chapter.rs`](crate::chapter) faithfully captures that prose, leaving the
/// cross-reference to be resolved here against the full attribute set.
///
/// The match is shape-based (case- and whitespace-insensitive), never keyed on
/// a specific attribute name: any value of the form `(see NAME attribute)`,
/// where `NAME` is an attribute identifier (`[A-Za-z][A-Za-z0-9_:-]*`),
/// resolves to `NAME`. Returns `None` for anything else (real grammars,
/// alternations, malformed prose).
fn parse_see_reference(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    let inner = trimmed.strip_prefix('(')?.strip_suffix(')')?.trim();
    let (prefix, rest) = inner.split_once(char::is_whitespace)?;
    if !prefix.eq_ignore_ascii_case("see") {
        return None;
    }
    let mut tokens = rest.split_whitespace();
    let name = tokens.next()?;
    // The closing keyword must be exactly `attribute` and nothing may follow.
    match tokens.next() {
        Some(tail) if tail.eq_ignore_ascii_case("attribute") && tokens.next().is_none() => {}
        _ => return None,
    }
    let mut chars = name.chars();
    let first_is_alpha = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic());
    let rest_is_ident = name
        .chars()
        .skip(1)
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | ':' | '-'));
    if first_is_alpha && rest_is_ident {
        Some(name)
    } else {
        None
    }
}

/// Select the canonical `(value, keywords)` of a grouped attribute definition,
/// mirroring the canonical-def selection in [`resolve_property_values`]: prefer
/// the first unscoped definition, else the bearer-scoped definition whose
/// element name sorts first.
fn canonical_def_value_keywords(
    defs: &[PropertyValueDef],
) -> Option<(Option<String>, Vec<String>)> {
    let unscoped = defs.iter().find(|def| def.dfn_for.is_none());
    let canonical = unscoped.or_else(|| {
        defs.iter()
            .filter(|def| def.dfn_for.is_some())
            .min_by(|left, right| left.dfn_for.cmp(&right.dfn_for))
    })?;
    Some((canonical.value.clone(), canonical.keywords.clone()))
}

/// Resolve spec "see-references" (`(see <attr> attribute)`) by inheriting the
/// referenced attribute's value space, returning an owned, rewritten copy of
/// the input definitions.
///
/// Extraction keeps such cross-references as faithful prose; resolution needs
/// the whole attribute set (the referenced grammar), so it runs here once the
/// definitions are grouped by name. A bounded fixpoint resolves reference
/// chains (`a -> b -> concrete`) while a visited set keeps cycles safe: a
/// reference is only rewritten when its target's value is itself concrete (not
/// another unresolved see-reference), and iteration stops once no definition
/// changes. Dangling or cyclic references are left as their original prose,
/// never fabricated.
fn resolve_see_references(properties: &[PropertyValueDef]) -> Vec<PropertyValueDef> {
    let mut resolved: Vec<PropertyValueDef> = properties.to_vec();
    // Bound the fixpoint by the definition count: each pass resolves at least
    // one more link of any acyclic chain, so chains can be at most this long.
    for _ in 0..resolved.len() {
        // Snapshot each name's canonical `(value, keywords)` into an owned map so
        // the rewrite below can mutate `resolved` without aliasing the lookup.
        let mut groups: BTreeMap<&str, Vec<PropertyValueDef>> = BTreeMap::new();
        for def in &resolved {
            groups
                .entry(def.name.as_str())
                .or_default()
                .push(def.clone());
        }
        let canonical: BTreeMap<String, (Option<String>, Vec<String>)> = groups
            .into_iter()
            .filter_map(|(name, defs)| {
                canonical_def_value_keywords(&defs).map(|values| (name.to_owned(), values))
            })
            .collect();
        let mut changed = false;
        for def in &mut resolved {
            let Some(target) = def.value.as_deref().and_then(parse_see_reference) else {
                continue;
            };
            let Some((target_value, target_keywords)) = canonical.get(target) else {
                continue;
            };
            // Only inherit from a concrete target; deferring to another
            // see-reference (or to nothing) is left for a later pass or, if it
            // never concretizes (cycle/dangling), left as prose.
            let target_is_reference = target_value
                .as_deref()
                .is_some_and(|value| parse_see_reference(value).is_some());
            if target_value.is_none() || target_is_reference {
                continue;
            }
            def.value = target_value.clone();
            def.keywords = target_keywords.clone();
            changed = true;
        }
        if !changed {
            break;
        }
    }
    resolved
}

/// Convert a CSS property grammar into the runtime value-space subset we know
/// how to represent today.
fn values_for_property(
    property: &PropertyValueDef,
    descriptions_by_id: &BTreeMap<&str, &str>,
    grammar_inputs: Option<&crate::treesitter::GrammarProjectionInputs>,
) -> CatalogAttributeValues {
    let mut keywords = property.keywords.clone();
    keywords.sort();
    keywords.dedup();
    if !keywords.is_empty()
        && property
            .value
            .as_deref()
            .is_some_and(is_keyword_only_grammar)
    {
        return CatalogAttributeValues::Enum { values: keywords };
    }

    let Some(raw_value) = property.value.as_deref() else {
        return CatalogAttributeValues::FreeText;
    };
    let grammar = repair_css_type_tokens(raw_value);
    let value = grammar.as_str();
    let normalized = value.to_ascii_lowercase();
    if is_see_below_value(value) {
        return property
            .id
            .as_deref()
            .and_then(|id| descriptions_by_id.get(id).copied())
            .map_or(
                CatalogAttributeValues::FreeText,
                value_from_see_below_description,
            );
    }
    if let Some(values) = value_from_referenced_syntax(value) {
        return values;
    }
    if is_path_data_value(&normalized) {
        return CatalogAttributeValues::PathData;
    }
    if is_semicolon_number_list_value(&normalized) {
        return CatalogAttributeValues::SemicolonNumberList;
    }
    if is_coordinate_pair_list_value(&normalized) {
        return CatalogAttributeValues::CoordinatePairList;
    }
    if is_coordinate_pair_value(&normalized) {
        return CatalogAttributeValues::CoordinatePair;
    }
    if is_suggested_file_name_prose_value(value) {
        return CatalogAttributeValues::SuggestedFileName;
    }
    // Semantic projection policy: `<transform-list>` currently becomes a typed
    // transform-function list for downstream consumers instead of remaining raw
    // CSS grammar text.
    if (property.name == "transform" || normalized.contains("<transform-list>"))
        && let Some(inputs) = grammar_inputs
    {
        let functions = inputs.transform_functions();
        if !functions.is_empty() {
            return CatalogAttributeValues::Transform { functions };
        }
    }
    if normalized == "<color>" || normalized == "<paint>" {
        return CatalogAttributeValues::Color;
    }
    if normalized == "<url>" || normalized == "<iri>" {
        return CatalogAttributeValues::Url;
    }
    if normalized == "<number> | <percentage>" || normalized == "<percentage> | <number>" {
        return CatalogAttributeValues::NumberOrPercentage;
    }
    if matches!(
        normalized.as_str(),
        "<length>" | "<length-percentage>" | "<length> | <percentage>"
    ) {
        return CatalogAttributeValues::Length;
    }
    let graph = css_grammar_graph(value);
    CatalogAttributeValues::CssGrammar { grammar, graph }
}

fn is_see_below_value(value: &str) -> bool {
    let trimmed = value.trim();
    let unwrapped = trimmed
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .map_or(trimmed, str::trim);
    unwrapped.eq_ignore_ascii_case("see below")
}

fn value_from_referenced_syntax(value: &str) -> Option<CatalogAttributeValues> {
    // Only exact fetched syntax citations are classified here; anything else
    // stays raw rather than being inferred by substring heuristics.
    match normalize_prose_phrase(value).as_str() {
        "boolean attribute [html]" => Some(CatalogAttributeValues::Boolean),
        "space-separated valid non-empty url tokens [html]" => {
            Some(CatalogAttributeValues::UrlTokenList)
        }
        "set of space-separated tokens [html]" | "space-separated keyword tokens [html]" => {
            Some(CatalogAttributeValues::TokenList)
        }
        "set of comma-separated tokens [html]" => Some(CatalogAttributeValues::CommaTokenList),
        "language-tag [abnf]" | "a bcp 47 language tag string [html]" => {
            Some(CatalogAttributeValues::LanguageTag)
        }
        "valid integer [html]" => Some(CatalogAttributeValues::Integer),
        "url [url]" => Some(CatalogAttributeValues::Url),
        "a referrer policy string [referrerpolicy]" => Some(CatalogAttributeValues::ReferrerPolicy),
        "a mime type string [html]" => Some(CatalogAttributeValues::MediaType),
        _ => None,
    }
}

fn is_path_data_value(value: &str) -> bool {
    matches!(value.trim(), "path data" | "svg-path [ebnf]")
}

fn is_semicolon_number_list_value(value: &str) -> bool {
    let value = value.trim();
    value.contains("<number>") && value.contains("[; <number>]*")
}

fn is_coordinate_pair_value(value: &str) -> bool {
    let value = value.trim();
    value == "x, y coordinate pair"
}

fn is_coordinate_pair_list_value(value: &str) -> bool {
    let value = value.trim();
    value == "semicolon-separated x, y coordinate pairs"
}

fn is_suggested_file_name_prose_value(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("any value") && lower.contains("suggested file name")
}

fn value_from_see_below_description(prose: &str) -> CatalogAttributeValues {
    // Semantic projection policy: `(see below)` is resolved from fetched prose
    // here, not from a first-class grammar production.
    let normalized = normalize_prose_phrase(prose);
    if normalized.contains("must not be an empty string")
        && normalized.contains("must not contain any whitespace")
    {
        return CatalogAttributeValues::Id;
    }
    let enum_values = quoted_keyword_values(prose);
    if !enum_values.is_empty()
        && (normalized.contains("possible values")
            || normalized.contains("values are the strings")
            || normalized.contains("values are "))
    {
        return CatalogAttributeValues::Enum {
            values: enum_values,
        };
    }
    if normalized.contains("parsed as a media_query_list") {
        return CatalogAttributeValues::MediaQueryList;
    }
    if normalized.contains("style sheet language as a media type") {
        return CatalogAttributeValues::MediaType;
    }
    if normalized.contains("parsed as a declaration-list") {
        return CatalogAttributeValues::CssDeclarationList;
    }
    CatalogAttributeValues::FreeText
}

fn normalize_prose_phrase(text: &str) -> String {
    normalize_ws(text).to_ascii_lowercase()
}

fn quoted_keyword_values(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    for quoted in text.split('\'').skip(1).step_by(2) {
        if is_keyword_token(quoted) {
            values.push(quoted.to_owned());
        }
    }
    values.sort();
    values.dedup();
    values
}

/// Whether the raw value grammar is just bare keyword alternatives.
fn is_keyword_only_grammar(value: &str) -> bool {
    value.split('|').all(|token| is_keyword_token(token.trim()))
}

fn repair_css_type_tokens(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut repaired = String::with_capacity(value.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'<'
            && bytes
                .get(index + 1)
                .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'\'')
        {
            let start = index;
            repaired.push('<');
            index += 1;
            while index < bytes.len()
                && bytes[index] != b'>'
                && !bytes[index].is_ascii_whitespace()
                && !matches!(bytes[index], b'|' | b'[' | b']' | b',')
            {
                repaired.push(char::from(bytes[index]));
                index += 1;
            }
            repaired.push('>');
            if bytes.get(index) == Some(&b'>') {
                index += 1;
            }
            if index == start {
                index += 1;
            }
            continue;
        }
        repaired.push(char::from(bytes[index]));
        index += 1;
    }
    repaired
}

#[derive(Clone, Copy)]
struct GrammarContext {
    node_id: u16,
    last_child: Option<u16>,
}

fn css_grammar_graph(value: &str) -> CatalogCssGrammarGraph {
    let mut graph = empty_css_grammar_graph();
    let mut contexts = vec![GrammarContext {
        node_id: 0,
        last_child: None,
    }];
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => index += 1,
            b'<' => {
                let Some(end) = value[index..].find('>') else {
                    break;
                };
                let text = &value[index..=index + end];
                push_grammar_node(
                    &mut graph,
                    &mut contexts,
                    CatalogCssGrammarNodeKind::Type,
                    Some(text),
                );
                index += end + 1;
            }
            b'[' => {
                let id = push_grammar_node(
                    &mut graph,
                    &mut contexts,
                    CatalogCssGrammarNodeKind::Group,
                    None,
                );
                contexts.push(GrammarContext {
                    node_id: id,
                    last_child: None,
                });
                index += 1;
            }
            b']' => {
                if contexts.len() > 1 {
                    contexts.pop();
                }
                index += 1;
            }
            b'|' if bytes.get(index + 1) == Some(&b'|') => {
                push_grammar_node(
                    &mut graph,
                    &mut contexts,
                    CatalogCssGrammarNodeKind::Operator,
                    Some("||"),
                );
                index += 2;
            }
            b'&' if bytes.get(index + 1) == Some(&b'&') => {
                push_grammar_node(
                    &mut graph,
                    &mut contexts,
                    CatalogCssGrammarNodeKind::Operator,
                    Some("&&"),
                );
                index += 2;
            }
            b'|' | b',' | b'?' | b'*' | b'+' | b'#' | b'!' => {
                let text = &value[index..=index];
                push_grammar_node(
                    &mut graph,
                    &mut contexts,
                    CatalogCssGrammarNodeKind::Operator,
                    Some(text),
                );
                index += 1;
            }
            byte if byte.is_ascii_alphanumeric() || byte == b'-' => {
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'-')
                {
                    index += 1;
                }
                let text = &value[start..index];
                let kind = if bytes.get(index) == Some(&b'(') {
                    while index < bytes.len() && bytes[index] != b')' {
                        index += 1;
                    }
                    if index < bytes.len() {
                        index += 1;
                    }
                    CatalogCssGrammarNodeKind::Function
                } else {
                    CatalogCssGrammarNodeKind::Keyword
                };
                push_grammar_node(&mut graph, &mut contexts, kind, Some(text));
            }
            _ => index += 1,
        }
    }

    graph
}

fn empty_css_grammar_graph() -> CatalogCssGrammarGraph {
    CatalogCssGrammarGraph {
        root: 0,
        nodes: vec![CatalogCssGrammarNode {
            id: 0,
            kind: CatalogCssGrammarNodeKind::Root,
            text: None,
        }],
        edges: Vec::new(),
    }
}

fn push_grammar_node(
    graph: &mut CatalogCssGrammarGraph,
    contexts: &mut [GrammarContext],
    kind: CatalogCssGrammarNodeKind,
    text: Option<&str>,
) -> u16 {
    let Ok(id) = u16::try_from(graph.nodes.len()) else {
        return u16::MAX;
    };
    graph.nodes.push(CatalogCssGrammarNode {
        id,
        kind,
        text: text.map(str::to_owned),
    });

    let Some(current) = contexts.last_mut() else {
        return id;
    };
    graph.edges.push(CatalogCssGrammarEdge {
        from: current.node_id,
        to: id,
        kind: CatalogCssGrammarEdgeKind::Contains,
    });
    if let Some(previous) = current.last_child {
        graph.edges.push(CatalogCssGrammarEdge {
            from: previous,
            to: id,
            kind: CatalogCssGrammarEdgeKind::Next,
        });
    }
    current.last_child = Some(id);
    id
}

/// Whether a CSS/SVG property definition says the property is animatable.
fn property_is_animatable(property: &PropertyValueDef) -> bool {
    let Some(animation_type) = property.animation_type.as_deref() else {
        return false;
    };
    !matches!(
        animation_type.trim().to_ascii_lowercase().as_str(),
        "" | "no" | "none" | "not animatable"
    )
}

/// Resolve `href` against a module base unless it is already absolute.
fn resolve_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_owned()
    } else {
        format!("{base}{href}")
    }
}

/// Canonicalize legacy spellings as an explicit compatibility layer before the
/// semantic catalog is written.
pub fn canonical_attribute_name(name: &str) -> std::borrow::Cow<'_, str> {
    match name {
        "xlink:href" => std::borrow::Cow::Borrowed("href"),
        other => std::borrow::Cow::Borrowed(other),
    }
}

/// Flatten an element's allowed child categories (resolved to their members)
/// unioned with its explicit allowed elements, sorted and deduped.
fn flatten_children(
    element: &crate::extract::ElementDef,
    members: &BTreeMap<&str, Vec<&str>>,
) -> Vec<String> {
    let mut names: Vec<String> = element
        .allowed_element_categories
        .iter()
        .filter_map(|category| members.get(category.as_str()))
        .flatten()
        .map(|name| (*name).to_owned())
        .chain(element.allowed_elements.iter().cloned())
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::{CompatAttribute, CompatCatalog};
    use crate::extract::{
        AttributeCategory, AttributeRef, ContentModelKind, ElementDef, PropertyDef,
    };

    fn attr(name: &str, href: &str, animatable: Option<bool>) -> AttributeRef {
        AttributeRef {
            name: name.to_owned(),
            href: Some(href.to_owned()),
            animatable,
            elements: Vec::new(),
        }
    }

    fn element(name: &str) -> ElementDef {
        ElementDef {
            name: name.to_owned(),
            href: Some(format!("{name}.html#{name}")),
            content_model: Some(ContentModelKind::AnyOf),
            content_model_description: None,
            allowed_element_categories: Vec::new(),
            allowed_elements: Vec::new(),
            attribute_categories: vec!["core".to_owned()],
            common_attributes: Vec::new(),
            geometry_properties: Vec::new(),
            interfaces: Vec::new(),
            attributes: Vec::new(),
        }
    }

    fn defs() -> Definitions {
        let mut rect = element("rect");
        rect.attribute_categories.push("presentation".to_owned());
        rect.geometry_properties.push("x".to_owned());
        rect.common_attributes.push("viewBox".to_owned());

        let mut link = element("a");
        link.attributes
            .push(attr("xlink:href", "linking.html#XLinkHref", Some(true)));
        link.attributes.push(attr(
            "href",
            "linking.html#AElementHrefAttribute",
            Some(true),
        ));

        Definitions {
            anchor_base: None,
            elements: vec![rect, link, element("metadata")],
            global_attributes: vec![
                attr("id", "struct.html#IDAttribute", None),
                attr("viewBox", "coords.html#ViewBoxAttribute", Some(true)),
            ],
            properties: vec![PropertyDef {
                name: "fill".to_owned(),
                href: Some("painting.html#FillProperty".to_owned()),
            }],
            element_categories: Vec::new(),
            attribute_categories: vec![
                AttributeCategory {
                    name: "core".to_owned(),
                    href: None,
                    attributes: vec![
                        attr("class", "struct.html#ClassAttribute", None),
                        attr("id", "struct.html#IDAttribute", None),
                    ],
                    presentation_attributes: Vec::new(),
                },
                AttributeCategory {
                    name: "presentation".to_owned(),
                    href: None,
                    attributes: Vec::new(),
                    presentation_attributes: vec!["fill".to_owned()],
                },
            ],
            terms: Vec::new(),
            symbols: Vec::new(),
            interfaces: Vec::new(),
        }
    }

    fn property(name: &str, value: &str, keywords: &[&str]) -> PropertyValueDef {
        PropertyValueDef {
            name: name.to_owned(),
            dfn_for: None,
            id: None,
            value: Some(value.to_owned()),
            keywords: keywords
                .iter()
                .map(|keyword| (*keyword).to_owned())
                .collect(),
            initial: None,
            applies_to: None,
            inherited: None,
            computed_value: None,
            animation_type: Some("by computed value type".to_owned()),
        }
    }

    /// An element-attrdef definition scoped to a bearer element (carries the
    /// source `data-dfn-for`), used to model attributes whose value grammar
    /// diverges across elements (e.g. `operator`).
    fn scoped_property(
        name: &str,
        element: &str,
        value: &str,
        keywords: &[&str],
    ) -> PropertyValueDef {
        PropertyValueDef {
            dfn_for: Some(element.to_owned()),
            ..property(name, value, keywords)
        }
    }

    fn compat_source(name: &str) -> CatalogPackageSource {
        CatalogPackageSource {
            name: name.to_owned(),
            version: "0.0.0".to_owned(),
            url: format!("https://example.test/{name}.json"),
        }
    }

    fn description(id: &str, text: &str) -> AnchorDescription {
        AnchorDescription {
            id: id.to_owned(),
            description: text.to_owned(),
        }
    }

    fn panic_catalog(result: Result<Catalog, Box<dyn std::error::Error>>) -> Catalog {
        match result {
            Ok(catalog) => catalog,
            Err(error) => panic!("catalog: {error}"),
        }
    }

    fn catalog_with_test_descriptions() -> Catalog {
        let descriptions = [
            description("rect", "The rect element defines a rectangle."),
            description("FillProperty", "The fill property paints shapes."),
            description(
                "ViewBoxAttribute",
                "The viewBox attribute defines the viewport.",
            ),
            description(
                "AElementHrefAttribute",
                "The href attribute identifies a linked resource.",
            ),
        ];
        panic_catalog(build_catalog(
            &[defs()],
            &[property(
                "fill",
                "none | context-fill",
                &["context-fill", "none"],
            )],
            &descriptions,
            "https://example.test/",
            "abc",
            None,
            CatalogLegacyInputs {
                sources: &[],
                value_overrides: &BTreeMap::new(),
                inventories: &[],
                grammar_inputs: None,
            },
        ))
    }

    fn catalog_attribute<'a>(
        catalog: &'a Catalog,
        name: &str,
    ) -> Result<&'a CatalogAttribute, Box<dyn std::error::Error>> {
        catalog
            .attributes
            .iter()
            .find(|attribute| attribute.name == name)
            .ok_or_else(|| format!("missing {name}").into())
    }

    fn graph_has_node(catalog: &Catalog, id: &str, kind: CatalogGraphNodeKind, name: &str) -> bool {
        catalog
            .graph
            .nodes
            .iter()
            .any(|node| node.id == id && node.kind == kind && node.name == name)
    }

    fn graph_has_edge(catalog: &Catalog, from: &str, to: &str, kind: CatalogGraphEdgeKind) -> bool {
        catalog
            .graph
            .edges
            .iter()
            .any(|edge| edge.from == from && edge.to == to && edge.kind == kind)
    }

    fn assert_fill_attribute(fill: &CatalogAttribute) {
        assert_eq!(fill.presentation_attribute.as_deref(), Some("fill"));
        assert_eq!(
            fill.applicability,
            CatalogAttributeApplicability::Elements {
                elements: vec!["rect".to_owned()]
            }
        );
        assert_eq!(
            fill.values,
            CatalogAttributeValues::Enum {
                values: vec!["context-fill".to_owned(), "none".to_owned()]
            }
        );
        assert_eq!(
            fill.description.as_deref(),
            Some("The fill property paints shapes.")
        );
        assert!(fill.animatable);
    }

    fn assert_href_attribute(href: &CatalogAttribute) {
        assert!(href.animatable);
        assert_eq!(
            href.spec_url.as_deref(),
            Some("https://example.test/linking.html#AElementHrefAttribute")
        );
        assert_eq!(
            href.description.as_deref(),
            Some("The href attribute identifies a linked resource.")
        );
        assert_eq!(
            href.applicability,
            CatalogAttributeApplicability::Elements {
                elements: vec!["a".to_owned()]
            }
        );
    }

    #[test]
    fn value_overrides_are_threaded_from_legacy_sources() -> Result<(), Box<dyn std::error::Error>>
    {
        let overrides = BTreeMap::from([(
            "fill".to_owned(),
            vec![CatalogAttributeValueOverride {
                profile: CatalogSpecSnapshotId::Svg11Rec20110816,
                values: CatalogAttributeValues::Enum {
                    values: vec!["legacy-fill".to_owned()],
                },
            }],
        )]);
        let catalog = panic_catalog(build_catalog(
            &[defs()],
            &[property(
                "fill",
                "none | context-fill",
                &["context-fill", "none"],
            )],
            &[],
            "https://example.test/",
            "abc",
            None,
            CatalogLegacyInputs {
                sources: &[],
                value_overrides: &overrides,
                inventories: &[],
                grammar_inputs: None,
            },
        ));

        let fill = catalog_attribute(&catalog, "fill")?;
        assert_eq!(fill.value_overrides, overrides["fill"]);
        assert!(graph_has_node(
            &catalog,
            "value:fill@Svg11Rec20110816",
            CatalogGraphNodeKind::ValueGrammar,
            "fill@Svg11Rec20110816 (enum)"
        ));
        assert!(graph_has_edge(
            &catalog,
            "value:fill@Svg11Rec20110816",
            "profile:Svg11Rec20110816",
            CatalogGraphEdgeKind::OverridesValueInProfile
        ));
        Ok(())
    }

    #[test]
    fn semantic_projection_policy_classifies_transform_lists() {
        let transform = property("transform", "<transform-list>", &[]);
        let inputs = crate::treesitter::GrammarProjectionInputs::for_tests();
        assert_eq!(
            values_for_property(&transform, &BTreeMap::new(), Some(&inputs)),
            CatalogAttributeValues::Transform {
                functions: ["matrix", "translate", "scale", "rotate", "skewX", "skewY"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            }
        );
    }

    #[test]
    fn semantic_projection_policy_resolves_see_below_prose() {
        let mut xml_space = property("xml:space", "(see below)", &[]);
        xml_space.id = Some("XmlSpaceValue".to_owned());
        let descriptions =
            BTreeMap::from([("XmlSpaceValue", "The values are 'default' and 'preserve'.")]);
        assert_eq!(
            values_for_property(&xml_space, &descriptions, None),
            CatalogAttributeValues::Enum {
                values: vec!["default".to_owned(), "preserve".to_owned()],
            }
        );
    }

    #[test]
    fn semantic_projection_policy_falls_back_from_prose_content_model() {
        let mut element = element("animateMotion");
        element.content_model = None;
        element.content_model_description = Some(
            "Any number of descriptive elements, 'script' and at most one 'mpath' element, in any \
             order."
                .to_owned(),
        );
        let members = BTreeMap::from([("descriptive", vec!["desc", "metadata", "title"])]);
        assert_eq!(
            description_derived_content_model(&element, &members),
            Some(CatalogContentModel::ChildrenSet {
                elements: vec![
                    "desc".to_owned(),
                    "metadata".to_owned(),
                    "mpath".to_owned(),
                    "script".to_owned(),
                    "title".to_owned(),
                ],
            })
        );
    }

    #[test]
    fn resolve_property_values_keeps_divergent_element_value_spaces() {
        let composite = scoped_property(
            "operator",
            "feComposite",
            "over | in | out | atop | xor | lighter | arithmetic",
            &["over", "in", "out", "atop", "xor", "lighter", "arithmetic"],
        );
        let morphology = scoped_property(
            "operator",
            "feMorphology",
            "erode | dilate",
            &["erode", "dilate"],
        );

        // feMorphology is intentionally listed first to prove the canonical
        // value space is chosen by sorted bearer element, not insertion order.
        let (values, element_values) =
            resolve_property_values(&[&morphology, &composite], &BTreeMap::new(), None);

        assert_eq!(
            values,
            CatalogAttributeValues::Enum {
                values: vec![
                    "arithmetic".to_owned(),
                    "atop".to_owned(),
                    "in".to_owned(),
                    "lighter".to_owned(),
                    "out".to_owned(),
                    "over".to_owned(),
                    "xor".to_owned(),
                ],
            },
        );
        assert_eq!(
            element_values,
            vec![
                CatalogAttributeElementValues {
                    element: "feComposite".to_owned(),
                    values: CatalogAttributeValues::Enum {
                        values: vec![
                            "arithmetic".to_owned(),
                            "atop".to_owned(),
                            "in".to_owned(),
                            "lighter".to_owned(),
                            "out".to_owned(),
                            "over".to_owned(),
                            "xor".to_owned(),
                        ],
                    },
                },
                CatalogAttributeElementValues {
                    element: "feMorphology".to_owned(),
                    values: CatalogAttributeValues::Enum {
                        values: vec!["dilate".to_owned(), "erode".to_owned()],
                    },
                },
            ],
        );
    }

    #[test]
    fn resolve_property_values_keeps_unscoped_propdef_as_canonical() {
        // `offset`: the authoritative `stop` propdef is `<number> | <percentage>`
        // (no `data-dfn-for`); the `feFunc*` element-attrdefs narrow it to bare
        // `<number>`. The unscoped propdef must stay canonical so the narrower
        // bearer grammar never silently widens or drops the percentage form.
        let stop = property("offset", "<number> | <percentage>", &[]);
        let func = scoped_property("offset", "feFuncR", "<number>", &[]);

        let (values, element_values) =
            resolve_property_values(&[&func, &stop], &BTreeMap::new(), None);

        assert_eq!(values, CatalogAttributeValues::NumberOrPercentage);
        assert_eq!(
            element_values,
            vec![CatalogAttributeElementValues {
                element: "feFuncR".to_owned(),
                values: CatalogAttributeValues::CssGrammar {
                    grammar: "<number>".to_owned(),
                    graph: css_grammar_graph("<number>"),
                },
            }],
        );
    }

    #[test]
    fn resolve_property_values_no_divergence_for_single_definition() {
        let single = property("clip-rule", "nonzero | evenodd", &["nonzero", "evenodd"]);
        let (values, element_values) = resolve_property_values(&[&single], &BTreeMap::new(), None);
        assert_eq!(
            values,
            CatalogAttributeValues::Enum {
                values: vec!["evenodd".to_owned(), "nonzero".to_owned()],
            },
        );
        assert!(element_values.is_empty());
    }

    #[test]
    fn operator_retains_both_element_scoped_value_spaces() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut composite = element("feComposite");
        composite.attributes.push(attr(
            "operator",
            "filters.html#feCompositeOperatorAttribute",
            Some(true),
        ));
        let mut morphology = element("feMorphology");
        morphology.attributes.push(attr(
            "operator",
            "filters.html#feMorphologyOperatorAttribute",
            Some(true),
        ));

        let module = Definitions {
            anchor_base: None,
            elements: vec![composite, morphology],
            global_attributes: Vec::new(),
            properties: Vec::new(),
            element_categories: Vec::new(),
            attribute_categories: Vec::new(),
            terms: Vec::new(),
            symbols: Vec::new(),
            interfaces: Vec::new(),
        };

        let catalog = panic_catalog(build_catalog(
            &[module],
            &[
                scoped_property(
                    "operator",
                    "feComposite",
                    "over | in | out | atop | xor | lighter | arithmetic",
                    &["over", "in", "out", "atop", "xor", "lighter", "arithmetic"],
                ),
                scoped_property(
                    "operator",
                    "feMorphology",
                    "erode | dilate",
                    &["erode", "dilate"],
                ),
            ],
            &[],
            "https://example.test/",
            "abc",
            None,
            CatalogLegacyInputs {
                sources: &[],
                value_overrides: &BTreeMap::new(),
                inventories: &[],
                grammar_inputs: None,
            },
        ));

        let operator = catalog_attribute(&catalog, "operator")?;
        assert_eq!(
            operator.values,
            CatalogAttributeValues::Enum {
                values: vec![
                    "arithmetic".to_owned(),
                    "atop".to_owned(),
                    "in".to_owned(),
                    "lighter".to_owned(),
                    "out".to_owned(),
                    "over".to_owned(),
                    "xor".to_owned(),
                ],
            },
        );
        assert_eq!(
            operator.element_values,
            vec![
                CatalogAttributeElementValues {
                    element: "feComposite".to_owned(),
                    values: CatalogAttributeValues::Enum {
                        values: vec![
                            "arithmetic".to_owned(),
                            "atop".to_owned(),
                            "in".to_owned(),
                            "lighter".to_owned(),
                            "out".to_owned(),
                            "over".to_owned(),
                            "xor".to_owned(),
                        ],
                    },
                },
                CatalogAttributeElementValues {
                    element: "feMorphology".to_owned(),
                    values: CatalogAttributeValues::Enum {
                        values: vec!["dilate".to_owned(), "erode".to_owned()],
                    },
                },
            ],
        );
        Ok(())
    }

    #[test]
    fn builds_attribute_catalog_with_explicit_applicability()
    -> Result<(), Box<dyn std::error::Error>> {
        let catalog = catalog_with_test_descriptions();

        let rect = catalog
            .elements
            .iter()
            .find(|element| element.name == "rect")
            .ok_or("missing rect")?;
        assert_eq!(
            rect.description.as_deref(),
            Some("The rect element defines a rectangle.")
        );

        let id = catalog
            .attributes
            .iter()
            .find(|attribute| attribute.name == "id")
            .ok_or("missing id")?;
        assert_eq!(id.applicability, CatalogAttributeApplicability::Global);

        assert_fill_attribute(catalog_attribute(&catalog, "fill")?);
        assert_href_attribute(catalog_attribute(&catalog, "href")?);

        let x = catalog
            .attributes
            .iter()
            .find(|attribute| attribute.name == "x")
            .ok_or("missing x")?;
        assert_eq!(
            x.applicability,
            CatalogAttributeApplicability::Elements {
                elements: vec!["rect".to_owned()]
            }
        );

        let view_box = catalog
            .attributes
            .iter()
            .find(|attribute| attribute.name == "viewBox")
            .ok_or("missing viewBox")?;
        assert_eq!(
            view_box.applicability,
            CatalogAttributeApplicability::Elements {
                elements: vec!["rect".to_owned()]
            }
        );
        assert_eq!(
            view_box.description.as_deref(),
            Some("The viewBox attribute defines the viewport.")
        );
        Ok(())
    }

    #[test]
    fn catalog_graph_links_catalog_shapes() {
        let catalog = catalog_with_test_descriptions();

        assert!(graph_has_node(
            &catalog,
            "element:rect",
            CatalogGraphNodeKind::Element,
            "rect"
        ));
        assert!(graph_has_node(
            &catalog,
            "attribute:fill",
            CatalogGraphNodeKind::Attribute,
            "fill"
        ));
        assert!(graph_has_node(
            &catalog,
            "css-property:fill",
            CatalogGraphNodeKind::CssProperty,
            "fill"
        ));
        assert!(graph_has_node(
            &catalog,
            "value:fill",
            CatalogGraphNodeKind::ValueGrammar,
            "fill (enum)"
        ));
        assert!(graph_has_edge(
            &catalog,
            "attribute:fill",
            "element:rect",
            CatalogGraphEdgeKind::AppliesTo
        ));
        assert!(graph_has_edge(
            &catalog,
            "attribute:fill",
            "attribute-category:presentation",
            CatalogGraphEdgeKind::MemberOf
        ));
        assert!(graph_has_edge(
            &catalog,
            "attribute:fill",
            "css-property:fill",
            CatalogGraphEdgeKind::UsesCssProperty
        ));
        assert!(graph_has_edge(
            &catalog,
            "attribute:fill",
            "value:fill",
            CatalogGraphEdgeKind::HasValueGrammar
        ));
        assert!(graph_has_edge(
            &catalog,
            "element:rect",
            "attribute-category:global",
            CatalogGraphEdgeKind::AcceptsGlobalAttributes
        ));
    }

    #[test]
    fn catalog_graph_links_inventory_presence() {
        let inventory = CatalogInventory {
            profile: CatalogSpecSnapshotId::Svg11Rec20110816,
            sources: vec!["https://example.test/attindex.html".to_owned()],
            elements: vec![CatalogInventoryElement {
                name: "a".to_owned(),
                attributes: vec!["xlink:href".to_owned()],
            }],
            attributes: vec!["xlink:href".to_owned()],
        };
        let catalog = panic_catalog(build_catalog(
            &[defs()],
            &[property("fill", "none", &["none"])],
            &[],
            "https://example.test/",
            "abc",
            None,
            CatalogLegacyInputs {
                sources: &[],
                value_overrides: &BTreeMap::new(),
                inventories: &[inventory],
                grammar_inputs: None,
            },
        ));

        assert_eq!(
            catalog.snapshots,
            [CatalogSnapshotRef {
                profile: CatalogSpecSnapshotId::Svg11Rec20110816,
                href: "snapshots/svg11-rec-20110816.json".to_owned(),
            }]
        );
        assert!(graph_has_edge(
            &catalog,
            "element:a",
            "profile:Svg11Rec20110816",
            CatalogGraphEdgeKind::PresentIn
        ));
        assert!(graph_has_edge(
            &catalog,
            "attribute:href",
            "profile:Svg11Rec20110816",
            CatalogGraphEdgeKind::PresentIn
        ));
    }

    #[test]
    fn bcd_only_attributes_are_added_with_bcd_bearers() -> Result<(), Box<dyn std::error::Error>> {
        let compat = CompatCatalog {
            provenance: CatalogCompatProvenance {
                browser_compat_data: compat_source("bcd"),
                web_features: compat_source("web-features"),
                unmodeled_features: Vec::new(),
            },
            elements: std::collections::BTreeMap::new(),
            attributes: std::collections::BTreeMap::from([(
                "fetchpriority".to_owned(),
                CompatAttribute {
                    global_facts: None,
                    element_facts: std::collections::BTreeMap::from([(
                        "rect".to_owned(),
                        CatalogCompatFacts {
                            mdn_url: Some(
                                "https://developer.mozilla.org/docs/Web/SVG/Reference/Attribute/fetchpriority"
                                    .to_owned(),
                            ),
                            ..CatalogCompatFacts::default()
                        },
                    )]),
                },
            )]),
        };
        let catalog = panic_catalog(build_catalog(
            &[defs()],
            &[],
            &[],
            "https://example.test/",
            "abc",
            Some(&compat),
            CatalogLegacyInputs {
                sources: &[],
                value_overrides: &BTreeMap::new(),
                inventories: &[],
                grammar_inputs: None,
            },
        ));

        let fetchpriority = catalog
            .attributes
            .iter()
            .find(|attribute| attribute.name == "fetchpriority")
            .ok_or("missing fetchpriority")?;

        assert_eq!(fetchpriority.spec_url, None);
        assert_eq!(
            fetchpriority.mdn_url.as_deref(),
            Some("https://developer.mozilla.org/docs/Web/SVG/Reference/Attribute/fetchpriority")
        );
        assert_eq!(fetchpriority.values, CatalogAttributeValues::FreeText);
        assert_eq!(fetchpriority.element_compat.len(), 1);
        assert_eq!(
            fetchpriority.applicability,
            CatalogAttributeApplicability::Elements {
                elements: vec!["rect".to_owned()]
            }
        );
        Ok(())
    }

    #[test]
    fn element_scoped_attribute_compat_does_not_flatten_by_name()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut text_path = element("textPath");
        text_path.attributes.push(attr(
            "path",
            "text.html#TextPathElementPathAttribute",
            Some(false),
        ));
        let mut animate_motion = element("animateMotion");
        animate_motion.attributes.push(attr(
            "path",
            "animate.html#AnimateMotionElementPathAttribute",
            Some(false),
        ));
        let defs = Definitions {
            elements: vec![text_path, animate_motion],
            ..defs()
        };
        let compat = CompatCatalog {
            provenance: CatalogCompatProvenance {
                browser_compat_data: compat_source("bcd"),
                web_features: compat_source("web-features"),
                unmodeled_features: Vec::new(),
            },
            elements: std::collections::BTreeMap::new(),
            attributes: std::collections::BTreeMap::from([(
                "path".to_owned(),
                CompatAttribute {
                    global_facts: None,
                    element_facts: std::collections::BTreeMap::from([
                        (
                            "animateMotion".to_owned(),
                            CatalogCompatFacts {
                                experimental: false,
                                standard_track: Some(true),
                                ..CatalogCompatFacts::default()
                            },
                        ),
                        (
                            "textPath".to_owned(),
                            CatalogCompatFacts {
                                experimental: true,
                                standard_track: Some(true),
                                ..CatalogCompatFacts::default()
                            },
                        ),
                    ]),
                },
            )]),
        };
        let catalog = panic_catalog(build_catalog(
            &[defs],
            &[],
            &[],
            "https://example.test/",
            "abc",
            Some(&compat),
            CatalogLegacyInputs {
                sources: &[],
                value_overrides: &BTreeMap::new(),
                inventories: &[],
                grammar_inputs: None,
            },
        ));

        let path = catalog
            .attributes
            .iter()
            .find(|attribute| attribute.name == "path")
            .ok_or("missing path")?;

        assert!(
            !path.experimental,
            "mixed per-element facts must not mark every path attribute experimental"
        );
        assert_eq!(path.element_compat.len(), 2);
        let text_path = path
            .element_compat
            .iter()
            .find(|compat| compat.element == "textPath")
            .ok_or("missing textPath path compat")?;
        let animate_motion = path
            .element_compat
            .iter()
            .find(|compat| compat.element == "animateMotion")
            .ok_or("missing animateMotion path compat")?;
        assert!(text_path.facts.experimental);
        assert!(!animate_motion.facts.experimental);
        Ok(())
    }

    #[test]
    fn mixed_property_grammars_keep_css_grammar_shape() {
        let stroke_dasharray = property("stroke-dasharray", "none | <dasharray>", &["none"]);
        let CatalogAttributeValues::CssGrammar { grammar, graph } =
            values_for_property(&stroke_dasharray, &BTreeMap::new(), None)
        else {
            panic!("mixed grammar should keep a CSS grammar graph");
        };
        assert_eq!(grammar, "none | <dasharray>");
        assert!(graph_contains(
            &graph,
            CatalogCssGrammarNodeKind::Keyword,
            "none"
        ));
        assert!(graph_contains(
            &graph,
            CatalogCssGrammarNodeKind::Type,
            "<dasharray>"
        ));
        assert!(
            graph
                .edges
                .iter()
                .any(|edge| edge.kind == CatalogCssGrammarEdgeKind::Next)
        );

        let linecap = property(
            "stroke-linecap",
            "butt | round | square",
            &["butt", "round", "square"],
        );
        assert_eq!(
            values_for_property(&linecap, &BTreeMap::new(), None),
            CatalogAttributeValues::Enum {
                values: vec!["butt".to_owned(), "round".to_owned(), "square".to_owned()]
            }
        );
    }

    #[test]
    fn css_grammar_values_extract_type_and_function_hints() {
        let cursor = property(
            "cursor",
            "[ [ <url> [ <x> <y> ]? , ]* [ auto | pointer ] ]",
            &[],
        );
        let CatalogAttributeValues::CssGrammar { graph, .. } =
            values_for_property(&cursor, &BTreeMap::new(), None)
        else {
            panic!("cursor should keep a CSS grammar graph");
        };
        assert!(graph_contains(
            &graph,
            CatalogCssGrammarNodeKind::Type,
            "<url>"
        ));
        assert!(graph_contains(
            &graph,
            CatalogCssGrammarNodeKind::Type,
            "<x>"
        ));
        assert!(graph_contains(
            &graph,
            CatalogCssGrammarNodeKind::Type,
            "<y>"
        ));
        assert!(
            graph
                .nodes
                .iter()
                .any(|node| node.kind == CatalogCssGrammarNodeKind::Group)
        );

        let filter = property("filter", "none | blur() | url()", &["none"]);
        let CatalogAttributeValues::CssGrammar { graph, .. } =
            values_for_property(&filter, &BTreeMap::new(), None)
        else {
            panic!("filter should keep a CSS grammar graph");
        };
        assert!(graph_contains(
            &graph,
            CatalogCssGrammarNodeKind::Keyword,
            "none"
        ));
        assert!(graph_contains(
            &graph,
            CatalogCssGrammarNodeKind::Function,
            "blur"
        ));
        assert!(graph_contains(
            &graph,
            CatalogCssGrammarNodeKind::Function,
            "url"
        ));
    }

    #[test]
    fn referenced_spec_values_are_semantic() {
        let cases = [
            ("boolean attribute [HTML]", CatalogAttributeValues::Boolean),
            (
                "set of space-separated tokens [HTML]",
                CatalogAttributeValues::TokenList,
            ),
            (
                "set of comma-separated tokens [HTML]",
                CatalogAttributeValues::CommaTokenList,
            ),
            ("Language-Tag [ABNF]", CatalogAttributeValues::LanguageTag),
            (
                "A BCP 47 language tag string [HTML]",
                CatalogAttributeValues::LanguageTag,
            ),
            (
                "space-separated valid non-empty URL tokens [HTML]",
                CatalogAttributeValues::UrlTokenList,
            ),
            ("valid integer [HTML]", CatalogAttributeValues::Integer),
            ("URL [URL]", CatalogAttributeValues::Url),
            (
                "A referrer policy string [REFERRERPOLICY]",
                CatalogAttributeValues::ReferrerPolicy,
            ),
            (
                "A MIME type string [HTML]",
                CatalogAttributeValues::MediaType,
            ),
        ];
        for (value, expected) in cases {
            assert_eq!(
                values_for_property(&property("attr", value, &[]), &BTreeMap::new(), None),
                expected,
                "{value} must not become a CSS grammar"
            );
        }
    }

    #[test]
    fn suggested_file_name_prose_gets_semantic_value_shape() {
        assert_eq!(
            values_for_property(
                &property(
                    "download",
                    "any value (if non-empty, value represents a suggested file name)",
                    &[],
                ),
                &BTreeMap::new(),
                None,
            ),
            CatalogAttributeValues::SuggestedFileName,
        );
    }

    #[test]
    fn known_animation_type_scope_is_preserved() {
        let known = known_element_values();
        let Some((_, _, values)) = known
            .iter()
            .find(|(attribute, element, _)| *attribute == "type" && *element == "animateTransform")
        else {
            panic!("type on animateTransform invariant missing");
        };
        assert_eq!(
            *values,
            CatalogAttributeValues::Enum {
                values: ["rotate", "scale", "skewX", "skewY", "translate"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            },
        );
    }

    #[test]
    fn known_animate_motion_rotate_scope_is_preserved() {
        let known = known_element_values();
        let Some((_, _, values)) = known
            .iter()
            .find(|(attribute, element, _)| *attribute == "rotate" && *element == "animateMotion")
        else {
            panic!("rotate on animateMotion invariant missing");
        };
        assert_eq!(
            *values,
            CatalogAttributeValues::CssGrammar {
                grammar: "<number> | auto | auto-reverse".to_owned(),
                graph: css_grammar_graph("<number> | auto | auto-reverse"),
            },
        );
    }

    #[test]
    fn known_animate_motion_value_shapes_are_semantic() {
        let expected = [
            ("calcMode", "enum"),
            ("keyPoints", "semicolon_number_list"),
            ("origin", "enum"),
            ("path", "path_data"),
        ];
        let known = known_attribute_values();
        for (name, expected) in expected {
            let Some((_, values)) = known.iter().find(|(attribute, _)| *attribute == name) else {
                panic!("attribute invariant missing");
            };
            let actual = match values {
                CatalogAttributeValues::Enum { .. } => "enum",
                CatalogAttributeValues::PathData => "path_data",
                CatalogAttributeValues::SemicolonNumberList => "semicolon_number_list",
                _ => "other",
            };
            assert_eq!(actual, expected, "{name} value shape");
        }
    }

    #[test]
    fn known_animate_motion_coordinate_scopes_are_preserved() {
        let known = known_element_values();
        for name in ["from", "to", "by"] {
            let Some((_, _, values)) = known
                .iter()
                .find(|(attribute, element, _)| *attribute == name && *element == "animateMotion")
            else {
                panic!("coordinate-pair invariant missing");
            };
            assert_eq!(*values, CatalogAttributeValues::CoordinatePair);
        }

        let Some((_, _, values)) = known
            .iter()
            .find(|(attribute, element, _)| *attribute == "values" && *element == "animateMotion")
        else {
            panic!("values on animateMotion invariant missing");
        };
        assert_eq!(*values, CatalogAttributeValues::CoordinatePairList,);
    }

    #[test]
    fn see_below_values_are_derived_from_prose() {
        let cases = [
            (
                "IDAttribute",
                "Must reflect the element's ID. The id attribute must be unique within the node \
                 tree, must not be an empty string, and must not contain any whitespace \
                 characters.",
                CatalogAttributeValues::Id,
            ),
            (
                "XMLSpaceAttribute",
                "The only possible values are the strings 'default' and 'preserve', without white \
                 space.",
                CatalogAttributeValues::Enum {
                    values: vec!["default".to_owned(), "preserve".to_owned()],
                },
            ),
            (
                "StyleElementTypeAttribute",
                "This attribute specifies the style sheet language as a media type.",
                CatalogAttributeValues::MediaType,
            ),
            (
                "StyleElementMediaAttribute",
                "Its value is parsed as a media_query_list.",
                CatalogAttributeValues::MediaQueryList,
            ),
            (
                "StyleAttribute",
                "The attribute is parsed as a declaration-list.",
                CatalogAttributeValues::CssDeclarationList,
            ),
            (
                "OnBeginEventAttribute",
                "There are no restrictions on the values of this attribute.",
                CatalogAttributeValues::FreeText,
            ),
        ];
        for (id, prose, expected) in cases {
            let descriptions_by_id = BTreeMap::from([(id, prose)]);
            let mut property = property("attr", "(see below)", &[]);
            property.id = Some(id.to_owned());
            assert_eq!(
                values_for_property(&property, &descriptions_by_id, None),
                expected
            );
        }
    }

    #[test]
    fn resolves_see_attribute_cross_reference() {
        // `in2 = "(see in attribute)"` must inherit `in`'s value space verbatim,
        // not survive as junk prose keywords.
        let defs = vec![
            property("in", "<filter-primitive-reference>", &[]),
            property("in2", "(see in attribute)", &[]),
        ];
        let resolved = resolve_see_references(&defs);
        let Some(in2) = resolved.iter().find(|def| def.name == "in2") else {
            panic!("in2 present");
        };
        let Some(source) = resolved.iter().find(|def| def.name == "in") else {
            panic!("in present");
        };
        assert_eq!(in2.value.as_deref(), Some("<filter-primitive-reference>"));
        assert_eq!(in2.keywords, source.keywords);
    }

    #[test]
    fn see_reference_to_missing_target_left_intact() {
        // A dangling reference (no `in` definition) is preserved as prose rather
        // than fabricated or dropped.
        let defs = vec![property("in2", "(see in attribute)", &[])];
        let resolved = resolve_see_references(&defs);
        assert_eq!(resolved[0].value.as_deref(), Some("(see in attribute)"));
    }

    #[test]
    fn see_reference_parse_is_shape_based() {
        // Matching is purely structural: case- and whitespace-insensitive, never
        // keyed on a specific attribute name.
        assert_eq!(parse_see_reference("(see in attribute)"), Some("in"));
        assert_eq!(
            parse_see_reference("  (See  Fill  attribute) "),
            Some("Fill")
        );
        assert_eq!(parse_see_reference("(SEE in attribute)"), Some("in"));
        assert_eq!(parse_see_reference("<filter-primitive-reference>"), None);
        assert_eq!(parse_see_reference("a | b"), None);
        assert_eq!(parse_see_reference("(see in)"), None);
        assert_eq!(parse_see_reference("see in attribute"), None);
    }

    #[test]
    fn see_reference_chain_resolves_to_concrete() {
        // `a -> b -> c` collapses to `c`'s concrete value via the bounded
        // fixpoint.
        let defs = vec![
            property("c", "<number>", &[]),
            property("b", "(see c attribute)", &[]),
            property("a", "(see b attribute)", &[]),
        ];
        let resolved = resolve_see_references(&defs);
        for name in ["a", "b"] {
            let Some(def) = resolved.iter().find(|def| def.name == name) else {
                panic!("{name} present");
            };
            assert_eq!(def.value.as_deref(), Some("<number>"), "{name} resolved");
        }
    }

    #[test]
    fn see_reference_cycle_is_safe() {
        // A mutual cycle (`x -> y -> x`) terminates and leaves both as prose.
        let defs = vec![
            property("x", "(see y attribute)", &[]),
            property("y", "(see x attribute)", &[]),
        ];
        let resolved = resolve_see_references(&defs);
        assert_eq!(resolved[0].value.as_deref(), Some("(see y attribute)"));
        assert_eq!(resolved[1].value.as_deref(), Some("(see x attribute)"));
    }

    fn graph_contains(
        graph: &CatalogCssGrammarGraph,
        kind: CatalogCssGrammarNodeKind,
        text: &str,
    ) -> bool {
        graph
            .nodes
            .iter()
            .any(|node| node.kind == kind && node.text.as_deref() == Some(text))
    }
}
