use schemars::JsonSchema;
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
    /// Pre-computed compat verdicts per spec snapshot. One entry per
    /// snapshot the element is tracked in; empty slice when no verdict
    /// could be derived (defensive — shouldn't happen for covered
    /// snapshots). Consumers look up the verdict for the active profile
    /// with [`crate::compat_verdict_for_element`].
    pub verdicts: &'static [(SpecSnapshotId, CompatVerdict)],
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
    /// The element accepts the explicitly listed child elements by name,
    /// rather than by category. Mirrors the snapshot-schema `ElementSet` case
    /// for descriptors like `<animate>`'s narrowly defined content model.
    ChildrenSet(&'static [&'static str]),
    /// The element accepts any element from the SVG namespace. Mirrors the
    /// snapshot-schema `AnySvg` case used for root containers like `<svg>`.
    AnySvg,
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
    /// Pre-computed compat verdicts per spec snapshot. One entry per
    /// snapshot the attribute is tracked in; empty slice when no verdict
    /// could be derived. Consumers look up the verdict for the active
    /// profile with [`crate::compat_verdict_for_attribute`].
    pub verdicts: &'static [(SpecSnapshotId, CompatVerdict)],
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

/// Literal upstream `version_added` value from BCD, preserved verbatim.
///
/// Distinguishes three states the old flat-string shape conflated:
/// - `Text("50")` — supported since that version,
/// - `Flag(false)` — explicitly not supported,
/// - `Flag(true)` — supported with unknown version,
/// - `Null` — upstream has no data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawVersionAdded {
    /// Upstream version string (e.g. `"50"`, `"≤50"`).
    Text(&'static str),
    /// Explicit boolean: `true` = supported, `false` = unsupported.
    Flag(bool),
    /// Upstream explicitly emitted `null` / was absent.
    Null,
}

/// A single BCD flag statement (preference or runtime flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserFlag {
    /// Flag category, e.g. `"preference"` or `"runtime_flag"`.
    pub flag_type: &'static str,
    /// Preference / flag name.
    pub name: &'static str,
    /// Value the flag must be set to for the feature to work.
    pub value_to_set: Option<&'static str>,
}

/// Browser support state for a single browser.
///
/// Mirrors the worker's `BrowserVersion` sub-object. Every upstream
/// signal survives end-to-end: explicit non-support, version removal,
/// partial implementation, vendor prefix, runtime flags, and notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserVersion {
    /// Literal upstream value, byte-for-byte.
    pub raw_value_added: RawVersionAdded,
    /// Parsed version string when `raw_value_added` was a usable literal.
    pub version_added: Option<&'static str>,
    /// Qualifier on `version_added` (`≤` / `≥` / `~`).
    pub version_qualifier: Option<BaselineQualifier>,
    /// `Some(false)` when BCD explicitly stated "not supported";
    /// `Some(true)` when supported with unknown version.
    pub supported: Option<bool>,
    /// Upstream `version_removed` — present when support was dropped.
    pub version_removed: Option<&'static str>,
    /// Qualifier on `version_removed`.
    pub version_removed_qualifier: Option<BaselineQualifier>,
    /// `true` when the browser ships the feature but deviates from the spec.
    pub partial_implementation: bool,
    /// Vendor prefix required (e.g. `"-webkit-"`).
    pub prefix: Option<&'static str>,
    /// Alternative name under which the feature ships.
    pub alternative_name: Option<&'static str>,
    /// Preference / runtime flags gating the feature.
    pub flags: &'static [BrowserFlag],
    /// Free-form caveats, normalised to a slice of strings.
    pub notes: &'static [&'static str],
}

impl BrowserVersion {
    /// Empty sentinel used where a browser entry is missing entirely.
    pub const EMPTY: Self = Self {
        raw_value_added: RawVersionAdded::Null,
        version_added: None,
        version_qualifier: None,
        supported: None,
        version_removed: None,
        version_removed_qualifier: None,
        partial_implementation: false,
        prefix: None,
        alternative_name: None,
        flags: &[],
        notes: &[],
    };
}

/// Per-browser support data for the four major desktop browsers.
///
/// `None` means upstream is silent for that browser. A `Some(..)` with
/// `supported == Some(false)` means upstream explicitly says "not
/// supported" — this is a different state from "no data".
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

/// Recommendation level for a compat verdict.
///
/// Forms a total order `Safe < Caution < Avoid < Forbid` so that a
/// reason at a higher tier always overrides one at a lower tier. Maps
/// 1:1 to an LSP diagnostic severity so a lint rule and a hover badge
/// can't disagree on how urgent a compat issue is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VerdictRecommendation {
    /// Safe to use today. Wide baseline, not deprecated, no partial
    /// implementation or vendor prefix required.
    Safe,
    /// Use with care: partial implementation, prefix needed, flag
    /// required, or non-Baseline status. Behaviour may differ from
    /// the spec in at least one tracked engine.
    Caution,
    /// Avoid in new work: deprecated in BCD or in the selected spec
    /// profile, but still functional.
    Avoid,
    /// Do not use: explicitly removed from the current spec, or
    /// explicitly unsupported in every tracked engine.
    Forbid,
}

/// A single reason contributing to a compat verdict.
///
/// Renderers consume these as glyphs, bullet points, or diagnostic
/// messages. Multiple reasons at the same tier can co-exist — the
/// final recommendation is the max tier across all collected reasons,
/// but all reasons are preserved for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerdictReason {
    /// BCD marks the feature `deprecated: true`.
    BcdDeprecated,
    /// BCD marks the feature `experimental: true`.
    BcdExperimental,
    /// The feature is absent from the currently-selected spec snapshot.
    /// `last_seen` is the most recent snapshot it was still defined in.
    ProfileObsolete {
        /// Most recent snapshot in which the feature was still defined.
        last_seen: SpecSnapshotId,
    },
    /// The feature is experimental (draft-only) in the current profile.
    ProfileExperimental,
    /// Baseline `"limited"` across major browsers.
    BaselineLimited,
    /// Baseline `"newly available"` — supported in every engine but not
    /// long enough to qualify as widely available.
    BaselineNewly {
        /// Year the feature reached newly-available baseline.
        since: u16,
        /// Qualifier when the upstream date was inexact.
        qualifier: Option<BaselineQualifier>,
    },
    /// Some tracked browser ships a partial implementation.
    PartialImplementationIn(&'static str),
    /// Some tracked browser needs a vendor prefix.
    PrefixRequiredIn {
        /// Browser identifier (`"chrome"`, `"edge"`, `"firefox"`, `"safari"`).
        browser: &'static str,
        /// Prefix literal the browser requires (e.g. `"-webkit-"`).
        prefix: &'static str,
    },
    /// Some tracked browser gates the feature behind a preference or runtime flag.
    BehindFlagIn(&'static str),
    /// Some tracked browser explicitly reports no support.
    UnsupportedIn(&'static str),
    /// Some tracked browser removed support at a specific version.
    RemovedIn {
        /// Browser identifier.
        browser: &'static str,
        /// Version in which support was removed.
        version: &'static str,
        /// Qualifier on the removal version when upstream was inexact.
        qualifier: Option<BaselineQualifier>,
    },
}

/// A fully-reconciled compatibility verdict.
///
/// Both the LSP hover and the lint diagnostic paths consume this
/// struct — they never inspect raw `deprecated` / `baseline` /
/// `browser_support` fields directly. This guarantees the two
/// surfaces cannot disagree on urgency or phrasing.
///
/// `Copy` because `headline_template` is a static string key (not a
/// rendered message) and `reasons` is a `&'static` slice pointer.
/// Actual human-readable text interpolation happens in a non-Copy
/// formatter at the call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompatVerdict {
    /// Highest-tier recommendation across all collected reasons.
    pub recommendation: VerdictRecommendation,
    /// Static template key for the hover headline (e.g. `"removed from SVG 2"`).
    /// Renderer interpolates the feature name separately.
    pub headline_template: &'static str,
    /// Contributing reasons, sorted by tier descending then by
    /// collection order for tie-breaking.
    pub reasons: &'static [VerdictReason],
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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
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
    /// Tip of the catalogued snapshot timeline. Single source of truth for
    /// "is this the latest profile?" checks — shared between the build
    /// script (verdict synthesis), the runtime catalog (`lifecycle_for_profile`)
    /// and downstream lint rules (BCD-advice scoping). When a new snapshot
    /// lands, update this one constant.
    pub const LATEST: Self = Self::Svg2EditorsDraft20250914;

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
