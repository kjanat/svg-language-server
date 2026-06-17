//! Build script: generate the runtime catalog from the committed, extracted
//! structured spec data.
//!
//! The extraction pipeline (a separate `regen` step) fetches canonical upstream
//! and writes structured JSON under `data/`. This script reads that JSON and
//! emits `catalog.rs` (static `ElementDef`/`AttributeDef` arrays) into `OUT_DIR`,
//! which `src/catalog.rs` includes. When the data has not been generated yet,
//! it emits an empty catalog so the crate still compiles.

use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

const CATALOG_SCHEMA_VERSION: u16 = 1;

/// The committed root manifest, mirroring `svg-data-regen`'s output shape.
#[derive(Deserialize)]
struct CatalogManifest {
    schema_version: u16,
    core: CatalogRef,
    #[serde(default)]
    compat: Option<CatalogRef>,
    graph: CatalogRef,
    #[serde(default)]
    snapshots: Vec<SnapshotRef>,
}

/// Reference from `catalog.json` to another catalog document.
#[derive(Deserialize)]
struct CatalogRef {
    href: String,
}

/// Canonical latest catalog data.
#[derive(Deserialize)]
struct CatalogCore {
    schema_version: u16,
    elements: Vec<Element>,
    #[serde(default)]
    attributes: Vec<Attribute>,
}

/// Reference from `catalog.json` to a version-specific overlay document.
#[derive(Deserialize)]
struct SnapshotRef {
    profile: SpecSnapshot,
    href: String,
}

/// Browser-compat metadata from `catalog.compat.json`.
#[derive(Deserialize)]
struct CompatMetadata {
    schema_version: u16,
    #[serde(default)]
    unmodeled_features: Vec<CompatSubfeature>,
}

/// Derived graph data from `catalog.graph.json`.
#[derive(Deserialize)]
struct CatalogGraphDocument {
    schema_version: u16,
    #[serde(flatten)]
    graph: CatalogGraph,
}

/// A BCD feature kept out of the element/attribute catalog.
#[derive(Deserialize)]
struct CompatSubfeature {
    compat_key: String,
    kind: CompatSubfeatureKind,
    element: String,
    name: String,
    #[serde(flatten)]
    facts: CompatFacts,
}

/// Why a BCD feature was kept out of the element/attribute catalog.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum CompatSubfeatureKind {
    Behavior,
    LegacyXlinkAlias,
}

/// One element entry from `catalog.json`.
#[derive(Deserialize)]
struct Element {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    mdn_url: Option<String>,
    spec_url: Option<String>,
    #[serde(default)]
    deprecated: bool,
    #[serde(default)]
    experimental: bool,
    #[serde(default)]
    standard_track: Option<bool>,
    #[serde(default)]
    baseline: Option<BaselineStatus>,
    #[serde(default)]
    browser_support: Option<BrowserSupport>,
    content_model: ContentModel,
    attrs: Vec<String>,
    global_attrs: bool,
}

/// The content-model shapes the catalog encodes (already flattened).
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ContentModel {
    ChildrenSet { elements: Vec<String> },
    AnySvg,
    Foreign,
    Text,
}

/// One attribute entry from `catalog.json`.
#[derive(Deserialize)]
struct Attribute {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    mdn_url: Option<String>,
    spec_url: Option<String>,
    #[serde(default)]
    deprecated: bool,
    #[serde(default)]
    experimental: bool,
    #[serde(default)]
    standard_track: Option<bool>,
    animatable: bool,
    #[serde(rename = "presentation_attribute")]
    presentation: Option<String>,
    #[serde(default)]
    baseline: Option<BaselineStatus>,
    #[serde(default)]
    browser_support: Option<BrowserSupport>,
    #[serde(default)]
    element_compat: Vec<AttributeElementCompat>,
    #[serde(default)]
    value_overrides: Vec<AttributeValueOverride>,
    values: AttributeValues,
    applicability: AttributeApplicability,
}

/// Element-scoped compat facts for an attribute.
#[derive(Deserialize)]
struct AttributeElementCompat {
    element: String,
    #[serde(flatten)]
    facts: CompatFacts,
}

/// Per-profile value-space override for one attribute.
#[derive(Deserialize)]
struct AttributeValueOverride {
    profile: SpecSnapshot,
    values: AttributeValues,
}

/// One generated per-snapshot inventory.
#[derive(Clone, Deserialize)]
struct Inventory {
    profile: SpecSnapshot,
    elements: Vec<InventoryElement>,
}

/// One generated snapshot overlay file.
#[derive(Deserialize)]
struct SnapshotDocument {
    schema_version: u16,
    profile: SpecSnapshot,
    #[serde(default)]
    aliases: Vec<String>,
    inventory: SnapshotInventory,
    #[serde(default)]
    lifecycle: SnapshotLifecycle,
    #[serde(default)]
    value_overrides: Vec<SnapshotValueOverride>,
}

/// The lifecycle overlay payload inside one snapshot overlay.
#[derive(Default, Deserialize)]
struct SnapshotLifecycle {
    #[serde(default)]
    elements: Vec<LifecycleEntry>,
    #[serde(default)]
    attributes: Vec<LifecycleEntry>,
}

/// One feature lifecycle fact in a snapshot overlay.
#[derive(Deserialize)]
struct LifecycleEntry {
    name: String,
    #[serde(default)]
    catalog_name: Option<String>,
    present: bool,
    lifecycle: LifecycleStatus,
    #[serde(default)]
    known_in: Vec<SpecSnapshot>,
}

/// Lifecycle statuses emitted by snapshot overlays.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum LifecycleStatus {
    Stable,
    Experimental,
    Obsolete,
    NotYetIntroduced,
}

/// The inventory payload inside one snapshot overlay.
#[derive(Deserialize)]
struct SnapshotInventory {
    elements: Vec<InventoryElement>,
}

/// One value-space override scoped by the containing snapshot document.
#[derive(Deserialize)]
struct SnapshotValueOverride {
    attribute: String,
    values: AttributeValues,
}

/// One element's generated per-snapshot inventory.
#[derive(Clone, Deserialize)]
struct InventoryElement {
    name: String,
    attributes: Vec<String>,
}

/// SVG specification snapshots encoded in the catalog.
#[derive(Clone, Copy, Deserialize, PartialEq, Eq, Debug)]
enum SpecSnapshot {
    Svg11Rec20030114,
    Svg11Rec20110816,
    Svg2Cr20181004,
    Svg2EditorsDraft,
}

/// Objective browser-compat facts.
#[derive(Clone, Deserialize)]
struct CompatFacts {
    #[serde(default)]
    deprecated: bool,
    #[serde(default)]
    experimental: bool,
    #[serde(default)]
    standard_track: Option<bool>,
    #[serde(default)]
    baseline: Option<BaselineStatus>,
    #[serde(default)]
    browser_support: Option<BrowserSupport>,
}

/// Web-platform baseline status of a feature.
#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BaselineStatus {
    Widely {
        since: u16,
        qualifier: Option<BaselineQualifier>,
    },
    Newly {
        since: u16,
        qualifier: Option<BaselineQualifier>,
    },
    Limited,
}

/// Inexactness qualifier on a baseline / version date.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BaselineQualifier {
    Before,
    After,
    Approximately,
}

/// Per-browser support across the four tracked engines.
#[derive(Clone, Deserialize)]
struct BrowserSupport {
    #[serde(default)]
    chrome: Option<BrowserVersion>,
    #[serde(default)]
    edge: Option<BrowserVersion>,
    #[serde(default)]
    firefox: Option<BrowserVersion>,
    #[serde(default)]
    safari: Option<BrowserVersion>,
}

/// Baked support detail for one browser.
#[derive(Clone, Deserialize)]
struct BrowserVersion {
    #[serde(default)]
    supported: Option<bool>,
    #[serde(default)]
    partial_implementation: bool,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    alternative_name: Option<String>,
    #[serde(default)]
    flags: Vec<BrowserFlag>,
    #[serde(default)]
    version_added: Option<String>,
    #[serde(default)]
    version_qualifier: Option<BaselineQualifier>,
    #[serde(default)]
    version_removed: Option<String>,
    #[serde(default)]
    version_removed_qualifier: Option<BaselineQualifier>,
}

/// Runtime flag a browser gates a feature behind.
#[derive(Clone, Deserialize)]
struct BrowserFlag {
    name: String,
}

/// The attribute value-space shapes the catalog encodes.
#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AttributeValues {
    Enum {
        values: Vec<String>,
    },
    Transform {
        functions: Vec<String>,
    },
    Color,
    Length,
    Url,
    NumberOrPercentage,
    CssGrammar {
        grammar: String,
        graph: CssGrammarGraph,
    },
    FreeText,
}

#[derive(Clone, Deserialize)]
struct CssGrammarGraph {
    root: u16,
    nodes: Vec<CssGrammarNode>,
    edges: Vec<CssGrammarEdge>,
}

#[derive(Clone, Deserialize)]
struct CssGrammarNode {
    id: u16,
    kind: CssGrammarNodeKind,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CssGrammarNodeKind {
    Root,
    Group,
    Keyword,
    Type,
    Function,
    Operator,
}

#[derive(Clone, Deserialize)]
struct CssGrammarEdge {
    from: u16,
    to: u16,
    kind: CssGrammarEdgeKind,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CssGrammarEdgeKind {
    Contains,
    Next,
}

/// Which elements the catalog says an attribute can appear on.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AttributeApplicability {
    Global,
    Elements { elements: Vec<String> },
    None,
}

/// Derived graph view from `catalog.json`.
#[derive(Deserialize)]
struct CatalogGraph {
    nodes: Vec<CatalogGraphNode>,
    edges: Vec<CatalogGraphEdge>,
}

/// One graph node from `catalog.json`.
#[derive(Deserialize)]
struct CatalogGraphNode {
    id: String,
    kind: CatalogGraphNodeKind,
    name: String,
}

/// Catalog graph node kind.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum CatalogGraphNodeKind {
    Element,
    Attribute,
    ElementCategory,
    AttributeCategory,
    Profile,
    CssProperty,
    ValueGrammar,
    CompatFeature,
}

/// One graph edge from `catalog.json`.
#[derive(Deserialize)]
struct CatalogGraphEdge {
    from: String,
    to: String,
    kind: CatalogGraphEdgeKind,
}

/// Catalog graph edge kind.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum CatalogGraphEdgeKind {
    AllowsChild,
    HasAttribute,
    AppliesTo,
    MemberOf,
    AcceptsGlobalAttributes,
    UsesCssProperty,
    HasValueGrammar,
    OverridesValueInProfile,
    Describes,
    PresentIn,
}

fn main() {
    let Some(out_dir) = env::var_os("OUT_DIR") else {
        panic!("OUT_DIR must be set by cargo");
    };
    let out_dir = PathBuf::from(out_dir);
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");

    println!("cargo::rerun-if-changed=data");
    println!("cargo::rerun-if-changed=build.rs");

    let catalog = generate_catalog(&data_dir);

    let catalog_path = out_dir.join("catalog.rs");
    if let Err(error) = fs::write(&catalog_path, catalog) {
        panic!("write {}: {error}", catalog_path.display());
    }
}

/// Generate the catalog source. Reads `data/catalog.json` when present, else
/// emits an empty catalog so the crate still compiles before a regeneration.
fn generate_catalog(data_dir: &Path) -> String {
    let catalog_path = data_dir.join("catalog.json");
    let Ok(json) = fs::read_to_string(&catalog_path) else {
        return empty_catalog();
    };
    let manifest: CatalogManifest = match serde_json::from_str(&json) {
        Ok(manifest) => manifest,
        Err(error) => panic!("parse {}: {error}", catalog_path.display()),
    };
    assert_eq!(
        manifest.schema_version,
        CATALOG_SCHEMA_VERSION,
        "{} has schema_version {}, expected {}",
        catalog_path.display(),
        manifest.schema_version,
        CATALOG_SCHEMA_VERSION
    );
    let mut core = load_catalog_core(data_dir, &manifest.core);
    let compat = manifest
        .compat
        .as_ref()
        .map(|reference| load_catalog_compat(data_dir, reference));
    let graph = load_catalog_graph(data_dir, &manifest.graph);
    let snapshots = load_catalog_snapshots(data_dir, &manifest.snapshots);
    apply_snapshot_value_overrides(&mut core.attributes, &snapshots);
    let inventories = snapshot_inventories(&snapshots);
    emit_catalog(&core, compat.as_ref(), &graph, &snapshots, &inventories)
}

/// Placeholder catalog used before any spec data has been extracted.
fn empty_catalog() -> String {
    [
        "// Generated by build.rs - empty (no extracted data yet).",
        "pub static ELEMENTS: &[crate::types::ElementDef] = &[];",
        "pub static ATTRIBUTES: &[crate::types::AttributeDef] = &[];",
        "pub static COMPAT_SUBFEATURES: &[crate::types::CompatSubfeature] = &[];",
        "pub static SNAPSHOT_METADATA: &[crate::types::SnapshotMetadata] = &[];",
        "pub static LIFECYCLE_OVERLAYS: &[crate::types::SnapshotLifecycle] = &[];",
        "pub static INVENTORIES: &[crate::inventory::Inventory] = &[];",
        "pub static CATALOG_GRAPH: crate::types::CatalogGraph = crate::types::CatalogGraph { nodes: &[], edges: &[] };",
    ]
    .join("\n")
}

fn load_catalog_core(data_dir: &Path, reference: &CatalogRef) -> CatalogCore {
    let path = resolve_catalog_ref(data_dir, &reference.href);
    println!("cargo::rerun-if-changed={}", path.display());
    let json = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let core: CatalogCore = serde_json::from_str(&json)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
    assert_schema_version(&path, core.schema_version);
    core
}

fn load_catalog_compat(data_dir: &Path, reference: &CatalogRef) -> CompatMetadata {
    let path = resolve_catalog_ref(data_dir, &reference.href);
    println!("cargo::rerun-if-changed={}", path.display());
    let json = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let compat: CompatMetadata = serde_json::from_str(&json)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
    assert_schema_version(&path, compat.schema_version);
    compat
}

fn load_catalog_graph(data_dir: &Path, reference: &CatalogRef) -> CatalogGraph {
    let path = resolve_catalog_ref(data_dir, &reference.href);
    println!("cargo::rerun-if-changed={}", path.display());
    let json = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let document: CatalogGraphDocument = serde_json::from_str(&json)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
    assert_schema_version(&path, document.schema_version);
    document.graph
}

fn load_catalog_snapshots(data_dir: &Path, snapshots: &[SnapshotRef]) -> Vec<SnapshotDocument> {
    let mut documents = Vec::new();
    for snapshot in snapshots {
        let path = resolve_catalog_ref(data_dir, &snapshot.href);
        println!("cargo::rerun-if-changed={}", path.display());
        let json = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let document: SnapshotDocument = serde_json::from_str(&json)
            .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
        assert_schema_version(&path, document.schema_version);
        assert_eq!(
            document.profile,
            snapshot.profile,
            "{} profile {:?} does not match catalog ref {:?}",
            path.display(),
            document.profile,
            snapshot.profile
        );
        documents.push(document);
    }
    documents
}

fn snapshot_inventories(snapshots: &[SnapshotDocument]) -> Vec<Inventory> {
    let mut inventories = Vec::new();
    for snapshot in snapshots {
        inventories.push(Inventory {
            profile: snapshot.profile,
            elements: snapshot.inventory.elements.clone(),
        });
    }
    inventories
}

fn apply_snapshot_value_overrides(attributes: &mut [Attribute], snapshots: &[SnapshotDocument]) {
    for snapshot in snapshots {
        for override_ in &snapshot.value_overrides {
            let Some(attribute) = attributes
                .iter_mut()
                .find(|attribute| attribute.name == override_.attribute)
            else {
                panic!(
                    "snapshot {:?} value override references unknown attribute {}",
                    snapshot.profile, override_.attribute
                );
            };
            attribute.value_overrides.push(AttributeValueOverride {
                profile: snapshot.profile,
                values: override_.values.clone(),
            });
        }
    }
}

fn assert_schema_version(path: &Path, schema_version: u16) {
    assert_eq!(
        schema_version,
        CATALOG_SCHEMA_VERSION,
        "{} has schema_version {}, expected {}",
        path.display(),
        schema_version,
        CATALOG_SCHEMA_VERSION
    );
}

fn resolve_catalog_ref(data_dir: &Path, href: &str) -> PathBuf {
    let relative = Path::new(href);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        panic!("catalog ref must be a clean relative path: {href}");
    }

    let path = data_dir.join(relative);
    let canonical_data_dir = data_dir
        .canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", data_dir.display()));
    let canonical_path = path
        .canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", path.display()));
    assert!(
        canonical_path.starts_with(&canonical_data_dir),
        "catalog ref escaped data directory: {href}"
    );
    canonical_path
}

/// Emit the catalog as Rust source: static `ElementDef` and `AttributeDef`
/// arrays, plus the still-empty snapshot array (populated by later phases).
fn emit_catalog(
    core: &CatalogCore,
    compat: Option<&CompatMetadata>,
    graph: &CatalogGraph,
    snapshots: &[SnapshotDocument],
    inventories: &[Inventory],
) -> String {
    let mut out = String::from("// Generated by build.rs from data/catalog.json.\n");
    out.push_str("pub static ELEMENTS: &[crate::types::ElementDef] = &[\n");
    for element in &core.elements {
        emit_element(&mut out, element);
    }
    out.push_str("];\n");
    out.push_str("pub static ATTRIBUTES: &[crate::types::AttributeDef] = &[\n");
    for attribute in &core.attributes {
        emit_attribute(&mut out, attribute);
    }
    out.push_str("];\n");
    out.push_str("pub static COMPAT_SUBFEATURES: &[crate::types::CompatSubfeature] = &[\n");
    if let Some(compat) = compat {
        for subfeature in &compat.unmodeled_features {
            emit_compat_subfeature(&mut out, subfeature);
        }
    }
    out.push_str("];\n");
    out.push_str("pub static SNAPSHOT_METADATA: &[crate::types::SnapshotMetadata] = &[\n");
    for snapshot in snapshots {
        emit_snapshot_metadata(&mut out, snapshot);
    }
    out.push_str("];\n");
    out.push_str("pub static LIFECYCLE_OVERLAYS: &[crate::types::SnapshotLifecycle] = &[\n");
    for snapshot in snapshots {
        emit_snapshot_lifecycle(&mut out, snapshot);
    }
    out.push_str("];\n");
    out.push_str("pub static INVENTORIES: &[crate::inventory::Inventory] = &[\n");
    for inventory in inventories {
        emit_inventory(&mut out, inventory);
    }
    out.push_str("];\n");
    let _ = writeln!(
        out,
        "pub static CATALOG_GRAPH: crate::types::CatalogGraph = {};",
        emit_catalog_graph(graph)
    );
    out
}

/// Append one generated snapshot metadata entry.
fn emit_snapshot_metadata(out: &mut String, snapshot: &SnapshotDocument) {
    let _ = writeln!(
        out,
        "    crate::types::SnapshotMetadata {{ snapshot: {}, aliases: &[{}] }},",
        emit_spec_snapshot(snapshot.profile),
        quote_list(&snapshot.aliases),
    );
}

/// Append one generated snapshot lifecycle overlay.
fn emit_snapshot_lifecycle(out: &mut String, snapshot: &SnapshotDocument) {
    let elements = snapshot
        .lifecycle
        .elements
        .iter()
        .map(emit_lifecycle_entry)
        .collect::<Vec<_>>()
        .join(", ");
    let attributes = snapshot
        .lifecycle
        .attributes
        .iter()
        .map(emit_lifecycle_entry)
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "    crate::types::SnapshotLifecycle {{ snapshot: {}, elements: &[{}], attributes: &[{}] }},",
        emit_spec_snapshot(snapshot.profile),
        elements,
        attributes,
    );
}

fn emit_lifecycle_entry(entry: &LifecycleEntry) -> String {
    let catalog_name = entry
        .catalog_name
        .as_ref()
        .map_or_else(|| "None".to_owned(), |name| format!("Some({name:?})"));
    let known_in = entry
        .known_in
        .iter()
        .map(|snapshot| emit_spec_snapshot(*snapshot))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "crate::types::FeatureLifecycle {{ name: {:?}, catalog_name: {catalog_name}, present: {}, lifecycle: {}, known_in: &[{}] }}",
        entry.name,
        entry.present,
        emit_lifecycle_status(&entry.lifecycle),
        known_in,
    )
}

const fn emit_lifecycle_status(status: &LifecycleStatus) -> &'static str {
    match status {
        LifecycleStatus::Stable => "crate::types::SpecLifecycle::Stable",
        LifecycleStatus::Experimental => "crate::types::SpecLifecycle::Experimental",
        LifecycleStatus::Obsolete | LifecycleStatus::NotYetIntroduced => {
            "crate::types::SpecLifecycle::Obsolete"
        }
    }
}

/// Append one generated edition inventory.
fn emit_inventory(out: &mut String, inventory: &Inventory) {
    let elements = inventory
        .elements
        .iter()
        .map(emit_inventory_element)
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "    crate::inventory::Inventory {{ edition: crate::inventory::EditionId::for_snapshot({}), elements: &[{}] }},",
        emit_spec_snapshot(inventory.profile),
        elements,
    );
}

fn emit_inventory_element(element: &InventoryElement) -> String {
    let attributes = element
        .attributes
        .iter()
        .map(|attribute| format!("crate::inventory::Attribute {{ name: {attribute:?} }}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "crate::inventory::Element {{ name: {:?}, attributes: &[{}] }}",
        element.name, attributes
    )
}

/// Render the derived catalog graph as Rust source.
fn emit_catalog_graph(graph: &CatalogGraph) -> String {
    format!(
        "crate::types::CatalogGraph {{ nodes: &[{}], edges: &[{}] }}",
        graph
            .nodes
            .iter()
            .map(emit_catalog_graph_node)
            .collect::<Vec<_>>()
            .join(", "),
        graph
            .edges
            .iter()
            .map(emit_catalog_graph_edge)
            .collect::<Vec<_>>()
            .join(", "),
    )
}

fn emit_catalog_graph_node(node: &CatalogGraphNode) -> String {
    format!(
        "crate::types::CatalogGraphNode {{ id: {:?}, kind: {}, name: {:?} }}",
        node.id,
        emit_catalog_graph_node_kind(&node.kind),
        node.name,
    )
}

const fn emit_catalog_graph_node_kind(kind: &CatalogGraphNodeKind) -> &'static str {
    match kind {
        CatalogGraphNodeKind::Element => "crate::types::CatalogGraphNodeKind::Element",
        CatalogGraphNodeKind::Attribute => "crate::types::CatalogGraphNodeKind::Attribute",
        CatalogGraphNodeKind::ElementCategory => {
            "crate::types::CatalogGraphNodeKind::ElementCategory"
        }
        CatalogGraphNodeKind::AttributeCategory => {
            "crate::types::CatalogGraphNodeKind::AttributeCategory"
        }
        CatalogGraphNodeKind::Profile => "crate::types::CatalogGraphNodeKind::Profile",
        CatalogGraphNodeKind::CssProperty => "crate::types::CatalogGraphNodeKind::CssProperty",
        CatalogGraphNodeKind::ValueGrammar => "crate::types::CatalogGraphNodeKind::ValueGrammar",
        CatalogGraphNodeKind::CompatFeature => "crate::types::CatalogGraphNodeKind::CompatFeature",
    }
}

fn emit_catalog_graph_edge(edge: &CatalogGraphEdge) -> String {
    format!(
        "crate::types::CatalogGraphEdge {{ from: {:?}, to: {:?}, kind: {} }}",
        edge.from,
        edge.to,
        emit_catalog_graph_edge_kind(&edge.kind),
    )
}

const fn emit_catalog_graph_edge_kind(kind: &CatalogGraphEdgeKind) -> &'static str {
    match kind {
        CatalogGraphEdgeKind::AllowsChild => "crate::types::CatalogGraphEdgeKind::AllowsChild",
        CatalogGraphEdgeKind::HasAttribute => "crate::types::CatalogGraphEdgeKind::HasAttribute",
        CatalogGraphEdgeKind::AppliesTo => "crate::types::CatalogGraphEdgeKind::AppliesTo",
        CatalogGraphEdgeKind::MemberOf => "crate::types::CatalogGraphEdgeKind::MemberOf",
        CatalogGraphEdgeKind::AcceptsGlobalAttributes => {
            "crate::types::CatalogGraphEdgeKind::AcceptsGlobalAttributes"
        }
        CatalogGraphEdgeKind::UsesCssProperty => {
            "crate::types::CatalogGraphEdgeKind::UsesCssProperty"
        }
        CatalogGraphEdgeKind::HasValueGrammar => {
            "crate::types::CatalogGraphEdgeKind::HasValueGrammar"
        }
        CatalogGraphEdgeKind::OverridesValueInProfile => {
            "crate::types::CatalogGraphEdgeKind::OverridesValueInProfile"
        }
        CatalogGraphEdgeKind::Describes => "crate::types::CatalogGraphEdgeKind::Describes",
        CatalogGraphEdgeKind::PresentIn => "crate::types::CatalogGraphEdgeKind::PresentIn",
    }
}

/// Append one `ElementDef` literal.
fn emit_element(out: &mut String, element: &Element) {
    let description = element.description.as_deref().unwrap_or_default();
    let mdn_url = element.mdn_url.as_deref().unwrap_or_default();
    let spec_url = element
        .spec_url
        .as_ref()
        .map_or_else(|| "None".to_owned(), |url| format!("Some({url:?})"));
    let baseline = emit_baseline(element.baseline.as_ref());
    let browser_support = emit_browser_support(element.browser_support.as_ref());
    let content_model = match &element.content_model {
        ContentModel::ChildrenSet { elements } => {
            format!(
                "crate::types::ContentModel::ChildrenSet(&[{}])",
                quote_list(elements)
            )
        }
        ContentModel::AnySvg => "crate::types::ContentModel::AnySvg".to_owned(),
        ContentModel::Foreign => "crate::types::ContentModel::Foreign".to_owned(),
        ContentModel::Text => "crate::types::ContentModel::Text".to_owned(),
    };
    let _ = writeln!(
        out,
        "    crate::types::ElementDef {{ name: {:?}, description: {description:?}, mdn_url: {mdn_url:?}, \
         spec_url: {spec_url}, deprecated: {}, experimental: {}, standard_track: {}, baseline: {baseline}, \
         browser_support: {browser_support}, content_model: {content_model}, \
         attrs: &[{}], global_attrs: {} }},",
        element.name,
        element.deprecated,
        element.experimental,
        emit_option_bool(element.standard_track),
        quote_list(&element.attrs),
        element.global_attrs,
    );
}

/// Append one `AttributeDef` literal.
fn emit_attribute(out: &mut String, attribute: &Attribute) {
    let description = attribute.description.as_deref().unwrap_or_default();
    let mdn_url = attribute.mdn_url.as_deref().unwrap_or_default();
    let spec_url = attribute
        .spec_url
        .as_ref()
        .map_or_else(|| "None".to_owned(), |url| format!("Some({url:?})"));
    let presentation_attribute = attribute
        .presentation
        .as_ref()
        .map_or_else(|| "None".to_owned(), |name| format!("Some({name:?})"));
    let base_facts = CompatFacts {
        deprecated: attribute.deprecated,
        experimental: attribute.experimental,
        standard_track: attribute.standard_track,
        baseline: attribute.baseline.clone(),
        browser_support: attribute.browser_support.clone(),
    };
    let baseline = emit_baseline(base_facts.baseline.as_ref());
    let browser_support = emit_browser_support(base_facts.browser_support.as_ref());
    let element_compat = emit_attribute_element_compat(&attribute.element_compat);
    let value_overrides = emit_attribute_value_overrides(&attribute.value_overrides);
    let values = emit_attribute_values(&attribute.values);
    let applicability = emit_attribute_applicability(&attribute.applicability);
    let _ = writeln!(
        out,
        "    crate::types::AttributeDef {{ name: {:?}, description: {description:?}, mdn_url: {mdn_url:?}, \
         spec_url: {spec_url}, deprecated: {}, experimental: {}, standard_track: {}, animatable: {}, \
         presentation_attribute: {presentation_attribute}, baseline: {baseline}, browser_support: {browser_support}, \
         element_compat: {element_compat}, values: {values}, value_overrides: {value_overrides}, applicability: {applicability} }},",
        attribute.name,
        attribute.deprecated,
        attribute.experimental,
        emit_option_bool(attribute.standard_track),
        attribute.animatable,
    );
}

fn emit_attribute_value_overrides(overrides: &[AttributeValueOverride]) -> String {
    if overrides.is_empty() {
        return "&[]".to_owned();
    }
    let entries = overrides
        .iter()
        .map(|override_| {
            format!(
                "({}, {})",
                emit_spec_snapshot(override_.profile),
                emit_attribute_values(&override_.values)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("&[{entries}]")
}

const fn emit_spec_snapshot(snapshot: SpecSnapshot) -> &'static str {
    match snapshot {
        SpecSnapshot::Svg11Rec20030114 => "crate::types::SpecSnapshotId::Svg11Rec20030114",
        SpecSnapshot::Svg11Rec20110816 => "crate::types::SpecSnapshotId::Svg11Rec20110816",
        SpecSnapshot::Svg2Cr20181004 => "crate::types::SpecSnapshotId::Svg2Cr20181004",
        SpecSnapshot::Svg2EditorsDraft => "crate::types::SpecSnapshotId::Svg2EditorsDraft",
    }
}

fn emit_attribute_element_compat(overrides: &[AttributeElementCompat]) -> String {
    if overrides.is_empty() {
        return "&[]".to_owned();
    }
    let entries = overrides
        .iter()
        .map(|override_| {
            format!(
                "crate::types::AttributeElementCompat {{ element: {:?}, facts: {} }}",
                override_.element,
                emit_compat_facts(&override_.facts)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("&[{entries}]")
}

fn emit_compat_facts(facts: &CompatFacts) -> String {
    format!(
        "crate::types::CompatFacts {{ deprecated: {}, experimental: {}, standard_track: {}, baseline: {}, browser_support: {} }}",
        facts.deprecated,
        facts.experimental,
        emit_option_bool(facts.standard_track),
        emit_baseline(facts.baseline.as_ref()),
        emit_browser_support(facts.browser_support.as_ref()),
    )
}

fn emit_compat_subfeature(out: &mut String, subfeature: &CompatSubfeature) {
    let kind = match subfeature.kind {
        CompatSubfeatureKind::Behavior => "crate::types::CompatSubfeatureKind::Behavior",
        CompatSubfeatureKind::LegacyXlinkAlias => {
            "crate::types::CompatSubfeatureKind::LegacyXlinkAlias"
        }
    };
    let facts = emit_compat_facts(&subfeature.facts);
    let _ = writeln!(
        out,
        "    crate::types::CompatSubfeature {{ compat_key: {:?}, kind: {kind}, element: {:?}, name: {:?}, facts: {facts} }},",
        subfeature.compat_key, subfeature.element, subfeature.name,
    );
}

/// Render a baseline literal.
fn emit_baseline(baseline: Option<&BaselineStatus>) -> String {
    match baseline {
        Some(BaselineStatus::Widely { since, qualifier }) => format!(
            "Some(crate::types::BaselineStatus::Widely {{ since: {since}, qualifier: {} }})",
            emit_baseline_qualifier(qualifier.as_ref())
        ),
        Some(BaselineStatus::Newly { since, qualifier }) => format!(
            "Some(crate::types::BaselineStatus::Newly {{ since: {since}, qualifier: {} }})",
            emit_baseline_qualifier(qualifier.as_ref())
        ),
        Some(BaselineStatus::Limited) => "Some(crate::types::BaselineStatus::Limited)".to_owned(),
        None => "None".to_owned(),
    }
}

/// Render a baseline/version qualifier literal.
fn emit_baseline_qualifier(qualifier: Option<&BaselineQualifier>) -> String {
    match qualifier {
        Some(BaselineQualifier::Before) => {
            "Some(crate::types::BaselineQualifier::Before)".to_owned()
        }
        Some(BaselineQualifier::After) => "Some(crate::types::BaselineQualifier::After)".to_owned(),
        Some(BaselineQualifier::Approximately) => {
            "Some(crate::types::BaselineQualifier::Approximately)".to_owned()
        }
        None => "None".to_owned(),
    }
}

/// Render per-browser support as a Rust literal.
fn emit_browser_support(support: Option<&BrowserSupport>) -> String {
    let Some(support) = support else {
        return "None".to_owned();
    };
    format!(
        "Some(crate::types::BrowserSupport {{ chrome: {}, edge: {}, firefox: {}, safari: {} }})",
        emit_browser_version(support.chrome.as_ref()),
        emit_browser_version(support.edge.as_ref()),
        emit_browser_version(support.firefox.as_ref()),
        emit_browser_version(support.safari.as_ref()),
    )
}

/// Render one browser support record as a Rust literal.
fn emit_browser_version(version: Option<&BrowserVersion>) -> String {
    let Some(version) = version else {
        return "None".to_owned();
    };
    format!(
        "Some(crate::types::BrowserVersion {{ supported: {}, partial_implementation: {}, notes: &[{}], \
         prefix: {}, alternative_name: {}, flags: &[{}], version_added: {}, version_qualifier: {}, \
         version_removed: {}, version_removed_qualifier: {} }})",
        emit_option_bool(version.supported),
        version.partial_implementation,
        quote_list(&version.notes),
        emit_option_str(version.prefix.as_deref()),
        emit_option_str(version.alternative_name.as_deref()),
        emit_browser_flags(&version.flags),
        emit_option_str(version.version_added.as_deref()),
        emit_baseline_qualifier(version.version_qualifier.as_ref()),
        emit_option_str(version.version_removed.as_deref()),
        emit_baseline_qualifier(version.version_removed_qualifier.as_ref()),
    )
}

fn emit_browser_flags(flags: &[BrowserFlag]) -> String {
    flags
        .iter()
        .map(|flag| format!("crate::types::BrowserFlag {{ name: {:?} }}", flag.name))
        .collect::<Vec<_>>()
        .join(", ")
}

const fn emit_option_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "Some(true)",
        Some(false) => "Some(false)",
        None => "None",
    }
}

fn emit_option_str(value: Option<&str>) -> String {
    value.map_or_else(|| "None".to_owned(), |value| format!("Some({value:?})"))
}

/// Render an attribute value-space literal.
fn emit_attribute_values(values: &AttributeValues) -> String {
    match values {
        AttributeValues::Enum { values } => {
            format!(
                "crate::types::AttributeValues::Enum(&[{}])",
                quote_list(values)
            )
        }
        AttributeValues::Transform { functions } => {
            format!(
                "crate::types::AttributeValues::Transform(&[{}])",
                quote_list(functions)
            )
        }
        AttributeValues::Color => "crate::types::AttributeValues::Color".to_owned(),
        AttributeValues::Length => "crate::types::AttributeValues::Length".to_owned(),
        AttributeValues::Url => "crate::types::AttributeValues::Url".to_owned(),
        AttributeValues::NumberOrPercentage => {
            "crate::types::AttributeValues::NumberOrPercentage".to_owned()
        }
        AttributeValues::CssGrammar { grammar, graph } => {
            format!(
                "crate::types::AttributeValues::CssGrammar {{ grammar: {grammar:?}, graph: {} }}",
                emit_css_grammar_graph(graph)
            )
        }
        AttributeValues::FreeText => "crate::types::AttributeValues::FreeText".to_owned(),
    }
}

fn emit_css_grammar_graph(graph: &CssGrammarGraph) -> String {
    format!(
        "crate::types::CssGrammarGraph {{ root: {}, nodes: &[{}], edges: &[{}] }}",
        graph.root,
        graph
            .nodes
            .iter()
            .map(emit_css_grammar_node)
            .collect::<Vec<_>>()
            .join(", "),
        graph
            .edges
            .iter()
            .map(emit_css_grammar_edge)
            .collect::<Vec<_>>()
            .join(", "),
    )
}

fn emit_css_grammar_node(node: &CssGrammarNode) -> String {
    format!(
        "crate::types::CssGrammarNode {{ id: {}, kind: {}, text: {} }}",
        node.id,
        emit_css_grammar_node_kind(&node.kind),
        emit_option_str(node.text.as_deref())
    )
}

const fn emit_css_grammar_node_kind(kind: &CssGrammarNodeKind) -> &'static str {
    match kind {
        CssGrammarNodeKind::Root => "crate::types::CssGrammarNodeKind::Root",
        CssGrammarNodeKind::Group => "crate::types::CssGrammarNodeKind::Group",
        CssGrammarNodeKind::Keyword => "crate::types::CssGrammarNodeKind::Keyword",
        CssGrammarNodeKind::Type => "crate::types::CssGrammarNodeKind::Type",
        CssGrammarNodeKind::Function => "crate::types::CssGrammarNodeKind::Function",
        CssGrammarNodeKind::Operator => "crate::types::CssGrammarNodeKind::Operator",
    }
}

fn emit_css_grammar_edge(edge: &CssGrammarEdge) -> String {
    format!(
        "crate::types::CssGrammarEdge {{ from: {}, to: {}, kind: {} }}",
        edge.from,
        edge.to,
        emit_css_grammar_edge_kind(&edge.kind)
    )
}

const fn emit_css_grammar_edge_kind(kind: &CssGrammarEdgeKind) -> &'static str {
    match kind {
        CssGrammarEdgeKind::Contains => "crate::types::CssGrammarEdgeKind::Contains",
        CssGrammarEdgeKind::Next => "crate::types::CssGrammarEdgeKind::Next",
    }
}

/// Render an attribute applicability literal.
fn emit_attribute_applicability(applicability: &AttributeApplicability) -> String {
    match applicability {
        AttributeApplicability::Global => "crate::types::AttributeApplicability::Global".to_owned(),
        AttributeApplicability::Elements { elements } => {
            format!(
                "crate::types::AttributeApplicability::Elements(&[{}])",
                quote_list(elements)
            )
        }
        AttributeApplicability::None => "crate::types::AttributeApplicability::None".to_owned(),
    }
}

/// Render a slice of strings as comma-separated Rust string literals.
fn quote_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("{item:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}
