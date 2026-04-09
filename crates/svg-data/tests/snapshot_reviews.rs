//! Regression coverage for deterministic snapshot review reports.

use std::{fs, path::Path};

use svg_data::{
    review::{ReviewInput, build_review},
    snapshot_schema::{
        CategoriesFile, ElementAttributeMatrixFile, ExceptionsFile, GrammarFile, ReviewFile,
        SnapshotAttributeRecord, SnapshotElementRecord,
    },
    spec_snapshots,
};

#[test]
fn checked_in_snapshot_reviews_match_derived_audit() {
    for snapshot in spec_snapshots() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("data/specs/{}", snapshot.as_str()));
        let elements: Vec<SnapshotElementRecord> = read_json(&root.join("elements.json"));
        let attributes: Vec<SnapshotAttributeRecord> = read_json(&root.join("attributes.json"));
        let grammars: GrammarFile = read_json(&root.join("grammars.json"));
        let categories: CategoriesFile = read_json(&root.join("categories.json"));
        let element_attribute_matrix: ElementAttributeMatrixFile =
            read_json(&root.join("element_attribute_matrix.json"));
        let exceptions: ExceptionsFile = read_json(&root.join("exceptions.json"));
        let expected: ReviewFile = read_json(&root.join("review.json"));

        let actual = build_review(ReviewInput {
            elements: &elements,
            attributes: &attributes,
            grammars: &grammars,
            categories: &categories,
            element_attribute_matrix: &element_attribute_matrix,
            exceptions: &exceptions,
            manual_notes: &expected.manual_notes,
        });

        assert_eq!(actual, expected, "review drift for {}", snapshot.as_str());
        assert!(
            actual.unresolved.is_empty(),
            "unresolved review issues for {}",
            snapshot.as_str()
        );
        assert!(
            actual
                .applicability
                .elements_missing_matrix_entries
                .is_empty()
        );
        assert_eq!(actual.exception_inventory.total, actual.counts.exceptions);
        assert_eq!(actual.provenance.elements.missing, 0);
        assert_eq!(actual.provenance.attributes.missing, 0);
        assert_eq!(actual.provenance.grammars.missing, 0);
        assert_eq!(actual.provenance.applicability_edges.missing, 0);
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
