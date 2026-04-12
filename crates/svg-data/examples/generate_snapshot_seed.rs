//! Generate checked-in seeded snapshot data from the current profile-aware catalog.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    path::Path,
};

use svg_data::{
    ProfileLookup, attribute_for_profile, attributes, attributes_for_with_profile,
    elements_in_category, elements_with_profile,
    extraction::{SnapshotDataset, SnapshotDatasetWriter, SourceManifest},
    review::{Input, build_report},
    snapshot_schema::{
        AnimationBehavior, AttributeCategoryMembership, AttributeDefaultValue,
        AttributeRequirement, CategoriesFile, ElementAttributeEdge, ElementAttributeMatrixFile,
        ElementCategoryMembership, ElementContentModel, ExceptionsFile, ExtractionConfidence,
        FactProvenance, GrammarDefinition, GrammarFile, GrammarNode, ProvenanceSourceKind,
        SNAPSHOT_SCHEMA_VERSION, SnapshotAttributeRecord, SnapshotElementRecord,
        SnapshotMetadataFile, SourceLocator, ValueSyntax,
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
    let bindings = seed_source_bindings(snapshot);
    let foreign_manifest = foreign_manifest(snapshot)?;
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

    let elements = build_seed_elements(snapshot, manifest, &bindings, &elements_with_profile)?;
    let SeedAttributeData {
        attributes,
        grammars,
    } = build_seed_attributes(
        snapshot,
        manifest,
        foreign_manifest.as_ref(),
        &bindings,
        &attributes_with_profile,
    )?;
    let element_categories = build_seed_categories(manifest, &bindings, &elements_with_profile)?;
    let edges = build_seed_edges(snapshot, manifest, &bindings, &elements_with_profile)?;

    let mut manual_notes = vec![
        format!(
            "Seeded from the current profile-aware catalog to establish checked-in snapshot truth for {}.",
            snapshot.as_str()
        ),
        String::from(
            "SVG-owned value grammar is normalized into `grammars.json`; only free-form text stays opaque in this seed.",
        ),
    ];
    if foreign_manifest.is_some() {
        manual_notes.push(String::from(
            "SVG 2 foreign grammars and external modules stay as pinned typed references instead of ad hoc local strings.",
        ));
    }
    manual_notes.push(bindings.review_note.to_string());

    let exceptions = ExceptionsFile {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        exceptions: Vec::new(),
    };
    let categories = CategoriesFile {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        element_categories,
        attribute_categories: Vec::<AttributeCategoryMembership>::new(),
    };
    let element_attribute_matrix = ElementAttributeMatrixFile {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        edges,
    };
    let review = build_report(Input {
        elements: &elements,
        attributes: &attributes,
        grammars: &grammars,
        categories: &categories,
        element_attribute_matrix: &element_attribute_matrix,
        exceptions: &exceptions,
        manual_notes: &manual_notes,
    });

    Ok(SnapshotDataset {
        metadata: build_seed_metadata(manifest, foreign_manifest.as_ref())?,
        elements,
        attributes,
        grammars,
        categories,
        element_attribute_matrix,
        exceptions,
        review,
    })
}

fn build_seed_metadata(
    manifest: &SourceManifest,
    foreign_manifest: Option<&SourceManifest>,
) -> svg_data::extraction::Result<SnapshotMetadataFile> {
    let mut metadata = manifest.snapshot_metadata("snapshot-seed-v1", "2026-04-09")?;

    if let Some(foreign_manifest) = foreign_manifest {
        metadata.pinned_sources.extend(
            foreign_reference_source_ids()
                .iter()
                .map(|input_id| foreign_manifest.source_ref(input_id))
                .collect::<svg_data::extraction::Result<Vec<_>>>()?,
        );
    }

    Ok(metadata)
}

fn build_seed_elements(
    snapshot: SpecSnapshotId,
    manifest: &SourceManifest,
    bindings: &SeedSourceBindings,
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
                        bindings.element_index_input,
                        bindings.element_index_source_kind,
                        bindings.element_index_locator(element.name),
                        bindings.element_index_confidence,
                    )?,
                    manifest.fact_provenance(
                        bindings.detail_input,
                        bindings.detail_source_kind,
                        bindings.element_locator(element.name),
                        bindings.detail_confidence,
                    )?,
                ],
            })
        })
        .collect()
}

struct SeedAttributeData {
    attributes: Vec<SnapshotAttributeRecord>,
    grammars: GrammarFile,
}

fn build_seed_attributes(
    snapshot: SpecSnapshotId,
    manifest: &SourceManifest,
    foreign_manifest: Option<&SourceManifest>,
    bindings: &SeedSourceBindings,
    attributes_with_profile: &[&svg_data::AttributeDef],
) -> svg_data::extraction::Result<SeedAttributeData> {
    let mut registry = SeedGrammarRegistry::default();
    let attributes = attributes_with_profile
        .iter()
        .map(|attribute| {
            let mut provenance = vec![
                manifest.fact_provenance(
                    bindings.attribute_index_input,
                    bindings.attribute_index_source_kind,
                    bindings.attribute_index_locator(attribute.name),
                    bindings.attribute_index_confidence,
                )?,
                manifest.fact_provenance(
                    bindings.detail_input,
                    bindings.detail_source_kind,
                    bindings.attribute_locator(attribute.name),
                    bindings.detail_confidence,
                )?,
            ];

            Ok(SnapshotAttributeRecord {
                name: attribute.name.to_string(),
                title: attribute.description.to_string(),
                value_syntax: normalize_value_syntax(
                    snapshot,
                    attribute.name,
                    &attribute.values,
                    &mut provenance,
                    foreign_manifest,
                    &mut registry,
                )?,
                default_value: AttributeDefaultValue::None,
                animatable: AnimationBehavior::Unspecified,
                provenance,
            })
        })
        .collect::<svg_data::extraction::Result<Vec<_>>>()?;

    Ok(SeedAttributeData {
        attributes,
        grammars: registry.finish(),
    })
}

fn build_seed_categories(
    manifest: &SourceManifest,
    bindings: &SeedSourceBindings,
    elements_with_profile: &[svg_data::ProfiledElement],
) -> svg_data::extraction::Result<Vec<ElementCategoryMembership>> {
    let mut element_categories = Vec::new();
    for profiled in elements_with_profile {
        for category in category_ids_for_element(profiled.element.name) {
            element_categories.push(ElementCategoryMembership {
                element: profiled.element.name.to_string(),
                category,
                provenance: vec![manifest.fact_provenance(
                    bindings.detail_input,
                    bindings.detail_source_kind,
                    bindings.element_locator(profiled.element.name),
                    bindings.detail_confidence,
                )?],
            });
        }
    }

    Ok(element_categories)
}

fn build_seed_edges(
    snapshot: SpecSnapshotId,
    manifest: &SourceManifest,
    bindings: &SeedSourceBindings,
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
                    bindings.detail_input,
                    bindings.detail_source_kind,
                    bindings.edge_locator(element.name, attribute.name),
                    bindings.detail_confidence,
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
        "Svg2Cr20181004" => Ok(SpecSnapshotId::Svg2Cr20181004),
        "Svg2EditorsDraft20250914" => Ok(SpecSnapshotId::Svg2EditorsDraft20250914),
        _ => Err(format!("snapshot seed generator does not support {value}").into()),
    }
}

fn manifest_path(snapshot: SpecSnapshotId) -> std::path::PathBuf {
    let file_name = match snapshot {
        SpecSnapshotId::Svg11Rec20030114 => "svg11-rec-20030114.toml",
        SpecSnapshotId::Svg11Rec20110816 => "svg11-rec-20110816.toml",
        SpecSnapshotId::Svg2Cr20181004 => "svg2-cr-20181004.toml",
        SpecSnapshotId::Svg2EditorsDraft20250914 => "svg2-ed-20250914.toml",
    };

    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data/sources")
        .join(file_name)
}

fn foreign_manifest(
    snapshot: SpecSnapshotId,
) -> svg_data::extraction::Result<Option<SourceManifest>> {
    if !matches!(
        snapshot,
        SpecSnapshotId::Svg2Cr20181004 | SpecSnapshotId::Svg2EditorsDraft20250914
    ) {
        return Ok(None);
    }

    Ok(Some(SourceManifest::read(&foreign_manifest_path())?))
}

fn foreign_manifest_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data/sources")
        .join("foreign-references.toml")
}

const fn foreign_reference_source_ids() -> &'static [&'static str] {
    &[
        "svg-animations",
        "filter-effects-1",
        "css-masking-1",
        "compositing-1",
        "css-values-3",
        "css-color-4",
    ]
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

fn normalize_value_syntax(
    snapshot: SpecSnapshotId,
    attribute_name: &str,
    values: &AttributeValues,
    provenance: &mut Vec<FactProvenance>,
    foreign_manifest: Option<&SourceManifest>,
    registry: &mut SeedGrammarRegistry,
) -> svg_data::extraction::Result<ValueSyntax> {
    if let Some(value_syntax) = foreign_value_syntax(
        snapshot,
        attribute_name,
        values,
        provenance,
        foreign_manifest,
    )? {
        return Ok(value_syntax);
    }

    match values {
        AttributeValues::Enum(values) => Ok(value_syntax_from_grammar_id(registry.attribute_enum(
            attribute_name,
            values,
            provenance,
        ))),
        AttributeValues::FreeText => Ok(ValueSyntax::Opaque {
            display: attribute_value_display(values),
            reason: String::from("free-form text stays explicit until a typed text model exists"),
        }),
        AttributeValues::Color => Ok(value_syntax_from_shared_grammar(
            registry,
            "color",
            "Color value",
            GrammarNode::DatatypeRef {
                name: String::from("color"),
            },
            provenance,
        )),
        AttributeValues::Length => Ok(value_syntax_from_shared_grammar(
            registry,
            "length",
            "Length value",
            GrammarNode::DatatypeRef {
                name: String::from("length"),
            },
            provenance,
        )),
        AttributeValues::Url => Ok(value_syntax_from_shared_grammar(
            registry,
            "url-reference",
            "URL reference",
            GrammarNode::DatatypeRef {
                name: String::from("url"),
            },
            provenance,
        )),
        AttributeValues::NumberOrPercentage => Ok(value_syntax_from_shared_grammar(
            registry,
            "number-or-percentage",
            "Number or percentage",
            GrammarNode::Choice {
                options: vec![
                    GrammarNode::DatatypeRef {
                        name: String::from("number"),
                    },
                    GrammarNode::DatatypeRef {
                        name: String::from("percentage"),
                    },
                ],
            },
            provenance,
        )),
        AttributeValues::Transform(functions) => Ok(value_syntax_from_grammar_id(
            registry.transform_list(functions, provenance),
        )),
        AttributeValues::ViewBox => Ok(value_syntax_from_shared_grammar(
            registry,
            "view-box",
            "viewBox tuple",
            GrammarNode::Sequence {
                items: vec![number_node(), number_node(), number_node(), number_node()],
            },
            provenance,
        )),
        AttributeValues::PreserveAspectRatio {
            alignments,
            meet_or_slice,
        } => Ok(preserve_aspect_ratio_value_syntax(
            registry,
            alignments,
            meet_or_slice,
            provenance,
        )),
        AttributeValues::Points => Ok(points_value_syntax(registry, provenance)),
        AttributeValues::PathData => Ok(value_syntax_from_shared_grammar(
            registry,
            "path-data",
            "Path data",
            GrammarNode::DatatypeRef {
                name: String::from("svg-path-data"),
            },
            provenance,
        )),
    }
}

fn foreign_value_syntax(
    snapshot: SpecSnapshotId,
    attribute_name: &str,
    values: &AttributeValues,
    provenance: &mut Vec<FactProvenance>,
    foreign_manifest: Option<&SourceManifest>,
) -> svg_data::extraction::Result<Option<ValueSyntax>> {
    let Some(binding) = foreign_reference_binding(snapshot, attribute_name, values) else {
        return Ok(None);
    };

    if let Some(foreign_manifest) = foreign_manifest {
        provenance.push(foreign_manifest.fact_provenance(
            binding.spec,
            ProvenanceSourceKind::ManualReview,
            SourceLocator::Fragment {
                anchor: binding.target.clone(),
            },
            ExtractionConfidence::Manual,
        )?);
    }

    Ok(Some(ValueSyntax::ForeignRef {
        spec: binding.spec.to_string(),
        target: binding.target,
    }))
}

struct ForeignReferenceBinding {
    spec: &'static str,
    target: String,
}

fn foreign_reference_binding(
    snapshot: SpecSnapshotId,
    attribute_name: &str,
    values: &AttributeValues,
) -> Option<ForeignReferenceBinding> {
    if !matches!(
        snapshot,
        SpecSnapshotId::Svg2Cr20181004 | SpecSnapshotId::Svg2EditorsDraft20250914
    ) {
        return None;
    }

    match attribute_name {
        "begin" | "dur" | "end" | "repeatDur" | "keyTimes" | "keySplines" | "restart"
        | "keyPoints" | "repeatCount" => Some(ForeignReferenceBinding {
            spec: "svg-animations",
            target: attribute_name.to_string(),
        }),
        "clip-path" | "mask" | "clipPathUnits" | "maskContentUnits" | "maskUnits" => {
            Some(ForeignReferenceBinding {
                spec: "css-masking-1",
                target: attribute_name.to_string(),
            })
        }
        "filter" => Some(ForeignReferenceBinding {
            spec: "filter-effects-1",
            target: attribute_name.to_string(),
        }),
        "operator" => Some(ForeignReferenceBinding {
            spec: "compositing-1",
            target: attribute_name.to_string(),
        }),
        "role" => Some(ForeignReferenceBinding {
            spec: "wai-aria-1.1",
            target: attribute_name.to_string(),
        }),
        _ if attribute_name.starts_with("aria-") => Some(ForeignReferenceBinding {
            spec: "wai-aria-1.1",
            target: attribute_name.to_string(),
        }),
        _ => match values {
            AttributeValues::Color => Some(ForeignReferenceBinding {
                spec: "css-color-4",
                target: String::from("<color>"),
            }),
            AttributeValues::Length => Some(ForeignReferenceBinding {
                spec: "css-values-3",
                target: String::from("<length>"),
            }),
            AttributeValues::NumberOrPercentage => Some(ForeignReferenceBinding {
                spec: "css-values-3",
                target: String::from("<number-or-percentage>"),
            }),
            _ => None,
        },
    }
}

fn value_syntax_from_shared_grammar(
    registry: &mut SeedGrammarRegistry,
    id: &str,
    title: &str,
    root: GrammarNode,
    provenance: &[FactProvenance],
) -> ValueSyntax {
    value_syntax_from_grammar_id(registry.shared(id, title, root, provenance))
}

const fn value_syntax_from_grammar_id(grammar_id: String) -> ValueSyntax {
    ValueSyntax::GrammarRef { grammar_id }
}

fn preserve_aspect_ratio_value_syntax(
    registry: &mut SeedGrammarRegistry,
    alignments: &[&str],
    meet_or_slice: &[&str],
    provenance: &[FactProvenance],
) -> ValueSyntax {
    value_syntax_from_shared_grammar(
        registry,
        "preserve-aspect-ratio",
        "preserveAspectRatio value",
        GrammarNode::Sequence {
            items: vec![
                grammar_choice_from_keywords(alignments),
                GrammarNode::Optional {
                    item: Box::new(grammar_choice_from_keywords(meet_or_slice)),
                },
            ],
        },
        provenance,
    )
}

fn points_value_syntax(
    registry: &mut SeedGrammarRegistry,
    provenance: &[FactProvenance],
) -> ValueSyntax {
    let coordinate_pair = registry.shared(
        "coordinate-pair",
        "Coordinate pair",
        GrammarNode::Sequence {
            items: vec![
                number_node(),
                GrammarNode::Optional {
                    item: Box::new(GrammarNode::Literal {
                        value: String::from(","),
                    }),
                },
                number_node(),
            ],
        },
        provenance,
    );

    value_syntax_from_shared_grammar(
        registry,
        "points",
        "Point list",
        GrammarNode::SpaceSeparated {
            item: Box::new(GrammarNode::GrammarRef {
                name: coordinate_pair,
            }),
        },
        provenance,
    )
}

fn number_node() -> GrammarNode {
    GrammarNode::DatatypeRef {
        name: String::from("number"),
    }
}

fn grammar_choice_from_keywords(values: &[&str]) -> GrammarNode {
    GrammarNode::Choice {
        options: values
            .iter()
            .map(|value| GrammarNode::Keyword {
                value: (*value).to_string(),
            })
            .collect(),
    }
}

#[derive(Default)]
struct SeedGrammarRegistry {
    definitions: BTreeMap<String, GrammarDefinition>,
}

impl SeedGrammarRegistry {
    fn finish(self) -> GrammarFile {
        GrammarFile {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            grammars: self.definitions.into_values().collect(),
        }
    }

    fn shared(
        &mut self,
        id: &str,
        title: &str,
        root: GrammarNode,
        provenance: &[FactProvenance],
    ) -> String {
        let grammar_id = id.to_string();
        self.definitions
            .entry(grammar_id.clone())
            .or_insert_with(|| GrammarDefinition {
                id: grammar_id.clone(),
                title: title.to_string(),
                root,
                provenance: provenance.to_vec(),
            });
        grammar_id
    }

    fn attribute_enum(
        &mut self,
        attribute_name: &str,
        values: &[&str],
        provenance: &[FactProvenance],
    ) -> String {
        let grammar_id = format!("enum-{}", stable_id_fragment(attribute_name));
        let title = format!("{attribute_name} keywords");
        self.shared(
            &grammar_id,
            &title,
            grammar_choice_from_keywords(values),
            provenance,
        )
    }

    fn transform_list(&mut self, functions: &[&str], provenance: &[FactProvenance]) -> String {
        if functions.is_empty() {
            return self.shared(
                "transform-list",
                "Transform list",
                GrammarNode::SpaceSeparated {
                    item: Box::new(GrammarNode::DatatypeRef {
                        name: String::from("transform-function"),
                    }),
                },
                provenance,
            );
        }

        let suffix = functions
            .iter()
            .map(|name| stable_id_fragment(name))
            .collect::<Vec<_>>()
            .join("-");
        let grammar_id = format!("transform-list-{suffix}");
        let title = format!("Transform list ({})", functions.join(", "));
        self.shared(
            &grammar_id,
            &title,
            GrammarNode::SpaceSeparated {
                item: Box::new(GrammarNode::Choice {
                    options: functions
                        .iter()
                        .map(|name| GrammarNode::DatatypeRef {
                            name: format!("{name}-transform-function"),
                        })
                        .collect(),
                }),
            },
            provenance,
        )
    }
}

fn stable_id_fragment(value: &str) -> String {
    let mut fragment = String::new();
    let mut last_was_separator = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            fragment.push(ch.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            fragment.push('-');
            last_was_separator = true;
        }
    }

    fragment.trim_matches('-').to_string()
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

struct SeedSourceBindings {
    element_index_input: &'static str,
    element_index_source_kind: ProvenanceSourceKind,
    element_index_confidence: ExtractionConfidence,
    element_index_file: Option<&'static str>,
    attribute_index_input: &'static str,
    attribute_index_source_kind: ProvenanceSourceKind,
    attribute_index_confidence: ExtractionConfidence,
    attribute_index_file: Option<&'static str>,
    detail_input: &'static str,
    detail_source_kind: ProvenanceSourceKind,
    detail_confidence: ExtractionConfidence,
    detail_file: Option<&'static str>,
    review_note: &'static str,
}

impl SeedSourceBindings {
    fn element_index_locator(&self, element_name: &str) -> SourceLocator {
        Self::index_locator(self.element_index_file, element_name)
    }

    fn attribute_index_locator(&self, attribute_name: &str) -> SourceLocator {
        Self::index_locator(self.attribute_index_file, attribute_name)
    }

    fn element_locator(&self, element_name: &str) -> SourceLocator {
        self.detail_locator(element_name)
    }

    fn attribute_locator(&self, attribute_name: &str) -> SourceLocator {
        self.detail_locator(attribute_name)
    }

    fn edge_locator(&self, element_name: &str, attribute_name: &str) -> SourceLocator {
        self.detail_locator(&format!("{element_name}@{attribute_name}"))
    }

    fn index_locator(file: Option<&str>, id: &str) -> SourceLocator {
        file.map_or_else(
            || SourceLocator::Fragment {
                anchor: id.to_string(),
            },
            |file| SourceLocator::Definition {
                file: file.to_string(),
                id: id.to_string(),
            },
        )
    }

    fn detail_locator(&self, id: &str) -> SourceLocator {
        Self::index_locator(self.detail_file, id)
    }
}

const fn seed_source_bindings(snapshot: SpecSnapshotId) -> SeedSourceBindings {
    match snapshot {
        SpecSnapshotId::Svg11Rec20030114 | SpecSnapshotId::Svg11Rec20110816 => SeedSourceBindings {
            element_index_input: "element-index",
            element_index_source_kind: ProvenanceSourceKind::Index,
            element_index_confidence: ExtractionConfidence::Derived,
            element_index_file: None,
            attribute_index_input: "attribute-index",
            attribute_index_source_kind: ProvenanceSourceKind::Index,
            attribute_index_confidence: ExtractionConfidence::Derived,
            attribute_index_file: None,
            detail_input: "flattened-dtd",
            detail_source_kind: ProvenanceSourceKind::Dtd,
            detail_confidence: ExtractionConfidence::Derived,
            detail_file: Some("DTD/svg11-flat.dtd"),
            review_note: "SVG 1.1 seed facts are backed by the flattened DTD plus TR indices until source-native ingestion replaces the seed.",
        },
        SpecSnapshotId::Svg2Cr20181004 => SeedSourceBindings {
            element_index_input: "element-index",
            element_index_source_kind: ProvenanceSourceKind::Index,
            element_index_confidence: ExtractionConfidence::Derived,
            element_index_file: None,
            attribute_index_input: "attribute-index",
            attribute_index_source_kind: ProvenanceSourceKind::Index,
            attribute_index_confidence: ExtractionConfidence::Derived,
            attribute_index_file: None,
            detail_input: "tr-root",
            detail_source_kind: ProvenanceSourceKind::Html,
            detail_confidence: ExtractionConfidence::Derived,
            detail_file: None,
            review_note: "SVG 2 CR seed facts stay SVG-owned only; foreign grammar and module references remain explicit follow-up work for later ingestion phases.",
        },
        SpecSnapshotId::Svg2EditorsDraft20250914 => SeedSourceBindings {
            element_index_input: "definitions",
            element_index_source_kind: ProvenanceSourceKind::DefinitionsXml,
            element_index_confidence: ExtractionConfidence::Derived,
            element_index_file: Some("definitions.xml"),
            attribute_index_input: "definitions",
            attribute_index_source_kind: ProvenanceSourceKind::DefinitionsXml,
            attribute_index_confidence: ExtractionConfidence::Derived,
            attribute_index_file: Some("definitions.xml"),
            detail_input: "definitions",
            detail_source_kind: ProvenanceSourceKind::DefinitionsXml,
            detail_confidence: ExtractionConfidence::Derived,
            detail_file: Some("definitions.xml"),
            review_note: "SVG 2 ED seed facts are anchored to the pinned svgwg commit and `definitions.xml`; chapter HTML and companion definitions files remain follow-up validation inputs until source-native ingestion replaces the seed.",
        },
    }
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
