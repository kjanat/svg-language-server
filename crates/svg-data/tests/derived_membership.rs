//! Regression coverage for derived union membership and adjacent snapshot overlays.

use std::{collections::BTreeSet, fs, path::Path};

use svg_data::{
    derived::{
        AttributeMembershipFile, ElementMembershipFile, ReviewedSnapshotMembershipInput,
        SnapshotOverlayFile, attribute_set_for_snapshot, build_membership_artifacts,
        element_set_for_snapshot,
    },
    snapshot_schema::{ReviewFile, SnapshotAttributeRecord, SnapshotElementRecord},
    spec_snapshots,
    types::SpecSnapshotId,
};

#[test]
fn checked_in_derived_membership_matches_reviewed_snapshots() {
    let owned_inputs = load_snapshot_inputs();
    let borrowed_inputs: Vec<_> = owned_inputs
        .iter()
        .map(|input| ReviewedSnapshotMembershipInput {
            snapshot: input.snapshot,
            elements: &input.elements,
            attributes: &input.attributes,
            review: &input.review,
        })
        .collect();

    let derived = build_membership_artifacts(&borrowed_inputs)
        .unwrap_or_else(|error| panic!("derivation should succeed: {error}"));
    let expected_elements: ElementMembershipFile =
        read_json(&derived_root().join("union/elements.json"));
    let expected_attributes: AttributeMembershipFile =
        read_json(&derived_root().join("union/attributes.json"));
    let expected_overlays: Vec<SnapshotOverlayFile> = spec_snapshots()
        .windows(2)
        .map(|pair| {
            let [from, to] = pair else {
                unreachable!("adjacent windows always have length two")
            };
            read_json(&derived_root().join(format!(
                "overlays/{}__{}.json",
                from.as_str(),
                to.as_str()
            )))
        })
        .collect();

    assert_eq!(derived.elements, expected_elements);
    assert_eq!(derived.attributes, expected_attributes);
    assert_eq!(derived.overlays, expected_overlays);
}

#[test]
fn union_membership_reconstructs_snapshot_sets_losslessly() {
    let owned_inputs = load_snapshot_inputs();
    let borrowed_inputs: Vec<_> = owned_inputs
        .iter()
        .map(|input| ReviewedSnapshotMembershipInput {
            snapshot: input.snapshot,
            elements: &input.elements,
            attributes: &input.attributes,
            review: &input.review,
        })
        .collect();
    let derived = build_membership_artifacts(&borrowed_inputs)
        .unwrap_or_else(|error| panic!("derivation should succeed: {error}"));

    for input in &owned_inputs {
        let expected_elements: BTreeSet<&str> = input
            .elements
            .iter()
            .map(|record| record.name.as_str())
            .collect();
        let expected_attributes: BTreeSet<&str> = input
            .attributes
            .iter()
            .map(|record| record.name.as_str())
            .collect();

        assert_eq!(
            element_set_for_snapshot(&derived.elements, input.snapshot),
            expected_elements,
            "element membership drift for {}",
            input.snapshot.as_str()
        );
        assert_eq!(
            attribute_set_for_snapshot(&derived.attributes, input.snapshot),
            expected_attributes,
            "attribute membership drift for {}",
            input.snapshot.as_str()
        );
    }
}

#[test]
fn overlays_match_adjacent_snapshot_diffs() {
    let owned_inputs = load_snapshot_inputs();
    let borrowed_inputs: Vec<_> = owned_inputs
        .iter()
        .map(|input| ReviewedSnapshotMembershipInput {
            snapshot: input.snapshot,
            elements: &input.elements,
            attributes: &input.attributes,
            review: &input.review,
        })
        .collect();
    let derived = build_membership_artifacts(&borrowed_inputs)
        .unwrap_or_else(|error| panic!("derivation should succeed: {error}"));

    let overlay_2011_to_2018 = derived
        .overlays
        .iter()
        .find(|overlay| {
            overlay.from_snapshot == SpecSnapshotId::Svg11Rec20110816
                && overlay.to_snapshot == SpecSnapshotId::Svg2Cr20181004
        })
        .unwrap_or_else(|| panic!("expected SVG 1.1 second -> SVG 2 CR overlay"));
    assert_eq!(overlay_2011_to_2018.elements.added, ["feDropShadow"]);
    assert!(overlay_2011_to_2018.elements.removed.is_empty());
    assert_eq!(overlay_2011_to_2018.attributes.added, ["href", "method"]);
    assert_eq!(
        overlay_2011_to_2018.attributes.removed,
        ["xlink:actuate", "xlink:href", "xlink:show", "xlink:title"]
    );

    for (overlay, pair) in derived.overlays.iter().zip(owned_inputs.windows(2)) {
        let [from, to] = pair else {
            unreachable!("adjacent windows always have length two")
        };
        let from_elements: BTreeSet<&str> = from
            .elements
            .iter()
            .map(|record| record.name.as_str())
            .collect();
        let to_elements: BTreeSet<&str> = to
            .elements
            .iter()
            .map(|record| record.name.as_str())
            .collect();
        let from_attributes: BTreeSet<&str> = from
            .attributes
            .iter()
            .map(|record| record.name.as_str())
            .collect();
        let to_attributes: BTreeSet<&str> = to
            .attributes
            .iter()
            .map(|record| record.name.as_str())
            .collect();

        assert_eq!(
            overlay.elements.added,
            to_elements
                .difference(&from_elements)
                .copied()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            overlay.elements.removed,
            from_elements
                .difference(&to_elements)
                .copied()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            overlay.attributes.added,
            to_attributes
                .difference(&from_attributes)
                .copied()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            overlay.attributes.removed,
            from_attributes
                .difference(&to_attributes)
                .copied()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
    }
}

#[derive(Debug)]
struct OwnedSnapshotMembershipInput {
    snapshot: SpecSnapshotId,
    elements: Vec<SnapshotElementRecord>,
    attributes: Vec<SnapshotAttributeRecord>,
    review: ReviewFile,
}

fn load_snapshot_inputs() -> Vec<OwnedSnapshotMembershipInput> {
    spec_snapshots()
        .iter()
        .copied()
        .map(|snapshot| {
            let root = specs_root().join(snapshot.as_str());
            OwnedSnapshotMembershipInput {
                snapshot,
                elements: read_json(&root.join("elements.json")),
                attributes: read_json(&root.join("attributes.json")),
                review: read_json(&root.join("review.json")),
            }
        })
        .collect()
}

fn specs_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/specs")
}

fn derived_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/derived")
}

fn read_json<T>(path: &Path) -> T
where
    T: serde::de::DeserializeOwned,
{
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}
