//! `svg-data-regen` - the deterministic regeneration pipeline for `svg-data`.
//!
//! Fetches the canonical SVG specification straight from upstream (`w3c/svgwg`
//! on GitHub, resolved at the default-branch HEAD or an explicit ref) and
//! extracts structured data. Nothing upstream is vendored; only the derived
//! structured output is committed, and permalinks pin to the fetched commit.
//!
//! Run with `cargo run -p svg-data-regen [-- <ref>]`, where `<ref>` is an
//! optional branch name, tag, or commit SHA to pin (default: the repository's
//! resolved default-branch HEAD).
//!
//! This entry point wires the phases together; the work lives in the
//! submodules: [`fetch`] (network), [`discover`] (manifest parse), and
//! [`provenance`] (typed run identity).

mod catalog;
mod chapter;
mod compat;
mod css;
mod discover;
mod extract;
mod fetch;
mod inventory;
mod legacy;
mod paths;
mod provenance;
mod schema;
mod treesitter;
mod util;

use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

use std::process::ExitCode;

use discover::PublishGraph;
use provenance::{BaseUrls, Provenance};
use serde::Serialize;

/// Upstream repository the catalog derives from.
const REPO_SLUG: &str = "w3c/svgwg";
/// Canonical browse URL for the repository (recorded in provenance).
const REPO_URL: &str = "https://github.com/w3c/svgwg";
/// Path within the repo to the SVG 2 publication manifest.
const PUBLISH_PATH: &str = "master/publish.xml";
/// Directory the manifest's relative hrefs resolve against.
const PUBLISH_DIR: &str = "master";

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("svg-data-regen: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Fallible<()> {
    // Resolve the ref to pin: an explicit CLI arg, else the default branch.
    let reference = match std::env::args().nth(1) {
        Some(arg) => arg,
        None => fetch::default_branch(REPO_SLUG)?,
    };
    let head = fetch::resolve_head(REPO_SLUG, &reference)?;

    // Fetch and parse the publication manifest at the pinned commit.
    let publish_xml = fetch::raw_file(REPO_SLUG, &head.sha, PUBLISH_PATH)?;
    let graph = discover::parse_publish(&publish_xml)?;

    let provenance = Provenance {
        repository: REPO_URL.to_owned(),
        reference,
        commit_sha: head.sha,
        commit_date: head.committed_date,
        maturity: graph.maturity.clone(),
        base_urls: BaseUrls::from_links(&graph.versions),
    };

    report(&provenance, &graph)
}

/// Print the resolved provenance and discovered graph, then prove the
/// canonical-fetch model by actually fetching every definitions module and one
/// chapter at the pinned commit (verifying href resolution end to end).
fn report(provenance: &Provenance, graph: &PublishGraph) -> Fallible<()> {
    println!("# svg-data-regen - canonical fetch + provenance");
    println!("repository:  {}", provenance.repository);
    println!("ref:         {}", provenance.reference);
    println!("commit:      {}", provenance.commit_sha);
    println!("commit date: {}", provenance.commit_date);
    println!("maturity:    {}", provenance.maturity);

    println!("\n## permalink base URLs (publish.xml <versions>)");
    print_base(
        "editors-draft (cvs)",
        provenance.base_urls.editors_draft.as_deref(),
    );
    print_base("dated (this)       ", provenance.base_urls.dated.as_deref());
    print_base(
        "latest             ",
        provenance.base_urls.latest.as_deref(),
    );
    print_base(
        "latest-rec         ",
        provenance.base_urls.latest_rec.as_deref(),
    );
    println!(
        "  ({} version links, {} index references discovered)",
        graph.versions.len(),
        graph.references.len()
    );

    println!(
        "\n## input graph: {} definitions, {} chapters, {} appendices",
        graph.definitions.len(),
        graph.chapters.len(),
        graph.appendices.len()
    );

    // REGEN_SAMPLE=<name> prints the element, property, and/or term with that
    // name as a JSON sample (whichever exist). Unset: the first of each kind.
    let want_sample = std::env::var("REGEN_SAMPLE").ok();
    let extracted = extract_definitions_modules(provenance, graph, want_sample.as_deref())?;
    let DefinitionsExtraction {
        modules: all_defs,
        macros,
        sample,
    } = extracted;

    if let Some(element) = &sample {
        println!("\n## sample extracted element (as JSON)");
        println!("{}", serde_json::to_string_pretty(element)?);
    }

    let editors_draft = provenance
        .base_urls
        .editors_draft
        .as_deref()
        .unwrap_or_default();
    let chapter_report = report_chapters(provenance, graph, &macros, want_sample.as_deref())?;
    let external_properties = fetch_and_report_external_property_defs(&all_defs, editors_draft)?;
    let compat = fetch_and_report_compat()?;
    let legacy = fetch_and_report_legacy_value_overrides()?;
    let inventories = fetch_and_report_inventories(&all_defs)?;
    let grammar_inputs =
        fetch_and_report_grammar_projection_inputs(REPO_SLUG, &provenance.commit_sha)?;
    let chapter_report = chapter_report.with_external_properties(external_properties.properties);

    let built = build_committed_catalog(
        &all_defs,
        &chapter_report,
        editors_draft,
        &provenance.commit_sha,
        Some(&compat),
        catalog::CatalogLegacyInputs {
            sources: &legacy.sources,
            value_overrides: &legacy.attributes,
            inventories: &inventories,
            grammar_inputs: Some(&grammar_inputs),
        },
    )?;
    let path = write_catalog(&built, &inventories)?;
    print_catalog_written(&built, &path);
    Ok(())
}

struct DefinitionsExtraction {
    modules: Vec<extract::Definitions>,
    macros: chapter::MacroIndex,
    sample: Option<extract::ElementDef>,
}

fn extract_definitions_modules(
    provenance: &Provenance,
    graph: &PublishGraph,
    want_sample: Option<&str>,
) -> Fallible<DefinitionsExtraction> {
    println!("\n## definitions extraction (at pinned commit)");
    let mut totals = Totals::default();
    let mut sample = None;
    let mut macros = chapter::MacroIndex::default();
    let mut modules = Vec::new();
    for module in &graph.definitions {
        let path = fetch::resolve_repo_path(PUBLISH_DIR, &module.href)?;
        let xml = fetch::raw_file(REPO_SLUG, &provenance.commit_sha, &path)?;
        let defs = extract::extract_definitions(&xml, module.base.clone())?;
        println!(
            "  {path:<44} {:>3} el  {:>3} attr  {:>3} prop  {:>2} elcat  {:>2} attrcat",
            defs.elements.len(),
            defs.global_attributes.len(),
            defs.properties.len(),
            defs.element_categories.len(),
            defs.attribute_categories.len(),
        );
        totals.add(&defs);
        index_categories(&defs, &mut macros);
        if sample.is_none() {
            sample = match want_sample {
                Some(name) => defs.elements.iter().find(|el| el.name == name).cloned(),
                None => defs.elements.first().cloned(),
            };
        }
        modules.push(defs);
    }
    println!(
        "  {:-<44} {:>3} el  {:>3} attr  {:>3} prop  {:>2} elcat  {:>2} attrcat",
        "TOTAL ",
        totals.elements,
        totals.global_attributes,
        totals.properties,
        totals.element_categories,
        totals.attribute_categories,
    );
    Ok(DefinitionsExtraction {
        modules,
        macros,
        sample,
    })
}

fn print_catalog_written(built: &catalog::Catalog, path: &Path) {
    println!(
        "\n## catalog written: {} elements, {} attributes, {} snapshots -> {}",
        built.elements.len(),
        built.attributes.len(),
        built.snapshots.len(),
        display_path(path)
    );
}

fn build_committed_catalog(
    definitions: &[extract::Definitions],
    chapter_report: &ChapterReport,
    editors_draft_base: &str,
    commit: &str,
    compat: Option<&compat::CompatCatalog>,
    legacy: catalog::CatalogLegacyInputs<'_>,
) -> Fallible<catalog::Catalog> {
    catalog::build_catalog(
        definitions,
        &chapter_report.properties,
        &chapter_report.descriptions,
        editors_draft_base,
        commit,
        compat,
        legacy,
    )
}

fn fetch_and_report_grammar_projection_inputs(
    repo_slug: &str,
    commit_sha: &str,
) -> Fallible<treesitter::GrammarProjectionInputs> {
    println!("\n## tree-sitter grammar projection inputs");
    let inputs = treesitter::fetch_grammar_projection_inputs(repo_slug, commit_sha)?;
    println!("  @webref/css extracted");
    println!(
        "  paths.html -> {} path command letter(s), d property = `{}`",
        inputs.paths.path_command_letters.len(),
        inputs.paths.path_data_property
    );
    Ok(inputs)
}

fn fetch_and_report_external_property_defs(
    modules: &[extract::Definitions],
    editors_draft_base: &str,
) -> Fallible<css::ExternalPropertyDefinitions> {
    println!("\n## external property definition extraction");
    let extracted = css::fetch_external_property_defs(modules, editors_draft_base)?;
    for page in &extracted.pages {
        println!(
            "  {} -> {}/{} property definition(s)",
            page.url, page.matched, page.requested
        );
    }
    println!(
        "  {:-<44} {:>3}/{} property definition(s)",
        "TOTAL ",
        extracted.properties.len(),
        extracted.requested_count,
    );
    Ok(extracted)
}

fn fetch_and_report_legacy_value_overrides() -> Fallible<legacy::LegacyValueOverrides> {
    println!("\n## legacy profile value overrides");
    let mut merged = legacy::LegacyValueOverrides::default();
    for source in legacy::SVG11_PROPERTY_INDEXES {
        let html = fetch::url_text(source.url, "text/html")?;
        let extracted = legacy::extract_svg11_property_index(source, &html)?;
        let count = extracted.attributes.values().map(Vec::len).sum::<usize>();
        println!("  {} -> {} value override(s)", source.url, count);
        legacy::merge_value_overrides(&mut merged, extracted);
    }
    Ok(merged)
}

fn fetch_and_report_inventories(
    definitions: &[extract::Definitions],
) -> Fallible<Vec<catalog::CatalogInventory>> {
    println!("\n## edition inventories");
    let mut inventories = Vec::new();
    for source in inventory::SNAPSHOT_INDEX_SOURCES {
        let element_html = fetch::url_text(source.element_index_url, "text/html")?;
        let attribute_html = fetch::url_text(source.attribute_index_url, "text/html")?;
        let extracted = inventory::extract_index_inventory(source, &element_html, &attribute_html)?;
        println!(
            "  {} -> {} elements, {} attributes",
            source.name,
            extracted.elements.len(),
            extracted.attributes.len()
        );
        inventories.push(extracted);
    }
    let editors_draft = inventory::inventory_from_definitions(
        catalog::CatalogSpecSnapshotId::Svg2EditorsDraft,
        definitions,
    );
    println!(
        "  SVG 2 Editor's Draft definitions -> {} elements, {} attributes",
        editors_draft.elements.len(),
        editors_draft.attributes.len()
    );
    inventories.push(editors_draft);
    Ok(inventories)
}

fn fetch_and_report_compat() -> Fallible<compat::CompatCatalog> {
    println!("\n## browser compat extraction");
    let compat = compat::fetch_compat_catalog()?;
    println!(
        "  {}@{}",
        compat.provenance.browser_compat_data.name, compat.provenance.browser_compat_data.version
    );
    println!(
        "  {}@{}",
        compat.provenance.web_features.name, compat.provenance.web_features.version
    );
    Ok(compat)
}

/// Write the derived catalog as deterministic pretty JSON into `svg-data`'s
/// `data/` directory, returning the path written.
fn write_catalog(
    built: &catalog::Catalog,
    inventories: &[catalog::CatalogInventory],
) -> Fallible<PathBuf> {
    let data_dir = catalog_data_dir()?;
    write_catalog_components(&data_dir, built)?;
    write_catalog_snapshots(&data_dir, built, inventories)?;
    let path = data_dir.join("catalog.json");
    write_json(&path, &built.manifest())?;
    for schema in schema::catalog_schema_documents()? {
        std::fs::write(data_dir.join(schema.file_name), schema.json)?;
    }
    Ok(path)
}

fn write_catalog_components(data_dir: &Path, built: &catalog::Catalog) -> Fallible<()> {
    write_json(
        &resolve_data_ref_for_write(data_dir, catalog::CATALOG_CORE_HREF)?,
        &built.core_document(),
    )?;
    if let Some(compat) = built.compat_document() {
        write_json(
            &resolve_data_ref_for_write(data_dir, catalog::CATALOG_COMPAT_HREF)?,
            &compat,
        )?;
    }
    write_json(
        &resolve_data_ref_for_write(data_dir, catalog::CATALOG_GRAPH_HREF)?,
        &built.graph_document(),
    )?;
    write_json(
        &resolve_data_ref_for_write(data_dir, catalog::CATALOG_TREE_SITTER_HREF)?,
        built.tree_sitter_document(),
    )?;
    Ok(())
}

fn write_catalog_snapshots(
    data_dir: &Path,
    built: &catalog::Catalog,
    inventories: &[catalog::CatalogInventory],
) -> Fallible<()> {
    let snapshots_dir = data_dir.join("snapshots");
    std::fs::create_dir_all(&snapshots_dir)?;
    let mut expected = BTreeSet::new();

    for inventory in inventories {
        let href = catalog::catalog_snapshot_href(inventory.profile);
        let path = resolve_data_ref_for_write(data_dir, href)?;
        let snapshot = catalog::CatalogSnapshot::from_inventory(
            inventory,
            inventories,
            &built.attributes,
            &built.legacy_sources,
        );
        write_json(&path, &snapshot)?;
        expected.insert(path.canonicalize()?);
    }

    for entry in std::fs::read_dir(&snapshots_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
            let canonical = path.canonicalize()?;
            if !expected.contains(&canonical) {
                std::fs::remove_file(path)?;
            }
        }
    }

    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Fallible<()> {
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    std::fs::write(path, json)?;
    Ok(())
}

fn resolve_data_ref_for_write(data_dir: &Path, href: &str) -> Fallible<PathBuf> {
    let relative = Path::new(href);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("catalog data ref must be a clean relative path: {href}").into());
    }

    let path = data_dir.join(relative);
    let Some(parent) = path.parent() else {
        return Err(format!("catalog data ref has no parent: {href}").into());
    };
    std::fs::create_dir_all(parent)?;
    let canonical_data_dir = data_dir.canonicalize()?;
    let canonical_parent = parent.canonicalize()?;
    if !canonical_parent.starts_with(&canonical_data_dir) {
        return Err(format!("catalog data ref escaped data directory: {href}").into());
    }

    Ok(path)
}

fn catalog_data_dir() -> Result<PathBuf, std::io::Error> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).canonicalize()?;
    let workspace_crates_dir = manifest_dir
        .parent()
        .ok_or_else(|| std::io::Error::other("svg-data-regen manifest has no parent"))?;
    let svg_data_dir = workspace_crates_dir.join("svg-data").canonicalize()?;
    let data_dir = svg_data_dir.join("data");
    std::fs::create_dir_all(&data_dir)?;
    let data_dir = data_dir.canonicalize()?;

    if !data_dir.starts_with(&svg_data_dir) {
        return Err(std::io::Error::other(format!(
            "catalog data directory escaped svg-data crate: {}",
            data_dir.display()
        )));
    }
    if data_dir.file_name().and_then(|name| name.to_str()) != Some("data") {
        return Err(std::io::Error::other(format!(
            "catalog data directory must end in `data`: {}",
            data_dir.display()
        )));
    }

    Ok(data_dir)
}

fn display_path(path: &Path) -> String {
    if let Ok(cwd) = std::env::current_dir()
        && let Ok(relative) = path.strip_prefix(cwd)
    {
        return relative.display().to_string();
    }
    path.display().to_string()
}

/// Record one module's element/attribute category membership for macro
/// expansion in chapter descriptions.
fn index_categories(defs: &extract::Definitions, macros: &mut chapter::MacroIndex) {
    for category in &defs.element_categories {
        macros
            .element_categories
            .insert(category.name.clone(), category.elements.clone());
    }
    for category in &defs.attribute_categories {
        let names = category
            .attributes
            .iter()
            .map(|a| a.name.clone())
            .chain(category.presentation_attributes.iter().cloned())
            .collect();
        macros
            .attribute_categories
            .insert(category.name.clone(), names);
    }
}

/// Extract and report the chapter/appendix pages: anchor, definition, example,
/// property, and term counts, plus a sample property and term as JSON. `want`
/// (from `REGEN_SAMPLE`) selects which property/term to print by name; when
/// `None`, the first of each is shown.
fn report_chapters(
    provenance: &Provenance,
    graph: &PublishGraph,
    macros: &chapter::MacroIndex,
    want: Option<&str>,
) -> Fallible<ChapterReport> {
    println!("\n## chapter extraction (anchors / dfns / examples / properties)");
    let mut anchors = 0usize;
    let mut dfns = 0usize;
    let mut examples = 0usize;
    let mut properties = 0usize;
    let mut terms = 0usize;
    let mut sample_property: Option<chapter::PropertyValueDef> = None;
    let mut sample_term: Option<chapter::TermDefinition> = None;
    let mut collected_properties = Vec::new();
    let mut collected_descriptions = Vec::new();
    let pages = graph.chapters.iter().chain(&graph.appendices);
    for name in pages {
        let path = format!("{PUBLISH_DIR}/{name}.html");
        let html = fetch::raw_file(REPO_SLUG, &provenance.commit_sha, &path)?;
        let extracted = chapter::extract_chapter(name, &html, macros)?;
        println!(
            "  {name:<12} {:>4} anchors  {:>3} dfns  {:>2} ex  {:>3} props  {:>3} terms",
            extracted.anchors.len(),
            extracted.dfns.len(),
            extracted.examples.len(),
            extracted.properties.len(),
            extracted.term_definitions.len(),
        );
        anchors += extracted.anchors.len();
        dfns += extracted.dfns.len();
        examples += extracted.examples.len();
        properties += extracted.properties.len();
        terms += extracted.term_definitions.len();
        if sample_property.is_none() {
            sample_property = match want {
                Some(name) => extracted
                    .properties
                    .iter()
                    .find(|p| p.name == name)
                    .cloned(),
                None => extracted.properties.first().cloned(),
            };
        }
        if sample_term.is_none() {
            sample_term = match want {
                Some(name) => extracted
                    .term_definitions
                    .iter()
                    .find(|t| t.term == name)
                    .cloned(),
                None => extracted.term_definitions.first().cloned(),
            };
        }
        collected_properties.extend(extracted.properties);
        collected_descriptions.extend(extracted.anchor_descriptions);
    }
    println!(
        "  {:-<12} {anchors:>4} anchors  {dfns:>3} dfns  {examples:>2} ex  {properties:>3} props  {terms:>3} terms ({} pages)",
        "TOTAL ",
        graph.chapters.len() + graph.appendices.len(),
    );

    if let Some(property) = &sample_property {
        println!("\n## sample extracted property (as JSON)");
        println!("{}", serde_json::to_string_pretty(property)?);
    }
    if let Some(term) = &sample_term {
        println!("\n## sample extracted term definition (as JSON)");
        println!("{}", serde_json::to_string_pretty(term)?);
    }

    Ok(ChapterReport {
        properties: collected_properties,
        descriptions: collected_descriptions,
    })
}

/// Chapter-derived records needed by the committed catalog.
struct ChapterReport {
    /// Property value grammars, used for presentation attribute value spaces.
    properties: Vec<chapter::PropertyValueDef>,
    /// Anchor descriptions, used for element/attribute hover prose.
    descriptions: Vec<chapter::AnchorDescription>,
}

impl ChapterReport {
    fn with_external_properties(mut self, properties: Vec<chapter::PropertyValueDef>) -> Self {
        self.properties.extend(properties);
        self
    }
}

/// Running totals across all definitions modules.
#[derive(Default)]
struct Totals {
    elements: usize,
    global_attributes: usize,
    properties: usize,
    element_categories: usize,
    attribute_categories: usize,
}

impl Totals {
    /// Fold one module's extracted counts into the running totals.
    const fn add(&mut self, defs: &extract::Definitions) {
        self.elements += defs.elements.len();
        self.global_attributes += defs.global_attributes.len();
        self.properties += defs.properties.len();
        self.element_categories += defs.element_categories.len();
        self.attribute_categories += defs.attribute_categories.len();
    }
}

/// Print one optional base URL, marking absent aliases explicitly.
fn print_base(label: &str, url: Option<&str>) {
    match url {
        Some(url) => println!("  {label}: {url}"),
        None => println!("  {label}: (absent)"),
    }
}
