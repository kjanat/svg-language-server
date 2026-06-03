//! Locks the baked full-spec SVG 2 ED inventory and its public query API.
//!
//! This is the additive, spec-faithful companion to the curated catalog
//! (locked elsewhere). It asserts the inventory's audited shape (63 elements /
//! 313 attributes / 6752 edges, matching `tests/ed_presence_matrix.rs`), that
//! the classification buckets match the audited figures, that the snapshot-
//! keyed query API resolves element attributes and filters by classification,
//! and that raw upstream provenance survives on every record.
//!
//! Only the `Svg2EditorsDraft` snapshot carries a baked inventory; the older
//! snapshots are intentionally inventory-less (they are served by the curated
//! profile-aware catalog), which this test also pins.

use svg_data::inventory::{Attribute, Classification};
use svg_data::{
    SpecSnapshotId, spec_attributes, spec_attributes_for_element, spec_elements, spec_inventory,
};

const ED: SpecSnapshotId = SpecSnapshotId::Svg2EditorsDraft;

/// Resolve the ED inventory or fail loudly. Avoids `.expect()`, denied by the
/// workspace `expect_used` lint.
fn ed_inventory() -> &'static svg_data::inventory::Inventory {
    let Some(inventory) = spec_inventory(ED) else {
        panic!("Svg2EditorsDraft must carry a baked spec inventory")
    };
    inventory
}

/// Find an attribute record by name or fail loudly.
fn require_attribute(name: &str) -> &'static Attribute {
    let Some(attribute) = ed_inventory().attribute(name) else {
        panic!("`{name}` should be present in the ED spec inventory")
    };
    attribute
}

#[test]
fn ed_inventory_matches_audited_extractor_figures() {
    let inventory = ed_inventory();
    // Same figures locked by `tests/ed_presence_matrix.rs` at pin `19482daf`.
    assert_eq!(inventory.elements.len(), 63, "element count drifted");
    assert_eq!(inventory.attributes.len(), 313, "attribute count drifted");
    assert_eq!(inventory.edges.len(), 6752, "edge count drifted");

    // The snapshot-keyed convenience accessors agree with the inventory.
    assert_eq!(spec_elements(ED).len(), 63);
    assert_eq!(spec_attributes(ED).len(), 313);
}

#[test]
fn cr_snapshot_has_no_baked_inventory() {
    // Only the SVG 2 CR snapshot lacks a vendored machine-readable grammar of
    // its own, so it alone has no baked inventory. The two SVG 1.1
    // Recommendations now carry DTD-derived inventories (see
    // `tests/svg11_inventory.rs`), and the ED carries its `definitions*.xml`
    // inventory — those are covered elsewhere.
    let snapshot = SpecSnapshotId::Svg2Cr20181004;
    assert!(
        spec_inventory(snapshot).is_none(),
        "{snapshot:?} should not carry a baked full-spec inventory",
    );
    // The enumerating convenience accessors degrade to empty, not panic.
    assert!(spec_attributes(snapshot).is_empty());
    assert!(spec_elements(snapshot).is_empty());
    assert_eq!(
        spec_attributes_for_element(snapshot, "text").count(),
        0,
        "inventory-less snapshot must yield no element attributes",
    );
}

#[test]
fn svg11_snapshots_now_carry_baked_inventories() {
    // Both SVG 1.1 editions return a non-empty baked inventory derived from
    // their vendored flat DTD. Exact counts/buckets are locked in
    // `tests/svg11_inventory.rs`; here we only assert the API contract flipped
    // from `None` to `Some` and the convenience accessors are non-empty.
    for snapshot in [
        SpecSnapshotId::Svg11Rec20030114,
        SpecSnapshotId::Svg11Rec20110816,
    ] {
        assert!(
            spec_inventory(snapshot).is_some(),
            "{snapshot:?} should carry a baked SVG 1.1 inventory",
        );
        assert!(!spec_attributes(snapshot).is_empty());
        assert!(!spec_elements(snapshot).is_empty());
        // `rect` is a shape element in both editions and carries attributes.
        assert!(
            spec_attributes_for_element(snapshot, "rect").count() > 0,
            "{snapshot:?} rect should resolve attributes",
        );
    }
}

#[test]
fn classification_buckets_match_audited_figures() {
    let inventory = ed_inventory();

    // Aria: the 48 `aria-*` attributes plus `role` => 49.
    assert_eq!(
        inventory.count_with_classification(&Classification::Aria),
        49,
        "aria bucket drifted",
    );
    // Event handlers (`on*`) are present in force.
    assert_eq!(
        inventory.count_with_classification(&Classification::EventHandler),
        65,
        "event-handler bucket drifted",
    );
    // The deprecated `xlink:*` family is preserved.
    assert_eq!(
        inventory.count_with_classification(&Classification::Xlink),
        2,
        "xlink bucket drifted",
    );
    // Presentation attributes (CSS-property-backed) are the largest named bucket.
    assert_eq!(
        inventory.count_with_classification(&Classification::Presentation),
        62,
        "presentation bucket drifted",
    );
    assert_eq!(
        inventory.count_with_classification(&Classification::Core),
        7,
        "core bucket drifted",
    );
}

#[test]
fn deprecated_presentation_attributes_are_present() {
    // SVG 2 deprecated these presentation attributes but the spec inventory
    // must keep them so a lint can flag them — no spec datum dropped.
    for name in ["clip", "kerning"] {
        let attribute = require_attribute(name);
        assert!(
            attribute
                .classifications
                .contains(&Classification::Presentation),
            "`{name}` should classify as Presentation: {:?}",
            attribute.classifications,
        );
    }
}

#[test]
fn raw_provenance_is_present_on_classified_attributes() {
    // Every attribute that normalized to a named classification keeps the
    // verbatim upstream `attributecategory` string(s) for provenance.
    let fill = require_attribute("fill");
    assert!(
        fill.classifications.contains(&Classification::Presentation),
        "fill should be Presentation",
    );
    assert!(
        !fill.raw_categories.is_empty(),
        "fill must retain its raw upstream category provenance",
    );

    // `onunload` rides two distinct raw event categories that both collapse to
    // a single EventHandler classification: provenance distinguishes them.
    let onunload = require_attribute("onunload");
    let raw: Vec<&str> = onunload
        .raw_categories
        .iter()
        .map(std::convert::AsRef::as_ref)
        .collect();
    assert!(raw.contains(&"document event"), "raw categories: {raw:?}");
    assert!(raw.contains(&"window event"), "raw categories: {raw:?}");
}

#[test]
fn attributes_for_text_include_expected_text_attributes() {
    let resolved: Vec<&str> = spec_attributes_for_element(ED, "text")
        .map(|attribute| attribute.name.as_ref())
        .collect();
    for expected in [
        "x",
        "y",
        "dx",
        "dy",
        "rotate",
        "textLength",
        "lengthAdjust",
        "fill",
        "font-size",
    ] {
        assert!(
            resolved.contains(&expected),
            "text attributes should include `{expected}`; got {resolved:?}",
        );
    }
    // Resolved records carry full facts, not bare names: `fill` is Presentation.
    let fill = resolved.iter().find(|name| **name == "fill");
    assert!(
        fill.is_some(),
        "text should expose the fill attribute record"
    );
}

#[test]
fn excluding_aria_and_event_handlers_shrinks_the_set_materially() {
    let inventory = ed_inventory();
    let total = inventory.attributes.len();

    let kept = inventory
        .attributes_excluding(&[Classification::Aria, Classification::EventHandler])
        .count();

    // Aria (49) and EventHandler (65) are disjoint families, so excluding both
    // drops exactly 114 attributes — a material shrink, not a no-op.
    assert_eq!(kept, total - (49 + 65), "exclusion filter math drifted");
    assert!(kept < total, "exclusion must shrink the set");
    assert_eq!(kept, 199, "kept-attribute count drifted");

    // None of the kept attributes carry an excluded classification.
    for attribute in
        inventory.attributes_excluding(&[Classification::Aria, Classification::EventHandler])
    {
        assert!(
            !attribute.classifications.contains(&Classification::Aria),
            "`{}` leaked through the Aria exclusion",
            attribute.name,
        );
        assert!(
            !attribute
                .classifications
                .contains(&Classification::EventHandler),
            "`{}` leaked through the EventHandler exclusion",
            attribute.name,
        );
    }
}

#[test]
fn presentation_only_filter_returns_the_presentation_bucket() {
    let inventory = ed_inventory();
    let presentation: Vec<&str> = inventory
        .attributes_with_any(&[Classification::Presentation])
        .map(|attribute| attribute.name.as_ref())
        .collect();
    assert_eq!(presentation.len(), 62, "presentation-only filter drifted");
    assert!(presentation.contains(&"fill"));
    assert!(presentation.contains(&"stroke"));
    // Aria/event-handler attributes must not appear in a presentation request.
    assert!(!presentation.contains(&"aria-label"));
    assert!(!presentation.contains(&"onclick"));
}
