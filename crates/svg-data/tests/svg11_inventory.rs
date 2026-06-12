//! Locks the baked SVG 1.1 spec inventories and their audit figures.
//!
//! Two complementary inventories are baked from the vendored SVG 1.1 flat DTDs
//! (`build/dtd.rs` -> `build/inventory_codegen.rs`), exposed through the same
//! [`svg_data::spec_inventory`] API as the SVG 2 ED inventory. This test pins:
//!
//! 1. **counts** — element / attribute / edge totals per edition;
//! 2. **classification buckets** — the normalized [`Classification`] tallies
//!    derived from each attribute's `%…attrib;` DTD group;
//! 3. **known ATTLIST facts** — concrete `(element, attribute)` truths
//!    (`rect` carries `x/y/width/height/rx/ry`; the `%SVG.Core.attrib;` group
//!    expands to `id`/`xml:base`/`xml:lang`/`xml:space`; `width` is
//!    `#REQUIRED`);
//! 4. **the presence audit** — DTD-extracted element/attribute sets vs the
//!    committed curated snapshot (`data/specs/Svg11Rec*/…`). The snapshot is a
//!    *curated subset* (it drops the SVG 1.1 font/glyph machinery and the raw
//!    `on*`/`xml:*` families, and adds modern reconciled attributes), so the
//!    divergence is pinned here for human review rather than mass-rewritten;
//! 5. **the enum cross-check** — DTD enumerated ATTLIST values vs the
//!    `propidx.html`-derived property enums. The single known divergence
//!    (`visibility`, where the DTD omits `collapse`) is asserted as a finding.
//!
//! Counts feeding (1)/(2)/(4)/(5) are re-derived live from the same `build/`
//! modules the codegen uses, so a drift between the live extractor and the
//! baked statics fails loudly here.

#[path = "../build/classification.rs"]
mod classification;
#[path = "../build/dtd.rs"]
mod dtd;
#[path = "../build/propidx.rs"]
mod propidx;
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

/// One element/attribute record in a committed snapshot JSON carries (at least)
/// a `name`; the rest is irrelevant to the presence audit.
#[derive(Deserialize)]
struct Named {
    name: String,
}

/// The set of `name`s in a committed snapshot `elements.json`/`attributes.json`.
fn snapshot_names(path: &str) -> BTreeSet<String> {
    let items: Vec<Named> =
        serde_json::from_str(&read(path)).unwrap_or_else(|err| panic!("parse {path}: {err}"));
    items.into_iter().map(|named| named.name).collect()
}

/// The baked inventory for an SVG 1.1 snapshot, or panic — both editions must
/// have one.
fn require_inventory(snapshot: SpecSnapshotId) -> &'static Inventory {
    let Some(inventory) = spec_inventory(snapshot) else {
        panic!("{snapshot:?} should have a baked SVG 1.1 inventory")
    };
    inventory
}

#[test]
fn baked_counts_are_locked() {
    let first = require_inventory(SpecSnapshotId::Svg11Rec20030114);
    assert_eq!(first.elements.len(), 81, "20030114 element count drifted");
    assert_eq!(
        first.attributes.len(),
        268,
        "20030114 attribute count drifted"
    );
    assert_eq!(first.edges.len(), 2930, "20030114 edge count drifted");

    let second = require_inventory(SpecSnapshotId::Svg11Rec20110816);
    assert_eq!(second.elements.len(), 80, "20110816 element count drifted");
    assert_eq!(
        second.attributes.len(),
        268,
        "20110816 attribute count drifted"
    );
    assert_eq!(second.edges.len(), 4352, "20110816 edge count drifted");

    // The first edition uniquely carries `definition-src` (a font element the
    // 2011 second edition dropped); otherwise the element sets are identical.
    let first_only: Vec<&str> = first
        .elements
        .iter()
        .map(|element| element.name.as_ref())
        .filter(|name| {
            !second
                .elements
                .iter()
                .any(|element| element.name.as_ref() == *name)
        })
        .collect();
    assert_eq!(
        first_only,
        vec!["definition-src"],
        "edition element delta drifted"
    );
}

#[test]
fn classification_buckets_are_locked() {
    // Tallies of attributes carrying each normalized classification. The set is
    // shared with the SVG 2 ED model; SVG 1.1 has no ARIA collection, so the
    // Aria bucket is empty by construction.
    for (snapshot, core, presentation, conditional, xlink, events, aria) in [
        (
            SpecSnapshotId::Svg11Rec20030114,
            8usize,
            59usize,
            3usize,
            8usize,
            19usize,
            0usize,
        ),
        (SpecSnapshotId::Svg11Rec20110816, 8, 59, 3, 8, 19, 0),
    ] {
        let inventory = require_inventory(snapshot);
        let count =
            |classification: &Classification| inventory.count_with_classification(classification);
        assert_eq!(
            count(&Classification::Core),
            core,
            "{snapshot:?} Core bucket"
        );
        assert_eq!(
            count(&Classification::Presentation),
            presentation,
            "{snapshot:?} Presentation bucket"
        );
        assert_eq!(
            count(&Classification::ConditionalProcessing),
            conditional,
            "{snapshot:?} ConditionalProcessing bucket"
        );
        assert_eq!(
            count(&Classification::Xlink),
            xlink,
            "{snapshot:?} Xlink bucket"
        );
        assert_eq!(
            count(&Classification::EventHandler),
            events,
            "{snapshot:?} EventHandler bucket"
        );
        assert_eq!(
            count(&Classification::Aria),
            aria,
            "{snapshot:?} Aria bucket"
        );
    }
}

#[test]
fn live_extractor_matches_baked_classification_and_edges() {
    // The baked inventory derives every attribute's classification by mapping
    // its `%…attrib;` DTD groups through `dtd::classify_group` (see
    // `inventory_codegen::render_dtd_inventory`). Re-derive the same buckets
    // *live* from the DTD here and assert they match the baked
    // `count_with_classification`, so a drift between the live extractor and the
    // statics fails loudly rather than silently baking a stale classification.
    // The live `DtdInventory::edges()` count is locked against the baked edges
    // for the same reason.
    for (snapshot, dtd_path) in [
        (
            SpecSnapshotId::Svg11Rec20030114,
            "data/sources/svg11-rec-20030114/svg11-flat-20030114.dtd",
        ),
        (
            SpecSnapshotId::Svg11Rec20110816,
            "data/sources/svg11-rec-20110816/svg11-flat-20110816.dtd",
        ),
    ] {
        let parsed = dtd::parse(&read(dtd_path));
        let baked = require_inventory(snapshot);

        // Edge count: live matrix vs baked statics.
        assert_eq!(
            parsed.edges().len(),
            baked.edges.len(),
            "{snapshot:?}: live edge count diverged from baked"
        );

        // Live classification tally: for every distinct attribute name, map its
        // `%…attrib;` groups through `dtd::classify_group` (the build-side
        // taxonomy) and count the names whose normalized set carries each
        // bucket. The build-side `classification::Classification` and the public
        // `svg_data::inventory::Classification` are distinct types with the same
        // named buckets, so the comparison pairs them explicitly.
        let live_count = |bucket: &classification::Classification| -> usize {
            parsed
                .attribute_names()
                .iter()
                .filter(|name| {
                    parsed.attribute_groups.get(*name).is_some_and(|groups| {
                        groups
                            .iter()
                            .any(|group| &dtd::classify_group(group) == bucket)
                    })
                })
                .count()
        };

        // Lock the shared build-side `Classification::from_category` taxonomy
        // (the SVG 2 ED normalization that the same `classification` module
        // exposes to both editions): a representative name from each named
        // bucket plus the verbatim-preserving `Other` fallback.
        assert_eq!(
            classification::Classification::from_category("core"),
            classification::Classification::Core
        );
        assert_eq!(
            classification::Classification::from_category("presentation"),
            classification::Classification::Presentation
        );
        assert_eq!(
            classification::Classification::from_category("aria"),
            classification::Classification::Aria
        );
        assert_eq!(
            classification::Classification::from_category("window event"),
            classification::Classification::EventHandler
        );
        assert_eq!(
            classification::Classification::from_category("deprecated xlink"),
            classification::Classification::Xlink
        );
        assert_eq!(
            classification::Classification::from_category("conditional processing"),
            classification::Classification::ConditionalProcessing
        );
        assert_eq!(
            classification::Classification::from_category("filter primitive"),
            classification::Classification::Other("filter primitive".to_string())
        );

        for (build_bucket, baked_bucket) in [
            (classification::Classification::Core, Classification::Core),
            (
                classification::Classification::Presentation,
                Classification::Presentation,
            ),
            (
                classification::Classification::ConditionalProcessing,
                Classification::ConditionalProcessing,
            ),
            (classification::Classification::Xlink, Classification::Xlink),
            (
                classification::Classification::EventHandler,
                Classification::EventHandler,
            ),
            (classification::Classification::Aria, Classification::Aria),
        ] {
            assert_eq!(
                live_count(&build_bucket),
                baked.count_with_classification(&baked_bucket),
                "{snapshot:?}: live `{build_bucket:?}` tally diverged from baked"
            );
        }
    }
}

#[test]
fn known_attlist_facts_hold() {
    let inventory = require_inventory(SpecSnapshotId::Svg11Rec20030114);

    // `rect` carries its geometry attributes.
    let rect_attrs: BTreeSet<&str> = inventory
        .attributes_for_element("rect")
        .map(|attribute| attribute.name.as_ref())
        .collect();
    for geometry in ["x", "y", "width", "height", "rx", "ry"] {
        assert!(
            rect_attrs.contains(geometry),
            "rect should carry `{geometry}`: {rect_attrs:?}"
        );
    }
    // `transform` rides on `rect` too (it is a graphics element).
    assert!(rect_attrs.contains("transform"));

    // The `%SVG.Core.attrib;` group expands to the four core attributes; every
    // element carrying Core therefore carries these, and each is classified
    // Core with the raw group preserved as provenance.
    for core in ["id", "xml:base", "xml:lang", "xml:space"] {
        let Some(attribute) = inventory.attribute(core) else {
            panic!("`{core}` should be in the 20030114 inventory")
        };
        assert!(
            attribute.classifications.contains(&Classification::Core),
            "`{core}` should be classified Core: {:?}",
            attribute.classifications
        );
        assert!(
            rect_attrs.contains(core),
            "rect should inherit core attribute `{core}`"
        );
    }

    // Provenance survives verbatim: `id` came from the `SVG.Core.attrib` group.
    let Some(id) = inventory.attribute("id") else {
        panic!("`id` should be in the 20030114 inventory")
    };
    assert!(
        id.raw_categories
            .iter()
            .any(|category| category.as_ref() == "SVG.Core.attrib"),
        "id raw categories: {:?}",
        id.raw_categories
    );

    // `width` is `#REQUIRED` on `rect` in the DTD — re-parse to assert the
    // defaulting fact (the baked inventory keeps presence/edges, not the
    // required flag, so this checks the live extractor the bake derives from).
    let parsed = dtd::parse(&read(
        "data/sources/svg11-rec-20030114/svg11-flat-20030114.dtd",
    ));
    let width = &parsed.element_attributes["rect"]["width"];
    assert_eq!(
        width.defaulting,
        dtd::Defaulting::Required,
        "rect/width #REQUIRED"
    );
    let x = &parsed.element_attributes["rect"]["x"];
    assert_eq!(x.defaulting, dtd::Defaulting::Implied, "rect/x #IMPLIED");
}

#[test]
fn presence_audit_divergence_is_pinned() {
    // The committed snapshot is a curated subset of the raw DTD inventory. This
    // pins the exact divergence so a future curation change is reviewed, not
    // silently absorbed. (See module docs.)
    for (snapshot, dtd_path, els_json, attrs_json, only_dtd_els, only_dtd_attrs, only_snap_attrs) in [
        (
            SpecSnapshotId::Svg11Rec20030114,
            "data/sources/svg11-rec-20030114/svg11-flat-20030114.dtd",
            "data/specs/Svg11Rec20030114/elements.json",
            "data/specs/Svg11Rec20030114/attributes.json",
            19usize,
            88usize,
            21usize,
        ),
        (
            SpecSnapshotId::Svg11Rec20110816,
            "data/sources/svg11-rec-20110816/svg11-flat-20110816.dtd",
            "data/specs/Svg11Rec20110816/elements.json",
            "data/specs/Svg11Rec20110816/attributes.json",
            18,
            88,
            21,
        ),
    ] {
        let inventory = dtd::parse(&read(dtd_path));
        let dtd_els = inventory.elements.clone();
        let dtd_attrs = inventory.attribute_names();
        let snap_els = snapshot_names(els_json);
        let snap_attrs = snapshot_names(attrs_json);

        // Every curated snapshot element is present in the DTD (the curated set
        // is a subset of the DTD universe — never the other way around).
        let missing_from_dtd: Vec<&String> = snap_els.difference(&dtd_els).collect();
        assert!(
            missing_from_dtd.is_empty(),
            "{snapshot:?}: snapshot elements absent from DTD: {missing_from_dtd:?}"
        );

        assert_eq!(
            dtd_els.difference(&snap_els).count(),
            only_dtd_els,
            "{snapshot:?}: DTD-only element count drifted"
        );
        assert_eq!(
            dtd_attrs.difference(&snap_attrs).count(),
            only_dtd_attrs,
            "{snapshot:?}: DTD-only attribute count drifted"
        );
        assert_eq!(
            snap_attrs.difference(&dtd_attrs).count(),
            only_snap_attrs,
            "{snapshot:?}: snapshot-only attribute count drifted"
        );

        // Concrete subset spot-checks: the font machinery is DTD-only; the
        // modern reconciled attributes are snapshot-only.
        assert!(dtd_els.contains("font-face") && !snap_els.contains("font-face"));
        assert!(snap_attrs.contains("download") && !dtd_attrs.contains("download"));
    }
}

#[test]
fn enum_cross_check_against_propidx() {
    for (snapshot, dtd_path, propidx_path) in [
        (
            SpecSnapshotId::Svg11Rec20030114,
            "data/sources/svg11-rec-20030114/svg11-flat-20030114.dtd",
            "data/sources/svg11-rec-20030114/propidx.html",
        ),
        (
            SpecSnapshotId::Svg11Rec20110816,
            "data/sources/svg11-rec-20110816/svg11-flat-20110816.dtd",
            "data/sources/svg11-rec-20110816/propidx.html",
        ),
    ] {
        let inventory = dtd::parse(&read(dtd_path));
        let rows = propidx::parse_propidx(&read(propidx_path))
            .unwrap_or_else(|err| panic!("{snapshot:?}: {err}"));
        let propidx_enums: BTreeMap<String, Vec<String>> = rows
            .into_iter()
            .filter_map(|row| value_syntax::keyword_enum(&row.values).map(|kws| (row.name, kws)))
            .collect();

        let mut compared = 0usize;
        let mut mismatches: Vec<String> = Vec::new();
        for (attribute, enumeration) in &inventory.attribute_enums {
            let Some(property_keywords) = propidx_enums.get(attribute) else {
                continue;
            };
            compared += 1;
            // The DTD enum carries `inherit`; the property-index enum does not
            // (the extractor drops it). Compare on the keyword *set* sans
            // `inherit`.
            let dtd_set: BTreeSet<&str> = enumeration
                .keywords
                .iter()
                .map(String::as_str)
                .filter(|keyword| *keyword != "inherit")
                .collect();
            let property_set: BTreeSet<&str> =
                property_keywords.iter().map(String::as_str).collect();
            if dtd_set != property_set {
                mismatches.push(format!(
                    "{attribute}: dtd={dtd_set:?} propidx={property_set:?}"
                ));
            }
        }

        // A meaningful number of presentation-property enums overlap both
        // sources; if this collapses, the extractor or mapping broke.
        assert_eq!(compared, 24, "{snapshot:?}: enum overlap count drifted");

        // Exactly one genuine divergence: the SVG 1.1 DTD declares
        // `visibility ( visible | hidden | inherit )`, omitting the `collapse`
        // keyword that the property index lists. This is a real DTD
        // under-specification, surfaced as a finding (not papered over).
        assert_eq!(
            mismatches,
            vec![
                "visibility: dtd={\"hidden\", \"visible\"} propidx={\"collapse\", \"hidden\", \"visible\"}"
            ],
            "{snapshot:?}: enum cross-check findings drifted"
        );
    }
}
