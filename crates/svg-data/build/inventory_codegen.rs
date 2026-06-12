//! Codegen for the baked full-spec inventories (`src/inventory.rs`).
//!
//! Emits `static …_INVENTORY: Inventory = …` literals into `OUT_DIR`, which
//! `src/inventory.rs` `include!`s — mirroring the [`edition`](super::edition)
//! baked-index pattern. Two source families feed it, both hermetic (no
//! build-time network):
//!
//! - **SVG 2 Editor's Draft** — derived from the vendored `definitions*.xml`
//!   via [`spec_xml`](super::spec_xml) ([`generate`]).
//! - **SVG 1.1 Recommendations** — derived from each edition's vendored
//!   flat DTD via [`dtd`](super::dtd) ([`generate_svg11`]).
//!
//! This is **additive** — it does not touch the curated catalog codegen in
//! `build.rs`; it exposes the complete, unfiltered spec inventories as separate
//! baked datasets.

use std::{collections::BTreeSet, fmt::Write as _, path::Path};

use super::classification::Classification;
use super::codegen::escape;
use super::dtd::{self, DtdInventory};
use super::spec_xml::{self, SpecInventory, SpecXmlError};
use super::tr_index::CrInventory;

/// Render the `static SPEC_INVENTORY` source for the inventory derived from
/// the vendored ED `master/` directory.
///
/// # Errors
///
/// Returns a [`SpecXmlError`] if any vendored `definitions*.xml` file cannot
/// be read or parsed.
pub fn generate(master: &Path) -> Result<String, SpecXmlError> {
    let inventory = spec_xml::read_inventory(master)?;
    Ok(render_spec_inventory(&inventory))
}

/// Render a `static {static_name}: Inventory = …` source for an SVG 1.1
/// edition, derived from its vendored flat DTD at `dtd_path`.
///
/// `static_name` is the emitted Rust identifier (e.g. `SVG11_REC_20030114`),
/// `doc` is the rustdoc line attached to it.
///
/// # Errors
///
/// Returns an [`std::io::Error`] if the vendored DTD file cannot be read. The
/// parse itself is infallible (a malformed DTD yields a smaller inventory, not
/// an error), so any failure here is purely I/O.
pub fn generate_svg11(dtd_path: &Path, static_name: &str, doc: &str) -> std::io::Result<String> {
    let text = std::fs::read_to_string(dtd_path)?;
    let inventory = dtd::parse(&text);
    Ok(render_dtd_inventory(&inventory, static_name, doc))
}

/// Render a `static {static_name}: Inventory = …` source for the SVG 2
/// Candidate Recommendation, derived from its vendored published index tables
/// (`eltindex.html` + `attindex.html`) rooted at `cr_dir`.
///
/// The CR index pages do **not** carry the `attributecategory` groups the ED
/// `definitions*.xml` does, so every attribute's normalized [`Classification`]
/// set is **empty** — faithful to what the rendered index exposes. The only
/// per-attribute provenance the index supplies is the animatable flag, recorded
/// verbatim as a `raw_categories` marker (`animatable` for animatable
/// attributes, plus a `source=attindex` tag on every attribute) so the
/// provenance is honest about its sparse origin. The merged element scope is
/// emitted as `element_scope`.
///
/// # Errors
///
/// Returns a parse error string if either vendored index page cannot be read or
/// parsed.
pub fn generate_cr(cr_dir: &Path, static_name: &str, doc: &str) -> Result<String, String> {
    let eltindex = std::fs::read_to_string(cr_dir.join("eltindex.html"))
        .map_err(|err| format!("CR eltindex.html: {err}"))?;
    let attindex = std::fs::read_to_string(cr_dir.join("attindex.html"))
        .map_err(|err| format!("CR attindex.html: {err}"))?;
    let elements = super::tr_index::parse_eltindex(&eltindex)?;
    let rows = super::tr_index::parse_attindex(&attindex)?;
    let inventory = super::tr_index::build_inventory(elements, &rows)?;
    Ok(render_cr_inventory(&inventory, static_name, doc))
}

/// Emit a Rust `Classification` constructor expression for one normalized
/// classification.
fn classification_expr(classification: &Classification) -> String {
    match classification {
        Classification::Core => "Classification::Core".to_string(),
        Classification::Presentation => "Classification::Presentation".to_string(),
        Classification::Aria => "Classification::Aria".to_string(),
        Classification::EventHandler => "Classification::EventHandler".to_string(),
        Classification::Xlink => "Classification::Xlink".to_string(),
        Classification::ConditionalProcessing => {
            "Classification::ConditionalProcessing".to_string()
        }
        Classification::Other(category) => format!(
            "Classification::Other {{ category: Cow::Borrowed(\"{}\") }}",
            escape(category)
        ),
    }
}

/// Emit a `Cow::Borrowed(&[…])` slice of borrowed string literals.
fn borrowed_str_slice(values: impl IntoIterator<Item = String>) -> String {
    let mut out = String::from("Cow::Borrowed(&[");
    for (index, value) in values.into_iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "Cow::Borrowed(\"{}\")", escape(&value));
    }
    out.push_str("])");
    out
}

/// One attribute's render-ready facts: classification exprs, raw category
/// provenance strings, and element-scope names — the shape both source
/// families normalize into before emission.
struct RenderAttribute {
    name: String,
    classifications: Vec<String>,
    raw_categories: Vec<String>,
    element_scope: Vec<String>,
}

/// Render a complete `static {static_name}: Inventory = …` literal from
/// already-normalized parts. Shared by both source families so the emitted
/// shape is identical.
fn render_inventory(
    static_name: &str,
    doc: &str,
    elements: &BTreeSet<String>,
    attributes: &[RenderAttribute],
    edges: &BTreeSet<(String, String)>,
) -> String {
    let mut out = String::with_capacity(256 * 1024);
    out.push_str("// @generated by build/inventory_codegen.rs -- do not edit\n\n");
    let _ = writeln!(out, "{doc}");
    let _ = writeln!(out, "pub static {static_name}: Inventory = Inventory {{");

    out.push_str("    elements: Cow::Borrowed(&[\n");
    for name in elements {
        let _ = writeln!(
            out,
            "        Element {{ name: Cow::Borrowed(\"{}\") }},",
            escape(name)
        );
    }
    out.push_str("    ]),\n");

    out.push_str("    attributes: Cow::Borrowed(&[\n");
    for attribute in attributes {
        let classifications = attribute.classifications.join(", ");
        let raw_categories = borrowed_str_slice(attribute.raw_categories.iter().cloned());
        let element_scope = borrowed_str_slice(attribute.element_scope.iter().cloned());
        let _ = writeln!(
            out,
            "        Attribute {{ name: Cow::Borrowed(\"{name}\"), classifications: Cow::Borrowed(&[{classifications}]), raw_categories: {raw_categories}, element_scope: {element_scope} }},",
            name = escape(&attribute.name),
        );
    }
    out.push_str("    ]),\n");

    out.push_str("    edges: Cow::Borrowed(&[\n");
    for (element, attribute) in edges {
        let _ = writeln!(
            out,
            "        Edge {{ element: Cow::Borrowed(\"{}\"), attribute: Cow::Borrowed(\"{}\") }},",
            escape(element),
            escape(attribute)
        );
    }
    out.push_str("    ]),\n");

    out.push_str("};\n");
    out
}

/// Render the SVG 2 ED `SPEC_INVENTORY` from a [`SpecInventory`].
fn render_spec_inventory(inventory: &SpecInventory) -> String {
    let attributes: Vec<RenderAttribute> = inventory
        .attributes
        .iter()
        .map(|name| {
            let facts = inventory.attribute_facts.get(name);
            RenderAttribute {
                name: name.clone(),
                classifications: facts.map_or_else(Vec::new, |facts| {
                    facts
                        .classifications
                        .iter()
                        .map(classification_expr)
                        .collect()
                }),
                raw_categories: facts
                    .map(|facts| facts.raw_categories.iter().cloned().collect())
                    .unwrap_or_default(),
                element_scope: facts
                    .map(|facts| facts.element_scope.iter().cloned().collect())
                    .unwrap_or_default(),
            }
        })
        .collect();
    let edges: BTreeSet<(String, String)> = inventory
        .edges()
        .into_iter()
        .map(|(element, attribute)| (element.to_string(), attribute.to_string()))
        .collect();
    render_inventory(
        "SPEC_INVENTORY",
        "/// The complete SVG 2 Editor's Draft spec inventory, derived from the\n\
         /// vendored `definitions*.xml` at build time. See [`Inventory`].",
        &inventory.elements,
        &attributes,
        &edges,
    )
}

/// Render an SVG 1.1 edition inventory from a [`DtdInventory`].
///
/// Each attribute's `%…attrib;` group provenance is mapped to the normalized
/// [`Classification`] set (de-duplicated) via [`dtd::classify_group`], with the
/// raw group names retained verbatim as `raw_categories`. SVG 1.1 has no
/// element-scoped top-level attribute declarations, so `element_scope` is
/// always empty.
fn render_dtd_inventory(inventory: &DtdInventory, static_name: &str, doc: &str) -> String {
    let attributes: Vec<RenderAttribute> = inventory
        .attribute_names()
        .into_iter()
        .map(|name| {
            let groups = inventory.attribute_groups.get(&name);
            // Normalized classification set, de-duplicated and stably ordered.
            let mut classifications: BTreeSet<Classification> = BTreeSet::new();
            if let Some(groups) = groups {
                for group in groups {
                    classifications.insert(dtd::classify_group(group));
                }
            }
            // Raw provenance: the verbatim `%…attrib;` group names.
            let raw_categories: Vec<String> = groups
                .map(|groups| groups.iter().cloned().collect())
                .unwrap_or_default();
            RenderAttribute {
                name,
                classifications: classifications.iter().map(classification_expr).collect(),
                raw_categories,
                element_scope: Vec::new(),
            }
        })
        .collect();
    render_inventory(
        static_name,
        doc,
        &inventory.elements,
        &attributes,
        &inventory.edges(),
    )
}

/// Render the SVG 2 CR inventory from a [`CrInventory`].
///
/// Classifications are intentionally empty (the published index carries no
/// `attributecategory` groups — see [`generate_cr`]). The animatable flag is the
/// only datum the index attaches per attribute, recorded as a `raw_categories`
/// provenance marker alongside a constant `source=attindex` tag so consumers can
/// tell a CR attribute's provenance apart from the ED/DTD editions. The merged
/// element scope is emitted verbatim.
fn render_cr_inventory(inventory: &CrInventory, static_name: &str, doc: &str) -> String {
    let attributes: Vec<RenderAttribute> = inventory
        .attributes
        .iter()
        .map(|(name, facts)| {
            // Provenance markers, sorted for determinism. `source=attindex` on
            // every attribute (the index it came from); `animatable` only when
            // the index marked the attribute animatable.
            let mut raw_categories = vec!["source=attindex".to_string()];
            if facts.animatable {
                raw_categories.push("animatable".to_string());
            }
            raw_categories.sort();
            RenderAttribute {
                name: name.clone(),
                // The CR index does not expose attribute categories, so no
                // classification is fabricated — this is faithfully empty.
                classifications: Vec::new(),
                raw_categories,
                element_scope: facts.element_scope.iter().cloned().collect(),
            }
        })
        .collect();
    render_inventory(
        static_name,
        doc,
        &inventory.elements,
        &attributes,
        &inventory.edges,
    )
}
