//! SVG 2 Editor's Draft presence/matrix audit.
//!
//! Runs the deterministic `build/spec_xml.rs` extractor over the
//! **vendored** svgwg `definitions*.xml` family pinned for the
//! `Svg2EditorsDraft` snapshot, then audits the extractor output against the
//! committed snapshot data. This makes `definitions*.xml` the authoritative
//! source going forward for the facts the extractor can derive cleanly.
//!
//! ## What this test locks down (and why it is scoped)
//!
//! The **element set** is derived from `definitions*.xml` exactly and matches
//! the committed `Svg2EditorsDraft/elements.json` with zero divergence — so
//! that equality is asserted as a hard gate. Re-pointing the vendored files
//! or the snapshot can no longer silently drift the element inventory.
//!
//! The **attribute set** and the **element ↔ attribute matrix**, by contrast,
//! diverge **broadly and deliberately** between the raw `definitions*.xml`
//! and the committed snapshot. The committed snapshot encodes editorial
//! curation that is *not* a faithful read of this pinned `definitions*.xml`:
//!
//! - it **omits** the full `aria-*` family (48 attrs), every `on*` event
//!   handler (~67 attrs), `role`, `xml:space`, `xlink:*`, and the
//!   spec-listed-but-deprecated presentation attrs `clip`,
//!   `glyph-orientation-horizontal`, `color-profile`, `kerning`;
//! - it **adds** nine modern HTML/CSS attributes absent from this pinned
//!   `definitions*.xml` entirely — `async`, `attributeType`, `decoding`,
//!   `defer`, `fetchpriority`, `font-width`, `interestfor`, `text-overflow`,
//!   `white-space`.
//!
//! That is two different *models* of "what is an edge," not a handful of
//! spec-authoritative corrections. Per the derivation design (rows 11–13)
//! and the project rule to prefer correctness over a force-passing rewrite,
//! this test does **not** assert attribute/matrix equality. It instead
//! pins the *measured* divergence so any future change to either the
//! vendored spec or the snapshot that shifts these numbers fails loudly and
//! forces a human re-audit. The raw extractor inventory is exercised for
//! internal consistency so the deriver itself stays correct.

#[path = "../build/classification.rs"]
mod classification;
#[path = "../build/spec_xml.rs"]
mod spec_xml;

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use serde::Deserialize;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The vendored `master/` directory pinned for the `Svg2EditorsDraft`
/// snapshot. The pin (`19482daf`) is recorded in
/// `data/specs/Svg2EditorsDraft/snapshot.json`; the extractor must read the
/// **same** captured commit the snapshot was derived from.
fn ed_master() -> PathBuf {
    manifest_dir().join("data/sources/svgwg-19482daf/master")
}

fn ed_snapshot_dir() -> PathBuf {
    manifest_dir().join("data/specs/Svg2EditorsDraft")
}

#[derive(Debug, Deserialize)]
struct ElementRecord {
    name: String,
}

#[derive(Debug, Deserialize)]
struct AttributeRecord {
    name: String,
}

#[derive(Debug, Deserialize)]
struct MatrixFile {
    edges: Vec<MatrixEdge>,
}

#[derive(Debug, Deserialize)]
struct MatrixEdge {
    element: String,
    attribute: String,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => panic!("read {}: {error}", path.display()),
    };
    match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => panic!("parse {}: {error}", path.display()),
    }
}

/// Run the extractor over the vendored ED `master/`, panicking with the
/// extractor's own error on failure (the workspace `expect_used` deny rules
/// out `.expect()`).
fn ed_inventory() -> spec_xml::SpecInventory {
    match spec_xml::read_inventory(&ed_master()) {
        Ok(inventory) => inventory,
        Err(error) => panic!("extract ED inventory from definitions*.xml: {error}"),
    }
}

fn committed_elements() -> BTreeSet<String> {
    let records: Vec<ElementRecord> = read_json(&ed_snapshot_dir().join("elements.json"));
    records.into_iter().map(|record| record.name).collect()
}

fn committed_attributes() -> BTreeSet<String> {
    let records: Vec<AttributeRecord> = read_json(&ed_snapshot_dir().join("attributes.json"));
    records.into_iter().map(|record| record.name).collect()
}

fn committed_edges() -> BTreeSet<(String, String)> {
    let matrix: MatrixFile = read_json(&ed_snapshot_dir().join("element_attribute_matrix.json"));
    matrix
        .edges
        .into_iter()
        .map(|edge| (edge.element, edge.attribute))
        .collect()
}

/// The element set IS cleanly derivable from `definitions*.xml` and matches
/// the committed snapshot byte-for-byte (as names). This is the authoritative
/// gate: it makes `definitions*.xml` the source of truth for SVG 2 ED element
/// presence.
#[test]
fn extracted_element_set_equals_committed_snapshot() {
    let inventory = ed_inventory();

    let extracted = &inventory.elements;
    let committed = committed_elements();

    let in_spec_not_snapshot: Vec<&String> = extracted.difference(&committed).collect();
    let in_snapshot_not_spec: Vec<&String> = committed.difference(extracted).collect();

    assert!(
        in_spec_not_snapshot.is_empty() && in_snapshot_not_spec.is_empty(),
        "ED element set diverged from definitions*.xml.\n  spec-only: {in_spec_not_snapshot:?}\n  snapshot-only: {in_snapshot_not_spec:?}",
    );

    // The five SMIL animation elements are present only because
    // `definitions-animations.xml` is included in the extraction.
    for smil in [
        "animate",
        "animateMotion",
        "animateTransform",
        "set",
        "mpath",
    ] {
        assert!(
            extracted.contains(smil),
            "SMIL animation element <{smil}> missing — is definitions-animations.xml vendored and parsed?",
        );
    }

    // The elementcategory / term overcount trap: there are 14
    // `<elementcategory>` declarations across the files, but none must leak
    // into the element set. 63 real elements, not ~77.
    assert_eq!(
        extracted.len(),
        63,
        "expected exactly 63 SVG 2 ED elements (the elementcategory overcount trap must not leak)",
    );
}

/// The extractor's internal consistency: every attribute it reports as
/// present is attached to at least one element, and every matrix edge names a
/// real extracted element and a real extracted attribute. Guards the deriver
/// itself independent of the (deliberately divergent) committed snapshot.
#[test]
fn extracted_inventory_is_internally_consistent() {
    let inventory = ed_inventory();

    let edges = inventory.edges();
    for (element, attribute) in &edges {
        assert!(
            inventory.elements.contains(*element),
            "matrix edge names unknown element <{element}>",
        );
        assert!(
            inventory.attributes.contains(*attribute),
            "matrix edge names unknown attribute `{attribute}`",
        );
    }

    // Every reported attribute must appear on at least one element.
    let attributes_on_some_element: BTreeSet<&str> =
        edges.iter().map(|(_, attribute)| *attribute).collect();
    let orphans: Vec<&String> = inventory
        .attributes
        .iter()
        .filter(|attribute| !attributes_on_some_element.contains(attribute.as_str()))
        .collect();
    assert!(
        orphans.is_empty(),
        "extractor reported attributes attached to no element: {orphans:?}",
    );

    // `presentation` attributecategory expansion must have fired: every
    // element carrying the `presentation` category should expose a core
    // presentation attribute such as `fill`.
    assert!(
        inventory
            .element_attributes
            .get("circle")
            .is_some_and(|attrs| attrs.contains("fill")),
        "presentation attributecategory expansion did not reach <circle>",
    );
}

/// Audit gate for the attribute set and edge matrix.
///
/// These DELIBERATELY diverge from this pinned `definitions*.xml` (see the
/// module docs). Rather than force a rewrite, we pin the *measured*
/// divergence so any drift on either side fails and forces a human
/// re-audit. The exact figures are the audited reality as of pin
/// `19482daf` vs. the committed snapshot.
#[test]
fn attribute_and_matrix_divergence_is_pinned_for_human_review() {
    let inventory = ed_inventory();

    // --- attribute set divergence ---
    let extracted_attrs = &inventory.attributes;
    let committed_attrs = committed_attributes();
    let spec_only_attrs = extracted_attrs.difference(&committed_attrs).count();
    let snapshot_only_attrs = committed_attrs.difference(extracted_attrs).count();

    // Extractor derives 313 attribute names from definitions*.xml; the
    // committed snapshot curates 194. 128 spec attrs (aria-*/on*/xlink:*/
    // role/xml:space/deprecated presentation) are dropped by the snapshot;
    // 9 modern HTML/CSS attrs in the snapshot are absent from this pin.
    assert_eq!(
        extracted_attrs.len(),
        313,
        "derived attribute universe drifted; re-audit definitions*.xml expansion",
    );
    assert_eq!(
        spec_only_attrs, 128,
        "attribute spec-only count drifted; re-audit definitions*.xml vs snapshot attributes.json",
    );
    assert_eq!(
        snapshot_only_attrs, 9,
        "attribute snapshot-only count drifted; re-audit the curated modern-HTML additions",
    );

    // --- edge matrix divergence ---
    let extracted_edges: BTreeSet<(String, String)> = inventory
        .edges()
        .into_iter()
        .map(|(element, attribute)| (element.to_string(), attribute.to_string()))
        .collect();
    let committed_edges = committed_edges();
    let spec_only_edges = extracted_edges.difference(&committed_edges).count();
    let snapshot_only_edges = committed_edges.difference(&extracted_edges).count();
    let overlap_edges = extracted_edges.intersection(&committed_edges).count();

    // Both sides are large with a substantial-but-partial overlap. The exact
    // figures are the audited reality at pin `19482daf`: derive 6752 edges,
    // snapshot keeps 4434, 3208 agree. Drift on either side fails here and
    // forces a human re-audit rather than a silent mass rewrite.
    assert_eq!(
        extracted_edges.len(),
        6752,
        "derived edge count drifted; re-audit definitions*.xml matrix resolution",
    );
    assert_eq!(
        committed_edges.len(),
        4434,
        "committed ED edge count changed; re-audit the matrix against definitions*.xml",
    );
    assert_eq!(
        overlap_edges, 3208,
        "edge overlap drifted; re-audit spec-vs-snapshot matrix agreement",
    );
    assert_eq!(spec_only_edges, 3544, "spec-only edge count drifted");
    assert_eq!(
        snapshot_only_edges, 1226,
        "snapshot-only edge count drifted"
    );
}
