#![expect(
    dead_code,
    reason = "Build script defines types/functions not all code paths use"
)]

//! Build script that generates the baked SVG catalog from reviewed snapshot
//! datasets and browser-compat overlays.
//!
//! The generated Rust source is written into `OUT_DIR` and then included by
//! the `svg-data` crate at compile time.

#[path = "build/bcd.rs"]
mod bcd;
#[path = "build/codegen.rs"]
mod codegen;
#[path = "build/provenance_gate.rs"]
mod provenance_gate;
#[path = "build/reconcile.rs"]
mod reconcile;
#[path = "src/types.rs"]
mod types;
#[path = "build/verdict.rs"]
mod verdict;
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

/// Literal upstream `version_added` value mirrored at build time.
#[derive(Clone)]
enum RawVersionAddedValue {
    Text(String),
    Flag(bool),
    Null,
}

#[derive(Clone)]
struct BrowserFlagValue {
    flag_type: String,
    name: String,
    value_to_set: Option<String>,
}

#[derive(Clone)]
struct BrowserVersionValue {
    raw_value_added: RawVersionAddedValue,
    version_added: Option<String>,
    version_qualifier: Option<BaselineQualifierValue>,
    supported: Option<bool>,
    version_removed: Option<String>,
    version_removed_qualifier: Option<BaselineQualifierValue>,
    partial_implementation: bool,
    prefix: Option<String>,
    alternative_name: Option<String>,
    flags: Vec<BrowserFlagValue>,
    notes: Vec<String>,
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

/// Qualifier on a baseline year when the upstream date carries a
/// comparison prefix. Mirrors `svg_data::BaselineQualifier` so the
/// build-time and runtime representations stay identical.
#[derive(Clone, Copy)]
enum BaselineQualifierValue {
    Before,
    After,
    Approximately,
}

#[derive(Clone)]
enum BaselineValue {
    Widely {
        since: u16,
        qualifier: Option<BaselineQualifierValue>,
    },
    Newly {
        since: u16,
        qualifier: Option<BaselineQualifierValue>,
    },
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
            Self::Widely { since, .. } | Self::Newly { since, .. } => Some(*since),
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
    CommaWspSeparated {
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
    categories: Vec<String>,
    known_in: Vec<SpecSnapshotId>,
}

#[derive(Debug, Clone)]
struct UnionAttribute {
    name: String,
    description: String,
    mdn_url: String,
    spec_lifecycle: &'static str,
    values: UnionValues,
    /// Per-snapshot value overrides for attributes whose spec-defined value
    /// list genuinely differs between snapshots. Only populated for
    /// divergent snapshots — snapshots matching `values` are omitted and
    /// callers fall back to the union default.
    per_snapshot_value_overrides: BTreeMap<SpecSnapshotId, UnionValues>,
    elements: Vec<String>,
    known_in: Vec<SpecSnapshotId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Minimal record from `data/elements.json` used to augment the per-snapshot
/// profile attribute mappings with curated element→attribute edges.
#[derive(Debug, Deserialize)]
struct CuratedElementRecord {
    name: String,
    attrs: Vec<String>,
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

fn emit_rerun_if_changed(path: &Path) -> Result<(), Box<dyn Error>> {
    if !path.exists() {
        return Ok(());
    }

    if path.is_file() {
        println!("cargo::rerun-if-changed={}", path.display());
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        emit_rerun_if_changed(&entry.path())?;
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let svg_compat_dir = manifest_dir.join("../../workers/svg-compat");

    println!("cargo::rerun-if-changed=data/specs");
    println!("cargo::rerun-if-changed=data/derived");
    println!("cargo::rerun-if-changed=data/elements.json");
    println!("cargo::rerun-if-changed=data/placeholder_attribute_names.txt");
    println!("cargo::rerun-if-env-changed=SVG_DATA_OFFLINE");
    println!("cargo::rerun-if-env-changed=SVG_COMPAT_FILE");
    println!("cargo::rerun-if-env-changed=SVG_COMPAT_URL");
    emit_rerun_if_changed(&svg_compat_dir.join("src"))?;
    emit_rerun_if_changed(&svg_compat_dir.join("deno.json"))?;

    // Provenance referential-integrity gate: fail the build early if any
    // `source_id` in the checked-in snapshot data doesn't resolve to a
    // `pinned_sources[].input_id` in the same snapshot's `snapshot.json`.
    // Runs before `load_build_inputs()` so a bad snapshot edit surfaces
    // before we even try to deserialize the catalog-building structures.
    provenance_gate::run(manifest_dir, &canonical_snapshots())
        .map_err(|e| -> Box<dyn Error> { e.into() })?;

    let inputs = load_build_inputs()?;

    // BCD ↔ spec reconciliation: fail the build early if any feature is
    // BCD-deprecated AND still present in the latest spec snapshot
    // without a documented exception. Runs before any code emission so
    // a failure short-circuits before generating a stale catalog.
    if !inputs.compat.elements.is_empty() || !inputs.compat.attributes.is_empty() {
        let element_facts: Vec<reconcile::UnionElementFacts> = inputs
            .elements
            .iter()
            .map(|el| reconcile::UnionElementFacts {
                name: el.name.clone(),
                present_in: el.known_in.clone(),
            })
            .collect();
        let attribute_facts: Vec<reconcile::UnionAttributeFacts> = inputs
            .attributes
            .iter()
            .map(|attr| reconcile::UnionAttributeFacts {
                name: attr.name.clone(),
                present_in: attr.known_in.clone(),
                elements: attr.elements.clone(),
            })
            .collect();
        reconcile::run(
            manifest_dir,
            &inputs.compat,
            &element_facts,
            &attribute_facts,
            LATEST_SNAPSHOT,
            &inputs.compat.bcd_version,
        )
        .map_err(|e| -> Box<dyn Error> { e.into() })?;
    }

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
    write_attribute_values_profile_lookup(&mut out, &inputs.attributes, &attribute_idents)?;

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

    let curated_elements: Vec<CuratedElementRecord> =
        read_json(&manifest_dir.join("data/elements.json"))?;

    let elements = build_union_elements(&snapshot_data, &element_membership)?;
    let attributes = build_union_attributes(&snapshot_data, &attribute_membership)?;
    let profile_attributes =
        build_profile_attributes(&snapshots, &snapshot_data, &curated_elements);

    Ok(BuildInputs {
        out_path,
        elements,
        attributes,
        profile_attributes,
        compat,
    })
}

/// The most recent snapshot in [`canonical_snapshots`]. Features present only
/// here are `Experimental`; features absent here are `Obsolete`. Bump this and
/// append to `canonical_snapshots` together when a new snapshot lands.
const LATEST_SNAPSHOT: SpecSnapshotId = SpecSnapshotId::Svg2EditorsDraft20250914;

fn canonical_snapshots() -> Vec<SpecSnapshotId> {
    vec![
        SpecSnapshotId::Svg11Rec20030114,
        SpecSnapshotId::Svg11Rec20110816,
        SpecSnapshotId::Svg2Cr20181004,
        LATEST_SNAPSHOT,
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
        if is_placeholder_attribute_name(&edge.attribute) {
            continue;
        }
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
            categories: element
                .categories
                .iter()
                .map(|category| map_category_name(category))
                .collect::<Result<Vec<_>, _>>()?,
            known_in: feature.present_in.clone(),
        });
    }

    elements.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(elements)
}

/// Shared blocklist of upstream BCD/web-features IDs that do not
/// correspond to valid serialized SVG attribute names. Read from a plain
/// text file at compile time so `examples/generate_snapshot_seed.rs` can
/// `include_str!` the same source of truth without crossing the
/// lib-vs-build-script boundary.
const PLACEHOLDER_ATTRIBUTE_NAMES_RAW: &str = include_str!("data/placeholder_attribute_names.txt");

fn is_placeholder_attribute_name(name: &str) -> bool {
    PLACEHOLDER_ATTRIBUTE_NAMES_RAW
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .any(|blocked| blocked == name)
}

fn build_union_attributes(
    snapshot_data: &HashMap<SpecSnapshotId, SnapshotBuildData>,
    membership: &AttributeMembershipFile,
) -> Result<Vec<UnionAttribute>, Box<dyn Error>> {
    let mut attributes = Vec::with_capacity(membership.attributes.len());

    for feature in &membership.attributes {
        if is_placeholder_attribute_name(&feature.name) {
            continue;
        }
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

        let base_values = union_values_from_syntax(&attribute.value_syntax, &data.grammars);

        // Detect per-snapshot value divergence: compute union_values for
        // every snapshot where the attribute is present and keep only
        // entries whose value list differs from the latest-snapshot base.
        // See `examples/generate_snapshot_seed.rs` for the consumer that
        // relies on snapshot-specific values (e.g. SVG 1.1
        // `dominant-baseline` keywords differ from SVG 2 / CSS Inline 3).
        let mut per_snapshot_value_overrides: BTreeMap<SpecSnapshotId, UnionValues> =
            BTreeMap::new();
        for present_snapshot in &feature.present_in {
            if *present_snapshot == snapshot {
                continue;
            }
            let Some(snap_data) = snapshot_data.get(present_snapshot) else {
                continue;
            };
            let Some(snap_attribute) = snap_data.attributes.get(&feature.name) else {
                continue;
            };
            let snap_values =
                union_values_from_syntax(&snap_attribute.value_syntax, &snap_data.grammars);
            if snap_values != base_values {
                per_snapshot_value_overrides.insert(*present_snapshot, snap_values);
            }
        }

        attributes.push(UnionAttribute {
            name: attribute.name.clone(),
            description: attribute.title.clone(),
            mdn_url: format!(
                "https://developer.mozilla.org/docs/Web/SVG/Attribute/{}",
                attribute.name
            ),
            spec_lifecycle: union_lifecycle_expr(&feature.present_in),
            values: base_values,
            per_snapshot_value_overrides,
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
    curated_elements: &[CuratedElementRecord],
) -> Vec<(SpecSnapshotId, BTreeMap<String, Vec<String>>)> {
    snapshots
        .iter()
        .copied()
        .map(|snapshot| {
            let Some(data) = snapshot_data.get(&snapshot) else {
                return (snapshot, BTreeMap::default());
            };

            let mut mapping: BTreeMap<String, Vec<String>> = data
                .element_attributes
                .iter()
                .map(|(element, edges)| {
                    (
                        element.clone(),
                        edges.iter().map(|edge| edge.name.clone()).collect(),
                    )
                })
                .collect();

            // Augment with edges from the curated catalog (data/elements.json).
            // An attr is included only when it already has a record in this
            // snapshot's attributes.json — so version-specific attrs like `href`
            // (SVG 2 only) and `xlink:href` (SVG 1.1 only) are filtered correctly
            // without any explicit per-snapshot logic.
            for curated in curated_elements {
                if !data.elements.contains_key(&curated.name) {
                    continue;
                }
                let entry = mapping.entry(curated.name.clone()).or_default();
                for attr in &curated.attrs {
                    if data.attributes.contains_key(attr) {
                        let already_present = entry.iter().any(|e| e == attr);
                        if !already_present {
                            entry.push(attr.clone());
                        }
                    }
                }
                entry.sort();
            }

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
    if !present_in.contains(&LATEST_SNAPSHOT) {
        "SpecLifecycle::Obsolete"
    } else if present_in == [LATEST_SNAPSHOT] {
        "SpecLifecycle::Experimental"
    } else {
        "SpecLifecycle::Stable"
    }
}

/// Enum variant companion to [`union_lifecycle_expr`]. Used by the verdict
/// builder, which needs a real [`SpecLifecycle`] value (not its emitted
/// source form) to drive rule branches.
fn union_lifecycle_enum(present_in: &[SpecSnapshotId]) -> types::SpecLifecycle {
    if !present_in.contains(&LATEST_SNAPSHOT) {
        types::SpecLifecycle::Obsolete
    } else if present_in == [LATEST_SNAPSHOT] {
        types::SpecLifecycle::Experimental
    } else {
        types::SpecLifecycle::Stable
    }
}

/// Build the [`verdict::SpecFacts`] for an entry in a specific profile.
///
/// Returns `None` when the feature is absent from the given profile (caller
/// still emits a Forbid verdict via the Obsolete branch below). Otherwise
/// returns facts suitable for `verdict::compute`.
fn spec_facts_for_profile(
    present_in: &[SpecSnapshotId],
    profile: SpecSnapshotId,
) -> verdict::SpecFacts {
    let is_latest_profile = profile == LATEST_SNAPSHOT;
    if present_in.contains(&profile) {
        // Feature is defined in this profile: lifecycle is Stable unless
        // the feature only exists in the latest experimental snapshot.
        let lifecycle = if present_in == [LATEST_SNAPSHOT] && profile == LATEST_SNAPSHOT {
            types::SpecLifecycle::Experimental
        } else {
            types::SpecLifecycle::Stable
        };
        verdict::SpecFacts {
            lifecycle,
            last_seen: None,
            is_latest_profile,
        }
    } else {
        // Not present in this profile: obsolete. `last_seen` is the most
        // recent snapshot in which the feature was still defined.
        let last_seen = latest_present_snapshot(present_in).map(|snap| snap.as_str().to_string());
        verdict::SpecFacts {
            lifecycle: types::SpecLifecycle::Obsolete,
            last_seen,
            is_latest_profile,
        }
    }
}

/// Compute one [`verdict::Verdict`] per canonical snapshot.
///
/// Returned as tuples of `(snapshot_ident, verdict)` suitable for
/// `verdict::format_verdicts_slice`. Snapshots where the feature is
/// tracked-but-removed (e.g. `xlink:href` in SVG 2) get an Obsolete
/// verdict; snapshots where the feature is defined get a verdict whose
/// priority reflects BCD/browser signals.
fn verdicts_for_all_profiles(
    compat: Option<&CompatEntry>,
    present_in: &[SpecSnapshotId],
) -> Vec<(&'static str, verdict::Verdict)> {
    canonical_snapshots()
        .into_iter()
        .map(|profile| {
            let facts = spec_facts_for_profile(present_in, profile);
            let verdict = verdict::compute(compat, facts);
            (profile.as_str(), verdict)
        })
        .collect()
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
            // Url-valued attributes in SVG 2 that forward to CSS specs.
            // Without these the runtime catalog would fall back to FreeText
            // and the LSP would lose `url(#id)` completion for clip-path /
            // mask / filter.
            ("css-masking-1", "clip-path" | "mask") | ("filter-effects-1", "filter") => {
                UnionValues::Url
            }
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
    // SVG 1.1 uses `[defer] <align> [<meetOrSlice>]` — a 3-item sequence
    // starting with an optional `defer` keyword. SVG 2 dropped `defer`
    // and uses `<align> [<meetOrSlice>]`. Strip the optional defer prefix
    // when present so both shapes parse into the same
    // (alignments, meet_or_slice) tuple.
    let rest = match items.as_slice() {
        [GrammarNode::Optional { item }, rest @ ..]
            if matches!(
                item.as_ref(),
                GrammarNode::Keyword { value } if value == "defer"
            ) =>
        {
            rest
        }
        items => items,
    };
    let [alignments, meet_or_slice] = rest else {
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
    // Transform lists use `comma-wsp` separators per the SVG BNF. Older seed
    // data used `space_separated`, so we still accept that form to avoid
    // breaking any ungenerated snapshots.
    let (GrammarNode::CommaWspSeparated { item } | GrammarNode::SpaceSeparated { item }) = root
    else {
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
        for (snapshot, override_values) in &attribute.per_snapshot_value_overrides {
            let static_name = format!(
                "ATTR_{id}_VALUES_OVERRIDE_{}",
                snapshot.as_str().to_uppercase()
            );
            write_attribute_values_static(out, &static_name, override_values)?;
        }
        writeln!(out)?;
    }

    Ok(())
}

fn write_attribute_values_static(
    out: &mut String,
    name: &str,
    values: &UnionValues,
) -> std::fmt::Result {
    write!(out, "static {name}: AttributeValues = ")?;
    match values {
        UnionValues::Enum { values } => {
            out.push_str("AttributeValues::Enum(&[");
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write!(out, "\"{}\"", escape(value))?;
            }
            writeln!(out, "]);")
        }
        UnionValues::FreeText => writeln!(out, "AttributeValues::FreeText;"),
        UnionValues::Color => writeln!(out, "AttributeValues::Color;"),
        UnionValues::Length => writeln!(out, "AttributeValues::Length;"),
        UnionValues::Url => writeln!(out, "AttributeValues::Url;"),
        UnionValues::NumberOrPercentage => writeln!(out, "AttributeValues::NumberOrPercentage;"),
        UnionValues::Transform { functions } => {
            out.push_str("AttributeValues::Transform(&[");
            for (index, function) in functions.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write!(out, "\"{}\"", escape(function))?;
            }
            writeln!(out, "]);")
        }
        UnionValues::ViewBox => writeln!(out, "AttributeValues::ViewBox;"),
        UnionValues::PreserveAspectRatio {
            alignments,
            meet_or_slice,
        } => {
            out.push_str("AttributeValues::PreserveAspectRatio { alignments: &[");
            for (index, alignment) in alignments.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write!(out, "\"{}\"", escape(alignment))?;
            }
            out.push_str("], meet_or_slice: &[");
            for (index, keyword) in meet_or_slice.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write!(out, "\"{}\"", escape(keyword))?;
            }
            writeln!(out, "] }};")
        }
        UnionValues::Points => writeln!(out, "AttributeValues::Points;"),
        UnionValues::PathData => writeln!(out, "AttributeValues::PathData;"),
    }
}

fn write_attribute_values_profile_lookup(
    out: &mut String,
    attributes: &[UnionAttribute],
    attribute_idents: &HashMap<&str, String>,
) -> std::fmt::Result {
    writeln!(out, "#[allow(clippy::too_many_lines)]")?;
    writeln!(
        out,
        "pub fn generated_attribute_values_for_profile(snapshot: SpecSnapshotId, name: &str) -> Option<&'static AttributeValues> {{"
    )?;
    writeln!(out, "    match name {{")?;
    for attribute in attributes {
        if attribute.per_snapshot_value_overrides.is_empty() {
            continue;
        }
        let id = &attribute_idents[attribute.name.as_str()];
        writeln!(
            out,
            "        \"{}\" => match snapshot {{",
            escape(&attribute.name)
        )?;
        for snapshot in attribute.per_snapshot_value_overrides.keys() {
            let snapshot_id = snapshot.as_str();
            let snapshot_upper = snapshot_id.to_uppercase();
            writeln!(
                out,
                "            SpecSnapshotId::{snapshot_id} => Some(&ATTR_{id}_VALUES_OVERRIDE_{snapshot_upper}),",
            )?;
        }
        writeln!(out, "            _ => None,")?;
        writeln!(out, "        }},")?;
    }
    writeln!(out, "        _ => None,")?;
    writeln!(out, "    }}")?;
    writeln!(out, "}}")?;
    writeln!(out)
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
            ElementContentModelJson::AnySvg => "ContentModel::AnySvg".to_string(),
            ElementContentModelJson::CategorySet { .. } => {
                format!("ContentModel::Children(EL_{id}_CHILDREN)")
            }
            ElementContentModelJson::ElementSet { .. } => {
                format!("ContentModel::ChildrenSet(EL_{id}_CHILDREN_SET)")
            }
            ElementContentModelJson::ForeignNamespace => "ContentModel::Foreign".to_string(),
        };
        let compat_entry = compat.elements.get(&element.name);
        let (deprecated, experimental, spec_url_str, baseline_str, browser_support_str) =
            compat_entry.map_or_else(
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
        let profile_verdicts = verdicts_for_all_profiles(compat_entry, &element.known_in);
        let verdicts_str = verdict::format_verdicts_slice(&profile_verdicts);
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
        verdicts: {verdicts_str},
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
        let bcd_compat_entry = compat.attributes.get(&attribute.name).map(|a| &a.compat);
        let (deprecated, experimental, spec_url_str, baseline_str, browser_support_str) =
            bcd_compat_entry.map_or_else(
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
        let profile_verdicts = verdicts_for_all_profiles(bcd_compat_entry, &attribute.known_in);
        let verdicts_str = verdict::format_verdicts_slice(&profile_verdicts);
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
        verdicts: {verdicts_str},
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
        for category in &element.categories {
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
