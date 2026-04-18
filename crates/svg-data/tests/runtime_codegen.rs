//! Regression coverage for runtime codegen from reviewed snapshot data.

use std::{collections::BTreeSet, fs, path::Path};

use svg_data::{
    attributes, attributes_for_with_profile, element_for_profile, elements,
    types::{ProfileLookup, SpecSnapshotId},
};

#[derive(serde::Deserialize)]
struct ElementMembershipFile {
    elements: Vec<FeatureMembershipRecord>,
}

#[derive(serde::Deserialize)]
struct AttributeMembershipFile {
    attributes: Vec<FeatureMembershipRecord>,
}

#[derive(serde::Deserialize)]
struct FeatureMembershipRecord {
    name: String,
}

#[derive(serde::Deserialize)]
struct ElementAttributeMatrixFile {
    edges: Vec<ElementAttributeEdge>,
}

#[derive(serde::Deserialize)]
struct ElementAttributeEdge {
    element: String,
    attribute: String,
}

#[test]
fn generated_union_catalog_matches_checked_in_union_membership() {
    let expected_elements: ElementMembershipFile =
        read_json(&derived_root().join("union/elements.json"));
    let expected_attributes: AttributeMembershipFile =
        read_json(&derived_root().join("union/attributes.json"));

    let actual_elements: BTreeSet<&str> = elements().iter().map(|element| element.name).collect();
    let actual_attributes: BTreeSet<&str> = attributes()
        .iter()
        .map(|attribute| attribute.name)
        .collect();

    assert_eq!(
        actual_elements,
        expected_elements
            .elements
            .iter()
            .map(|record| record.name.as_str())
            .collect()
    );
    assert_eq!(
        actual_attributes,
        expected_attributes
            .attributes
            .iter()
            .map(|record| record.name.as_str())
            .collect()
    );
}

#[test]
fn profile_attribute_lookup_matches_checked_in_snapshot_matrix() {
    for snapshot in [
        SpecSnapshotId::Svg11Rec20030114,
        SpecSnapshotId::Svg11Rec20110816,
        SpecSnapshotId::Svg2Cr20181004,
        SpecSnapshotId::Svg2EditorsDraft20250914,
    ] {
        let matrix: ElementAttributeMatrixFile = read_json(
            &specs_root()
                .join(snapshot.as_str())
                .join("element_attribute_matrix.json"),
        );

        let expected_by_element: BTreeSet<(String, String)> = matrix
            .edges
            .into_iter()
            .map(|edge| (edge.element, edge.attribute))
            .collect();

        let actual_by_element: BTreeSet<(String, String)> = elements()
            .iter()
            .filter_map(
                |element| match element_for_profile(snapshot, element.name) {
                    ProfileLookup::Present { value, .. } => Some(value.name),
                    ProfileLookup::UnsupportedInProfile { .. } | ProfileLookup::Unknown => None,
                },
            )
            .flat_map(|element_name| {
                attributes_for_with_profile(snapshot, element_name)
                    .into_iter()
                    .map(move |attribute| {
                        (
                            element_name.to_string(),
                            attribute.attribute.name.to_string(),
                        )
                    })
            })
            .collect();

        assert_eq!(
            actual_by_element,
            expected_by_element,
            "matrix drift for {}",
            snapshot.as_str()
        );
    }
}

fn derived_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/derived")
}

fn specs_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/specs")
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
