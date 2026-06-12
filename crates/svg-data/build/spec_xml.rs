//! Shared `definitions*.xml` reader for the SVG 2 Editor's Draft.
//!
//! Deterministically derives, from the **vendored** svgwg
//! `definitions*.xml` family, the three presence facts the snapshot
//! pipeline cares about:
//!
//! 1. **element presence** — every `<element name='…'>` across the five
//!    definition files (the four core files plus `definitions-animations.xml`,
//!    which supplies the five SMIL animation elements);
//! 2. **attribute presence** — every distinct attribute *name* the spec
//!    attaches to any element, after resolving the `attributecategory`
//!    indirection;
//! 3. the **element ↔ attribute matrix** — the resolved set of attributes
//!    each element carries.
//!
//! It deliberately **ignores** the `<elementcategory>` / `<term>` wrapper
//! tags when counting elements (the design's "17-elementcategory overcount
//! trap"): only `local_name == "element"` is an element. `<elementcategory>`
//! is consumed *only* as an indirection table when resolving an element's
//! `elementcategories='…'` content-model scope.
//!
//! ## Resolution model (faithful to the upstream comments)
//!
//! For each `<element>`:
//!
//! - its nested `<attribute name='…'>` children are element-specific
//!   attributes;
//! - `attributes='a, b, …'` names additional shared attributes;
//! - `geometryproperties='cx, cy, …'` names geometry presentation
//!   attributes;
//! - `attributecategories='core, presentation, …'` expands through the
//!   globally-collected `<attributecategory>` table. A category either
//!   lists nested `<attribute>` children **or** carries a
//!   `presentationattributes='…'` list (the `presentation` category), which
//!   we expand into attribute names directly;
//! - a top-level `<attribute name='x' elements='tspan, …'>` adds `x` to each
//!   named element.
//!
//! `attributecategory` definitions are collected across **all** files first
//! (filters defines `filter primitive` / `transfer function element`,
//! animations defines the `animation *` categories) so an element in one
//! file can reference a category defined in another.
//!
//! This module performs **no** network I/O and parses only the vendored
//! files handed to it, keeping the build hermetic.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use quick_xml::{Reader, events::Event};

use super::classification::Classification;

/// The five vendored definition files, in the order the snapshot pipeline
/// treats them. The first four are the "core" SVG 2 inventory; the fifth
/// supplies the SMIL animation elements (`animate`, `animateMotion`,
/// `animateTransform`, `set`, `mpath`).
pub const DEFINITION_FILES: [&str; 5] = [
    "definitions.xml",
    "definitions-filters.xml",
    "definitions-masking.xml",
    "definitions-compositing.xml",
    "definitions-animations.xml",
];

/// Errors surfaced while reading or parsing a vendored definitions file.
#[derive(Debug)]
pub enum SpecXmlError {
    Io {
        path: String,
        source: std::io::Error,
    },
    Xml {
        path: String,
        source: quick_xml::Error,
    },
}

impl std::fmt::Display for SpecXmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "spec_xml read {path}: {source}"),
            Self::Xml { path, source } => write!(f, "spec_xml parse {path}: {source}"),
        }
    }
}

impl std::error::Error for SpecXmlError {}

/// The spec-faithful facts derived for a single attribute name: every
/// upstream `attributecategory` it was declared under (raw, for provenance),
/// the normalized [`Classification`] set, and — for top-level
/// `<attribute elements='…'>` declarations — the elements it was scoped to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributeFacts {
    /// Raw upstream `attributecategory` names this attribute appeared under,
    /// sorted and de-duplicated. Empty for an attribute that only appears as
    /// a nested `<attribute>` child of an `<element>` or as a top-level
    /// scoped attribute (no category wrapper).
    pub raw_categories: BTreeSet<String>,
    /// Normalized classifications derived from [`Self::raw_categories`].
    pub classifications: BTreeSet<Classification>,
    /// Elements named in a top-level `<attribute name='x' elements='…'>`
    /// declaration. Empty for attributes that are not element-scoped.
    pub element_scope: BTreeSet<String>,
}

/// The deterministic presence inventory derived from `definitions*.xml`.
#[derive(Debug, Clone, Default)]
pub struct SpecInventory {
    /// Every `<element>` local name, sorted and de-duplicated.
    pub elements: BTreeSet<String>,
    /// Every attribute name attached to at least one element, after
    /// `attributecategory` expansion.
    pub attributes: BTreeSet<String>,
    /// Resolved element → attribute-name matrix.
    pub element_attributes: BTreeMap<String, BTreeSet<String>>,
    /// `<elementcategory>` indirection table: category name → member
    /// element names (used for content-model scope, not for element
    /// presence).
    pub element_categories: BTreeMap<String, BTreeSet<String>>,
    /// Per-attribute classification/provenance facts. Keyed by attribute
    /// name; every name in [`Self::attributes`] has an entry (defaulting to
    /// empty facts for purely element-local or unscoped attributes).
    pub attribute_facts: BTreeMap<String, AttributeFacts>,
}

impl SpecInventory {
    /// The set of `(element, attribute)` edges, the matrix in flat form.
    pub fn edges(&self) -> BTreeSet<(&str, &str)> {
        let mut edges = BTreeSet::new();
        for (element, attributes) in &self.element_attributes {
            for attribute in attributes {
                edges.insert((element.as_str(), attribute.as_str()));
            }
        }
        edges
    }
}

/// One raw `<element>` declaration before attribute-category expansion.
#[derive(Debug, Default)]
struct RawElement {
    attribute_categories: Vec<String>,
    extra_attributes: Vec<String>,
    geometry_properties: Vec<String>,
    local_attributes: Vec<String>,
}

/// Intermediate registries collected in a single pass over all files.
#[derive(Debug, Default)]
struct Registries {
    elements: BTreeMap<String, RawElement>,
    /// `attributecategory` name → contained attribute names.
    attribute_categories: BTreeMap<String, Vec<String>>,
    /// `elementcategory` name → member element names.
    element_categories: BTreeMap<String, BTreeSet<String>>,
    /// Top-level `<attribute name='x' elements='a, b'>` scope entries.
    scoped_attributes: Vec<(String, Vec<String>)>,
}

/// Split a comma-separated attribute-list string into trimmed,
/// non-empty tokens.
fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// Read a single start/empty tag's named attribute as an owned `String`.
fn attr_value(
    tag: &quick_xml::events::BytesStart<'_>,
    key: &[u8],
    path: &str,
) -> Result<Option<String>, SpecXmlError> {
    for attribute in tag.attributes() {
        let attribute = attribute.map_err(|source| SpecXmlError::Xml {
            path: path.to_string(),
            source: source.into(),
        })?;
        if attribute.key.local_name().as_ref() == key {
            // The definitions files carry no XML declaration, so XML 1.0 is
            // assumed (`Implicit1_0`); attribute values are plain
            // comma-lists with no entities, but normalization is the
            // spec-correct, non-deprecated accessor.
            let value = attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|source| SpecXmlError::Xml {
                    path: path.to_string(),
                    source,
                })?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

/// Tags whose nested `<attribute>` children belong to an enclosing
/// declaration (`<element>` or `<attributecategory>`), tracked so a nested
/// `<attribute>` is attributed to the right owner rather than treated as a
/// top-level scoped attribute.
enum OpenOwner {
    Element(String),
    AttributeCategory(String),
}

/// Walk one `definitions*.xml` file, folding its declarations into the
/// shared [`Registries`]. `quick-xml` drives the structural walk; nested
/// `<attribute>` children are attributed to the nearest open
/// `<element>` / `<attributecategory>` owner.
fn parse_file(registries: &mut Registries, content: &str, path: &str) -> Result<(), SpecXmlError> {
    let mut reader = Reader::from_str(content);
    let mut owner_stack: Vec<OpenOwner> = Vec::new();

    loop {
        let event = reader.read_event().map_err(|source| SpecXmlError::Xml {
            path: path.to_string(),
            source,
        })?;
        match event {
            Event::Eof => break,
            // A self-closing `<x/>` tag (`Event::Empty`) opens and closes in
            // one event: it never pushes an owner. A `<x>…</x>` start tag
            // (`Event::Start`) may own nested `<attribute>` children, so it
            // pushes onto the owner stack and is popped on its `End`.
            Event::Empty(tag) => {
                handle_open_tag(registries, &mut owner_stack, &tag, path, true)?;
            }
            Event::Start(tag) => {
                handle_open_tag(registries, &mut owner_stack, &tag, path, false)?;
            }
            Event::End(tag) => {
                let local = tag.local_name();
                if matches!(local.as_ref(), b"element" | b"attributecategory") {
                    owner_stack.pop();
                }
            }
            _ => {}
        }
    }

    Ok(())
}

/// Fold one open (start or empty) tag into the registries.
fn handle_open_tag(
    registries: &mut Registries,
    owner_stack: &mut Vec<OpenOwner>,
    tag: &quick_xml::events::BytesStart<'_>,
    path: &str,
    is_empty: bool,
) -> Result<(), SpecXmlError> {
    match tag.local_name().as_ref() {
        b"element" => {
            let Some(name) = attr_value(tag, b"name", path)? else {
                return Ok(());
            };
            let entry = registries.elements.entry(name.clone()).or_default();
            if let Some(categories) = attr_value(tag, b"attributecategories", path)? {
                entry.attribute_categories = split_list(&categories);
            }
            if let Some(extra) = attr_value(tag, b"attributes", path)? {
                entry.extra_attributes = split_list(&extra);
            }
            if let Some(geometry) = attr_value(tag, b"geometryproperties", path)? {
                entry.geometry_properties = split_list(&geometry);
            }
            if !is_empty {
                owner_stack.push(OpenOwner::Element(name));
            }
        }
        b"attributecategory" => {
            let Some(name) = attr_value(tag, b"name", path)? else {
                return Ok(());
            };
            registries
                .attribute_categories
                .entry(name.clone())
                .or_default();
            // The `presentation` category lists its members in a
            // `presentationattributes='…'` attribute rather than nested
            // `<attribute>` children.
            if let Some(presentation) = attr_value(tag, b"presentationattributes", path)? {
                registries
                    .attribute_categories
                    .entry(name.clone())
                    .or_default()
                    .extend(split_list(&presentation));
            }
            if !is_empty {
                owner_stack.push(OpenOwner::AttributeCategory(name));
            }
        }
        b"elementcategory" => {
            if let (Some(name), Some(elements)) = (
                attr_value(tag, b"name", path)?,
                attr_value(tag, b"elements", path)?,
            ) {
                registries
                    .element_categories
                    .entry(name)
                    .or_default()
                    .extend(split_list(&elements));
            }
        }
        b"attribute" => {
            let Some(name) = attr_value(tag, b"name", path)? else {
                return Ok(());
            };
            match owner_stack.last() {
                Some(OpenOwner::Element(element)) => {
                    if let Some(entry) = registries.elements.get_mut(element) {
                        entry.local_attributes.push(name);
                    }
                }
                Some(OpenOwner::AttributeCategory(category)) => {
                    registries
                        .attribute_categories
                        .entry(category.clone())
                        .or_default()
                        .push(name);
                }
                None => {
                    // Top-level `<attribute>`: scoped via `elements='…'` to
                    // the listed elements, otherwise a shared attribute that
                    // only applies where an element names it (handled by the
                    // element's own `attributes='…'`), so a bare top-level
                    // attribute contributes no edges on its own.
                    if let Some(elements) = attr_value(tag, b"elements", path)? {
                        registries
                            .scoped_attributes
                            .push((name, split_list(&elements)));
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Resolve the collected registries into the final [`SpecInventory`].
fn resolve(registries: Registries) -> SpecInventory {
    let mut inventory = SpecInventory::default();

    // Element → scoped attributes from top-level `<attribute elements='…'>`.
    let mut scoped: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (attribute, elements) in &registries.scoped_attributes {
        for element in elements {
            scoped
                .entry(element.clone())
                .or_default()
                .insert(attribute.clone());
        }
    }

    for (name, raw) in &registries.elements {
        inventory.elements.insert(name.clone());

        let mut attributes: BTreeSet<String> = BTreeSet::new();
        attributes.extend(raw.local_attributes.iter().cloned());
        attributes.extend(raw.extra_attributes.iter().cloned());
        attributes.extend(raw.geometry_properties.iter().cloned());
        for category in &raw.attribute_categories {
            if let Some(members) = registries.attribute_categories.get(category) {
                attributes.extend(members.iter().cloned());
            }
        }
        if let Some(extra) = scoped.get(name) {
            attributes.extend(extra.iter().cloned());
        }

        inventory.attributes.extend(attributes.iter().cloned());
        inventory
            .element_attributes
            .insert(name.clone(), attributes);
    }

    inventory.attribute_facts = resolve_attribute_facts(&registries, &inventory.attributes);
    inventory.element_categories = registries.element_categories;
    inventory
}

/// Build the per-attribute classification/provenance map.
///
/// Inverts the `attributecategory` → members registry into attribute →
/// categories, normalizes each raw category into a [`Classification`], and
/// folds in the element scope from any top-level `<attribute elements='…'>`
/// declaration. Every name in `attribute_universe` gets an entry so consumers
/// can look up facts for any attribute the extractor reports.
fn resolve_attribute_facts(
    registries: &Registries,
    attribute_universe: &BTreeSet<String>,
) -> BTreeMap<String, AttributeFacts> {
    let mut facts: BTreeMap<String, AttributeFacts> = attribute_universe
        .iter()
        .map(|name| (name.clone(), AttributeFacts::default()))
        .collect();

    // Category provenance: each `attributecategory` contributes its raw name
    // (and the derived classification) to every attribute it lists that is
    // part of the resolved universe. Category members an element never pulls
    // in via `attributecategories='…'` are not edges, so they are absent from
    // the universe and are intentionally skipped here — the facts map mirrors
    // the reported attribute set, no phantom entries.
    for (category, members) in &registries.attribute_categories {
        let classification = Classification::from_category(category);
        for member in members {
            if let Some(entry) = facts.get_mut(member) {
                entry.raw_categories.insert(category.clone());
                entry.classifications.insert(classification.clone());
            }
        }
    }

    // Element scope from top-level `<attribute name='x' elements='…'>`.
    for (attribute, elements) in &registries.scoped_attributes {
        if let Some(entry) = facts.get_mut(attribute) {
            entry.element_scope.extend(elements.iter().cloned());
        }
    }

    facts
}

/// Read and parse every vendored definitions file under `master`,
/// returning the deterministic [`SpecInventory`]. Missing files are an
/// error: the SVG 2 ED inventory is incomplete without all five.
pub fn read_inventory(master: &Path) -> Result<SpecInventory, SpecXmlError> {
    let mut registries = Registries::default();
    for file in DEFINITION_FILES {
        let full = master.join(file);
        let content = std::fs::read_to_string(&full).map_err(|source| SpecXmlError::Io {
            path: full.display().to_string(),
            source,
        })?;
        parse_file(&mut registries, &content, file)?;
    }
    Ok(resolve(registries))
}
