//! Locks the additive, edition-keyed inventory layer.
//!
//! The curated four-snapshot [`svg_data::spec_inventory`] API keys inventories
//! by [`SpecSnapshotId`]. This layer is **additive**: it keys the *same* baked
//! inventories — plus the two older SVG 2 Candidate Recommendations that have no
//! `SpecSnapshotId` (2016-09-15, 2018-08-07) — by their natural
//! [`EditionId`] (`(Series, date)`), through one uniform
//! [`svg_data::for_edition`] entry point. This test pins:
//!
//! 1. **resolution** — every registered edition (the four curated snapshots
//!    *and* the two extra CRs) resolves to a baked inventory through the
//!    edition key; an unregistered edition resolves to `None`;
//! 2. **snapshot bridge** — each `SpecSnapshotId`'s edition key resolves to the
//!    *same* `&'static Inventory` the curated `for_snapshot` returns, so the two
//!    APIs never diverge;
//! 3. **counts** — element/attribute/edge totals of the two newly-registered CR
//!    inventories are locked (each CR has dozens of elements, hundreds of
//!    attributes);
//! 4. **cross-edition comparison** — concrete attribute/element membership
//!    differences across the three CR editions (the 2016 CR's `hatch`/`mesh`
//!    elements and `d`/`hatchUnits` attributes dropped by 2018-10; the
//!    `ping`/`referrerpolicy`/`on*` clipboard attributes added by 2018-10).

use std::collections::BTreeSet;

use svg_data::{
    SpecSnapshotId,
    edition::Series,
    inventory::{
        Classification, EditionDate, EditionId, Inventory, for_edition, registered_editions,
    },
    spec_inventory,
};

/// Resolve an edition or panic with a clear message (avoids `.expect()`, which
/// the workspace `expect_used` lint denies).
fn require_edition(id: &EditionId) -> &'static Inventory {
    let Some(inventory) = for_edition(id) else {
        panic!("edition {id:?} should have a baked inventory")
    };
    inventory
}

/// Look up an attribute by name or panic (avoids `.expect()`, which the
/// workspace `expect_used` lint denies).
fn require_named_attribute<'a>(
    inventory: &'a Inventory,
    name: &str,
) -> &'a svg_data::inventory::Attribute {
    let Some(attribute) = inventory.attribute(name) else {
        panic!("`{name}` should be in the inventory")
    };
    attribute
}

/// The set of attribute names in an inventory.
fn attribute_names(inventory: &Inventory) -> BTreeSet<&str> {
    inventory
        .attributes
        .iter()
        .map(|attribute| attribute.name.as_ref())
        .collect()
}

/// The set of element names in an inventory.
fn element_names(inventory: &Inventory) -> BTreeSet<&str> {
    inventory
        .elements
        .iter()
        .map(|element| element.name.as_ref())
        .collect()
}

#[test]
fn every_registered_edition_resolves() {
    // The four curated snapshots, keyed by their natural edition id.
    for snapshot in [
        SpecSnapshotId::Svg11Rec20030114,
        SpecSnapshotId::Svg11Rec20110816,
        SpecSnapshotId::Svg2Cr20181004,
        SpecSnapshotId::Svg2EditorsDraft,
    ] {
        let id = EditionId::for_snapshot(snapshot);
        assert!(
            for_edition(&id).is_some(),
            "snapshot {snapshot:?} should resolve through its edition key {id:?}"
        );
    }

    // The two *additional* SVG 2 CRs that have no `SpecSnapshotId` — the whole
    // point of the additive layer.
    for date in ["2016-09-15", "2018-08-07"] {
        let id = EditionId::dated(Series::Svg2, date);
        assert!(
            for_edition(&id).is_some(),
            "non-snapshot SVG2 CR {date} should resolve through its edition key"
        );
    }

    // The SVG 1.0 REC and the SVG 1.1 PR — additive editions with no
    // `SpecSnapshotId`, baked from their vendored flat DTDs.
    assert!(
        for_edition(&EditionId::dated(Series::Svg10, "2001-09-04")).is_some(),
        "SVG 1.0 REC should resolve through its edition key"
    );
    assert!(
        for_edition(&EditionId::dated(Series::Svg11, "2011-06-09")).is_some(),
        "SVG 1.1 PR should resolve through its edition key"
    );

    // The SVG 1.1 RECs and the editor's draft resolve through their keys too.
    assert!(for_edition(&EditionId::dated(Series::Svg11, "2003-01-14")).is_some());
    assert!(for_edition(&EditionId::dated(Series::Svg11, "2011-08-16")).is_some());
    assert!(for_edition(&EditionId::editors_draft(Series::Svg2)).is_some());
}

#[test]
fn unregistered_edition_resolves_to_none() {
    // A date with no vendored inventory.
    assert!(for_edition(&EditionId::dated(Series::Svg2, "1999-01-01")).is_none());
    // A series/date pair that exists in another series but not this one.
    assert!(for_edition(&EditionId::dated(Series::Svg10, "2003-01-14")).is_none());
    // SVG 1.1 has no rolling editor's draft.
    assert!(for_edition(&EditionId::editors_draft(Series::Svg11)).is_none());
}

#[test]
fn registered_editions_lists_all_editions() {
    let editions = registered_editions();
    // Eight registered editions: 1 SVG 1.0 REC + 1 SVG 1.1 PR + 2 SVG 1.1 RECs
    // + 3 SVG 2 CRs + 1 editor's draft.
    assert_eq!(editions.len(), 8, "registered edition count drifted");

    let expected: BTreeSet<EditionId> = [
        EditionId::dated(Series::Svg10, "2001-09-04"),
        EditionId::dated(Series::Svg11, "2003-01-14"),
        EditionId::dated(Series::Svg11, "2011-06-09"),
        EditionId::dated(Series::Svg11, "2011-08-16"),
        EditionId::dated(Series::Svg2, "2016-09-15"),
        EditionId::dated(Series::Svg2, "2018-08-07"),
        EditionId::dated(Series::Svg2, "2018-10-04"),
        EditionId::editors_draft(Series::Svg2),
    ]
    .into_iter()
    .collect();
    let actual: BTreeSet<EditionId> = editions.into_iter().collect();
    assert_eq!(actual, expected, "registered edition set drifted");

    // Every listed edition resolves (no dangling registration).
    for id in &expected {
        assert!(
            for_edition(id).is_some(),
            "registered edition {id:?} should resolve"
        );
    }
}

#[test]
fn snapshot_bridge_matches_for_snapshot() {
    // Each curated snapshot's edition key must resolve to the *exact same*
    // baked inventory the curated `for_snapshot`/`spec_inventory` API returns —
    // the edition layer reuses, never re-bakes, the snapshot inventories.
    for snapshot in [
        SpecSnapshotId::Svg11Rec20030114,
        SpecSnapshotId::Svg11Rec20110816,
        SpecSnapshotId::Svg2Cr20181004,
        SpecSnapshotId::Svg2EditorsDraft,
    ] {
        let Some(via_snapshot) = spec_inventory(snapshot) else {
            panic!("snapshot {snapshot:?} should have a curated inventory")
        };
        let via_edition = require_edition(&EditionId::for_snapshot(snapshot));
        assert!(
            std::ptr::eq(via_snapshot, via_edition),
            "edition key for {snapshot:?} should resolve to the same inventory as for_snapshot"
        );
    }
}

#[test]
fn editors_draft_edition_id_is_undated() {
    // The rolling editor's draft has no `/TR/` date, so its edition key uses the
    // undated sentinel — it must never masquerade as a dated edition.
    let id = EditionId::for_snapshot(SpecSnapshotId::Svg2EditorsDraft);
    assert_eq!(id.series, Series::Svg2);
    assert_eq!(id.date, EditionDate::EditorsDraft);
}

#[test]
fn new_cr_edition_counts_are_locked() {
    // SVG 2 CR 2016-09-15: the largest CR (still carried the `hatch`/`mesh`
    // paint-server families later dropped). Dozens of elements, hundreds of
    // attributes.
    let cr2016 = require_edition(&EditionId::dated(Series::Svg2, "2016-09-15"));
    assert_eq!(cr2016.elements.len(), 77, "CR-2016 element count drifted");
    assert_eq!(
        cr2016.attributes.len(),
        260,
        "CR-2016 attribute count drifted"
    );
    assert_eq!(cr2016.edges.len(), 5026, "CR-2016 edge count drifted");

    // SVG 2 CR 2018-08-07: the immediate predecessor of the curated 2018-10 CR.
    let cr201808 = require_edition(&EditionId::dated(Series::Svg2, "2018-08-07"));
    assert_eq!(
        cr201808.elements.len(),
        69,
        "CR-2018-08 element count drifted"
    );
    assert_eq!(
        cr201808.attributes.len(),
        259,
        "CR-2018-08 attribute count drifted"
    );
    assert_eq!(cr201808.edges.len(), 4554, "CR-2018-08 edge count drifted");

    // Sanity: every CR carries dozens of elements and hundreds of attributes.
    for inventory in [cr2016, cr201808] {
        assert!(
            inventory.elements.len() >= 20,
            "a CR should declare dozens of elements"
        );
        assert!(
            inventory.attributes.len() >= 100,
            "a CR should declare hundreds of attributes"
        );
    }
}

#[test]
fn cross_edition_comparison_works() {
    let cr2016 = require_edition(&EditionId::dated(Series::Svg2, "2016-09-15"));
    let cr201808 = require_edition(&EditionId::dated(Series::Svg2, "2018-08-07"));
    let Some(cr201810) = spec_inventory(SpecSnapshotId::Svg2Cr20181004) else {
        panic!("Svg2Cr20181004 should have a baked inventory")
    };

    let attrs2016 = attribute_names(cr2016);
    let attrs201810 = attribute_names(cr201810);
    let els2016 = element_names(cr2016);
    let els201810 = element_names(cr201810);

    // Elements the 2016 CR carried that the 2018-10 CR dropped: the `hatch` and
    // `mesh` paint-server families, plus `cursor` and `solidcolor`.
    let dropped_elements: Vec<&str> = els2016.difference(&els201810).copied().collect();
    assert_eq!(
        dropped_elements,
        vec![
            "cursor",
            "hatch",
            "hatchpath",
            "mesh",
            "meshgradient",
            "meshpatch",
            "meshrow",
            "solidcolor",
        ],
        "2016->2018-10 dropped-element set drifted"
    );
    // 2018-10 introduced no element absent from 2016 (it is an element subset).
    assert!(
        els201810.difference(&els2016).next().is_none(),
        "2018-10 should declare no element the 2016 CR lacked"
    );

    // Attributes the 2016 CR carried that 2018-10 dropped (the hatch geometry
    // attributes and the `d` presentation attribute experiment, plus `pitch`).
    let dropped_attrs: Vec<&str> = attrs2016.difference(&attrs201810).copied().collect();
    assert_eq!(
        dropped_attrs,
        vec!["d", "hatchContentUnits", "hatchUnits", "pitch"],
        "2016->2018-10 dropped-attribute set drifted"
    );

    // Attributes 2018-10 added over 2016: the clipboard `on*` handlers and the
    // `ping`/`referrerpolicy` hyperlink attributes.
    let added_attrs: Vec<&str> = attrs201810.difference(&attrs2016).copied().collect();
    assert_eq!(
        added_attrs,
        vec!["oncopy", "oncut", "onpaste", "ping", "referrerpolicy"],
        "2016->2018-10 added-attribute set drifted"
    );

    // The 2018-08 CR is the immediate predecessor of 2018-10: identical but for
    // the two hyperlink attributes 2018-10 added.
    let attrs201808 = attribute_names(cr201808);
    let added_since_201808: Vec<&str> = attrs201810.difference(&attrs201808).copied().collect();
    assert_eq!(
        added_since_201808,
        vec!["ping", "referrerpolicy"],
        "2018-08->2018-10 added-attribute set drifted"
    );
    assert!(
        attrs201808.difference(&attrs201810).next().is_none(),
        "2018-08 should carry no attribute 2018-10 dropped"
    );
}

#[test]
fn svg10_and_svg11_pr_editions_are_locked() {
    // SVG 1.0 (2001-09-04): the pre-modular REC, baked from its flat DTD through
    // the same `build/dtd.rs` reader as the SVG 1.1 editions (SVG 1.0's flat
    // attribute-collection naming is handled by the parser's allowlist).
    let svg10 = require_edition(&EditionId::dated(Series::Svg10, "2001-09-04"));
    assert_eq!(svg10.elements.len(), 81, "SVG 1.0 element count drifted");
    assert_eq!(
        svg10.attributes.len(),
        268,
        "SVG 1.0 attribute count drifted"
    );
    assert_eq!(svg10.edges.len(), 2822, "SVG 1.0 edge count drifted");

    // `rect` carries its geometry attributes, resolved through the edges.
    let svg10_rect: BTreeSet<&str> = svg10
        .attributes_for_element("rect")
        .map(|attribute| attribute.name.as_ref())
        .collect();
    for geometry in ["x", "y", "width", "height", "rx", "ry"] {
        assert!(
            svg10_rect.contains(geometry),
            "SVG 1.0 rect should carry `{geometry}`: {svg10_rect:?}"
        );
    }
    assert!(
        svg10_rect.contains("transform"),
        "SVG 1.0 rect should carry `transform`"
    );

    // SVG 1.0's pre-modular collections classify through the shared taxonomy:
    // `id` (from `stdAttrs`) is Core, `fill` (from a `PresentationAttributes-*`
    // collection) is Presentation, `requiredFeatures` (from `testAttrs`) is
    // ConditionalProcessing — none are fabricated, all derive from the DTD group.
    let id = require_named_attribute(svg10, "id");
    assert!(
        id.classifications.contains(&Classification::Core),
        "SVG 1.0 `id` should be Core: {:?}",
        id.classifications
    );
    let fill = require_named_attribute(svg10, "fill");
    assert!(
        fill.classifications.contains(&Classification::Presentation),
        "SVG 1.0 `fill` should be Presentation"
    );
    let required_features = require_named_attribute(svg10, "requiredFeatures");
    assert!(
        required_features
            .classifications
            .contains(&Classification::ConditionalProcessing),
        "SVG 1.0 `requiredFeatures` should be ConditionalProcessing"
    );
    // The verbatim DTD group name survives as provenance (no datum dropped).
    assert!(
        id.raw_categories
            .iter()
            .any(|category| category.as_ref() == "stdAttrs"),
        "SVG 1.0 `id` provenance should name `stdAttrs`: {:?}",
        id.raw_categories
    );

    // SVG 1.1 PR (2011-06-09): the Proposed Recommendation DTD is identical to
    // the 2011-08-16 Second Edition REC DTD, so its inventory must match the
    // 2011-08-16 REC inventory exactly (same counts and members).
    let svg11_pr = require_edition(&EditionId::dated(Series::Svg11, "2011-06-09"));
    assert_eq!(
        svg11_pr.elements.len(),
        80,
        "SVG 1.1 PR element count drifted"
    );
    assert_eq!(
        svg11_pr.attributes.len(),
        268,
        "SVG 1.1 PR attribute count drifted"
    );
    assert_eq!(svg11_pr.edges.len(), 4352, "SVG 1.1 PR edge count drifted");

    let svg11_se = require_edition(&EditionId::dated(Series::Svg11, "2011-08-16"));
    assert_eq!(
        element_names(svg11_pr),
        element_names(svg11_se),
        "SVG 1.1 PR element set should equal the 2011-08-16 REC"
    );
    assert_eq!(
        attribute_names(svg11_pr),
        attribute_names(svg11_se),
        "SVG 1.1 PR attribute set should equal the 2011-08-16 REC"
    );

    let svg11_pr_rect: BTreeSet<&str> = svg11_pr
        .attributes_for_element("rect")
        .map(|attribute| attribute.name.as_ref())
        .collect();
    for geometry in ["x", "y", "width", "height", "rx", "ry"] {
        assert!(
            svg11_pr_rect.contains(geometry),
            "SVG 1.1 PR rect should carry `{geometry}`"
        );
    }
}

#[test]
fn svg11_rec_inventories_are_unchanged_by_new_editions() {
    // Registering the SVG 1.0 REC and SVG 1.1 PR is purely additive: the two
    // frozen SVG 1.1 REC inventories must keep their locked counts. (The full
    // bucket/edge audit lives in `tests/svg11_inventory.rs`; this guards the
    // edition-keyed view specifically.)
    let first = require_edition(&EditionId::dated(Series::Svg11, "2003-01-14"));
    assert_eq!(first.elements.len(), 81, "SVG 1.1 FE element count drifted");
    assert_eq!(
        first.attributes.len(),
        268,
        "SVG 1.1 FE attribute count drifted"
    );
    assert_eq!(first.edges.len(), 2930, "SVG 1.1 FE edge count drifted");

    let second = require_edition(&EditionId::dated(Series::Svg11, "2011-08-16"));
    assert_eq!(
        second.elements.len(),
        80,
        "SVG 1.1 SE element count drifted"
    );
    assert_eq!(
        second.attributes.len(),
        268,
        "SVG 1.1 SE attribute count drifted"
    );
    assert_eq!(second.edges.len(), 4352, "SVG 1.1 SE edge count drifted");

    // The edition-keyed lookup returns the *same* static the snapshot API does.
    let Some(via_snapshot) = spec_inventory(SpecSnapshotId::Svg11Rec20110816) else {
        panic!("Svg11Rec20110816 should have a curated inventory")
    };
    assert!(
        std::ptr::eq(via_snapshot, second),
        "SVG 1.1 SE edition key should resolve to the same inventory as the snapshot API"
    );
}
