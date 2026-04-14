//! Regression guard: spec-authoritative element→attribute edges that were
//! previously missing from the snapshot data and are now fixed.
//!
//! Add an entry here whenever a spec gap is confirmed and fixed so it cannot
//! silently regress.

use svg_data::{attribute_for_profile, attributes_for_with_profile, types::SpecSnapshotId};

/// Assert that `element` has `attribute` in the compiled catalog for `snapshot`.
fn assert_edge(snapshot: SpecSnapshotId, element: &str, attribute: &str) {
    let present = attributes_for_with_profile(snapshot, element)
        .iter()
        .any(|pa| pa.attribute.name == attribute);
    assert!(
        present,
        "spec gap regression: {} → {} missing from {} catalog",
        element,
        attribute,
        snapshot.as_str()
    );
}

/// Assert that `attribute` exists in the catalog for `snapshot` at all.
fn assert_attr_in_snapshot(snapshot: SpecSnapshotId, attribute: &str) {
    let present = matches!(
        attribute_for_profile(snapshot, attribute),
        svg_data::ProfileLookup::Present { .. }
    );
    assert!(
        present,
        "spec gap regression: attribute {} missing from {} catalog",
        attribute,
        snapshot.as_str()
    );
}

const SVG2: &[SpecSnapshotId] = &[
    SpecSnapshotId::Svg2Cr20181004,
    SpecSnapshotId::Svg2EditorsDraft20250914,
];

const ALL: &[SpecSnapshotId] = &[
    SpecSnapshotId::Svg11Rec20030114,
    SpecSnapshotId::Svg11Rec20110816,
    SpecSnapshotId::Svg2Cr20181004,
    SpecSnapshotId::Svg2EditorsDraft20250914,
];

#[test]
fn mpath_href_present_in_svg2() {
    for &snapshot in SVG2 {
        assert_edge(snapshot, "mpath", "href");
    }
}

#[test]
fn fe_point_light_position_attrs() {
    for &snapshot in ALL {
        assert_edge(snapshot, "fePointLight", "x");
        assert_edge(snapshot, "fePointLight", "y");
    }
}

#[test]
fn fe_spot_light_position_attrs() {
    for &snapshot in ALL {
        assert_edge(snapshot, "feSpotLight", "x");
        assert_edge(snapshot, "feSpotLight", "y");
    }
}

#[test]
fn fe_component_transfer_input_attr() {
    for &snapshot in ALL {
        assert_edge(snapshot, "feComponentTransfer", "in");
    }
}

#[test]
fn fe_merge_node_input_attr() {
    for &snapshot in ALL {
        assert_edge(snapshot, "feMergeNode", "in");
    }
}

#[test]
fn fe_image_href_in_svg2() {
    for &snapshot in SVG2 {
        assert_edge(snapshot, "feImage", "href");
    }
}

#[test]
fn fe_image_preserve_aspect_ratio() {
    for &snapshot in ALL {
        assert_edge(snapshot, "feImage", "preserveAspectRatio");
    }
}

#[test]
fn transfer_function_type_and_offset() {
    for &snapshot in ALL {
        for element in &["feFuncR", "feFuncG", "feFuncB", "feFuncA"] {
            assert_edge(snapshot, element, "type");
            assert_edge(snapshot, element, "offset");
        }
    }
}

#[test]
fn mask_position_attrs() {
    for &snapshot in ALL {
        assert_edge(snapshot, "mask", "x");
        assert_edge(snapshot, "mask", "y");
    }
}

#[test]
fn filter_position_attrs() {
    for &snapshot in ALL {
        assert_edge(snapshot, "filter", "x");
        assert_edge(snapshot, "filter", "y");
    }
}

#[test]
fn pattern_position_attrs() {
    for &snapshot in ALL {
        assert_edge(snapshot, "pattern", "x");
        assert_edge(snapshot, "pattern", "y");
    }
}

#[test]
fn animate_value_attrs() {
    for &snapshot in ALL {
        assert_edge(snapshot, "animate", "by");
        assert_edge(snapshot, "animate", "values");
        assert_edge(snapshot, "animate", "calcMode");
    }
}

#[test]
fn animate_motion_timing_attrs() {
    for &snapshot in ALL {
        assert_edge(snapshot, "animateMotion", "dur");
        assert_edge(snapshot, "animateMotion", "from");
        assert_edge(snapshot, "animateMotion", "to");
        assert_edge(snapshot, "animateMotion", "by");
        assert_edge(snapshot, "animateMotion", "values");
        assert_edge(snapshot, "animateMotion", "repeatCount");
    }
}

#[test]
fn animate_transform_core_attrs() {
    for &snapshot in ALL {
        assert_edge(snapshot, "animateTransform", "attributeName");
        assert_edge(snapshot, "animateTransform", "dur");
        assert_edge(snapshot, "animateTransform", "values");
        assert_edge(snapshot, "animateTransform", "repeatCount");
        assert_edge(snapshot, "animateTransform", "calcMode");
    }
}

#[test]
fn set_core_attrs() {
    for &snapshot in ALL {
        assert_edge(snapshot, "set", "attributeName");
        assert_edge(snapshot, "set", "dur");
    }
}

#[test]
fn text_path_method_in_svg2() {
    for &snapshot in SVG2 {
        assert_edge(snapshot, "textPath", "method");
    }
    // Confirm these attrs exist in the catalog at all
    for &snapshot in SVG2 {
        assert_attr_in_snapshot(snapshot, "method");
    }
}
