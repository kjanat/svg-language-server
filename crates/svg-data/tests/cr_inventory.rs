//! Locks the baked SVG 2 Candidate Recommendation (`CR-SVG2-20181004`)
//! inventory and audits it against the committed CR snapshot.
//!
//! Unlike the SVG 2 ED (`definitions*.xml`) and the SVG 1.1 editions (flat
//! DTDs), the dated CR ships no vendored machine-readable grammar. Its
//! authoritative machine-readable artifacts are the *published* appendix index
//! tables (`eltindex.html` + `attindex.html` + `propidx.html`), parsed by
//! `build/tr_index.rs` and baked through `build/inventory_codegen.rs` into the
//! [`svg_data::spec_inventory`] API exactly like the other editions. This test
//! pins:
//!
//! 1. **counts** — element / attribute / edge totals of the baked inventory,
//!    re-derived live from the same `build/tr_index.rs` extractor the codegen
//!    uses (a drift between live and baked fails loudly);
//! 2. **classification sparsity** — the CR index carries no
//!    `attributecategory` groups, so every CR attribute is faithfully
//!    *unclassified* (the animatable flag is retained as provenance instead).
//!    This is asserted, not papered over;
//! 3. **attindex matrix facts** — concrete `(element, attribute)` truths from
//!    the rendered attribute index (e.g. `fill` applies to the animation
//!    elements; `aria-label` rides every element listed for it);
//! 4. **the presence audit** — inventory element/attribute/edge sets vs the
//!    committed curated snapshot (`data/specs/Svg2Cr20181004/…`). The curated
//!    snapshot folds in the `propidx.html` presentation *properties* (which the
//!    attribute index omits) and the ED reconciliation, and drops the bulky
//!    `aria-*` family, so the divergence is large and is *pinned* here for
//!    human review rather than mass-rewritten;
//! 5. **the enum cross-check** — `propidx.html`-derived pure-keyword property
//!    enums vs the committed `grammars.json` `enum-*` grammars. The genuine
//!    divergences (where the curated grammar adopted the modern CSS keyword set
//!    over the CR property table's older values) are asserted as findings.

#[path = "../build/tr_index.rs"]
mod tr_index;
#[path = "../build/value_syntax.rs"]
mod value_syntax;

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use svg_data::{
    SpecSnapshotId,
    inventory::{Classification, Inventory},
    spec_inventory,
};

/// Read a crate-relative data file.
fn read(path: &str) -> String {
    let full = format!("{}/{path}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("read {full}: {err}"))
}

/// The baked CR inventory, or panic.
fn cr_inventory() -> &'static Inventory {
    let Some(inventory) = spec_inventory(SpecSnapshotId::Svg2Cr20181004) else {
        panic!("Svg2Cr20181004 should have a baked inventory")
    };
    inventory
}

/// Live-parse the CR inventory from the vendored index pages (the same parse the
/// codegen bakes).
fn live_cr_inventory() -> tr_index::CrInventory {
    let elements = tr_index::parse_eltindex(&read("data/sources/svg2-cr-20181004/eltindex.html"))
        .unwrap_or_else(|err| panic!("eltindex: {err}"));
    let rows = tr_index::parse_attindex(&read("data/sources/svg2-cr-20181004/attindex.html"))
        .unwrap_or_else(|err| panic!("attindex: {err}"));
    tr_index::build_inventory(elements, &rows).unwrap_or_else(|err| panic!("{err}"))
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

/// `name`s in a committed snapshot `elements.json`/`attributes.json`.
fn snapshot_names(path: &str) -> BTreeSet<String> {
    let items: Vec<Named> =
        serde_json::from_str(&read(path)).unwrap_or_else(|err| panic!("parse {path}: {err}"));
    items.into_iter().map(|named| named.name).collect()
}

/// One committed matrix edge carries (at least) an element + attribute name.
#[derive(Deserialize)]
struct MatrixEdge {
    element: String,
    attribute: String,
}

#[derive(Deserialize)]
struct Matrix {
    edges: Vec<MatrixEdge>,
}

/// The `(element, attribute)` edge set of the committed CR matrix.
fn committed_edges() -> BTreeSet<(String, String)> {
    let matrix: Matrix = serde_json::from_str(&read(
        "data/specs/Svg2Cr20181004/element_attribute_matrix.json",
    ))
    .unwrap_or_else(|err| panic!("parse matrix: {err}"));
    matrix
        .edges
        .into_iter()
        .map(|edge| (edge.element, edge.attribute))
        .collect()
}

#[test]
fn baked_counts_are_locked() {
    let inventory = cr_inventory();
    assert_eq!(inventory.elements.len(), 69, "CR element count drifted");
    assert_eq!(
        inventory.attributes.len(),
        261,
        "CR attribute count drifted"
    );
    assert_eq!(inventory.edges.len(), 4556, "CR edge count drifted");

    // The live extractor must agree with the baked statics, or a stale bake
    // slipped in.
    let live = live_cr_inventory();
    assert_eq!(
        live.elements.len(),
        inventory.elements.len(),
        "live CR element count diverged from baked"
    );
    assert_eq!(
        live.attributes.len(),
        inventory.attributes.len(),
        "live CR attribute count diverged from baked"
    );
    assert_eq!(
        live.edges.len(),
        inventory.edges.len(),
        "live CR edge count diverged from baked"
    );
}

#[test]
fn cr_attributes_are_faithfully_unclassified() {
    let inventory = cr_inventory();
    // The published CR index carries no `attributecategory` groups, so *no*
    // attribute may carry a normalized classification — fabricating one would
    // be dishonest about what the CR artifact exposes.
    for attribute in inventory.attributes.iter() {
        assert!(
            attribute.classifications.is_empty(),
            "CR attribute `{}` should be unclassified (index has no categories): {:?}",
            attribute.name,
            attribute.classifications
        );
    }
    for bucket in [
        Classification::Core,
        Classification::Presentation,
        Classification::Aria,
        Classification::EventHandler,
        Classification::Xlink,
        Classification::ConditionalProcessing,
    ] {
        assert_eq!(
            inventory.count_with_classification(&bucket),
            0,
            "CR bucket {bucket:?} should be empty"
        );
    }

    // Provenance the index *does* expose is retained: every attribute is tagged
    // with its source index, and the 97 animatable attributes carry the
    // animatable marker.
    let with_source = inventory
        .attributes
        .iter()
        .filter(|attribute| {
            attribute
                .raw_categories
                .iter()
                .any(|category| category.as_ref() == "source=attindex")
        })
        .count();
    assert_eq!(
        with_source,
        inventory.attributes.len(),
        "every CR attribute should carry its source-index provenance"
    );
    let animatable = inventory
        .attributes
        .iter()
        .filter(|attribute| {
            attribute
                .raw_categories
                .iter()
                .any(|category| category.as_ref() == "animatable")
        })
        .count();
    assert_eq!(animatable, 97, "CR animatable provenance count drifted");
}

#[test]
fn attindex_matrix_facts_hold() {
    let inventory = cr_inventory();

    // In the attribute index `fill` is the animation-target attribute: it
    // applies to the four animation elements (the *paint* `fill` is a property,
    // living in `propidx.html`, not the attribute index).
    let fill_scope: BTreeSet<&str> = inventory
        .attributes_for_element("animate")
        .map(|attribute| attribute.name.as_ref())
        .collect();
    assert!(
        fill_scope.contains("fill"),
        "`fill` should apply to `animate` in the attindex: {fill_scope:?}"
    );
    let Some(fill) = inventory.attribute("fill") else {
        panic!("`fill` should be in the CR inventory")
    };
    let fill_elements: BTreeSet<&str> = fill.element_scope.iter().map(AsRef::as_ref).collect();
    assert_eq!(
        fill_elements,
        ["animate", "animateMotion", "animateTransform", "set"]
            .into_iter()
            .collect::<BTreeSet<&str>>(),
        "`fill` element scope drifted"
    );

    // The `aria-*` family is present in the CR attribute index (the curated
    // snapshot drops it, audited below). `aria-label` is one such attribute.
    assert!(
        inventory.attribute("aria-label").is_some(),
        "`aria-label` should be in the CR inventory"
    );

    // `rect` carries the conditional-processing and core/identification
    // attributes the index lists for it (the geometric/presentation attributes
    // are *properties* and live in `propidx.html`, not the attribute index, so
    // the attindex `rect` row is the conditional/core/aria/event set).
    let rect_attrs: BTreeSet<&str> = inventory
        .attributes_for_element("rect")
        .map(|attribute| attribute.name.as_ref())
        .collect();
    for expected in [
        "id",
        "class",
        "style",
        "pathLength",
        "requiredExtensions",
        "systemLanguage",
        "tabindex",
    ] {
        assert!(
            rect_attrs.contains(expected),
            "`rect` should carry `{expected}` per the attindex: {rect_attrs:?}"
        );
    }
}

#[test]
fn presence_audit_divergence_is_pinned() {
    // The committed curated snapshot folds together the attribute index, the
    // `propidx.html` presentation properties, and the ED reconciliation, and
    // drops the bulky `aria-*` family. The pure attribute-index inventory is
    // therefore a *different* universe, not a subset. Pin the exact divergence
    // so a future curation or index change is reviewed, not silently absorbed.
    let inventory = cr_inventory();
    let inv_els: BTreeSet<String> = inventory
        .elements
        .iter()
        .map(|element| element.name.to_string())
        .collect();
    let inv_attrs: BTreeSet<String> = inventory
        .attributes
        .iter()
        .map(|attribute| attribute.name.to_string())
        .collect();
    let inv_edges: BTreeSet<(String, String)> = inventory
        .edges
        .iter()
        .map(|edge| (edge.element.to_string(), edge.attribute.to_string()))
        .collect();

    let snap_els = snapshot_names("data/specs/Svg2Cr20181004/elements.json");
    let snap_attrs = snapshot_names("data/specs/Svg2Cr20181004/attributes.json");
    let snap_edges = committed_edges();

    assert_eq!(snap_els.len(), 63, "committed element count drifted");
    assert_eq!(snap_attrs.len(), 194, "committed attribute count drifted");
    assert_eq!(snap_edges.len(), 4434, "committed edge count drifted");

    // Elements: the attribute index references 6 elements the curated snapshot
    // omits (the embedded-HTML and animation/placeholder elements); every
    // curated element is present in the index (the index is a superset here).
    assert_eq!(
        inv_els.difference(&snap_els).cloned().collect::<Vec<_>>(),
        vec![
            "audio".to_string(),
            "canvas".to_string(),
            "discard".to_string(),
            "iframe".to_string(),
            "unknown".to_string(),
            "video".to_string(),
        ],
        "index-only element set drifted"
    );
    assert_eq!(
        snap_els.difference(&inv_els).count(),
        0,
        "every curated CR element should appear in the published index"
    );

    // Attributes: large two-way divergence. The curated snapshot adds the
    // `propidx.html` properties (`fill-rule`, `color`, …) the attribute index
    // omits; the attribute index adds the `aria-*` family the curated snapshot
    // drops.
    assert_eq!(
        inv_attrs.difference(&snap_attrs).count(),
        135,
        "index-only attribute count drifted"
    );
    assert_eq!(
        snap_attrs.difference(&inv_attrs).count(),
        68,
        "curated-only attribute count drifted"
    );
    // Spot-check the *kind* of each side: aria-only in the index, properties
    // only in the curated snapshot.
    assert!(
        inv_attrs.contains("aria-label") && !snap_attrs.contains("aria-label"),
        "aria family should be index-only"
    );
    assert!(
        snap_attrs.contains("fill-rule") && !inv_attrs.contains("fill-rule"),
        "presentation properties should be curated-only"
    );

    // Edges: likewise a large two-way divergence (the curated matrix applies
    // each property to its `propidx` `Applies to` element set).
    assert_eq!(
        inv_edges.intersection(&snap_edges).count(),
        640,
        "shared edge count drifted"
    );
    assert_eq!(
        inv_edges.difference(&snap_edges).count(),
        3916,
        "index-only edge count drifted"
    );
    assert_eq!(
        snap_edges.difference(&inv_edges).count(),
        3794,
        "curated-only edge count drifted"
    );
}

#[derive(Deserialize)]
struct GrammarFile {
    grammars: Vec<GrammarEntry>,
}

#[derive(Deserialize)]
struct GrammarEntry {
    id: String,
    root: GrammarRoot,
}

/// A grammar root node. Only `choice` roots whose options are *all* keywords
/// are pure-keyword enums; every other shape leaves `kind`/`options` and is
/// rejected below. Modeled flat (non-recursive) since the cross-check only
/// inspects one `choice` level.
#[derive(Deserialize)]
struct GrammarRoot {
    kind: String,
    #[serde(default)]
    options: Vec<GrammarOption>,
}

/// One option of a `choice` root: a `keyword` carries its `value`; any other
/// `kind` (datatype ref, sequence, …) leaves `value` `None` and disqualifies
/// the enum.
#[derive(Deserialize)]
struct GrammarOption {
    kind: String,
    #[serde(default)]
    value: Option<String>,
}

/// Pure-keyword `enum-*` grammars from the committed CR `grammars.json`, keyed
/// by grammar id, value = keyword set.
fn committed_keyword_enums() -> BTreeMap<String, BTreeSet<String>> {
    let file: GrammarFile = serde_json::from_str(&read("data/specs/Svg2Cr20181004/grammars.json"))
        .unwrap_or_else(|err| panic!("parse grammars: {err}"));
    let mut out = BTreeMap::new();
    for grammar in file.grammars {
        if grammar.root.kind != "choice" {
            continue;
        }
        let mut keywords = BTreeSet::new();
        let mut pure = true;
        for option in &grammar.root.options {
            if option.kind == "keyword" {
                if let Some(value) = &option.value {
                    keywords.insert(value.clone());
                }
            } else {
                pure = false;
                break;
            }
        }
        if pure && !keywords.is_empty() {
            out.insert(grammar.id, keywords);
        }
    }
    out
}

/// Map committed attribute name -> its `enum-*` grammar id (when its
/// `value_syntax` is a `grammar_ref` to one).
fn committed_attr_enum_ids() -> BTreeMap<String, String> {
    #[derive(Deserialize)]
    struct Attr {
        name: String,
        value_syntax: ValueSyntax,
    }
    #[derive(Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum ValueSyntax {
        GrammarRef {
            grammar_id: String,
        },
        #[serde(other)]
        Other,
    }
    let attrs: Vec<Attr> = serde_json::from_str(&read("data/specs/Svg2Cr20181004/attributes.json"))
        .unwrap_or_else(|err| panic!("parse attributes: {err}"));
    attrs
        .into_iter()
        .filter_map(|attr| match attr.value_syntax {
            ValueSyntax::GrammarRef { grammar_id } if grammar_id.starts_with("enum-") => {
                Some((attr.name, grammar_id))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn propidx_enum_cross_check_against_committed_grammars() {
    // Property enums recovered live from `propidx.html` via the shared
    // `value_syntax` tokenizer (the same one SVG 1.1 uses), keyed by property
    // name.
    let rows = tr_index::parse_propidx(&read("data/sources/svg2-cr-20181004/propidx.html"))
        .unwrap_or_else(|err| panic!("propidx: {err}"));
    let propidx_enums: BTreeMap<String, BTreeSet<String>> = rows
        .into_iter()
        .filter_map(|row| {
            value_syntax::keyword_enum(&row.values)
                .map(|keywords| (row.name, keywords.into_iter().collect()))
        })
        .collect();
    assert_eq!(
        propidx_enums.len(),
        19,
        "propidx pure-keyword enum count drifted"
    );

    let committed_enums = committed_keyword_enums();
    let attr_enum_ids = committed_attr_enum_ids();

    let mut compared = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for (attribute, grammar_id) in &attr_enum_ids {
        let Some(committed) = committed_enums.get(grammar_id) else {
            continue;
        };
        let Some(property) = propidx_enums.get(attribute) else {
            continue;
        };
        compared += 1;
        if committed != property {
            mismatches.push(format!(
                "{attribute}: committed={committed:?} propidx={property:?}"
            ));
        }
    }
    assert_eq!(
        compared, 9,
        "committed-enum vs propidx-enum overlap count drifted"
    );

    // The genuine divergences: in each case the curated `grammars.json` adopted
    // the modern (ED/CSS-aligned) keyword set, while the CR property table
    // (`propidx.html`) still lists the older CR-era values (`display` drops
    // `compact`/`marker`/`run-in`; `dominant-baseline` swaps the edge keywords;
    // `text-decoration` drops `blink`). These are real, surfaced as findings
    // (not papered over).
    let mismatch_attrs: BTreeSet<&str> = mismatches
        .iter()
        .filter_map(|line| line.split(':').next())
        .collect();
    assert_eq!(
        mismatch_attrs,
        ["display", "dominant-baseline", "text-decoration"]
            .into_iter()
            .collect::<BTreeSet<&str>>(),
        "enum cross-check divergence set drifted: {mismatches:?}"
    );
}
