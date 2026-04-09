//! Typed schema for checked-in per-snapshot SVG spec data.

use serde::{Deserialize, Serialize};

use crate::types::SpecSnapshotId;

/// Current checked-in schema version for normalized snapshot data.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Required checked-in files for each snapshot dataset.
pub const SNAPSHOT_REQUIRED_FILE_NAMES: &[&str] = &[
    "snapshot.json",
    "elements.json",
    "attributes.json",
    "grammars.json",
    "categories.json",
    "element_attribute_matrix.json",
    "exceptions.json",
    "review.json",
];

/// Typed payload for `snapshot.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotMetadataFile {
    /// Schema version for this checked-in snapshot payload.
    pub schema_version: u32,
    /// Canonical SVG snapshot id.
    pub snapshot: SpecSnapshotId,
    /// Human-readable title for review and generated reports.
    pub title: String,
    /// Publication date in `YYYY-MM-DD` form.
    pub date: String,
    /// Publication lifecycle of the snapshot.
    pub status: SnapshotStatus,
    /// Pinned source inputs used to derive the normalized facts.
    pub pinned_sources: Vec<SnapshotSourceRef>,
    /// Ingestion metadata for deterministic rebuilds.
    pub ingestion: IngestionMetadata,
}

/// Publication lifecycle of a tracked snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotStatus {
    /// W3C Recommendation snapshot.
    Recommendation,
    /// W3C Candidate Recommendation snapshot.
    CandidateRecommendation,
    /// Pinned editor's draft snapshot.
    EditorsDraft,
}

/// Pinned source reference used during extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotSourceRef {
    /// Checked-in manifest id under `data/sources/`.
    pub manifest_id: String,
    /// Input id within the manifest.
    pub input_id: String,
    /// Source authority classification.
    pub authority: SourceAuthority,
    /// Exact pin used for reproducible fetch.
    pub pin: SourcePin,
}

/// Whether an input is authoritative, assistive, or external.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceAuthority {
    /// Normative source for the fact set.
    Primary,
    /// Helpful extraction source that does not override authority.
    Supporting,
    /// Explicit reference into another spec ecosystem.
    ForeignReference,
}

/// Exact pin used to make source fetching reproducible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourcePin {
    /// Pinned absolute URL.
    Url {
        /// Canonical source URL.
        url: String,
    },
    /// Pinned repository commit and optional path.
    GitCommit {
        /// Repository URL.
        repository: String,
        /// Exact commit hash.
        commit: String,
        /// Optional path within the repo.
        path: Option<String>,
    },
}

/// Deterministic ingestion metadata for a snapshot dataset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestionMetadata {
    /// Version of the extractor pipeline that wrote the dataset.
    pub extractor_version: String,
    /// UTC date on which the snapshot was normalized.
    pub normalized_at: String,
}

/// Typed payload for `elements.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotElementRecord {
    /// Element tag name.
    pub name: String,
    /// Human-readable title or description.
    pub title: String,
    /// Category ids assigned to the element.
    pub categories: Vec<String>,
    /// Typed content-model description.
    pub content_model: ElementContentModel,
    /// Attributes explicitly associated with the element.
    pub attributes: Vec<String>,
    /// Source-backed provenance for this fact set.
    pub provenance: Vec<FactProvenance>,
}

/// Typed payload for `attributes.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotAttributeRecord {
    /// Attribute name.
    pub name: String,
    /// Human-readable title or description.
    pub title: String,
    /// Structured value syntax owned by SVG or a foreign spec.
    pub value_syntax: ValueSyntax,
    /// Defaulting behavior in the snapshot.
    pub default_value: AttributeDefaultValue,
    /// Whether the attribute is animatable.
    pub animatable: AnimationBehavior,
    /// Source-backed provenance for this fact set.
    pub provenance: Vec<FactProvenance>,
}

/// Structured element content model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ElementContentModel {
    /// Element must be empty.
    Empty,
    /// Element primarily contains text nodes.
    TextOnly,
    /// Element accepts any SVG child element.
    AnySvg,
    /// Element accepts children from listed categories.
    CategorySet {
        /// Category ids allowed as children.
        categories: Vec<String>,
    },
    /// Element accepts a fixed set of child element names.
    ElementSet {
        /// Child element names allowed in the snapshot.
        elements: Vec<String>,
    },
    /// Element accepts foreign-namespace content.
    ForeignNamespace,
}

/// Structured value syntax reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueSyntax {
    /// Value grammar is defined in `grammars.json`.
    GrammarRef {
        /// Grammar id in the same snapshot.
        grammar_id: String,
    },
    /// Value syntax is defined in another pinned specification.
    ForeignRef {
        /// External spec or module id.
        spec: String,
        /// Target id within that spec.
        target: String,
    },
    /// Temporary escape hatch for not-yet-normalized syntax.
    Opaque {
        /// Human-readable display form.
        display: String,
        /// Why structured normalization is not available yet.
        reason: String,
    },
}

/// Default-value representation for attributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttributeDefaultValue {
    /// No default value is defined.
    None,
    /// Concrete literal default value.
    Literal {
        /// Serialized default value.
        value: String,
    },
    /// Inherits from the parent element or cascade.
    Inherit,
}

/// Whether an attribute is animatable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationBehavior {
    /// The spec marks the attribute animatable.
    Animatable,
    /// The spec marks the attribute non-animatable.
    NotAnimatable,
    /// The source does not say clearly enough yet.
    Unspecified,
}

/// Typed payload for `grammars.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrammarFile {
    /// Schema version for the grammar payload.
    pub schema_version: u32,
    /// Grammar definitions keyed by stable id.
    pub grammars: Vec<GrammarDefinition>,
}

/// Named grammar definition for a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrammarDefinition {
    /// Stable grammar id.
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Structured grammar tree.
    pub root: GrammarNode,
    /// Provenance for the grammar definition.
    pub provenance: Vec<FactProvenance>,
}

/// Structured grammar AST node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GrammarNode {
    /// Exact keyword token.
    Keyword {
        /// Keyword text.
        value: String,
    },
    /// Built-in datatype reference.
    DatatypeRef {
        /// Datatype id.
        name: String,
    },
    /// Reference to another grammar in the same snapshot.
    GrammarRef {
        /// Grammar id.
        name: String,
    },
    /// Ordered sequence of nodes.
    Sequence {
        /// Child nodes in order.
        items: Vec<Self>,
    },
    /// One of many alternatives.
    Choice {
        /// Alternative branches.
        options: Vec<Self>,
    },
    /// Optional child node.
    Optional {
        /// Optional child.
        item: Box<Self>,
    },
    /// Zero or more repetitions.
    ZeroOrMore {
        /// Repeated child.
        item: Box<Self>,
    },
    /// One or more repetitions.
    OneOrMore {
        /// Repeated child.
        item: Box<Self>,
    },
    /// Comma-separated repeated child.
    CommaSeparated {
        /// Repeated child.
        item: Box<Self>,
    },
    /// Space-separated repeated child.
    SpaceSeparated {
        /// Repeated child.
        item: Box<Self>,
    },
    /// Explicit bounded repetition.
    Repeat {
        /// Repeated child.
        item: Box<Self>,
        /// Minimum count.
        min: u16,
        /// Maximum count if bounded.
        max: Option<u16>,
    },
    /// Fixed literal token.
    Literal {
        /// Literal token.
        value: String,
    },
    /// Temporary opaque syntax escape hatch.
    Opaque {
        /// Human-readable display form.
        display: String,
        /// Why the syntax is still opaque.
        reason: String,
    },
    /// Reference into a pinned external grammar.
    ForeignRef {
        /// External spec or module id.
        spec: String,
        /// Target id within the foreign spec.
        target: String,
    },
}

/// Typed payload for `categories.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoriesFile {
    /// Schema version for the category payload.
    pub schema_version: u32,
    /// Element-category memberships.
    pub element_categories: Vec<ElementCategoryMembership>,
    /// Attribute-category memberships.
    pub attribute_categories: Vec<AttributeCategoryMembership>,
}

/// Category membership for one element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementCategoryMembership {
    /// Element name.
    pub element: String,
    /// Category id.
    pub category: String,
    /// Supporting provenance.
    pub provenance: Vec<FactProvenance>,
}

/// Category membership for one attribute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeCategoryMembership {
    /// Attribute name.
    pub attribute: String,
    /// Category id.
    pub category: String,
    /// Supporting provenance.
    pub provenance: Vec<FactProvenance>,
}

/// Typed payload for `element_attribute_matrix.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementAttributeMatrixFile {
    /// Schema version for the matrix payload.
    pub schema_version: u32,
    /// Explicit applicability edges.
    pub edges: Vec<ElementAttributeEdge>,
}

/// One explicit element-to-attribute applicability edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementAttributeEdge {
    /// Element name.
    pub element: String,
    /// Attribute name.
    pub attribute: String,
    /// Whether the attribute is required or optional.
    pub requirement: AttributeRequirement,
    /// Supporting provenance.
    pub provenance: Vec<FactProvenance>,
}

/// Requiredness of an applicability edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeRequirement {
    /// Attribute is required for valid use.
    Required,
    /// Attribute is allowed but not required.
    Optional,
}

/// Typed payload for `exceptions.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExceptionsFile {
    /// Schema version for the exceptions payload.
    pub schema_version: u32,
    /// Curated exceptions for prose-only or source-bug cases.
    pub exceptions: Vec<SnapshotException>,
}

/// Curated exception attached to a snapshot fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotException {
    /// Stable exception id.
    pub id: String,
    /// Narrow scope for the exception.
    pub scope: ExceptionScope,
    /// Human-reviewed disposition.
    pub disposition: ExceptionDisposition,
    /// Why the exception exists.
    pub reason: String,
    /// Provenance for the exception source.
    pub provenance: Vec<FactProvenance>,
}

/// Scope of a curated exception.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExceptionScope {
    /// Exception applies to snapshot metadata.
    Snapshot,
    /// Exception applies to one element.
    Element {
        /// Element name.
        name: String,
    },
    /// Exception applies to one attribute.
    Attribute {
        /// Attribute name.
        name: String,
    },
    /// Exception applies to one element/attribute edge.
    ElementAttribute {
        /// Element name.
        element: String,
        /// Attribute name.
        attribute: String,
    },
    /// Exception applies to one grammar.
    Grammar {
        /// Grammar id.
        grammar_id: String,
    },
}

/// Reviewer-approved action for an exception.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExceptionDisposition {
    /// Override extracted data with the curated fix.
    Corrected,
    /// Keep extracted data but flag it for manual review.
    Deferred,
}

/// Typed payload for `review.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewFile {
    /// Schema version for the review payload.
    pub schema_version: u32,
    /// Aggregate counts for the normalized dataset.
    pub counts: ReviewCounts,
    /// Remaining unresolved items.
    pub unresolved: Vec<ReviewIssue>,
    /// Free-form review notes for humans.
    pub manual_notes: Vec<String>,
}

/// Aggregate snapshot counts used during review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewCounts {
    /// Number of normalized elements.
    pub elements: usize,
    /// Number of normalized attributes.
    pub attributes: usize,
    /// Number of grammar definitions.
    pub grammars: usize,
    /// Number of applicability edges.
    pub applicability_edges: usize,
    /// Number of exceptions.
    pub exceptions: usize,
}

/// Review finding that still needs action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewIssue {
    /// Stable issue id.
    pub id: String,
    /// Severity for release gating.
    pub severity: ReviewSeverity,
    /// Human-readable summary.
    pub summary: String,
}

/// Severity for a snapshot review issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSeverity {
    /// Blocks review completion.
    Error,
    /// Needs follow-up but does not block all work.
    Warning,
    /// Informational review note.
    Info,
}

/// Provenance attached to a normalized fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactProvenance {
    /// Source input id from the pinned manifest.
    pub source_id: String,
    /// Kind of source material used for extraction.
    pub source_kind: ProvenanceSourceKind,
    /// Exact pinned location for the source.
    pub pin: SourcePin,
    /// Concrete source locator within the pinned input.
    pub locator: SourceLocator,
    /// Extraction confidence classification.
    pub confidence: ExtractionConfidence,
}

/// Source material used for a normalized fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceSourceKind {
    /// W3C TR or editor's draft chapter HTML.
    Html,
    /// Element or attribute index page.
    Index,
    /// Flattened DTD or DTD-derived source.
    Dtd,
    /// `definitions.xml` or companion definitions file.
    DefinitionsXml,
    /// Manual curated note.
    ManualReview,
}

/// Exact location inside a pinned source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceLocator {
    /// HTML fragment id.
    Fragment {
        /// Fragment id or anchor name.
        anchor: String,
    },
    /// Structured definition in a file.
    Definition {
        /// File path relative to the pinned source root.
        file: String,
        /// Definition id within the file.
        id: String,
    },
    /// Specific line range in a text file.
    LineRange {
        /// File path relative to the pinned source root.
        file: String,
        /// Starting 1-based line number.
        start_line: u32,
        /// Inclusive ending 1-based line number.
        end_line: u32,
    },
}

/// Confidence attached to an extracted fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionConfidence {
    /// Parsed from a structured authoritative source.
    Exact,
    /// Derived from a reliable but indirect source.
    Derived,
    /// Human-curated fallback.
    Manual,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_snapshot_files_match_design() {
        assert_eq!(
            SNAPSHOT_REQUIRED_FILE_NAMES,
            &[
                "snapshot.json",
                "elements.json",
                "attributes.json",
                "grammars.json",
                "categories.json",
                "element_attribute_matrix.json",
                "exceptions.json",
                "review.json",
            ]
        );
    }

    #[test]
    fn grammar_nodes_round_trip_with_explicit_tags() -> Result<(), serde_json::Error> {
        let node = GrammarNode::Sequence {
            items: vec![
                GrammarNode::Choice {
                    options: vec![
                        GrammarNode::Keyword {
                            value: "auto".into(),
                        },
                        GrammarNode::GrammarRef {
                            name: "paint".into(),
                        },
                    ],
                },
                GrammarNode::Optional {
                    item: Box::new(GrammarNode::ForeignRef {
                        spec: "css-values-4".into(),
                        target: "<length-percentage>".into(),
                    }),
                },
            ],
        };

        let json = serde_json::to_value(&node)?;
        let serde_json::Value::Object(object) = &json else {
            panic!("tagged grammar node should serialize as an object");
        };

        assert_eq!(
            object.get("kind"),
            Some(&serde_json::Value::String("sequence".into()))
        );

        let round_trip: GrammarNode = serde_json::from_value(json)?;
        assert_eq!(round_trip, node);
        Ok(())
    }

    #[test]
    fn snapshot_metadata_uses_typed_snapshot_ids_and_source_pins() -> Result<(), serde_json::Error>
    {
        let metadata = SnapshotMetadataFile {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            snapshot: SpecSnapshotId::Svg2EditorsDraft20250914,
            title: "SVG 2 Editor's Draft".into(),
            date: "2025-09-14".into(),
            status: SnapshotStatus::EditorsDraft,
            pinned_sources: vec![SnapshotSourceRef {
                manifest_id: "svg2-ed-20250914".into(),
                input_id: "publish-xml".into(),
                authority: SourceAuthority::Primary,
                pin: SourcePin::GitCommit {
                    repository: "https://github.com/w3c/svgwg".into(),
                    commit: "19482daf4094e72becde92b38c6a1c0d384b56a9".into(),
                    path: Some("master/publish.xml".into()),
                },
            }],
            ingestion: IngestionMetadata {
                extractor_version: "snapshot-schema-v1".into(),
                normalized_at: "2026-04-09".into(),
            },
        };

        let json = serde_json::to_value(&metadata)?;

        assert_eq!(
            json.get("snapshot"),
            Some(&serde_json::Value::String(
                "Svg2EditorsDraft20250914".into()
            ))
        );

        let round_trip: SnapshotMetadataFile = serde_json::from_value(json)?;
        assert_eq!(round_trip, metadata);
        Ok(())
    }
}
