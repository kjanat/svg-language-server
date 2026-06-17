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
mod discover;
mod extract;
mod fetch;
mod provenance;
mod schema;

use std::path::{Path, PathBuf};

use std::process::ExitCode;

use discover::PublishGraph;
use provenance::{BaseUrls, Provenance};

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

    println!("\n## definitions extraction (at pinned commit)");
    // REGEN_SAMPLE=<name> prints the element, property, and/or term with that
    // name as a JSON sample (whichever exist). Unset: the first of each kind.
    let want_sample = std::env::var("REGEN_SAMPLE").ok();
    let mut totals = Totals::default();
    let mut sample: Option<extract::ElementDef> = None;
    let mut macros = chapter::MacroIndex::default();
    let mut all_defs: Vec<extract::Definitions> = Vec::new();
    for module in &graph.definitions {
        let path = fetch::resolve_repo_path(PUBLISH_DIR, &module.href);
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
            sample = match &want_sample {
                Some(name) => defs.elements.iter().find(|el| &el.name == name).cloned(),
                None => defs.elements.first().cloned(),
            };
        }
        all_defs.push(defs);
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
    let compat = fetch_and_report_compat()?;

    let built = catalog::build_catalog(
        &all_defs,
        &chapter_report.properties,
        editors_draft,
        &provenance.commit_sha,
        Some(&compat),
    );
    let path = write_catalog(&built)?;
    println!(
        "\n## catalog written: {} elements, {} attributes -> {}",
        built.elements.len(),
        built.attributes.len(),
        path.display()
    );
    Ok(())
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
fn write_catalog(built: &catalog::Catalog) -> Fallible<PathBuf> {
    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../svg-data/data");
    std::fs::create_dir_all(&data_dir)?;
    let path = data_dir.join("catalog.json");
    let mut json = serde_json::to_string_pretty(built)?;
    json.push('\n');
    std::fs::write(&path, json)?;
    std::fs::write(
        data_dir.join(schema::CATALOG_SCHEMA_FILE),
        schema::catalog_schema_json()?,
    )?;
    Ok(path)
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
    })
}

/// Chapter-derived records needed by the committed catalog.
struct ChapterReport {
    /// Property value grammars, used for presentation attribute value spaces.
    properties: Vec<chapter::PropertyValueDef>,
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
