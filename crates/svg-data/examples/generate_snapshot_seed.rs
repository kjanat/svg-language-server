//! Generate checked-in seeded snapshot data for SVG 1.1 profiles.

use std::{collections::BTreeSet, error::Error, path::Path};

use svg_data::{
    ProfileLookup, attribute_for_profile, attributes, attributes_for_with_profile,
    elements_in_category, elements_with_profile,
    extraction::{SnapshotDataset, SnapshotDatasetWriter, SourceManifest},
    snapshot_schema::{
        AnimationBehavior, AttributeCategoryMembership, AttributeDefaultValue,
        AttributeRequirement, CategoriesFile, ElementAttributeEdge, ElementAttributeMatrixFile,
        ElementCategoryMembership, ElementContentModel, ExceptionsFile, ExtractionConfidence,
        GrammarFile, ProvenanceSourceKind, ReviewCounts, ReviewFile, SNAPSHOT_SCHEMA_VERSION,
        SnapshotAttributeRecord, SnapshotElementRecord, SourceLocator, ValueSyntax,
    },
    types::{AttributeValues, ContentModel, ElementCategory, SpecSnapshotId},
};

fn main() -> Result<(), Box<dyn Error>> {
    let Some(snapshot_arg) = std::env::args().nth(1) else {
        return Err(
            "usage: cargo run -p svg-data --example generate_snapshot_seed -- <snapshot-id>".into(),
        );
    };

    let snapshot = parse_snapshot_id(&snapshot_arg)?;
    let manifest = SourceManifest::read(&manifest_path(snapshot))?;
    let dataset = build_seed_dataset(snapshot, &manifest)?;

    let writer = SnapshotDatasetWriter::new(specs_root());
    let written = writer.write(&dataset)?;

    for path in written {
        println!("{}", path.display());
    }

    Ok(())
}

fn build_seed_dataset(
    snapshot: SpecSnapshotId,
    manifest: &SourceManifest,
) -> svg_data::extraction::Result<SnapshotDataset> {
    let elements_with_profile = elements_with_profile(snapshot);
    let attributes_with_profile: Vec<_> = attributes()
        .iter()
        .filter_map(
            |attribute| match attribute_for_profile(snapshot, attribute.name) {
                ProfileLookup::Present { value, .. } => Some(value),
                ProfileLookup::UnsupportedInProfile { .. } | ProfileLookup::Unknown => None,
            },
        )
        .collect();

    let elements = build_seed_elements(snapshot, manifest, &elements_with_profile)?;
    let attributes = build_seed_attributes(manifest, &attributes_with_profile)?;
    let element_categories = build_seed_categories(manifest, &elements_with_profile)?;
    let edges = build_seed_edges(snapshot, manifest, &elements_with_profile)?;

    let edge_count = edges.len();

    Ok(SnapshotDataset {
        metadata: manifest.snapshot_metadata("snapshot-seed-v1", "2026-04-09")?,
        elements,
        attributes,
        grammars: GrammarFile {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            grammars: Vec::new(),
        },
        categories: CategoriesFile {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            element_categories,
            attribute_categories: Vec::<AttributeCategoryMembership>::new(),
        },
        element_attribute_matrix: ElementAttributeMatrixFile {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            edges,
        },
        exceptions: ExceptionsFile {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            exceptions: Vec::new(),
        },
        review: ReviewFile {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            counts: ReviewCounts {
                elements: elements_with_profile.len(),
                attributes: attributes_with_profile.len(),
                grammars: 0,
                applicability_edges: edge_count,
                exceptions: 0,
            },
            unresolved: Vec::new(),
            manual_notes: vec![
                String::from(
                    "Seeded from the current profile-aware catalog to establish checked-in snapshot truth for Svg11Rec20030114.",
                ),
                String::from(
                    "Value grammar stays opaque in this snapshot seed; phase 4 normalizes structured grammar coverage.",
                ),
            ],
        },
    })
}

fn build_seed_elements(
    snapshot: SpecSnapshotId,
    manifest: &SourceManifest,
    elements_with_profile: &[svg_data::ProfiledElement],
) -> svg_data::extraction::Result<Vec<SnapshotElementRecord>> {
    elements_with_profile
        .iter()
        .map(|profiled| {
            let element = profiled.element;
            let attributes = attributes_for_with_profile(snapshot, element.name)
                .into_iter()
                .map(|profiled_attribute| profiled_attribute.attribute.name.to_string())
                .collect();

            Ok(SnapshotElementRecord {
                name: element.name.to_string(),
                title: element.description.to_string(),
                categories: category_ids_for_element(element.name),
                content_model: normalize_content_model(&element.content_model),
                attributes,
                provenance: vec![
                    manifest.fact_provenance(
                        "element-index",
                        ProvenanceSourceKind::Index,
                        SourceLocator::Fragment {
                            anchor: element.name.to_string(),
                        },
                        ExtractionConfidence::Derived,
                    )?,
                    manifest.fact_provenance(
                        "flattened-dtd",
                        ProvenanceSourceKind::Dtd,
                        SourceLocator::Definition {
                            file: String::from("DTD/svg11-flat.dtd"),
                            id: element.name.to_string(),
                        },
                        ExtractionConfidence::Derived,
                    )?,
                ],
            })
        })
        .collect()
}

fn build_seed_attributes(
    manifest: &SourceManifest,
    attributes_with_profile: &[&svg_data::AttributeDef],
) -> svg_data::extraction::Result<Vec<SnapshotAttributeRecord>> {
    attributes_with_profile
        .iter()
        .map(|attribute| {
            Ok(SnapshotAttributeRecord {
                name: attribute.name.to_string(),
                title: attribute.description.to_string(),
                value_syntax: normalize_value_syntax(&attribute.values),
                default_value: AttributeDefaultValue::None,
                animatable: AnimationBehavior::Unspecified,
                provenance: vec![
                    manifest.fact_provenance(
                        "attribute-index",
                        ProvenanceSourceKind::Index,
                        SourceLocator::Fragment {
                            anchor: attribute.name.to_string(),
                        },
                        ExtractionConfidence::Derived,
                    )?,
                    manifest.fact_provenance(
                        "flattened-dtd",
                        ProvenanceSourceKind::Dtd,
                        SourceLocator::Definition {
                            file: String::from("DTD/svg11-flat.dtd"),
                            id: attribute.name.to_string(),
                        },
                        ExtractionConfidence::Derived,
                    )?,
                ],
            })
        })
        .collect()
}

fn build_seed_categories(
    manifest: &SourceManifest,
    elements_with_profile: &[svg_data::ProfiledElement],
) -> svg_data::extraction::Result<Vec<ElementCategoryMembership>> {
    let mut element_categories = Vec::new();
    for profiled in elements_with_profile {
        for category in category_ids_for_element(profiled.element.name) {
            element_categories.push(ElementCategoryMembership {
                element: profiled.element.name.to_string(),
                category,
                provenance: vec![manifest.fact_provenance(
                    "flattened-dtd",
                    ProvenanceSourceKind::Dtd,
                    SourceLocator::Definition {
                        file: String::from("DTD/svg11-flat.dtd"),
                        id: profiled.element.name.to_string(),
                    },
                    ExtractionConfidence::Derived,
                )?],
            });
        }
    }

    Ok(element_categories)
}

fn build_seed_edges(
    snapshot: SpecSnapshotId,
    manifest: &SourceManifest,
    elements_with_profile: &[svg_data::ProfiledElement],
) -> svg_data::extraction::Result<Vec<ElementAttributeEdge>> {
    let mut edges = Vec::new();
    for profiled in elements_with_profile {
        let element = profiled.element;
        let required: BTreeSet<&str> = element.required_attrs.iter().copied().collect();
        for profiled_attribute in attributes_for_with_profile(snapshot, element.name) {
            let attribute = profiled_attribute.attribute;
            edges.push(ElementAttributeEdge {
                element: element.name.to_string(),
                attribute: attribute.name.to_string(),
                requirement: if required.contains(attribute.name) {
                    AttributeRequirement::Required
                } else {
                    AttributeRequirement::Optional
                },
                provenance: vec![manifest.fact_provenance(
                    "flattened-dtd",
                    ProvenanceSourceKind::Dtd,
                    SourceLocator::Definition {
                        file: String::from("DTD/svg11-flat.dtd"),
                        id: format!("{}@{}", element.name, attribute.name),
                    },
                    ExtractionConfidence::Derived,
                )?],
            });
        }
    }

    Ok(edges)
}

fn parse_snapshot_id(value: &str) -> Result<SpecSnapshotId, Box<dyn Error>> {
    match value {
        "Svg11Rec20030114" => Ok(SpecSnapshotId::Svg11Rec20030114),
        "Svg11Rec20110816" => Ok(SpecSnapshotId::Svg11Rec20110816),
        _ => Err(format!("snapshot seed generator does not support {value}").into()),
    }
}

fn manifest_path(snapshot: SpecSnapshotId) -> std::path::PathBuf {
    let file_name = match snapshot {
        SpecSnapshotId::Svg11Rec20030114 => "svg11-rec-20030114.toml",
        SpecSnapshotId::Svg11Rec20110816 => "svg11-rec-20110816.toml",
        SpecSnapshotId::Svg2Cr20181004 | SpecSnapshotId::Svg2EditorsDraft20250914 => {
            unreachable!("unsupported snapshot seed")
        }
    };

    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data/sources")
        .join(file_name)
}

fn specs_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/specs")
}

fn normalize_content_model(content_model: &ContentModel) -> ElementContentModel {
    match content_model {
        ContentModel::Children(categories) => ElementContentModel::CategorySet {
            categories: categories
                .iter()
                .map(|category| category_id(*category).to_string())
                .collect(),
        },
        ContentModel::Foreign => ElementContentModel::ForeignNamespace,
        ContentModel::Void => ElementContentModel::Empty,
        ContentModel::Text => ElementContentModel::TextOnly,
    }
}

fn normalize_value_syntax(values: &AttributeValues) -> ValueSyntax {
    ValueSyntax::Opaque {
        display: attribute_value_display(values),
        reason: String::from("seeded from current catalog before grammar normalization"),
    }
}

fn attribute_value_display(values: &AttributeValues) -> String {
    match values {
        AttributeValues::Enum(values) => values.join(" | "),
        AttributeValues::FreeText => String::from("text"),
        AttributeValues::Color => String::from("color"),
        AttributeValues::Length => String::from("length"),
        AttributeValues::Url => String::from("url"),
        AttributeValues::NumberOrPercentage => String::from("number-or-percentage"),
        AttributeValues::Transform([]) => String::from("transform-list"),
        AttributeValues::Transform(functions) => {
            format!("transform-list({})", functions.join(", "))
        }
        AttributeValues::ViewBox => String::from("viewBox"),
        AttributeValues::PreserveAspectRatio { .. } => String::from("preserveAspectRatio"),
        AttributeValues::Points => String::from("points"),
        AttributeValues::PathData => String::from("path-data"),
    }
}

fn category_ids_for_element(element_name: &str) -> Vec<String> {
    all_categories()
        .iter()
        .copied()
        .filter(|category| elements_in_category(*category).contains(&element_name))
        .map(|category| category_id(category).to_string())
        .collect()
}

const fn all_categories() -> &'static [ElementCategory] {
    &[
        ElementCategory::Container,
        ElementCategory::Shape,
        ElementCategory::Text,
        ElementCategory::Gradient,
        ElementCategory::Filter,
        ElementCategory::Descriptive,
        ElementCategory::Structural,
        ElementCategory::Animation,
        ElementCategory::PaintServer,
        ElementCategory::ClipMask,
        ElementCategory::LightSource,
        ElementCategory::FilterPrimitive,
        ElementCategory::TransferFunction,
        ElementCategory::MergeNode,
        ElementCategory::MotionPath,
        ElementCategory::NeverRendered,
    ]
}

const fn category_id(category: ElementCategory) -> &'static str {
    match category {
        ElementCategory::Container => "container",
        ElementCategory::Shape => "shape",
        ElementCategory::Text => "text",
        ElementCategory::Gradient => "gradient",
        ElementCategory::Filter => "filter",
        ElementCategory::Descriptive => "descriptive",
        ElementCategory::Structural => "structural",
        ElementCategory::Animation => "animation",
        ElementCategory::PaintServer => "paint_server",
        ElementCategory::ClipMask => "clip_mask",
        ElementCategory::LightSource => "light_source",
        ElementCategory::FilterPrimitive => "filter_primitive",
        ElementCategory::TransferFunction => "transfer_function",
        ElementCategory::MergeNode => "merge_node",
        ElementCategory::MotionPath => "motion_path",
        ElementCategory::NeverRendered => "never_rendered",
    }
}
