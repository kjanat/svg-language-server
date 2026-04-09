//! Regression coverage for the checked-in SVG 1.1 first-edition snapshot seed.

use std::{collections::BTreeSet, fs, path::Path};

use svg_data::{
    ProfileLookup, attribute_for_profile, attributes as catalog_attributes,
    attributes_for_with_profile, elements_with_profile,
    snapshot_schema::{
        CategoriesFile, ElementAttributeMatrixFile, ReviewFile, SnapshotAttributeRecord,
        SnapshotElementRecord, SnapshotMetadataFile,
    },
    types::SpecSnapshotId,
};

#[test]
fn svg11_first_snapshot_matches_profile_seed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/specs/Svg11Rec20030114");

    let metadata: SnapshotMetadataFile = read_json(&root.join("snapshot.json"));
    let elements: Vec<SnapshotElementRecord> = read_json(&root.join("elements.json"));
    let snapshot_attributes: Vec<SnapshotAttributeRecord> =
        read_json(&root.join("attributes.json"));
    let categories: CategoriesFile = read_json(&root.join("categories.json"));
    let matrix: ElementAttributeMatrixFile = read_json(&root.join("element_attribute_matrix.json"));
    let review: ReviewFile = read_json(&root.join("review.json"));

    assert_eq!(metadata.snapshot, SpecSnapshotId::Svg11Rec20030114);
    assert_eq!(metadata.date, "2003-01-14");
    assert_eq!(metadata.pinned_sources.len(), 4);

    let expected_elements: BTreeSet<&str> = elements_with_profile(SpecSnapshotId::Svg11Rec20030114)
        .iter()
        .map(|profiled| profiled.element.name)
        .collect();
    let actual_elements: BTreeSet<&str> = elements
        .iter()
        .map(|element| element.name.as_str())
        .collect();
    assert_eq!(actual_elements, expected_elements);

    let expected_attributes: BTreeSet<&str> = catalog_attributes()
        .iter()
        .filter_map(|attribute| {
            match attribute_for_profile(SpecSnapshotId::Svg11Rec20030114, attribute.name) {
                ProfileLookup::Present { value, .. } => Some(value.name),
                ProfileLookup::UnsupportedInProfile { .. } | ProfileLookup::Unknown => None,
            }
        })
        .collect();
    let actual_attributes: BTreeSet<&str> = snapshot_attributes
        .iter()
        .map(|attribute| attribute.name.as_str())
        .collect();
    assert_eq!(actual_attributes, expected_attributes);

    let expected_edges: BTreeSet<(String, String)> =
        elements_with_profile(SpecSnapshotId::Svg11Rec20030114)
            .iter()
            .flat_map(|profiled| {
                attributes_for_with_profile(SpecSnapshotId::Svg11Rec20030114, profiled.element.name)
                    .into_iter()
                    .map(move |attribute| {
                        (
                            profiled.element.name.to_string(),
                            attribute.attribute.name.to_string(),
                        )
                    })
            })
            .collect();
    let actual_edges: BTreeSet<(String, String)> = matrix
        .edges
        .iter()
        .map(|edge| (edge.element.clone(), edge.attribute.clone()))
        .collect();
    assert_eq!(actual_edges, expected_edges);

    assert_eq!(review.counts.elements, elements.len());
    assert_eq!(review.counts.attributes, snapshot_attributes.len());
    assert_eq!(review.counts.applicability_edges, matrix.edges.len());
    assert_eq!(review.counts.exceptions, 0);
    assert!(review.unresolved.is_empty());

    assert!(categories.attribute_categories.is_empty());
    assert!(
        elements
            .iter()
            .all(|element| !element.provenance.is_empty())
    );
    assert!(
        snapshot_attributes
            .iter()
            .all(|attribute| !attribute.provenance.is_empty())
    );
    assert!(matrix.edges.iter().all(|edge| !edge.provenance.is_empty()));

    assert!(elements.iter().any(|element| element.name == "svg"));
    assert!(
        snapshot_attributes
            .iter()
            .any(|attribute| attribute.name == "xlink:href")
    );
    assert!(
        !snapshot_attributes
            .iter()
            .any(|attribute| attribute.name == "href")
    );
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
