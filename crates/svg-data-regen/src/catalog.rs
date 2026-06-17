//! Map the extracted spec data into the committed catalog JSON that
//! `svg-data`'s `build.rs` turns into static `ElementDef` arrays.
//!
//! The extraction types mirror the spec's own shapes; this module reshapes them
//! into the runtime catalog's vocabulary. The structural content model is
//! flattened here (categories resolved to their member elements, unioned with
//! the explicit allowed elements) so the runtime never depends on a category
//! enum staying in sync with the spec's taxonomy. The output is sorted, so the
//! same upstream commit always yields byte-identical JSON.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::Serialize;

use crate::{
    chapter::{AnchorDescription, PropertyValueDef},
    compat::CompatCatalog,
    extract::{AttributeRef, ContentModelKind, Definitions, PropertyDef},
};

/// The full derived catalog written to `svg-data/data/catalog.json`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Catalog {
    /// Version of the JSON catalog/schema contract.
    pub schema_version: u16,
    /// The upstream commit this catalog was derived from.
    pub commit: String,
    /// Browser-compat data sources used for objective compat facts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compat: Option<CatalogCompatProvenance>,
    /// Authoritative legacy-profile sources used for snapshot-specific data.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub legacy_sources: Vec<CatalogLegacySource>,
    /// Version-specific overlay documents, sorted by profile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snapshots: Vec<CatalogSnapshotRef>,
    /// Element definitions, sorted by name.
    pub elements: Vec<CatalogElement>,
    /// Attribute definitions, sorted by canonical name.
    pub attributes: Vec<CatalogAttribute>,
    /// Derived graph view over the catalog.
    pub graph: CatalogGraph,
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    /// Whether compat data marks the element experimental.
    #[serde(default, skip_serializing_if = "is_false")]
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    /// Whether compat data marks the attribute experimental.
    #[serde(default, skip_serializing_if = "is_false")]
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
    /// Per-profile value-space overrides, when older snapshots accepted a
    /// different grammar.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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
#[derive(Debug, Clone, Serialize, JsonSchema)]
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
    /// Exact source URLs used to derive this snapshot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    /// Per-snapshot element/attribute inventory.
    pub inventory: CatalogSnapshotInventory,
}

/// Per-snapshot element/attribute inventory payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogSnapshotInventory {
    /// Elements present in this profile.
    pub elements: Vec<CatalogInventoryElement>,
    /// Attributes present anywhere in this profile.
    pub attributes: Vec<String>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[allow(dead_code)]
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
    pub fn from_inventory(inventory: &CatalogInventory) -> Self {
        Self {
            schema_version: crate::schema::CATALOG_SCHEMA_VERSION,
            profile: inventory.profile,
            sources: inventory.sources.clone(),
            inventory: CatalogSnapshotInventory {
                elements: inventory.elements.clone(),
                attributes: inventory.attributes.clone(),
            },
        }
    }
}

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
    #[serde(default, skip_serializing_if = "is_false")]
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    /// Whether compat data marks the feature experimental.
    #[serde(default, skip_serializing_if = "is_false")]
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

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(value: &bool) -> bool {
    !*value
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
#[derive(Debug, Serialize, JsonSchema)]
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
}

/// Build the catalog from every definitions module's extracted entities.
///
/// `editors_draft_base` is the SVG 2 editor's-draft base URL (from
/// `publish.xml`'s `<cvs>`), used to resolve permalinks for the modules defined
/// within `svgwg` itself; modules carrying their own external `anchor_base`
/// (CSS drafts) resolve against that instead.
#[must_use]
pub fn build_catalog(
    modules: &[Definitions],
    properties: &[PropertyValueDef],
    descriptions: &[AnchorDescription],
    editors_draft_base: &str,
    commit: &str,
    compat: Option<&CompatCatalog>,
    legacy: CatalogLegacyInputs<'_>,
) -> Catalog {
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
    );
    let inventories = legacy.inventories.to_vec();
    let graph = build_catalog_graph(
        modules,
        properties,
        compat,
        &elements,
        &attributes,
        &inventories,
    );
    let snapshots = inventories
        .iter()
        .map(|inventory| CatalogSnapshotRef {
            profile: inventory.profile,
            href: catalog_snapshot_href(inventory.profile).to_owned(),
        })
        .collect();
    Catalog {
        schema_version: crate::schema::CATALOG_SCHEMA_VERSION,
        commit: commit.to_owned(),
        compat: compat.map(|compat| compat.provenance.clone()),
        legacy_sources: legacy.sources.to_vec(),
        snapshots,
        elements,
        attributes,
        graph,
    }
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
        None => CatalogContentModel::AnySvg,
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
) -> Vec<CatalogAttribute> {
    let all_elements: BTreeSet<String> = modules
        .iter()
        .flat_map(|module| module.elements.iter().map(|element| element.name.clone()))
        .collect();
    let properties_by_name: BTreeMap<&str, &PropertyValueDef> = properties
        .iter()
        .map(|property| (property.name.as_str(), property))
        .collect();
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
            let mut attribute =
                attribute.finish(&all_elements, &properties_by_name, legacy_value_overrides);
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
        properties_by_name: &BTreeMap<&str, &PropertyValueDef>,
        legacy_value_overrides: &BTreeMap<String, Vec<CatalogAttributeValueOverride>>,
    ) -> CatalogAttribute {
        let property = properties_by_name.get(self.name.as_str()).copied();
        let values = property.map_or(CatalogAttributeValues::FreeText, values_for_property);
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

/// Convert a CSS property grammar into the runtime value-space subset we know
/// how to represent today.
fn values_for_property(property: &PropertyValueDef) -> CatalogAttributeValues {
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
    if property.name == "transform" || normalized.contains("<transform-list>") {
        return CatalogAttributeValues::Transform {
            functions: ["matrix", "translate", "scale", "rotate", "skewX", "skewY"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        };
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

/// Whether the raw value grammar is just bare keyword alternatives.
fn is_keyword_only_grammar(value: &str) -> bool {
    value.split('|').all(|token| is_keyword_token(token.trim()))
}

fn repair_css_type_tokens(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut repaired = String::with_capacity(value.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'<' && bytes.get(index + 1).is_some_and(u8::is_ascii_alphabetic) {
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

/// Whether a value grammar token is a bare keyword.
fn is_keyword_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// Resolve `href` against a module base unless it is already absolute.
fn resolve_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_owned()
    } else {
        format!("{base}{href}")
    }
}

/// Canonicalize legacy xlink spellings without depending on the runtime crate.
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
        build_catalog(
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
            },
        )
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
        let catalog = build_catalog(
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
            },
        );

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
        let catalog = build_catalog(
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
            },
        );

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
        let catalog = build_catalog(
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
            },
        );

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
        let catalog = build_catalog(
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
            },
        );

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
            values_for_property(&stroke_dasharray)
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
            values_for_property(&linecap),
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
        let CatalogAttributeValues::CssGrammar { graph, .. } = values_for_property(&cursor) else {
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
        let CatalogAttributeValues::CssGrammar { graph, .. } = values_for_property(&filter) else {
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
