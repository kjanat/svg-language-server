//! Typed constraint model for SVG **profiles**.
//!
//! A *profile* is a reductive subset of a base SVG edition — distinct from a
//! point-in-time version snapshot ([`crate::types::SpecSnapshotId`]). The first
//! profile modelled here is **SVG Native**, defined by the W3C SVG Working
//! Group as a set of reductive differences from SVG 2's *Secure Static Mode*.
//!
//! This module only carries the **data model**. The deterministic extractor
//! that parses the vendored Bikeshed spec source into a [`SvgNative`]
//! lives in the build tree (`build/svg_native.rs`) and is exercised by the
//! `svg_native_profile` reproduction/faithfulness test. The extracted dataset
//! is committed at `data/profiles/svg-native.json`.
//!
//! These types are deliberately *not* wired into the LSP profile axis yet —
//! that integration is a separate follow-on. The goal here is to get the
//! constraint data into the crate, typed and shareable.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What kind of SVG construct a single [`Constraint`] talks about.
///
/// The SVG Native spec phrases its reductive differences against four distinct
/// vocabularies — elements, attributes, presentation properties, and value
/// keywords — plus a handful of capability-level features (masking, external
/// resource loading, DTD subsets). Keeping the kind explicit lets consumers
/// route a constraint to the right namespace without re-parsing the name.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ConstraintKind {
    /// An SVG element, e.g. `text`, `marker`, `pattern`. Written `<{name}>` in
    /// the Bikeshed source.
    Element,
    /// An attribute or presentation property the spec lists without
    /// distinguishing the two, e.g. `clip`, `pathLength`. Written `'name'`.
    ///
    /// The spec prose does not consistently separate presentation *properties*
    /// from XML *attributes*; both appear as `'name'`. The [`Property`] kind is
    /// reserved for the cases the spec explicitly calls a property (the `<{ }>`
    /// dfn-style property references and the styling/text property lists).
    ///
    /// [`Property`]: ConstraintKind::Property
    Attribute,
    /// A presentation property the spec explicitly names as a property, e.g.
    /// `display`, `color`, `pointer-events`.
    Property,
    /// A value keyword, e.g. `context-fill`, `objectBoundingBox`, `calc()`.
    /// Written `''value''` in the Bikeshed source.
    Value,
    /// A capability or document-level feature with no single element/attribute
    /// name, e.g. masking, external resource loading, the XML DTD subset.
    Feature,
}

/// The scope an allowlist-style constraint is restricted to.
///
/// SVG Native expresses two shapes of constraint: flat removals ("X is not
/// supported") and *supported-only* allowlists ("transform is only supported
/// on these elements"). [`ConstraintScope`] captures the allowlist target so a
/// [`ConstraintRule::SupportedOnly`] is self-describing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "on", rename_all = "kebab-case")]
pub enum ConstraintScope {
    /// Supported only when carried by one of the listed elements (transform,
    /// viewBox, preserveAspectRatio bearers).
    Elements {
        /// Element names the feature is permitted on. Sorted, de-duplicated.
        names: Vec<String>,
    },
    /// Supported only for the listed value keywords (e.g. `gradientUnits` only
    /// accepts `userSpaceOnUse`).
    Values {
        /// Value keywords permitted for the constrained name. Sorted.
        names: Vec<String>,
    },
    /// Supported only for the listed unit tokens (the SVG Native length-unit
    /// allowlist). `(unitless)` is recorded as the literal token `unitless`.
    Units {
        /// Unit tokens permitted, sorted.
        names: Vec<String>,
    },
    /// Supported only for the listed raster image formats (image `data:` URL
    /// payloads).
    ImageFormats {
        /// Image format tokens permitted, sorted.
        names: Vec<String>,
    },
}

/// The rule a [`Constraint`] imposes on its named construct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "rule", rename_all = "kebab-case")]
pub enum ConstraintRule {
    /// The named construct is entirely unsupported by the profile — the most
    /// common SVG Native difference ("X is not supported by SVG Native").
    Unsupported,
    /// The named construct is supported only within the given scope; all other
    /// usages are unsupported ("transform is only supported on these
    /// elements", "only `userSpaceOnUse` is supported for `gradientUnits`").
    SupportedOnly {
        /// The allowlist the construct is narrowed to.
        scope: ConstraintScope,
    },
}

/// A single typed constraint the SVG Native profile imposes, with the spec
/// section it was extracted from for provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Constraint {
    /// Which vocabulary `name` belongs to.
    pub kind: ConstraintKind,
    /// The normalised construct name (backticks / link markup / Bikeshed
    /// reference syntax stripped). For [`ConstraintKind::Feature`] this is a
    /// stable slug describing the capability, e.g. `masking`.
    pub name: String,
    /// The rule imposed on `name`.
    #[serde(flatten)]
    pub rule: ConstraintRule,
    /// The Bikeshed section id (`{#anchor}`) the constraint was extracted from,
    /// e.g. `painting`, `text`, `coords`. Provenance for audit.
    pub section: String,
}

/// Provenance pin for the vendored spec source the dataset was extracted from.
///
/// Mirrors the `[pin]` table in the vendored `PROVENANCE.toml`. SVG Native is a
/// rolling Editor's Draft with no immutable dated URL, so the pin is a git
/// commit + capture date rather than a `/TR/` URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProvenancePin {
    /// Upstream repository the Bikeshed source was captured from.
    pub repository: String,
    /// The svgwg commit the source was captured at.
    pub commit: String,
    /// ISO-8601 capture date.
    pub capture_date: String,
    /// The base edition the profile is a reductive subset of.
    pub basis: String,
}

/// A section the extractor could not parse into structured constraints with
/// confidence — recorded so coverage gaps are explicit rather than silently
/// dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CoverageGap {
    /// The Bikeshed section id the gap is in.
    pub section: String,
    /// Why the prose resisted reliable structured extraction.
    pub reason: String,
}

/// The complete extracted SVG Native profile constraint dataset.
///
/// This is the root of the committed `data/profiles/svg-native.json` file and
/// the in-memory product of the build extractor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SvgNative {
    /// Schema version for forward-compatible evolution of this file shape.
    pub schema_version: u32,
    /// The profile slug, always `SvgNative` for this file.
    pub profile: String,
    /// Provenance of the vendored spec source.
    pub source_pin: ProvenancePin,
    /// Every extracted constraint, ordered deterministically by
    /// `(kind, name, section)`.
    pub constraints: Vec<Constraint>,
    /// Sections (or known facts) the heuristic prose extractor could not turn
    /// into structured constraints with confidence. Never empty silently — a
    /// gap recorded here is honest about coverage.
    pub coverage_gaps: Vec<CoverageGap>,
}

impl SvgNative {
    /// All constraints of a given [`ConstraintKind`].
    #[must_use]
    pub fn of_kind(&self, kind: ConstraintKind) -> Vec<&Constraint> {
        self.constraints.iter().filter(|c| c.kind == kind).collect()
    }

    /// Whether a construct of the given kind and name is recorded as fully
    /// [`ConstraintRule::Unsupported`].
    #[must_use]
    pub fn is_unsupported(&self, kind: ConstraintKind, name: &str) -> bool {
        self.constraints.iter().any(|c| {
            c.kind == kind && c.name == name && matches!(c.rule, ConstraintRule::Unsupported)
        })
    }

    /// The first `SupportedOnly` scope recorded for the given kind/name, if any.
    #[must_use]
    pub fn supported_only(&self, kind: ConstraintKind, name: &str) -> Option<&ConstraintScope> {
        self.constraints.iter().find_map(|c| match &c.rule {
            ConstraintRule::SupportedOnly { scope } if c.kind == kind && c.name == name => {
                Some(scope)
            }
            _ => None,
        })
    }
}

/// The baked SVG Native profile, parsed once from the committed
/// `data/profiles/svg-native.json`.
///
/// The dataset is a build-time invariant: the `svg_native_profile` reproduction
/// test proves the committed JSON deserialises and round-trips, so this parse
/// cannot fail in a shipped build. A corrupt baked file is a packaging bug, not
/// a recoverable runtime condition — it panics loudly rather than silently
/// degrading enforcement to "no constraints" (which would let unsupported
/// constructs through unnoticed).
///
/// # Panics
///
/// Panics if the baked `data/profiles/svg-native.json` is not valid `SvgNative`
/// JSON — a packaging/build invariant the `svg_native_profile` reproduction test
/// guarantees, so this cannot happen in a correctly built artifact.
#[must_use]
pub fn svg_native() -> &'static SvgNative {
    static CACHE: std::sync::OnceLock<SvgNative> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        match serde_json::from_str(include_str!("../data/profiles/svg-native.json")) {
            Ok(profile) => profile,
            Err(error) => {
                panic!("baked data/profiles/svg-native.json is not valid SvgNative JSON: {error}")
            }
        }
    })
}
