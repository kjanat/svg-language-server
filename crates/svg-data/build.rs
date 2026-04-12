#![allow(dead_code)]

//! Build script that generates the baked SVG catalog from reviewed snapshot
//! datasets and browser-compat overlays.
//!
//! The generated Rust source is written into `OUT_DIR` and then included by
//! the `svg-data` crate at compile time.

#[path = "build/bcd.rs"]
mod bcd;
#[path = "build/codegen.rs"]
mod codegen;
#[path = "src/types.rs"]
mod types;
#[path = "src/worker_schema.rs"]
mod worker_schema;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    error::Error,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use codegen::{escape, format_baseline, format_browser_support, format_option_str, ident_from};
use serde::Deserialize;
use types::SpecSnapshotId;

const CACHE_MAX_AGE_SECS: u64 = 24 * 60 * 60;
const ALL_ELEMENT_CATEGORIES: &[&str] = &[
    "Container",
    "Shape",
    "Text",
    "Gradient",
    "Filter",
    "Descriptive",
    "Structural",
    "Animation",
    "PaintServer",
    "ClipMask",
    "LightSource",
    "FilterPrimitive",
    "TransferFunction",
    "MergeNode",
    "MotionPath",
    "NeverRendered",
];

#[derive(Clone)]
enum BrowserVersionValue {
    Unknown,
    Version(String),
}

#[derive(Clone, Default)]
struct BrowserSupportValue {
    chrome: Option<BrowserVersionValue>,
    edge: Option<BrowserVersionValue>,
    firefox: Option<BrowserVersionValue>,
    safari: Option<BrowserVersionValue>,
}

struct CompatEntry {
    deprecated: bool,
    experimental: bool,
    spec_url: Option<String>,
    baseline: Option<BaselineValue>,
    browser_support: Option<BrowserSupportValue>,
}

#[derive(Clone)]
enum BaselineValue {
    Widely { since: u16 },
    Newly { since: u16 },
    Limited,
}

impl BaselineValue {
    const fn rank(&self) -> u8 {
        match self {
            Self::Limited => 0,
            Self::Newly { .. } => 1,
            Self::Widely { .. } => 2,
        }
    }

    const fn since(&self) -> Option<u16> {
        match self {
            Self::Widely { since } | Self::Newly { since } => Some(*since),
            Self::Limited => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SnapshotElementRecord {
    name: String,
    title: String,
    categories: Vec<String>,
    content_model: ElementContentModelJson,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ElementContentModelJson {
    Empty,
    TextOnly,
    AnySvg,
    CategorySet { categories: Vec<String> },
    ElementSet { elements: Vec<String> },
    ForeignNamespace,
}

#[derive(Debug, Clone, Deserialize)]
struct SnapshotAttributeRecord {
    name: String,
    title: String,
    value_syntax: ValueSyntaxJson,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ValueSyntaxJson {
    GrammarRef { grammar_id: String },
    ForeignRef { spec: String, target: String },
    Opaque { display: String, reason: String },
}

#[derive(Debug, Clone, Deserialize)]
struct GrammarFile {
    grammars: Vec<GrammarDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
struct GrammarDefinition {
    id: String,
    root: GrammarNode,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum GrammarNode {
    Keyword {
        value: String,
    },
    DatatypeRef {
        name: String,
    },
    GrammarRef {
        name: String,
    },
    Sequence {
        items: Vec<Self>,
    },
    Choice {
        options: Vec<Self>,
    },
    Optional {
        item: Box<Self>,
    },
    ZeroOrMore {
        item: Box<Self>,
    },
    OneOrMore {
        item: Box<Self>,
    },
    CommaSeparated {
        item: Box<Self>,
    },
    SpaceSeparated {
        item: Box<Self>,
    },
    Repeat {
        item: Box<Self>,
        min: u16,
        max: Option<u16>,
    },
    Literal {
        value: String,
    },
    Opaque {
        display: String,
        reason: String,
    },
    ForeignRef {
        spec: String,
        target: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct ElementAttributeMatrixFile {
    edges: Vec<ElementAttributeEdge>,
}

#[derive(Debug, Clone, Deserialize)]
struct ElementAttributeEdge {
    element: String,
    attribute: String,
    requirement: AttributeRequirementJson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AttributeRequirementJson {
    Required,
    Optional,
}

#[derive(Debug, Clone, Deserialize)]
struct ElementMembershipFile {
    elements: Vec<FeatureMembershipRecord>,
}

#[derive(Debug, Clone, Deserialize)]
struct AttributeMembershipFile {
    attributes: Vec<FeatureMembershipRecord>,
}

#[derive(Debug, Clone, Deserialize)]
struct FeatureMembershipRecord {
    name: String,
    present_in: Vec<SpecSnapshotId>,
}

#[derive(Debug, Clone)]
struct SnapshotBuildData {
    elements: HashMap<String, SnapshotElementRecord>,
    attributes: HashMap<String, SnapshotAttributeRecord>,
    grammars: HashMap<String, GrammarDefinition>,
    element_attributes: BTreeMap<String, Vec<AttributeEdgeRecord>>,
    attribute_elements: BTreeMap<String, Vec<String>>,
    global_attributes: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct AttributeEdgeRecord {
    name: String,
    required: bool,
}

#[derive(Debug, Clone)]
struct UnionElement {
    name: String,
    description: String,
    mdn_url: String,
    spec_lifecycle: &'static str,
    content_model: ElementContentModelJson,
    required_attrs: Vec<String>,
    attrs: Vec<String>,
    global_attrs: bool,
    category: Option<String>,
    known_in: Vec<SpecSnapshotId>,
}

#[derive(Debug, Clone)]
struct UnionAttribute {
    name: String,
    description: String,
    mdn_url: String,
    spec_lifecycle: &'static str,
    values: UnionValues,
    elements: Vec<String>,
    known_in: Vec<SpecSnapshotId>,
}

#[derive(Debug, Clone)]
enum UnionValues {
    Enum {
        values: Vec<String>,
    },
    FreeText,
    Color,
    Length,
    Url,
    NumberOrPercentage,
    Transform {
        functions: Vec<String>,
    },
    ViewBox,
    PreserveAspectRatio {
        alignments: Vec<String>,
        meet_or_slice: Vec<String>,
    },
    Points,
    PathData,
}

struct BuildInputs {
    out_path: PathBuf,
    elements: Vec<UnionElement>,
    attributes: Vec<UnionAttribute>,
    profile_attributes: Vec<(SpecSnapshotId, BTreeMap<String, Vec<String>>)>,
    compat: bcd::CompatData,
}

fn ensure_cached(url: &str, dest: &Path, offline: bool) -> Result<bool, String> {
    if offline {
        if dest.exists() {
            println!(
                "cargo::warning=compat: using existing cache (offline mode): {}",
                dest.display()
            );
            return Ok(true);
        }
        println!(
            "cargo::warning=compat: no cache and offline mode — skipping {}",
            dest.display()
        );
        return Ok(false);
    }

    if dest.exists()
        && let Ok(meta) = fs::metadata(dest)
        && let Ok(modified) = meta.modified()
        && let Ok(age) = SystemTime::now().duration_since(modified)
        && age.as_secs() < CACHE_MAX_AGE_SECS
    {
        return Ok(true);
    }

    let mut response = ureq::get(url)
        .call()
        .map_err(|e| format!("fetch {url}: {e}"))?;

    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("read body {url}: {e}"))?;

    fs::write(dest, &body).map_err(|e| format!("write {}: {e}", dest.display()))?;

    Ok(true)
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rerun-if-changed=data/specs");
    println!("cargo::rerun-if-changed=data/derived");
    println!("cargo::rerun-if-env-changed=SVG_DATA_OFFLINE");
    println!("cargo::rerun-if-env-changed=SVG_COMPAT_URL");

    let inputs = load_build_inputs()?;
    let mut out = String::with_capacity(64 * 1024);
    let element_idents = element_idents(&inputs.elements);
    let attribute_idents = attribute_idents(&inputs.attributes);

    writeln!(out, "// @generated by build.rs -- do not edit")?;
    writeln!(out)?;

    write_element_statics(&mut out, &inputs.elements, &element_idents)?;
    write_attribute_statics(&mut out, &inputs.attributes, &attribute_idents)?;
    write_elements_array(&mut out, &inputs.elements, &element_idents, &inputs.compat)?;
    write_attributes_array(
        &mut out,
        &inputs.attributes,
        &attribute_idents,
        &inputs.compat,
    )?;
    write_category_mapping(&mut out, &inputs.elements)?;
    write_membership_lookup(
        &mut out,
        &inputs.elements,
        &inputs.attributes,
        &element_idents,
        &attribute_idents,
    )?;
    write_profile_attribute_lookup(&mut out, &inputs.profile_attributes)?;

    fs::write(&inputs.out_path, out)?;
    Ok(())
}

fn load_build_inputs() -> Result<BuildInputs, Box<dyn Error>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    let out_path = out_dir.join("catalog.rs");
    let compat = bcd::fetch_compat_data(&out_dir);

    let snapshots = canonical_snapshots();
    let snapshot_data: HashMap<SpecSnapshotId, SnapshotBuildData> = snapshots
        .iter()
        .copied()
        .map(|snapshot| {
            load_snapshot_build_data(manifest_dir, snapshot).map(|data| (snapshot, data))
        })
        .collect::<Result<_, _>>()?;

    let element_membership: ElementMembershipFile =
        read_json(&manifest_dir.join("data/derived/union/elements.json"))?;
    let attribute_membership: AttributeMembershipFile =
        read_json(&manifest_dir.join("data/derived/union/attributes.json"))?;

    let elements = build_union_elements(&snapshot_data, &element_membership)?;
    let attributes = build_union_attributes(&snapshot_data, &attribute_membership)?;
    let profile_attributes = build_profile_attributes(&snapshots, &snapshot_data);

    Ok(BuildInputs {
        out_path,
        elements,
        attributes,
        profile_attributes,
        compat,
    })
}

fn canonical_snapshots() -> Vec<SpecSnapshotId> {
    vec![
        SpecSnapshotId::Svg11Rec20030114,
        SpecSnapshotId::Svg11Rec20110816,
        SpecSnapshotId::Svg2Cr20181004,
        SpecSnapshotId::Svg2EditorsDraft20250914,
    ]
}

fn load_snapshot_build_data(
    manifest_dir: &Path,
    snapshot: SpecSnapshotId,
) -> Result<SnapshotBuildData, Box<dyn Error>> {
    let root = manifest_dir.join("data/specs").join(snapshot.as_str());
    let elements: Vec<SnapshotElementRecord> = read_json(&root.join("elements.json"))?;
    let attributes: Vec<SnapshotAttributeRecord> = read_json(&root.join("attributes.json"))?;
    let grammars: GrammarFile = read_json(&root.join("grammars.json"))?;
    let matrix: ElementAttributeMatrixFile =
        read_json(&root.join("element_attribute_matrix.json"))?;

    let element_names: HashSet<String> = elements
        .iter()
        .map(|element| element.name.clone())
        .collect();
    let mut element_attributes: BTreeMap<String, Vec<AttributeEdgeRecord>> = BTreeMap::new();
    let mut attribute_elements: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut attribute_counts: HashMap<String, usize> = HashMap::new();

    for edge in matrix.edges {
        element_attributes
            .entry(edge.element.clone())
            .or_default()
            .push(AttributeEdgeRecord {
                name: edge.attribute.clone(),
                required: edge.requirement == AttributeRequirementJson::Required,
            });
        attribute_elements
            .entry(edge.attribute.clone())
            .or_default()
            .push(edge.element);
        *attribute_counts.entry(edge.attribute).or_default() += 1;
    }

    for edges in element_attributes.values_mut() {
        edges.sort_by(|left, right| left.name.cmp(&right.name));
    }
    for elements in attribute_elements.values_mut() {
        elements.sort();
    }

    let global_attributes = attribute_counts
        .into_iter()
        .filter(|(_, count)| *count == element_names.len())
        .map(|(name, _)| name)
        .collect();

    Ok(SnapshotBuildData {
        elements: elements
            .into_iter()
            .map(|element| (element.name.clone(), element))
            .collect(),
        attributes: attributes
            .into_iter()
            .map(|attribute| (attribute.name.clone(), attribute))
            .collect(),
        grammars: grammars
            .grammars
            .into_iter()
            .map(|grammar| (grammar.id.clone(), grammar))
            .collect(),
        element_attributes,
        attribute_elements,
        global_attributes,
    })
}

fn build_union_elements(
    snapshot_data: &HashMap<SpecSnapshotId, SnapshotBuildData>,
    membership: &ElementMembershipFile,
) -> Result<Vec<UnionElement>, Box<dyn Error>> {
    let mut elements = Vec::with_capacity(membership.elements.len());

    for feature in &membership.elements {
        let snapshot = latest_present_snapshot(&feature.present_in)
            .ok_or_else(|| format!("element {} has no present snapshots", feature.name))?;
        let data = snapshot_data
            .get(&snapshot)
            .ok_or_else(|| format!("missing snapshot data for {}", snapshot.as_str()))?;
        let element = data
            .elements
            .get(&feature.name)
            .ok_or_else(|| format!("missing element {} in {}", feature.name, snapshot.as_str()))?;
        let edges = data
            .element_attributes
            .get(&feature.name)
            .cloned()
            .unwrap_or_default();
        let attrs: Vec<String> = edges.iter().map(|edge| edge.name.clone()).collect();
        let required_attrs: Vec<String> = edges
            .iter()
            .filter(|edge| edge.required)
            .map(|edge| edge.name.clone())
            .collect();
        let attr_set: BTreeSet<String> = attrs.iter().cloned().collect();
        let global_attrs = !data.global_attributes.is_empty()
            && data
                .global_attributes
                .iter()
                .all(|attribute| attr_set.contains(attribute));

        elements.push(UnionElement {
            name: element.name.clone(),
            description: element.title.clone(),
            mdn_url: format!(
                "https://developer.mozilla.org/docs/Web/SVG/Element/{}",
                element.name
            ),
            spec_lifecycle: union_lifecycle_expr(&feature.present_in),
            content_model: element.content_model.clone(),
            required_attrs,
            attrs,
            global_attrs,
            category: element
                .categories
                .first()
                .map(|category| map_category_name(category))
                .transpose()?,
            known_in: feature.present_in.clone(),
        });
    }

    elements.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(elements)
}

fn build_union_attributes(
    snapshot_data: &HashMap<SpecSnapshotId, SnapshotBuildData>,
    membership: &AttributeMembershipFile,
) -> Result<Vec<UnionAttribute>, Box<dyn Error>> {
    let mut attributes = Vec::with_capacity(membership.attributes.len());

    for feature in &membership.attributes {
        let snapshot = latest_present_snapshot(&feature.present_in)
            .ok_or_else(|| format!("attribute {} has no present snapshots", feature.name))?;
        let data = snapshot_data
            .get(&snapshot)
            .ok_or_else(|| format!("missing snapshot data for {}", snapshot.as_str()))?;
        let attribute = data.attributes.get(&feature.name).ok_or_else(|| {
            format!(
                "missing attribute {} in {}",
                feature.name,
                snapshot.as_str()
            )
        })?;
        let elements = data
            .attribute_elements
            .get(&feature.name)
            .cloned()
            .unwrap_or_default();

        attributes.push(UnionAttribute {
            name: attribute.name.clone(),
            description: attribute.title.clone(),
            mdn_url: format!(
                "https://developer.mozilla.org/docs/Web/SVG/Attribute/{}",
                attribute.name
            ),
            spec_lifecycle: union_lifecycle_expr(&feature.present_in),
            values: union_values_from_syntax(&attribute.value_syntax, &data.grammars),
            elements,
            known_in: feature.present_in.clone(),
        });
    }

    attributes.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(attributes)
}

fn build_profile_attributes(
    snapshots: &[SpecSnapshotId],
    snapshot_data: &HashMap<SpecSnapshotId, SnapshotBuildData>,
) -> Vec<(SpecSnapshotId, BTreeMap<String, Vec<String>>)> {
    snapshots
        .iter()
        .copied()
        .map(|snapshot| {
            let mapping = snapshot_data
                .get(&snapshot)
                .map(|data| {
                    data.element_attributes
                        .iter()
                        .map(|(element, edges)| {
                            (
                                element.clone(),
                                edges.iter().map(|edge| edge.name.clone()).collect(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            (snapshot, mapping)
        })
        .collect::<Vec<_>>()
}

fn read_json<T>(path: &Path) -> Result<T, Box<dyn Error>>
where
    T: serde::de::DeserializeOwned,
{
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

fn latest_present_snapshot(present_in: &[SpecSnapshotId]) -> Option<SpecSnapshotId> {
    canonical_snapshots()
        .into_iter()
        .rev()
        .find(|snapshot| present_in.contains(snapshot))
}

fn union_lifecycle_expr(present_in: &[SpecSnapshotId]) -> &'static str {
    if !present_in.contains(&SpecSnapshotId::Svg2EditorsDraft20250914) {
        "SpecLifecycle::Obsolete"
    } else if present_in == [SpecSnapshotId::Svg2EditorsDraft20250914] {
        "SpecLifecycle::Experimental"
    } else {
        "SpecLifecycle::Stable"
    }
}

fn map_category_name(value: &str) -> Result<String, Box<dyn Error>> {
    let category = match value {
        "container" => "Container",
        "shape" => "Shape",
        "text" => "Text",
        "gradient" => "Gradient",
        "filter" => "Filter",
        "descriptive" => "Descriptive",
        "structural" => "Structural",
        "animation" => "Animation",
        "paint_server" => "PaintServer",
        "clip_mask" => "ClipMask",
        "light_source" => "LightSource",
        "filter_primitive" => "FilterPrimitive",
        "transfer_function" => "TransferFunction",
        "merge_node" => "MergeNode",
        "motion_path" => "MotionPath",
        "never_rendered" => "NeverRendered",
        _ => return Err(format!("unknown element category {value}").into()),
    };
    Ok(category.to_string())
}

fn union_values_from_syntax(
    syntax: &ValueSyntaxJson,
    grammars: &HashMap<String, GrammarDefinition>,
) -> UnionValues {
    match syntax {
        ValueSyntaxJson::GrammarRef { grammar_id } => grammar_values(grammar_id, grammars),
        ValueSyntaxJson::ForeignRef { spec, target } => match (spec.as_str(), target.as_str()) {
            ("css-color-4", "<color>") => UnionValues::Color,
            ("css-values-3", "<length>") => UnionValues::Length,
            ("css-values-3", "<number-or-percentage>") => UnionValues::NumberOrPercentage,
            _ => UnionValues::FreeText,
        },
        ValueSyntaxJson::Opaque { .. } => UnionValues::FreeText,
    }
}

fn grammar_values(grammar_id: &str, grammars: &HashMap<String, GrammarDefinition>) -> UnionValues {
    if grammar_id == "path-data" {
        return UnionValues::PathData;
    }
    if grammar_id == "color" {
        return UnionValues::Color;
    }
    if grammar_id == "length" {
        return UnionValues::Length;
    }
    if grammar_id == "number-or-percentage" {
        return UnionValues::NumberOrPercentage;
    }
    if grammar_id == "points" {
        return UnionValues::Points;
    }
    if grammar_id == "url-reference" {
        return UnionValues::Url;
    }
    if grammar_id == "view-box" {
        return UnionValues::ViewBox;
    }

    let Some(grammar) = grammars.get(grammar_id) else {
        return UnionValues::FreeText;
    };

    if let Some(values) = enum_values(&grammar.root) {
        return UnionValues::Enum { values };
    }
    if grammar_id == "preserve-aspect-ratio"
        && let Some((alignments, meet_or_slice)) = preserve_aspect_ratio_values(&grammar.root)
    {
        return UnionValues::PreserveAspectRatio {
            alignments,
            meet_or_slice,
        };
    }
    if grammar_id.starts_with("transform-list-")
        && let Some(functions) = transform_functions(&grammar.root)
    {
        return UnionValues::Transform { functions };
    }

    UnionValues::FreeText
}

fn enum_values(root: &GrammarNode) -> Option<Vec<String>> {
    let GrammarNode::Choice { options } = root else {
        return None;
    };
    options
        .iter()
        .map(|option| match option {
            GrammarNode::Keyword { value } => Some(value.clone()),
            _ => None,
        })
        .collect()
}

fn preserve_aspect_ratio_values(root: &GrammarNode) -> Option<(Vec<String>, Vec<String>)> {
    let GrammarNode::Sequence { items } = root else {
        return None;
    };
    let [alignments, meet_or_slice] = items.as_slice() else {
        return None;
    };
    let alignments = enum_values(alignments)?;
    let meet_or_slice = match meet_or_slice {
        GrammarNode::Optional { item } => enum_values(item)?,
        _ => return None,
    };
    Some((alignments, meet_or_slice))
}

fn transform_functions(root: &GrammarNode) -> Option<Vec<String>> {
    let GrammarNode::SpaceSeparated { item } = root else {
        return None;
    };
    let GrammarNode::Choice { options } = item.as_ref() else {
        return None;
    };

    options
        .iter()
        .map(|option| match option {
            GrammarNode::DatatypeRef { name } => {
                name.strip_suffix("-transform-function").map(str::to_string)
            }
            _ => None,
        })
        .collect()
}

fn element_idents(elements: &[UnionElement]) -> HashMap<&str, String> {
    elements
        .iter()
        .map(|element| (element.name.as_str(), ident_from(&element.name)))
        .collect()
}

fn attribute_idents(attributes: &[UnionAttribute]) -> HashMap<&str, String> {
    attributes
        .iter()
        .map(|attribute| (attribute.name.as_str(), ident_from(&attribute.name)))
        .collect()
}

fn write_static_str_slice(out: &mut String, name: &str, values: &[String]) -> std::fmt::Result {
    write!(out, "static {name}: &[&str] = &[")?;
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        write!(out, "\"{}\"", escape(value))?;
    }
    writeln!(out, "];")
}

fn write_snapshot_slice(
    out: &mut String,
    name: &str,
    values: &[SpecSnapshotId],
) -> std::fmt::Result {
    write!(out, "static {name}: &[SpecSnapshotId] = &[")?;
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        write!(out, "SpecSnapshotId::{}", value.as_str())?;
    }
    writeln!(out, "];")
}

fn write_element_statics(
    out: &mut String,
    elements: &[UnionElement],
    element_idents: &HashMap<&str, String>,
) -> std::fmt::Result {
    for element in elements {
        let id = &element_idents[element.name.as_str()];
        write_static_str_slice(
            out,
            &format!("EL_{id}_REQUIRED_ATTRS"),
            &element.required_attrs,
        )?;
        write_static_str_slice(out, &format!("EL_{id}_ATTRS"), &element.attrs)?;
        write_snapshot_slice(out, &format!("EL_{id}_SNAPSHOTS"), &element.known_in)?;
        match &element.content_model {
            ElementContentModelJson::CategorySet { categories } => {
                write!(out, "static EL_{id}_CHILDREN: &[ElementCategory] = &[")?;
                for (index, category) in categories.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    write!(
                        out,
                        "ElementCategory::{}",
                        map_category_name(category).map_err(|_| std::fmt::Error)?
                    )?;
                }
                writeln!(out, "];")?;
            }
            ElementContentModelJson::ElementSet { elements } => {
                write_static_str_slice(out, &format!("EL_{id}_CHILDREN_SET"), elements)?;
            }
            ElementContentModelJson::Empty
            | ElementContentModelJson::TextOnly
            | ElementContentModelJson::AnySvg
            | ElementContentModelJson::ForeignNamespace => {}
        }
        writeln!(out)?;
    }

    Ok(())
}

fn write_attribute_statics(
    out: &mut String,
    attributes: &[UnionAttribute],
    attribute_idents: &HashMap<&str, String>,
) -> std::fmt::Result {
    for attribute in attributes {
        let id = &attribute_idents[attribute.name.as_str()];
        write_static_str_slice(out, &format!("ATTR_{id}_ELEMENTS"), &attribute.elements)?;
        write_snapshot_slice(out, &format!("ATTR_{id}_SNAPSHOTS"), &attribute.known_in)?;
        match &attribute.values {
            UnionValues::Enum { values } => {
                write_static_str_slice(out, &format!("ATTR_{id}_VALUES"), values)?;
            }
            UnionValues::Transform { functions } => {
                write_static_str_slice(out, &format!("ATTR_{id}_FUNCTIONS"), functions)?;
            }
            UnionValues::PreserveAspectRatio {
                alignments,
                meet_or_slice,
            } => {
                write_static_str_slice(out, &format!("ATTR_{id}_ALIGNMENTS"), alignments)?;
                write_static_str_slice(out, &format!("ATTR_{id}_MEET_OR_SLICE"), meet_or_slice)?;
            }
            UnionValues::FreeText
            | UnionValues::Color
            | UnionValues::Length
            | UnionValues::Url
            | UnionValues::NumberOrPercentage
            | UnionValues::ViewBox
            | UnionValues::Points
            | UnionValues::PathData => {}
        }
        writeln!(out)?;
    }

    Ok(())
}

fn write_elements_array(
    out: &mut String,
    elements: &[UnionElement],
    element_idents: &HashMap<&str, String>,
    compat: &bcd::CompatData,
) -> std::fmt::Result {
    writeln!(out, "pub static ELEMENTS: &[ElementDef] = &[")?;

    for element in elements {
        let id = &element_idents[element.name.as_str()];
        let content_model = match &element.content_model {
            ElementContentModelJson::Empty => "ContentModel::Void".to_string(),
            ElementContentModelJson::TextOnly => "ContentModel::Text".to_string(),
            ElementContentModelJson::AnySvg
            | ElementContentModelJson::CategorySet { .. }
            | ElementContentModelJson::ElementSet { .. } => {
                format!("ContentModel::Children(EL_{id}_CHILDREN)")
            }
            ElementContentModelJson::ForeignNamespace => "ContentModel::Foreign".to_string(),
        };
        let (deprecated, experimental, spec_url_str, baseline_str, browser_support_str) =
            compat.elements.get(&element.name).map_or_else(
                || {
                    (
                        false,
                        false,
                        "None".to_string(),
                        "None".to_string(),
                        "None".to_string(),
                    )
                },
                |entry| {
                    (
                        entry.deprecated,
                        entry.experimental,
                        format_option_str(entry.spec_url.as_deref()),
                        format_baseline(entry.baseline.as_ref()),
                        format_browser_support(entry.browser_support.as_ref()),
                    )
                },
            );
        let name = escape(&element.name);
        let description = escape(&element.description);
        let mdn_url = escape(&element.mdn_url);

        write!(
            out,
            r#"    ElementDef {{
        name: "{name}",
        description: "{description}",
        mdn_url: "{mdn_url}",
        spec_lifecycle: {},
        deprecated: {deprecated},
        experimental: {experimental},
        spec_url: {spec_url_str},
        baseline: {baseline_str},
        browser_support: {browser_support_str},
        content_model: {content_model},
        required_attrs: EL_{id}_REQUIRED_ATTRS,
        attrs: EL_{id}_ATTRS,
        global_attrs: {},
    }},
"#,
            element.spec_lifecycle, element.global_attrs
        )?;
    }

    writeln!(out, "];")?;
    writeln!(out)
}

fn write_attributes_array(
    out: &mut String,
    attributes: &[UnionAttribute],
    attribute_idents: &HashMap<&str, String>,
    compat: &bcd::CompatData,
) -> std::fmt::Result {
    writeln!(out, "pub static ATTRIBUTES: &[AttributeDef] = &[")?;

    for attribute in attributes {
        let id = &attribute_idents[attribute.name.as_str()];
        let values = attribute_values_expr(id, &attribute.values);
        let (deprecated, experimental, spec_url_str, baseline_str, browser_support_str) =
            compat.attributes.get(&attribute.name).map_or_else(
                || {
                    (
                        false,
                        false,
                        "None".to_string(),
                        "None".to_string(),
                        "None".to_string(),
                    )
                },
                |bcd_attribute| {
                    (
                        bcd_attribute.compat.deprecated,
                        bcd_attribute.compat.experimental,
                        format_option_str(bcd_attribute.compat.spec_url.as_deref()),
                        format_baseline(bcd_attribute.compat.baseline.as_ref()),
                        format_browser_support(bcd_attribute.compat.browser_support.as_ref()),
                    )
                },
            );
        let name = escape(&attribute.name);
        let description = escape(&attribute.description);
        let mdn_url = escape(&attribute.mdn_url);

        write!(
            out,
            r#"    AttributeDef {{
        name: "{name}",
        description: "{description}",
        mdn_url: "{mdn_url}",
        spec_lifecycle: {},
        deprecated: {deprecated},
        experimental: {experimental},
        spec_url: {spec_url_str},
        baseline: {baseline_str},
        browser_support: {browser_support_str},
        values: {values},
        elements: ATTR_{id}_ELEMENTS,
    }},
"#,
            attribute.spec_lifecycle
        )?;
    }

    writeln!(out, "];")?;
    writeln!(out)
}

fn write_category_mapping(out: &mut String, elements: &[UnionElement]) -> std::fmt::Result {
    let mut category_map: HashMap<&str, Vec<&str>> = HashMap::new();
    for element in elements {
        if let Some(category) = &element.category {
            category_map
                .entry(category.as_str())
                .or_default()
                .push(element.name.as_str());
        }
    }

    let mut unknown_categories: Vec<&str> = category_map
        .keys()
        .copied()
        .filter(|category| !ALL_ELEMENT_CATEGORIES.contains(category))
        .collect();
    unknown_categories.sort_unstable();
    assert!(
        unknown_categories.is_empty(),
        "unknown element categories in reviewed snapshot data: {unknown_categories:?}"
    );

    writeln!(
        out,
        "pub const fn generated_elements_in_category(cat: ElementCategory) -> &'static [&'static str] {{"
    )?;
    writeln!(out, "    match cat {{")?;
    for names in category_map.values_mut() {
        names.sort_unstable();
    }
    for category in ALL_ELEMENT_CATEGORIES {
        if let Some(names) = category_map.get(category) {
            let names_str = names
                .iter()
                .map(|name| format!("\"{}\"", escape(name)))
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                out,
                "        ElementCategory::{category} => &[{names_str}],"
            )?;
        } else {
            writeln!(out, "        ElementCategory::{category} => &[],")?;
        }
    }
    writeln!(out, "    }}")?;
    writeln!(out, "}}")
}

fn write_membership_lookup(
    out: &mut String,
    elements: &[UnionElement],
    attributes: &[UnionAttribute],
    element_idents: &HashMap<&str, String>,
    attribute_idents: &HashMap<&str, String>,
) -> std::fmt::Result {
    writeln!(
        out,
        "pub fn generated_known_element_snapshots(name: &str) -> Option<&'static [SpecSnapshotId]> {{"
    )?;
    writeln!(out, "    match name {{")?;
    for element in elements {
        let id = &element_idents[element.name.as_str()];
        writeln!(
            out,
            "        \"{}\" => Some(EL_{id}_SNAPSHOTS),",
            escape(&element.name)
        )?;
    }
    writeln!(out, "        _ => None,")?;
    writeln!(out, "    }}")?;
    writeln!(out, "}}")?;
    writeln!(out)?;

    writeln!(out, "#[allow(clippy::too_many_lines)]")?;
    writeln!(
        out,
        "pub fn generated_known_attribute_snapshots(name: &str) -> Option<&'static [SpecSnapshotId]> {{"
    )?;
    writeln!(out, "    match name {{")?;
    for attribute in attributes {
        let id = &attribute_idents[attribute.name.as_str()];
        writeln!(
            out,
            "        \"{}\" => Some(ATTR_{id}_SNAPSHOTS),",
            escape(&attribute.name)
        )?;
    }
    writeln!(out, "        _ => None,")?;
    writeln!(out, "    }}")?;
    writeln!(out, "}}")?;
    writeln!(out)
}

fn write_profile_attribute_lookup(
    out: &mut String,
    profile_attributes: &[(SpecSnapshotId, BTreeMap<String, Vec<String>>)],
) -> std::fmt::Result {
    for (snapshot, elements) in profile_attributes {
        let snapshot_id = ident_from(snapshot.as_str());
        for (element, attributes) in elements {
            let element_id = ident_from(element);
            write_static_str_slice(
                out,
                &format!("PROFILE_{snapshot_id}_EL_{element_id}_ATTRS"),
                attributes,
            )?;
        }
    }
    writeln!(out)?;

    writeln!(out, "#[allow(clippy::too_many_lines)]")?;
    writeln!(
        out,
        "pub fn generated_attribute_names_for_profile(snapshot: SpecSnapshotId, element_name: &str) -> &'static [&'static str] {{"
    )?;
    writeln!(out, "    match snapshot {{")?;
    for (snapshot, elements) in profile_attributes {
        let snapshot_id = ident_from(snapshot.as_str());
        writeln!(
            out,
            "        SpecSnapshotId::{} => match element_name {{",
            snapshot.as_str()
        )?;
        for element in elements.keys() {
            let element_id = ident_from(element);
            writeln!(
                out,
                "            \"{}\" => PROFILE_{}_EL_{element_id}_ATTRS,",
                escape(element),
                snapshot_id
            )?;
        }
        writeln!(out, "            _ => &[],")?;
        writeln!(out, "        }},")?;
    }
    writeln!(out, "    }}")?;
    writeln!(out, "}}")
}

fn attribute_values_expr(id: &str, values: &UnionValues) -> String {
    match values {
        UnionValues::Enum { .. } => format!("AttributeValues::Enum(ATTR_{id}_VALUES)"),
        UnionValues::FreeText => "AttributeValues::FreeText".to_string(),
        UnionValues::Color => "AttributeValues::Color".to_string(),
        UnionValues::Length => "AttributeValues::Length".to_string(),
        UnionValues::Url => "AttributeValues::Url".to_string(),
        UnionValues::NumberOrPercentage => "AttributeValues::NumberOrPercentage".to_string(),
        UnionValues::Transform { .. } => format!("AttributeValues::Transform(ATTR_{id}_FUNCTIONS)"),
        UnionValues::ViewBox => "AttributeValues::ViewBox".to_string(),
        UnionValues::PreserveAspectRatio { .. } => format!(
            "AttributeValues::PreserveAspectRatio {{ alignments: ATTR_{id}_ALIGNMENTS, meet_or_slice: ATTR_{id}_MEET_OR_SLICE }}"
        ),
        UnionValues::Points => "AttributeValues::Points".to_string(),
        UnionValues::PathData => "AttributeValues::PathData".to_string(),
    }
}
