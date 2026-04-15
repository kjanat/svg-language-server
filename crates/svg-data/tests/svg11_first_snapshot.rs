//! Regression coverage for checked-in snapshot seeds.

use std::{collections::BTreeSet, fs, path::Path};

use svg_data::{
    ProfileLookup, attribute_for_profile, attributes as catalog_attributes,
    attributes_for_with_profile, elements_with_profile,
    snapshot_schema::{
        CategoriesFile, ElementAttributeMatrixFile, GrammarFile, GrammarNode, ReviewFile,
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
        13,
        "foreign references (svg-animations",
    );
}

#[test]
fn svg2_editors_draft_snapshot_matches_profile_seed() {
    assert_svg2_snapshot_matches_profile_seed(
        SpecSnapshotId::Svg2EditorsDraft20250914,
        "2025-09-14",
        13,
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
    assert!(
        metadata
            .pinned_sources
            .iter()
            .all(|source| !expected_foreign_ref_source_ids().contains(&source.input_id.as_str()))
    );

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
    assert_foreign_pinned_sources(&metadata);

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
    assert_svg2_foreign_refs(&snapshot_attributes);

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

    if !uses_foreign_css_refs(snapshot) {
        assert!(grammar_ids.contains("color"));
        assert!(grammar_ids.contains("length"));
        assert!(grammar_ids.contains("number-or-percentage"));
    }
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

        if let Some((spec, target)) =
            expected_foreign_ref(snapshot, catalog_attribute.name, &catalog_attribute.values)
        {
            assert!(matches!(
                &snapshot_attribute.value_syntax,
                ValueSyntax::ForeignRef {
                    spec: actual_spec,
                    target: actual_target,
                } if actual_spec == spec && actual_target == target
            ));
            continue;
        }

        match &catalog_attribute.values {
            AttributeValues::FreeText => {
                // Runtime codegen now intentionally collapses some checked-in
                // structured syntax to `FreeText` until consumers switch to
                // snapshot-native grammar handling.
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

const fn uses_foreign_css_refs(snapshot: SpecSnapshotId) -> bool {
    matches!(
        snapshot,
        SpecSnapshotId::Svg2Cr20181004 | SpecSnapshotId::Svg2EditorsDraft20250914
    )
}

fn expected_foreign_ref(
    snapshot: SpecSnapshotId,
    attribute_name: &str,
    values: &AttributeValues,
) -> Option<(&'static str, &'static str)> {
    if !uses_foreign_css_refs(snapshot) {
        return None;
    }

    match attribute_name {
        "begin" => Some(("svg-animations", "begin")),
        "dur" => Some(("svg-animations", "dur")),
        "end" => Some(("svg-animations", "end")),
        "repeatDur" => Some(("svg-animations", "repeatDur")),
        "keyTimes" => Some(("svg-animations", "keyTimes")),
        "keySplines" => Some(("svg-animations", "keySplines")),
        "restart" => Some(("svg-animations", "restart")),
        "keyPoints" => Some(("svg-animations", "keyPoints")),
        "repeatCount" => Some(("svg-animations", "repeatCount")),
        "clip-path" => Some(("css-masking-1", "clip-path")),
        "mask" => Some(("css-masking-1", "mask")),
        "clipPathUnits" => Some(("css-masking-1", "clipPathUnits")),
        "maskContentUnits" => Some(("css-masking-1", "maskContentUnits")),
        "maskUnits" => Some(("css-masking-1", "maskUnits")),
        "filter" => Some(("filter-effects-1", "filter")),
        "operator" => Some(("compositing-1", "operator")),
        _ => match values {
            AttributeValues::Color => Some(("css-color-4", "<color>")),
            AttributeValues::Length => Some(("css-values-3", "<length>")),
            AttributeValues::NumberOrPercentage => Some(("css-values-3", "<number-or-percentage>")),
            _ => None,
        },
    }
}

const fn expected_foreign_ref_source_ids() -> &'static [&'static str] {
    &[
        "svg-animations",
        "filter-effects-1",
        "css-masking-1",
        "compositing-1",
        "css-values-3",
        "css-color-4",
        "wai-aria-1.1",
    ]
}

fn assert_foreign_pinned_sources(metadata: &SnapshotMetadataFile) {
    let input_ids: BTreeSet<&str> = metadata
        .pinned_sources
        .iter()
        .map(|source| source.input_id.as_str())
        .collect();

    for input_id in expected_foreign_ref_source_ids() {
        assert!(
            input_ids.contains(input_id),
            "missing foreign pin {input_id}"
        );
    }
}

fn assert_svg2_foreign_refs(snapshot_attributes: &[SnapshotAttributeRecord]) {
    for (name, spec, target) in [
        ("fill", "css-color-4", "<color>"),
        ("stroke-width", "css-values-3", "<length>"),
        ("opacity", "css-values-3", "<number-or-percentage>"),
        ("clip-path", "css-masking-1", "clip-path"),
        ("mask", "css-masking-1", "mask"),
        ("filter", "filter-effects-1", "filter"),
        ("begin", "svg-animations", "begin"),
        ("dur", "svg-animations", "dur"),
        ("repeatCount", "svg-animations", "repeatCount"),
        ("keySplines", "svg-animations", "keySplines"),
        ("operator", "compositing-1", "operator"),
    ] {
        let attribute = snapshot_attributes
            .iter()
            .find(|attribute| attribute.name == name)
            .unwrap_or_else(|| panic!("missing attribute {name}"));
        assert!(matches!(
            &attribute.value_syntax,
            ValueSyntax::ForeignRef {
                spec: actual_spec,
                target: actual_target,
            } if actual_spec == spec && actual_target == target
        ));
        assert!(
            attribute
                .provenance
                .iter()
                .any(|provenance| provenance.source_id == spec),
            "missing foreign provenance for {name}"
        );
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

/// The 17 `display` keywords defined by CSS 2.0, which SVG 1.1 §11.5
/// and §11.5.1 normatively reference. The list must match the spec
/// verbatim — dropping `run-in` / `compact` / `marker` would regress the
/// known bug fixed in 2026-04 where the under-reported grammar only
/// exposed `inline | block | none`.
const SVG11_DISPLAY_VALUES: &[&str] = &[
    "inline",
    "block",
    "list-item",
    "run-in",
    "compact",
    "marker",
    "table",
    "inline-table",
    "table-row-group",
    "table-header-group",
    "table-footer-group",
    "table-row",
    "table-column-group",
    "table-column",
    "table-cell",
    "table-caption",
    "none",
];

/// The 15 `display` keywords defined by CSS 2.1, which SVG 2
/// `render.html#VisibilityControl` defers to. CSS 2.1 dropped CSS 2.0's
/// `run-in` / `compact` / `marker` and added `inline-block`; the SVG 2
/// snapshots must track that change.
const SVG2_DISPLAY_VALUES: &[&str] = &[
    "inline",
    "block",
    "list-item",
    "inline-block",
    "table",
    "inline-table",
    "table-row-group",
    "table-header-group",
    "table-footer-group",
    "table-row",
    "table-column-group",
    "table-column",
    "table-cell",
    "table-caption",
    "none",
];

#[test]
fn enum_display_grammar_carries_full_css_set_in_every_snapshot() {
    // Pin each snapshot to the exact value list its referenced CSS
    // profile defines. Hand-written on purpose: the round-trip tests
    // above verify the seed generator preserves the checked-in data,
    // but the seed generator itself has no independent source for
    // these keywords — the snapshot JSON is the ground truth. A
    // dedicated regression guard here makes sure the ground truth
    // doesn't silently slip back to `inline | block | none`.
    for (snapshot, expected) in [
        (SpecSnapshotId::Svg11Rec20030114, SVG11_DISPLAY_VALUES),
        (SpecSnapshotId::Svg11Rec20110816, SVG11_DISPLAY_VALUES),
        (SpecSnapshotId::Svg2Cr20181004, SVG2_DISPLAY_VALUES),
        (
            SpecSnapshotId::Svg2EditorsDraft20250914,
            SVG2_DISPLAY_VALUES,
        ),
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data/specs")
            .join(snapshot.as_str())
            .join("grammars.json");
        let grammars: GrammarFile = read_json(&path);
        let grammar = grammars
            .grammars
            .iter()
            .find(|g| g.id == "enum-display")
            .unwrap_or_else(|| panic!("enum-display grammar missing in {}", snapshot.as_str()));

        let GrammarNode::Choice { options } = &grammar.root else {
            panic!(
                "enum-display root is not a choice in {}: {:?}",
                snapshot.as_str(),
                grammar.root,
            );
        };
        let actual: Vec<&str> = options
            .iter()
            .map(|option| match option {
                GrammarNode::Keyword { value } => value.as_str(),
                other => panic!(
                    "enum-display option is not a keyword in {}: {:?}",
                    snapshot.as_str(),
                    other,
                ),
            })
            .collect();

        assert_eq!(
            actual,
            expected,
            "enum-display keyword list drifted from the spec for {}. \
             SVG 1.1 §11.5 inherits CSS 2.0 ({} values); SVG 2 defers to \
             CSS 2.1 ({} values). Regressing to `inline | block | none` \
             under-reports valid values and breaks completion + validation.",
            snapshot.as_str(),
            SVG11_DISPLAY_VALUES.len(),
            SVG2_DISPLAY_VALUES.len(),
        );

        // Provenance must be manually curated — the `display` value list
        // comes from hand-reading the spec prose, not from any structured
        // extractor. If a future regen path ever re-derives this entry,
        // it should carry a `manual_review` confidence trail.
        assert!(
            grammar.provenance.iter().any(|p| matches!(
                p.source_kind,
                svg_data::snapshot_schema::ProvenanceSourceKind::ManualReview
            )),
            "enum-display in {} lost its manual_review provenance tag",
            snapshot.as_str(),
        );
    }
}
