//! Deterministic audit helpers for checked-in snapshot review reports.

use std::collections::{BTreeMap, BTreeSet};

use crate::snapshot_schema::{
    ApplicabilityCoverage, CategoriesFile, ElementAttributeMatrixFile, ExceptionDisposition,
    ExceptionInventory, ExceptionScope, ExceptionsFile, GrammarDefinition, GrammarFile,
    ProvenanceCoverage, ProvenanceCoverageCount, ReviewCounts, ReviewFile, ReviewIssue,
    ReviewSeverity, SNAPSHOT_SCHEMA_VERSION, SnapshotAttributeRecord, SnapshotElementRecord,
};

/// Borrowed snapshot facts used to derive `review.json`.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    /// `elements.json` payload.
    pub elements: &'a [SnapshotElementRecord],
    /// `attributes.json` payload.
    pub attributes: &'a [SnapshotAttributeRecord],
    /// `grammars.json` payload.
    pub grammars: &'a GrammarFile,
    /// `categories.json` payload.
    pub categories: &'a CategoriesFile,
    /// `element_attribute_matrix.json` payload.
    pub element_attribute_matrix: &'a ElementAttributeMatrixFile,
    /// `exceptions.json` payload.
    pub exceptions: &'a ExceptionsFile,
    /// Human-authored notes to preserve in the review file.
    pub manual_notes: &'a [String],
}

/// Derive a complete review report from checked-in snapshot facts.
#[must_use]
pub fn build_report(input: Input<'_>) -> ReviewFile {
    let counts = ReviewCounts {
        elements: input.elements.len(),
        attributes: input.attributes.len(),
        grammars: input.grammars.grammars.len(),
        applicability_edges: input.element_attribute_matrix.edges.len(),
        exceptions: input.exceptions.exceptions.len(),
    };
    let applicability =
        build_applicability_coverage(input.elements, input.element_attribute_matrix);
    let provenance = ProvenanceCoverage {
        elements: provenance_count(
            input
                .elements
                .iter()
                .map(|record| !record.provenance.is_empty()),
        ),
        attributes: provenance_count(
            input
                .attributes
                .iter()
                .map(|record| !record.provenance.is_empty()),
        ),
        grammars: provenance_count(
            input
                .grammars
                .grammars
                .iter()
                .map(|record| !record.provenance.is_empty()),
        ),
        element_categories: provenance_count(
            input
                .categories
                .element_categories
                .iter()
                .map(|record| !record.provenance.is_empty()),
        ),
        attribute_categories: provenance_count(
            input
                .categories
                .attribute_categories
                .iter()
                .map(|record| !record.provenance.is_empty()),
        ),
        applicability_edges: provenance_count(
            input
                .element_attribute_matrix
                .edges
                .iter()
                .map(|record| !record.provenance.is_empty()),
        ),
        exceptions: provenance_count(
            input
                .exceptions
                .exceptions
                .iter()
                .map(|record| !record.provenance.is_empty()),
        ),
    };
    let exception_inventory = build_exception_inventory(input.exceptions);
    let unresolved = build_unresolved_issues(&ReviewFacts {
        elements: input.elements,
        attributes: input.attributes,
        grammars: &input.grammars.grammars,
        categories: input.categories,
        matrix: input.element_attribute_matrix,
        exceptions: input.exceptions,
        applicability: &applicability,
        provenance: &provenance,
    });

    ReviewFile {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        counts,
        applicability,
        provenance,
        exception_inventory,
        unresolved,
        manual_notes: input.manual_notes.to_vec(),
    }
}

fn build_applicability_coverage(
    elements: &[SnapshotElementRecord],
    matrix: &ElementAttributeMatrixFile,
) -> ApplicabilityCoverage {
    let requiring_matrix_entries: BTreeSet<&str> = elements
        .iter()
        .filter(|element| !element.attributes.is_empty())
        .map(|element| element.name.as_str())
        .collect();
    let matrix_elements: BTreeSet<&str> = matrix
        .edges
        .iter()
        .map(|edge| edge.element.as_str())
        .collect();
    let elements_missing_matrix_entries = requiring_matrix_entries
        .iter()
        .copied()
        .filter(|element| !matrix_elements.contains(element))
        .map(str::to_string)
        .collect();

    ApplicabilityCoverage {
        elements_requiring_matrix_entries: requiring_matrix_entries.len(),
        elements_with_matrix_entries: requiring_matrix_entries
            .iter()
            .filter(|element| matrix_elements.contains(**element))
            .count(),
        elements_missing_matrix_entries,
    }
}

fn provenance_count(has_provenance: impl Iterator<Item = bool>) -> ProvenanceCoverageCount {
    let (total, covered) = has_provenance.fold((0usize, 0usize), |(total, covered), present| {
        (total + 1, covered + usize::from(present))
    });

    ProvenanceCoverageCount {
        total,
        covered,
        missing: total.saturating_sub(covered),
    }
}

fn build_exception_inventory(exceptions: &ExceptionsFile) -> ExceptionInventory {
    let mut corrected = 0usize;
    let mut deferred = 0usize;
    let mut snapshot_scoped = 0usize;
    let mut element_scoped = 0usize;
    let mut attribute_scoped = 0usize;
    let mut element_attribute_scoped = 0usize;
    let mut grammar_scoped = 0usize;
    let mut ids = Vec::with_capacity(exceptions.exceptions.len());

    for exception in &exceptions.exceptions {
        ids.push(exception.id.clone());
        match exception.disposition {
            ExceptionDisposition::Corrected => corrected += 1,
            ExceptionDisposition::Deferred => deferred += 1,
        }
        match &exception.scope {
            ExceptionScope::Snapshot => snapshot_scoped += 1,
            ExceptionScope::Element { .. } => element_scoped += 1,
            ExceptionScope::Attribute { .. } => attribute_scoped += 1,
            ExceptionScope::ElementAttribute { .. } => element_attribute_scoped += 1,
            ExceptionScope::Grammar { .. } => grammar_scoped += 1,
        }
    }

    ExceptionInventory {
        total: exceptions.exceptions.len(),
        corrected,
        deferred,
        snapshot_scoped,
        element_scoped,
        attribute_scoped,
        element_attribute_scoped,
        grammar_scoped,
        ids,
    }
}

struct ReviewFacts<'a> {
    elements: &'a [SnapshotElementRecord],
    attributes: &'a [SnapshotAttributeRecord],
    grammars: &'a [GrammarDefinition],
    categories: &'a CategoriesFile,
    matrix: &'a ElementAttributeMatrixFile,
    exceptions: &'a ExceptionsFile,
    applicability: &'a ApplicabilityCoverage,
    provenance: &'a ProvenanceCoverage,
}

struct ReviewNameSets<'a> {
    element_names: BTreeSet<&'a str>,
    attribute_names: BTreeSet<&'a str>,
    grammar_ids: BTreeSet<&'a str>,
}

struct ReviewIssueCounts {
    attribute_list_mismatches: usize,
    dangling_matrix_elements: usize,
    dangling_matrix_attributes: usize,
    dangling_element_categories: usize,
    dangling_attribute_categories: usize,
    dangling_exception_elements: usize,
    dangling_exception_attributes: usize,
    dangling_exception_grammars: usize,
}

fn build_unresolved_issues(facts: &ReviewFacts<'_>) -> Vec<ReviewIssue> {
    let mut unresolved = Vec::new();
    let names = review_name_sets(facts);
    let issue_counts = collect_issue_counts(facts, &names);

    push_provenance_issues(&mut unresolved, facts.provenance);
    push_count_issue(
        &mut unresolved,
        "applicability-missing-elements",
        facts.applicability.elements_missing_matrix_entries.len(),
        "elements with declared attributes but no matrix coverage",
    );
    push_count_issue(
        &mut unresolved,
        "applicability-mismatches",
        issue_counts.attribute_list_mismatches,
        "elements whose declared attribute list disagrees with matrix edges",
    );
    push_count_issue(
        &mut unresolved,
        "matrix-dangling-elements",
        issue_counts.dangling_matrix_elements,
        "matrix edges that reference unknown elements",
    );
    push_count_issue(
        &mut unresolved,
        "matrix-dangling-attributes",
        issue_counts.dangling_matrix_attributes,
        "matrix edges that reference unknown attributes",
    );
    push_count_issue(
        &mut unresolved,
        "element-category-dangling-elements",
        issue_counts.dangling_element_categories,
        "element category memberships that reference unknown elements",
    );
    push_count_issue(
        &mut unresolved,
        "attribute-category-dangling-attributes",
        issue_counts.dangling_attribute_categories,
        "attribute category memberships that reference unknown attributes",
    );
    push_count_issue(
        &mut unresolved,
        "exception-dangling-elements",
        issue_counts.dangling_exception_elements,
        "exceptions that reference unknown elements",
    );
    push_count_issue(
        &mut unresolved,
        "exception-dangling-attributes",
        issue_counts.dangling_exception_attributes,
        "exceptions that reference unknown attributes",
    );
    push_count_issue(
        &mut unresolved,
        "exception-dangling-grammars",
        issue_counts.dangling_exception_grammars,
        "exceptions that reference unknown grammars",
    );

    unresolved
}

fn review_name_sets<'a>(facts: &'a ReviewFacts<'_>) -> ReviewNameSets<'a> {
    ReviewNameSets {
        element_names: facts
            .elements
            .iter()
            .map(|element| element.name.as_str())
            .collect(),
        attribute_names: facts
            .attributes
            .iter()
            .map(|attribute| attribute.name.as_str())
            .collect(),
        grammar_ids: facts
            .grammars
            .iter()
            .map(|grammar| grammar.id.as_str())
            .collect(),
    }
}

fn collect_issue_counts(facts: &ReviewFacts<'_>, names: &ReviewNameSets<'_>) -> ReviewIssueCounts {
    ReviewIssueCounts {
        attribute_list_mismatches: count_attribute_list_mismatches(facts),
        dangling_matrix_elements: facts
            .matrix
            .edges
            .iter()
            .filter(|edge| !names.element_names.contains(edge.element.as_str()))
            .count(),
        dangling_matrix_attributes: facts
            .matrix
            .edges
            .iter()
            .filter(|edge| !names.attribute_names.contains(edge.attribute.as_str()))
            .count(),
        dangling_element_categories: facts
            .categories
            .element_categories
            .iter()
            .filter(|membership| !names.element_names.contains(membership.element.as_str()))
            .count(),
        dangling_attribute_categories: facts
            .categories
            .attribute_categories
            .iter()
            .filter(|membership| {
                !names
                    .attribute_names
                    .contains(membership.attribute.as_str())
            })
            .count(),
        dangling_exception_elements: facts
            .exceptions
            .exceptions
            .iter()
            .filter(|exception| match &exception.scope {
                ExceptionScope::Element { name } => !names.element_names.contains(name.as_str()),
                ExceptionScope::ElementAttribute { element, .. } => {
                    !names.element_names.contains(element.as_str())
                }
                _ => false,
            })
            .count(),
        dangling_exception_attributes: facts
            .exceptions
            .exceptions
            .iter()
            .filter(|exception| match &exception.scope {
                ExceptionScope::Attribute { name } => {
                    !names.attribute_names.contains(name.as_str())
                }
                ExceptionScope::ElementAttribute { attribute, .. } => {
                    !names.attribute_names.contains(attribute.as_str())
                }
                _ => false,
            })
            .count(),
        dangling_exception_grammars: facts
            .exceptions
            .exceptions
            .iter()
            .filter(|exception| match &exception.scope {
                ExceptionScope::Grammar { grammar_id } => {
                    !names.grammar_ids.contains(grammar_id.as_str())
                }
                _ => false,
            })
            .count(),
    }
}

fn count_attribute_list_mismatches(facts: &ReviewFacts<'_>) -> usize {
    let matrix_edges_by_element =
        facts
            .matrix
            .edges
            .iter()
            .fold(BTreeMap::new(), |mut acc, edge| {
                acc.entry(edge.element.as_str())
                    .or_insert_with(BTreeSet::new)
                    .insert(edge.attribute.as_str());
                acc
            });

    facts
        .elements
        .iter()
        .filter(|element| {
            let declared: BTreeSet<&str> = element.attributes.iter().map(String::as_str).collect();
            let matrix_list = matrix_edges_by_element.get(element.name.as_str());
            matrix_list.map_or(!declared.is_empty(), |matrix_attributes| {
                declared != *matrix_attributes
            })
        })
        .count()
}

fn push_provenance_issues(unresolved: &mut Vec<ReviewIssue>, provenance: &ProvenanceCoverage) {
    for (id, label, coverage) in [
        ("elements-provenance", "elements.json", &provenance.elements),
        (
            "attributes-provenance",
            "attributes.json",
            &provenance.attributes,
        ),
        ("grammars-provenance", "grammars.json", &provenance.grammars),
        (
            "element-categories-provenance",
            "categories.json element_categories",
            &provenance.element_categories,
        ),
        (
            "attribute-categories-provenance",
            "categories.json attribute_categories",
            &provenance.attribute_categories,
        ),
        (
            "applicability-edges-provenance",
            "element_attribute_matrix.json",
            &provenance.applicability_edges,
        ),
        (
            "exceptions-provenance",
            "exceptions.json",
            &provenance.exceptions,
        ),
    ] {
        push_missing_provenance_issue(unresolved, id, label, coverage);
    }
}

fn push_missing_provenance_issue(
    unresolved: &mut Vec<ReviewIssue>,
    id: &str,
    label: &str,
    coverage: &ProvenanceCoverageCount,
) {
    if coverage.missing == 0 {
        return;
    }

    unresolved.push(ReviewIssue {
        id: id.to_string(),
        severity: ReviewSeverity::Error,
        summary: format!(
            "{} records in {} are missing provenance",
            coverage.missing, label
        ),
    });
}

fn push_count_issue(unresolved: &mut Vec<ReviewIssue>, id: &str, count: usize, label: &str) {
    if count == 0 {
        return;
    }

    unresolved.push(ReviewIssue {
        id: id.to_string(),
        severity: ReviewSeverity::Error,
        summary: format!("{count} {label}"),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot_schema::{
        AnimationBehavior, AttributeDefaultValue, AttributeRequirement, ElementAttributeEdge,
        ElementCategoryMembership, ElementContentModel, ExceptionsFile, FactProvenance,
        SnapshotException, SourceLocator, SourcePin,
    };

    #[test]
    fn review_derives_clean_coverage_for_consistent_dataset() {
        let provenance = vec![FactProvenance {
            source_id: String::from("tr-root"),
            source_kind: crate::snapshot_schema::ProvenanceSourceKind::Html,
            pin: SourcePin::Url {
                url: String::from("https://example.test/spec"),
            },
            locator: SourceLocator::Fragment {
                anchor: String::from("svg"),
            },
            confidence: crate::snapshot_schema::ExtractionConfidence::Exact,
        }];
        let notes = vec![String::from("seed note")];
        let review = build_report(Input {
            elements: &[SnapshotElementRecord {
                name: String::from("svg"),
                title: String::from("SVG root"),
                categories: vec![String::from("container")],
                content_model: ElementContentModel::AnySvg,
                attributes: vec![String::from("width")],
                provenance: provenance.clone(),
            }],
            attributes: &[SnapshotAttributeRecord {
                name: String::from("width"),
                title: String::from("Width"),
                value_syntax: crate::snapshot_schema::ValueSyntax::GrammarRef {
                    grammar_id: String::from("length"),
                },
                default_value: AttributeDefaultValue::None,
                animatable: AnimationBehavior::Unspecified,
                provenance: provenance.clone(),
            }],
            grammars: &GrammarFile {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                grammars: vec![GrammarDefinition {
                    id: String::from("length"),
                    title: String::from("Length"),
                    root: crate::snapshot_schema::GrammarNode::DatatypeRef {
                        name: String::from("length"),
                    },
                    provenance: provenance.clone(),
                }],
            },
            categories: &CategoriesFile {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                element_categories: vec![ElementCategoryMembership {
                    element: String::from("svg"),
                    category: String::from("container"),
                    provenance: provenance.clone(),
                }],
                attribute_categories: Vec::new(),
            },
            element_attribute_matrix: &ElementAttributeMatrixFile {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                edges: vec![ElementAttributeEdge {
                    element: String::from("svg"),
                    attribute: String::from("width"),
                    requirement: AttributeRequirement::Optional,
                    provenance: provenance.clone(),
                }],
            },
            exceptions: &ExceptionsFile {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                exceptions: vec![SnapshotException {
                    id: String::from("width-note"),
                    scope: ExceptionScope::Attribute {
                        name: String::from("width"),
                    },
                    disposition: ExceptionDisposition::Deferred,
                    reason: String::from("manual check"),
                    provenance,
                }],
            },
            manual_notes: &notes,
        });

        assert_eq!(review.counts.elements, 1);
        assert_eq!(
            review.applicability.elements_missing_matrix_entries,
            Vec::<String>::new()
        );
        assert_eq!(review.provenance.elements.missing, 0);
        assert_eq!(review.exception_inventory.total, 1);
        assert_eq!(review.exception_inventory.attribute_scoped, 1);
        assert!(review.unresolved.is_empty());
        assert_eq!(review.manual_notes, notes);
    }
}
