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
    chapter::PropertyValueDef,
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
    /// Element definitions, sorted by name.
    pub elements: Vec<CatalogElement>,
    /// Attribute definitions, sorted by canonical name.
    pub attributes: Vec<CatalogAttribute>,
}

/// One element's spec-derived catalog entry.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CatalogElement {
    /// Element tag name.
    pub name: String,
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

/// Attribute compat facts scoped to one element bearer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogAttributeElementCompat {
    /// Element name this compat record applies to.
    pub element: String,
    /// Objective compat facts for this attribute on `element`.
    #[serde(flatten)]
    pub facts: CatalogCompatFacts,
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
    /// Free-form text with no constrained grammar.
    FreeText,
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
    editors_draft_base: &str,
    commit: &str,
    compat: Option<&CompatCatalog>,
) -> Catalog {
    let members = category_members(modules);
    let mut elements = Vec::new();
    for module in modules {
        let base = module.anchor_base.as_deref().unwrap_or(editors_draft_base);
        for element in &module.elements {
            elements.push(build_element(
                element,
                base,
                &members,
                compat.and_then(|compat| compat.elements.get(&element.name)),
            ));
        }
    }
    elements.sort_by(|a, b| a.name.cmp(&b.name));
    Catalog {
        schema_version: crate::schema::CATALOG_SCHEMA_VERSION,
        commit: commit.to_owned(),
        compat: compat.map(|compat| compat.provenance.clone()),
        elements,
        attributes: build_attributes(modules, properties, editors_draft_base, compat),
    }
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
    editors_draft_base: &str,
    compat: Option<&CompatCatalog>,
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

    seed_attribute_metadata(modules, editors_draft_base, &mut attributes);
    seed_presentation_attribute_metadata(&presentation_attributes, &mut attributes);
    collect_element_attribute_bearers(
        modules,
        editors_draft_base,
        &presentation_attributes,
        &category_attributes,
        &mut attributes,
    );

    let mut attributes: Vec<CatalogAttribute> = attributes
        .into_values()
        .map(|attribute| {
            let mut attribute = attribute.finish(&all_elements, &properties_by_name);
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
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    for module in modules {
        let base = module.anchor_base.as_deref().unwrap_or(editors_draft_base);
        for property in &module.properties {
            accumulator_for(attributes, &property.name).merge_property(property, base);
        }
        for attribute in &module.global_attributes {
            let entry = accumulator_for(attributes, &attribute.name);
            entry.merge_ref(attribute, base);
            if !attribute.elements.is_empty() {
                entry.bearers.extend(attribute.elements.iter().cloned());
            }
        }
        for category in &module.attribute_categories {
            for attribute in &category.attributes {
                let entry = accumulator_for(attributes, &attribute.name);
                entry.merge_ref(attribute, base);
                if category.name == "presentation" {
                    entry.presentation_attribute = Some(entry.name.clone());
                }
            }
        }
    }
}

fn seed_presentation_attribute_metadata(
    presentation_attributes: &[PresentationAttribute],
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    for presentation in presentation_attributes {
        accumulator_for(attributes, &presentation.name)
            .merge_presentation_href(presentation.href.as_deref(), &presentation.base);
    }
}

fn collect_element_attribute_bearers(
    modules: &[Definitions],
    editors_draft_base: &str,
    presentation_attributes: &[PresentationAttribute],
    category_attributes: &BTreeMap<String, Vec<CategorizedAttribute>>,
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    for module in modules {
        let base = module.anchor_base.as_deref().unwrap_or(editors_draft_base);
        for element in &module.elements {
            collect_direct_attribute_bearers(element, base, attributes);
            collect_category_attribute_bearers(
                element,
                presentation_attributes,
                category_attributes,
                attributes,
            );
        }
    }
}

fn collect_direct_attribute_bearers(
    element: &crate::extract::ElementDef,
    base: &str,
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    for attribute in &element.attributes {
        let entry = accumulator_for(attributes, &attribute.name);
        entry.merge_ref(attribute, base);
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
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    for category_name in &element.attribute_categories {
        if category_name == "presentation" {
            collect_presentation_attribute_bearers(element, presentation_attributes, attributes);
        }
        if category_name == "deprecated xlink" {
            continue;
        }
        let Some(category) = category_attributes.get(category_name.as_str()) else {
            continue;
        };
        for attribute in category {
            collect_one_category_attribute_bearer(element, attribute, attributes);
        }
    }
}

fn collect_presentation_attribute_bearers(
    element: &crate::extract::ElementDef,
    presentation_attributes: &[PresentationAttribute],
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    for presentation in presentation_attributes {
        let entry = accumulator_for(attributes, &presentation.name);
        entry.merge_presentation_href(presentation.href.as_deref(), &presentation.base);
        entry.bearers.insert(element.name.clone());
    }
}

fn collect_one_category_attribute_bearer(
    element: &crate::extract::ElementDef,
    attribute: &CategorizedAttribute,
    attributes: &mut BTreeMap<String, AttributeAccumulator>,
) {
    let entry = accumulator_for(attributes, &attribute.attribute.name);
    entry.merge_ref(&attribute.attribute, &attribute.base);
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
    spec_url: Option<String>,
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
            spec_url: None,
            animatable: None,
            presentation_attribute: None,
            bearers: BTreeSet::new(),
            global: false,
        }
    }

    /// Merge href/animatability metadata from one spec attribute reference.
    fn merge_ref(&mut self, attribute: &AttributeRef, base: &str) {
        self.merge_href(attribute.href.as_deref(), base);
        match attribute.animatable {
            Some(true) => self.animatable = Some(true),
            Some(false) if self.animatable.is_none() => self.animatable = Some(false),
            _ => {}
        }
    }

    /// Merge href metadata from a CSS/SVG property.
    fn merge_property(&mut self, property: &PropertyDef, base: &str) {
        self.merge_href(property.href.as_deref(), base);
    }

    /// Merge href metadata and mark this attribute as a presentation attribute.
    fn merge_presentation_href(&mut self, href: Option<&str>, base: &str) {
        self.merge_href(href, base);
        self.presentation_attribute = Some(self.name.clone());
    }

    /// Set the spec URL once, preferring the first canonical declaration seen.
    fn merge_href(&mut self, href: Option<&str>, base: &str) {
        if self.spec_url.is_none() {
            self.spec_url = href.map(|href| resolve_url(base, href));
        }
    }

    /// Convert the accumulator into its serialized catalog entry.
    fn finish(
        self,
        all_elements: &BTreeSet<String>,
        properties_by_name: &BTreeMap<&str, &PropertyValueDef>,
    ) -> CatalogAttribute {
        let property = properties_by_name.get(self.name.as_str()).copied();
        let values = property.map_or(CatalogAttributeValues::FreeText, values_for_property);
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
            values,
            applicability,
        }
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

    let Some(value) = property.value.as_deref() else {
        return CatalogAttributeValues::FreeText;
    };
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
    CatalogAttributeValues::FreeText
}

/// Whether the raw value grammar is just bare keyword alternatives.
fn is_keyword_only_grammar(value: &str) -> bool {
    value.split('|').all(|token| is_keyword_token(token.trim()))
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
fn canonical_attribute_name(name: &str) -> std::borrow::Cow<'_, str> {
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

    #[test]
    fn builds_attribute_catalog_with_explicit_applicability()
    -> Result<(), Box<dyn std::error::Error>> {
        let catalog = build_catalog(
            &[defs()],
            &[property(
                "fill",
                "none | context-fill",
                &["context-fill", "none"],
            )],
            "https://example.test/",
            "abc",
            None,
        );

        let id = catalog
            .attributes
            .iter()
            .find(|attribute| attribute.name == "id")
            .ok_or("missing id")?;
        assert_eq!(id.applicability, CatalogAttributeApplicability::Global);

        let fill = catalog
            .attributes
            .iter()
            .find(|attribute| attribute.name == "fill")
            .ok_or("missing fill")?;
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
        assert!(fill.animatable);

        let href = catalog
            .attributes
            .iter()
            .find(|attribute| attribute.name == "href")
            .ok_or("missing href")?;
        assert!(href.animatable);
        assert_eq!(
            href.spec_url.as_deref(),
            Some("https://example.test/linking.html#XLinkHref")
        );
        assert_eq!(
            href.applicability,
            CatalogAttributeApplicability::Elements {
                elements: vec!["a".to_owned()]
            }
        );

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
        Ok(())
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
            "https://example.test/",
            "abc",
            Some(&compat),
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
        let catalog = build_catalog(&[defs], &[], "https://example.test/", "abc", Some(&compat));

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
    fn mixed_property_grammars_do_not_collapse_to_keyword_enums() {
        let stroke_dasharray = property("stroke-dasharray", "none | <dasharray>", &["none"]);
        assert_eq!(
            values_for_property(&stroke_dasharray),
            CatalogAttributeValues::FreeText
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
}
