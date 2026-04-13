use serde::{Deserialize, Serialize};

/// Definition of an SVG element.
#[derive(Debug, Clone)]
pub struct ElementDef {
    /// Element tag name, for example `rect`.
    pub name: &'static str,
    /// Short human-readable description.
    pub description: &'static str,
    /// MDN reference URL for the element.
    pub mdn_url: &'static str,
    /// Spec lifecycle derived from pinned SVG snapshot metadata.
    pub spec_lifecycle: SpecLifecycle,
    /// Whether browser/runtime compatibility data marks the element deprecated.
    pub deprecated: bool,
    /// Whether browser/runtime compatibility data marks the element experimental.
    pub experimental: bool,
    /// Primary specification URL when known.
    pub spec_url: Option<&'static str>,
    /// Baseline support status when known.
    pub baseline: Option<BaselineStatus>,
    /// Per-browser desktop support data when known.
    pub browser_support: Option<BrowserSupport>,
    /// Structural child-content model for the element.
    pub content_model: ContentModel,
    /// Attributes that must be present for valid usage.
    pub required_attrs: &'static [&'static str],
    /// Element-specific attributes accepted by this element.
    pub attrs: &'static [&'static str],
    /// Whether the element also accepts SVG global attributes.
    pub global_attrs: bool,
}

/// Whether an element is a container, void, or text-content element.
#[derive(Debug, Clone)]
pub enum ContentModel {
    /// The element accepts children from the listed categories.
    Children(&'static [ElementCategory]),
    /// The element accepts foreign-namespace content such as HTML.
    Foreign,
    /// The element is empty and must not have children.
    Void,
    /// The element primarily contains inline text content.
    Text,
}

/// Definition of an SVG attribute.
#[derive(Debug, Clone)]
pub struct AttributeDef {
    /// Attribute name, for example `fill`.
    pub name: &'static str,
    /// Short human-readable description.
    pub description: &'static str,
    /// MDN reference URL for the attribute.
    pub mdn_url: &'static str,
    /// Spec lifecycle derived from pinned SVG snapshot metadata.
    pub spec_lifecycle: SpecLifecycle,
    /// Whether browser/runtime compatibility data marks the attribute deprecated.
    pub deprecated: bool,
    /// Whether browser/runtime compatibility data marks the attribute experimental.
    pub experimental: bool,
    /// Primary specification URL when known.
    pub spec_url: Option<&'static str>,
    /// Baseline support status when known.
    pub baseline: Option<BaselineStatus>,
    /// Per-browser desktop support data when known.
    pub browser_support: Option<BrowserSupport>,
    /// High-level value-shape description for completion and hover logic.
    pub values: AttributeValues,
    /// Elements the attribute applies to; `*` means global applicability.
    pub elements: &'static [&'static str],
}

/// What kind of values an attribute accepts.
#[derive(Debug, Clone)]
pub enum AttributeValues {
    /// One of the listed keywords.
    Enum(&'static [&'static str]),
    /// Free-form text with no constrained grammar.
    FreeText,
    /// A CSS/SVG color value.
    Color,
    /// A length value with optional units.
    Length,
    /// A URL or fragment reference.
    Url,
    /// A numeric value or percentage.
    NumberOrPercentage,
    /// A transform list, optionally constrained to the listed function names.
    Transform(&'static [&'static str]),
    /// A `min-x min-y width height` viewBox tuple.
    ViewBox,
    /// A `preserveAspectRatio` value split into alignment and meet/slice parts.
    PreserveAspectRatio {
        /// Allowed alignment keywords.
        alignments: &'static [&'static str],
        /// Allowed `meet` / `slice` keywords.
        meet_or_slice: &'static [&'static str],
    },
    /// A point-list value such as `x,y x,y`.
    Points,
    /// SVG path-data syntax.
    PathData,
}

/// Qualifier on a baseline year when the upstream date carried a comparison prefix.
///
/// Mirrors `svg-compat` worker's `since_qualifier` field end-to-end so
/// the LSP hover and lint diagnostics can render `≤2021` rather than
/// silently lying about the precise year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BaselineQualifier {
    /// At-or-before the year (web-features `≤` / `<` / `<=`).
    Before,
    /// At-or-after the year (web-features `≥` / `>` / `>=`).
    After,
    /// Fuzzy or unknown upstream prefix (web-features `~`, or any
    /// future prefix the worker didn't recognise).
    Approximately,
}

/// Baseline browser support status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineStatus {
    /// Supported broadly across current browsers since the given year.
    Widely {
        /// First calendar year in which the feature was considered widely available.
        since: u16,
        /// Qualifier on `since` when the upstream date was inexact.
        qualifier: Option<BaselineQualifier>,
    },
    /// Newly in Baseline since the given year.
    Newly {
        /// First calendar year in which the feature entered Baseline.
        since: u16,
        /// Qualifier on `since` when the upstream date was inexact.
        qualifier: Option<BaselineQualifier>,
    },
    /// Not part of Baseline support.
    Limited,
}

/// Browser support status for a single browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserVersion {
    /// The feature is supported, but the first version is unknown.
    Unknown,
    /// The feature is supported starting with the given version.
    Version(&'static str),
}

/// Per-browser support data for the four major desktop browsers.
///
/// `None` means the browser does not support the feature.
/// `Some(BrowserVersion::Unknown)` means support is known but the first
/// version is not.
/// `Some(BrowserVersion::Version("85"))` means support was added in that
/// version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserSupport {
    /// Chrome desktop support data.
    pub chrome: Option<BrowserVersion>,
    /// Edge desktop support data.
    pub edge: Option<BrowserVersion>,
    /// Firefox desktop support data.
    pub firefox: Option<BrowserVersion>,
    /// Safari desktop support data.
    pub safari: Option<BrowserVersion>,
}

/// Spec lifecycle for a known SVG element or attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecLifecycle {
    /// Stable in the selected or union spec metadata.
    Stable,
    /// Present only in a draft or non-stable snapshot.
    Experimental,
    /// Explicitly deprecated by spec metadata.
    Deprecated,
    /// Removed from later snapshots but still known historically.
    Obsolete,
}

/// SVG element categories for content model grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementCategory {
    /// General grouping/container elements.
    Container,
    /// Shape elements such as `rect` and `circle`.
    Shape,
    /// Text-content elements.
    Text,
    /// Gradient elements.
    Gradient,
    /// Filter container elements.
    Filter,
    /// Descriptive metadata elements.
    Descriptive,
    /// Structural elements such as `svg`, `g`, and `defs`.
    Structural,
    /// Animation elements.
    Animation,
    /// Paint-server elements such as gradients and patterns.
    PaintServer,
    /// Clipping and masking elements.
    ClipMask,
    /// Filter light-source elements.
    LightSource,
    /// Filter primitive elements.
    FilterPrimitive,
    /// Transfer-function elements inside component transfer filters.
    TransferFunction,
    /// `feMergeNode`-style merge children.
    MergeNode,
    /// Motion-path helper elements.
    MotionPath,
    /// Elements that never render directly.
    NeverRendered,
}

/// Canonical SVG spec snapshot identifiers supported by profile-aware lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpecSnapshotId {
    /// SVG 1.1 Recommendation (2003-01-14).
    Svg11Rec20030114,
    /// SVG 1.1 Second Edition Recommendation (2011-08-16).
    Svg11Rec20110816,
    /// SVG 2 Candidate Recommendation (2018-10-04).
    Svg2Cr20181004,
    /// Pinned SVG 2 Editor's Draft snapshot (2025-09-14).
    Svg2EditorsDraft20250914,
}

impl SpecSnapshotId {
    /// Return the canonical stable string id used in config and diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Svg11Rec20030114 => "Svg11Rec20030114",
            Self::Svg11Rec20110816 => "Svg11Rec20110816",
            Self::Svg2Cr20181004 => "Svg2Cr20181004",
            Self::Svg2EditorsDraft20250914 => "Svg2EditorsDraft20250914",
        }
    }
}

/// Static metadata describing a supported SVG spec snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecSnapshotMetadata {
    /// Canonical snapshot id.
    pub canonical_id: SpecSnapshotId,
    /// Accepted aliases and long-form synonyms.
    pub aliases: &'static [&'static str],
    /// Upstream source URL for the snapshot.
    pub source_url: &'static str,
    /// Snapshot date in `YYYY-MM-DD` form.
    pub snapshot_date: &'static str,
    /// Stable baseline snapshot used to derive draft-only additions.
    pub stable_base: Option<SpecSnapshotId>,
    /// Whether published errata are folded into this snapshot.
    pub errata_folded: bool,
}

/// Result of resolving a known SVG feature against a specific profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileLookup<T> {
    /// The feature exists in the selected profile.
    Present {
        /// Canonical union definition for the feature.
        value: T,
        /// Spec lifecycle in the selected profile.
        lifecycle: SpecLifecycle,
    },
    /// The feature is known in SVG, but not in the selected profile.
    UnsupportedInProfile {
        /// Snapshots where the feature is known to exist.
        known_in: &'static [SpecSnapshotId],
    },
    /// The feature is not known in any tracked SVG snapshot.
    Unknown,
}

/// Element definition paired with lifecycle in a selected profile.
#[derive(Debug, Clone, Copy)]
pub struct ProfiledElement {
    /// Canonical union definition for the element.
    pub element: &'static ElementDef,
    /// Spec lifecycle in the selected profile.
    pub lifecycle: SpecLifecycle,
}

/// Attribute definition paired with lifecycle in a selected profile.
#[derive(Debug, Clone, Copy)]
pub struct ProfiledAttribute {
    /// Canonical union definition for the attribute.
    pub attribute: &'static AttributeDef,
    /// Spec lifecycle in the selected profile.
    pub lifecycle: SpecLifecycle,
}
