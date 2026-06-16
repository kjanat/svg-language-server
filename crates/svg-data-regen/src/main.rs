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

mod chapter;
mod discover;
mod extract;
mod fetch;
mod provenance;

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
    // The element to print as a JSON sample. Defaults to the first extracted;
    // override with REGEN_SAMPLE=<element-name> to inspect a specific one.
    let want_sample = std::env::var("REGEN_SAMPLE").ok();
    let mut totals = Totals::default();
    let mut sample: Option<extract::ElementDef> = None;
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
        if sample.is_none() {
            sample = match &want_sample {
                Some(name) => defs.elements.iter().find(|el| &el.name == name).cloned(),
                None => defs.elements.first().cloned(),
            };
        }
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
        println!("\n## sample extracted element (first, as JSON)");
        println!("{}", serde_json::to_string_pretty(element)?);
    }

    report_chapters(provenance, graph)
}

/// Extract and report the chapter/appendix pages: anchor, definition, example,
/// and property-table counts, plus a sample property as JSON.
fn report_chapters(provenance: &Provenance, graph: &PublishGraph) -> Fallible<()> {
    println!("\n## chapter extraction (anchors / dfns / examples / properties)");
    let mut anchors = 0usize;
    let mut dfns = 0usize;
    let mut examples = 0usize;
    let mut properties = 0usize;
    let mut terms = 0usize;
    let mut sample_property: Option<chapter::PropertyValueDef> = None;
    let mut sample_term: Option<chapter::TermDefinition> = None;
    let want_property = std::env::var("REGEN_PROPERTY").ok();
    let want_term = std::env::var("REGEN_TERM").ok();
    let pages = graph.chapters.iter().chain(&graph.appendices);
    for name in pages {
        let path = format!("{PUBLISH_DIR}/{name}.html");
        let html = fetch::raw_file(REPO_SLUG, &provenance.commit_sha, &path)?;
        let extracted = chapter::extract_chapter(name, &html)?;
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
            sample_property = match &want_property {
                Some(want) => extracted
                    .properties
                    .iter()
                    .find(|p| &p.name == want)
                    .cloned(),
                None => extracted.properties.first().cloned(),
            };
        }
        if sample_term.is_none() {
            sample_term = match &want_term {
                Some(want) => extracted
                    .term_definitions
                    .iter()
                    .find(|t| &t.term == want)
                    .cloned(),
                None => extracted.term_definitions.first().cloned(),
            };
        }
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

    Ok(())
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
