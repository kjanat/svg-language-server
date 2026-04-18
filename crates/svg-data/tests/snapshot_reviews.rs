//! Regression coverage for deterministic snapshot review reports.

use std::{collections::BTreeSet, fs, path::Path};

use svg_data::{
    review::{Input, build_report},
    snapshot_schema::{
        CategoriesFile, ElementAttributeMatrixFile, ExceptionsFile, FactProvenance, GrammarFile,
        ReviewFile, SnapshotAttributeRecord, SnapshotElementRecord, SnapshotMetadataFile,
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

        let actual = build_report(Input {
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

#[test]
fn every_provenance_source_id_resolves_to_a_pinned_input() {
    for snapshot in spec_snapshots() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("data/specs/{}", snapshot.as_str()));

        let metadata: SnapshotMetadataFile = read_json(&root.join("snapshot.json"));
        let allowed: BTreeSet<String> = metadata
            .pinned_sources
            .iter()
            .map(|source| source.input_id.clone())
            .collect();

        let elements: Vec<SnapshotElementRecord> = read_json(&root.join("elements.json"));
        let attributes: Vec<SnapshotAttributeRecord> = read_json(&root.join("attributes.json"));
        let grammars: GrammarFile = read_json(&root.join("grammars.json"));
        let categories: CategoriesFile = read_json(&root.join("categories.json"));
        let element_attribute_matrix: ElementAttributeMatrixFile =
            read_json(&root.join("element_attribute_matrix.json"));
        let exceptions: ExceptionsFile = read_json(&root.join("exceptions.json"));

        let mut provenance_sources: Vec<(&'static str, &FactProvenance)> = Vec::new();
        for element in &elements {
            for provenance in &element.provenance {
                provenance_sources.push(("elements.json", provenance));
            }
        }
        for attribute in &attributes {
            for provenance in &attribute.provenance {
                provenance_sources.push(("attributes.json", provenance));
            }
        }
        for grammar in &grammars.grammars {
            for provenance in &grammar.provenance {
                provenance_sources.push(("grammars.json", provenance));
            }
        }
        for membership in &categories.element_categories {
            for provenance in &membership.provenance {
                provenance_sources.push(("categories.json", provenance));
            }
        }
        for membership in &categories.attribute_categories {
            for provenance in &membership.provenance {
                provenance_sources.push(("categories.json", provenance));
            }
        }
        for edge in &element_attribute_matrix.edges {
            for provenance in &edge.provenance {
                provenance_sources.push(("element_attribute_matrix.json", provenance));
            }
        }
        for exception in &exceptions.exceptions {
            for provenance in &exception.provenance {
                provenance_sources.push(("exceptions.json", provenance));
            }
        }

        for (file_name, provenance) in provenance_sources {
            assert!(
                allowed.contains(&provenance.source_id),
                "{}: {} references source_id {:?} that is not pinned in snapshot.json (allowed: {:?})",
                snapshot.as_str(),
                file_name,
                provenance.source_id,
                allowed,
            );
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
