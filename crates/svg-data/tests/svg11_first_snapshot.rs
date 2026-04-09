//! Regression coverage for checked-in snapshot seeds.

use std::{collections::BTreeSet, fs, path::Path};

use svg_data::{
    ProfileLookup, attribute_for_profile, attributes as catalog_attributes,
    attributes_for_with_profile, elements_with_profile,
    snapshot_schema::{
        CategoriesFile, ElementAttributeMatrixFile, GrammarFile, ReviewFile,
        SnapshotAttributeRecord, SnapshotElementRecord, SnapshotMetadataFile, ValueSyntax,
    },
    types::{AttributeValues, SpecSnapshotId},
};

#[test]
fn svg11_first_snapshot_matches_profile_seed() {
    assert_svg11_snapshot_matches_profile_seed(SpecSnapshotId::Svg11Rec20030114, "2003-01-14", 4);
}

#[test]
fn svg11_second_snapshot_matches_profile_seed() {
    assert_svg11_snapshot_matches_profile_seed(SpecSnapshotId::Svg11Rec20110816, "2011-08-16", 5);
}

#[test]
fn svg2_cr_snapshot_matches_profile_seed() {
    assert_svg2_snapshot_matches_profile_seed(
        SpecSnapshotId::Svg2Cr20181004,
        "2018-10-04",
        6,
        "foreign grammar and module references",
    );
}

#[test]
fn svg2_editors_draft_snapshot_matches_profile_seed() {
    assert_svg2_snapshot_matches_profile_seed(
        SpecSnapshotId::Svg2EditorsDraft20250914,
        "2025-09-14",
        6,
        "pinned svgwg commit and `definitions.xml`",
    );
}

fn assert_svg11_snapshot_matches_profile_seed(
    snapshot: SpecSnapshotId,
    expected_date: &str,
    expected_pinned_sources: usize,
) {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("data/specs/{}", snapshot.as_str()));

    let metadata: SnapshotMetadataFile = read_json(&root.join("snapshot.json"));
    let elements: Vec<SnapshotElementRecord> = read_json(&root.join("elements.json"));
    let snapshot_attributes: Vec<SnapshotAttributeRecord> =
        read_json(&root.join("attributes.json"));
    let grammars: GrammarFile = read_json(&root.join("grammars.json"));
    let categories: CategoriesFile = read_json(&root.join("categories.json"));
    let matrix: ElementAttributeMatrixFile = read_json(&root.join("element_attribute_matrix.json"));
    let review: ReviewFile = read_json(&root.join("review.json"));

    assert_eq!(metadata.snapshot, snapshot);
    assert_eq!(metadata.date, expected_date);
    assert_eq!(metadata.pinned_sources.len(), expected_pinned_sources);

    let expected_elements: BTreeSet<&str> = elements_with_profile(snapshot)
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
        .filter_map(
            |attribute| match attribute_for_profile(snapshot, attribute.name) {
                ProfileLookup::Present { value, .. } => Some(value.name),
                ProfileLookup::UnsupportedInProfile { .. } | ProfileLookup::Unknown => None,
            },
        )
        .collect();
    let actual_attributes: BTreeSet<&str> = snapshot_attributes
        .iter()
        .map(|attribute| attribute.name.as_str())
        .collect();
    assert_eq!(actual_attributes, expected_attributes);

    let expected_edges: BTreeSet<(String, String)> = elements_with_profile(snapshot)
        .iter()
        .flat_map(|profiled| {
            attributes_for_with_profile(snapshot, profiled.element.name)
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
    assert_eq!(review.counts.grammars, grammars.grammars.len());
    assert_eq!(review.counts.applicability_edges, matrix.edges.len());
    assert_eq!(review.counts.exceptions, 0);
    assert!(review.unresolved.is_empty());
    assert!(
        review
            .manual_notes
            .iter()
            .any(|note| note.contains(snapshot.as_str()))
    );

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
    assert!(!grammars.grammars.is_empty());

    assert_snapshot_value_syntax_matches_catalog(&snapshot_attributes, &grammars, snapshot);

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

fn assert_svg2_snapshot_matches_profile_seed(
    snapshot: SpecSnapshotId,
    expected_date: &str,
    expected_pinned_sources: usize,
    expected_review_note: &str,
) {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("data/specs/{}", snapshot.as_str()));

    let metadata: SnapshotMetadataFile = read_json(&root.join("snapshot.json"));
    let elements: Vec<SnapshotElementRecord> = read_json(&root.join("elements.json"));
    let snapshot_attributes: Vec<SnapshotAttributeRecord> =
        read_json(&root.join("attributes.json"));
    let grammars: GrammarFile = read_json(&root.join("grammars.json"));
    let categories: CategoriesFile = read_json(&root.join("categories.json"));
    let matrix: ElementAttributeMatrixFile = read_json(&root.join("element_attribute_matrix.json"));
    let review: ReviewFile = read_json(&root.join("review.json"));

    assert_eq!(metadata.snapshot, snapshot);
    assert_eq!(metadata.date, expected_date);
    assert_eq!(metadata.pinned_sources.len(), expected_pinned_sources);

    let expected_elements: BTreeSet<&str> = elements_with_profile(snapshot)
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
        .filter_map(
            |attribute| match attribute_for_profile(snapshot, attribute.name) {
                ProfileLookup::Present { value, .. } => Some(value.name),
                ProfileLookup::UnsupportedInProfile { .. } | ProfileLookup::Unknown => None,
            },
        )
        .collect();
    let actual_attributes: BTreeSet<&str> = snapshot_attributes
        .iter()
        .map(|attribute| attribute.name.as_str())
        .collect();
    assert_eq!(actual_attributes, expected_attributes);

    let expected_edges: BTreeSet<(String, String)> = elements_with_profile(snapshot)
        .iter()
        .flat_map(|profiled| {
            attributes_for_with_profile(snapshot, profiled.element.name)
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
    assert_eq!(review.counts.grammars, grammars.grammars.len());
    assert_eq!(review.counts.applicability_edges, matrix.edges.len());
    assert_eq!(review.counts.exceptions, 0);
    assert!(review.unresolved.is_empty());
    assert!(
        review
            .manual_notes
            .iter()
            .any(|note| note.contains(expected_review_note))
    );

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
    assert!(!grammars.grammars.is_empty());

    assert_snapshot_value_syntax_matches_catalog(&snapshot_attributes, &grammars, snapshot);

    assert!(
        elements
            .iter()
            .any(|element| element.name == "feDropShadow")
    );
    assert!(
        snapshot_attributes
            .iter()
            .any(|attribute| attribute.name == "href")
    );
    assert!(
        !snapshot_attributes
            .iter()
            .any(|attribute| attribute.name == "xlink:href")
    );
}

fn assert_snapshot_value_syntax_matches_catalog(
    snapshot_attributes: &[SnapshotAttributeRecord],
    grammars: &GrammarFile,
    snapshot: SpecSnapshotId,
) {
    let grammar_ids: BTreeSet<&str> = grammars
        .grammars
        .iter()
        .map(|grammar| grammar.id.as_str())
        .collect();

    assert!(grammar_ids.contains("color"));
    assert!(grammar_ids.contains("length"));
    assert!(grammar_ids.contains("number-or-percentage"));
    assert!(
        grammar_ids
            .iter()
            .any(|grammar_id| grammar_id.starts_with("transform-list"))
    );
    assert!(grammar_ids.contains("view-box"));
    assert!(grammar_ids.contains("preserve-aspect-ratio"));
    assert!(grammar_ids.contains("points"));
    assert!(grammar_ids.contains("path-data"));
    assert!(grammar_ids.contains("url-reference"));

    for catalog_attribute in catalog_attributes() {
        let Some(snapshot_attribute) = snapshot_attributes
            .iter()
            .find(|attribute| attribute.name == catalog_attribute.name)
        else {
            match attribute_for_profile(snapshot, catalog_attribute.name) {
                ProfileLookup::UnsupportedInProfile { .. } | ProfileLookup::Unknown => continue,
                ProfileLookup::Present { .. } => {
                    panic!("snapshot attribute missing for {}", catalog_attribute.name);
                }
            }
        };

        match &catalog_attribute.values {
            AttributeValues::FreeText => {
                assert!(matches!(
                    snapshot_attribute.value_syntax,
                    ValueSyntax::Opaque { .. }
                ));
            }
            _ => match &snapshot_attribute.value_syntax {
                ValueSyntax::GrammarRef { grammar_id } => {
                    assert!(grammar_ids.contains(grammar_id.as_str()));
                }
                ValueSyntax::ForeignRef { .. } | ValueSyntax::Opaque { .. } => {
                    panic!(
                        "expected normalized grammar ref for {} in {}",
                        catalog_attribute.name,
                        snapshot.as_str()
                    );
                }
            },
        }
    }
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
