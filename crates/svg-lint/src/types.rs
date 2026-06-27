use std::{collections::HashMap, ops::Range, str::FromStr};

use svg_data::{CompatVerdict, SpecSnapshotId};

/// Runtime override flags for deprecated/experimental status.
///
/// When present in a [`LintOverrides`] map, these override the baked-in
/// `svg_data` catalog values for a given element or attribute name.
///
/// # Examples
///
/// ```rust
/// let flags = svg_lint::CompatFlags { deprecated: true, experimental: false };
/// assert!(flags.deprecated);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct CompatFlags {
    /// Whether the element/attribute is deprecated.
    pub deprecated: bool,
    /// Whether the element/attribute is experimental.
    pub experimental: bool,
}

/// Runtime compat overrides for lint checks.
///
/// Maps element/attribute names to runtime compat flags that replace the
/// baked-in compat deprecation/experimental values used for diagnostics.
/// Names absent from the maps use the baked-in catalog flags. These overrides
/// do not change profile membership or spec lifecycle.
///
/// # Examples
///
/// ```rust
/// let mut overrides = svg_lint::LintOverrides::default();
/// overrides.elements.insert(
///     "demo".to_owned(),
///     svg_lint::CompatFlags { deprecated: true, experimental: false },
/// );
/// assert!(overrides.elements.contains_key("demo"));
/// ```
#[derive(Debug, Clone, Default)]
pub struct LintOverrides {
    /// Element name → override flags.
    pub elements: HashMap<String, CompatFlags>,
    /// Attribute name → override flags.
    pub attributes: HashMap<String, CompatFlags>,
}

/// Runtime overrides for the catalog-derived [`CompatVerdict`] that drives advisory
/// diagnostics (deprecation phrasing plus the partial / prefix /
/// behind-flag hints).
///
/// A newer BCD load can supply a fresh verdict for an element or
/// attribute name so the lint advisory tracks current data without
/// rebuilding the catalog. Names absent from a map keep the catalog-derived
/// verdict, so an empty (or unsupplied) [`VerdictOverrides`] is exactly
/// catalog behaviour.
///
/// Kept separate from [`LintOverrides`] so the existing flag-override
/// channel stays source-compatible.
///
/// # Examples
///
/// ```rust
/// let overrides = svg_lint::VerdictOverrides::default();
/// assert!(overrides.elements.is_empty());
/// ```
#[derive(Debug, Clone, Default)]
pub struct VerdictOverrides {
    /// Element name → runtime-overridden compat verdict.
    pub elements: HashMap<String, CompatVerdict>,
    /// Attribute name → runtime-overridden compat verdict.
    pub attributes: HashMap<String, CompatVerdict>,
}

/// A single diagnostic produced by the SVG linter.
///
/// # Examples
///
/// ```rust
/// let diagnostics = svg_lint::lint(br#"<svg><rect id="dup"/><circle id="dup"/></svg>"#);
/// assert!(diagnostics.iter().any(|diag| diag.code == svg_lint::DiagnosticCode::DuplicateId));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvgDiagnostic {
    /// Byte range in the original source.
    pub byte_range: Range<usize>,
    /// Start line in zero-based row coordinates.
    pub start_row: usize,
    /// Start column in bytes within `start_row`.
    pub start_col: usize,
    /// End line in zero-based row coordinates.
    pub end_row: usize,
    /// End column in bytes within `end_row`.
    pub end_col: usize,
    /// Diagnostic severity.
    pub severity: Severity,
    /// Machine-readable diagnostic code.
    pub code: DiagnosticCode,
    /// Human-readable diagnostic message.
    pub message: String,
}

/// Options controlling profile-aware lint behavior.
///
/// # Examples
///
/// ```rust
/// let options = svg_lint::LintOptions::default();
/// assert_eq!(options.profile, svg_data::SpecSnapshotId::Svg2EditorsDraft);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LintOptions {
    /// Selected pinned SVG snapshot.
    pub profile: SpecSnapshotId,
    /// Reductive profile constraints (SVG Native) to additionally enforce on top
    /// of the snapshot. `None` for an ordinary snapshot/edition target, which
    /// imposes no extra reductive constraint beyond its base catalog.
    pub native: Option<&'static svg_data::profile::SvgNative>,
    /// The edition inventory to restrict to, for an edition that has no faithful
    /// snapshot (e.g. SVG 1.0). The base `profile` is the nearest snapshot; this
    /// inventory drops constructs the exact edition never declared. `None` for a
    /// plain snapshot target.
    pub edition: Option<&'static svg_data::inventory::Inventory>,
}

impl Default for LintOptions {
    fn default() -> Self {
        Self {
            profile: SpecSnapshotId::Svg2EditorsDraft,
            native: None,
            edition: None,
        }
    }
}

/// Diagnostic severity levels (mirrors LSP).
///
/// # Examples
///
/// ```rust
/// let severity = svg_lint::Severity::Warning;
/// assert_eq!(severity, svg_lint::Severity::Warning);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Error severity.
    Error,
    /// Warning severity.
    Warning,
    /// Informational severity.
    Information,
    /// Hint severity.
    Hint,
}

/// Machine-readable diagnostic codes.
///
/// # Examples
///
/// ```rust
/// assert_eq!(svg_lint::DiagnosticCode::UnknownElement.as_str(), "UnknownElement");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    /// Child element is not allowed under its parent.
    InvalidChild,
    /// Required attribute is missing.
    MissingRequiredAttr,
    /// Deprecated element usage.
    DeprecatedElement,
    /// Deprecated attribute usage.
    DeprecatedAttribute,
    /// Element removed from the current SVG profile (stronger than deprecated).
    ObsoleteElement,
    /// Attribute removed from the current SVG profile (stronger than deprecated).
    ObsoleteAttribute,
    /// Experimental element usage.
    ExperimentalElement,
    /// Experimental attribute usage.
    ExperimentalAttribute,
    /// Unknown element name.
    UnknownElement,
    /// Known SVG feature absent from the selected profile.
    UnsupportedInProfile,
    /// Unknown attribute name.
    UnknownAttribute,
    /// Duplicate `id` value.
    DuplicateId,
    /// Reference target such as `url(#id)` is missing a definition.
    MissingReferenceDefinition,
    /// A tracked browser ships a partial implementation of this feature.
    /// Shared across elements and attributes — the element/attribute
    /// distinction isn't a meaningful axis for this advisory signal.
    PartialImplementation,
    /// A tracked browser requires a vendor prefix for this feature.
    PrefixRequired,
    /// A tracked browser only exposes this feature behind a preference or
    /// runtime flag.
    BehindFlag,
    /// A suppression directive did not suppress anything.
    UnusedSuppression,
}

impl DiagnosticCode {
    /// All known diagnostic codes in stable display order.
    pub const ALL: &'static [Self] = &[
        Self::InvalidChild,
        Self::MissingRequiredAttr,
        Self::DeprecatedElement,
        Self::DeprecatedAttribute,
        Self::ObsoleteElement,
        Self::ObsoleteAttribute,
        Self::ExperimentalElement,
        Self::ExperimentalAttribute,
        Self::UnknownElement,
        Self::UnsupportedInProfile,
        Self::UnknownAttribute,
        Self::DuplicateId,
        Self::MissingReferenceDefinition,
        Self::PartialImplementation,
        Self::PrefixRequired,
        Self::BehindFlag,
        Self::UnusedSuppression,
    ];

    /// Return the stable string representation used in diagnostics and comments.
    ///
    /// # Examples
    ///
    /// ```rust
    /// assert_eq!(svg_lint::DiagnosticCode::DuplicateId.as_str(), "DuplicateId");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidChild => "InvalidChild",
            Self::MissingRequiredAttr => "MissingRequiredAttr",
            Self::DeprecatedElement => "DeprecatedElement",
            Self::DeprecatedAttribute => "DeprecatedAttribute",
            Self::ObsoleteElement => "ObsoleteElement",
            Self::ObsoleteAttribute => "ObsoleteAttribute",
            Self::ExperimentalElement => "ExperimentalElement",
            Self::ExperimentalAttribute => "ExperimentalAttribute",
            Self::UnknownElement => "UnknownElement",
            Self::UnsupportedInProfile => "UnsupportedInProfile",
            Self::UnknownAttribute => "UnknownAttribute",
            Self::DuplicateId => "DuplicateId",
            Self::MissingReferenceDefinition => "MissingReferenceDefinition",
            Self::PartialImplementation => "PartialImplementation",
            Self::PrefixRequired => "PrefixRequired",
            Self::BehindFlag => "BehindFlag",
            Self::UnusedSuppression => "UnusedSuppression",
        }
    }
}

impl FromStr for DiagnosticCode {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "InvalidChild" => Ok(Self::InvalidChild),
            "MissingRequiredAttr" => Ok(Self::MissingRequiredAttr),
            "DeprecatedElement" => Ok(Self::DeprecatedElement),
            "DeprecatedAttribute" => Ok(Self::DeprecatedAttribute),
            "ObsoleteElement" => Ok(Self::ObsoleteElement),
            "ObsoleteAttribute" => Ok(Self::ObsoleteAttribute),
            "ExperimentalElement" => Ok(Self::ExperimentalElement),
            "ExperimentalAttribute" => Ok(Self::ExperimentalAttribute),
            "UnknownElement" => Ok(Self::UnknownElement),
            "UnsupportedInProfile" => Ok(Self::UnsupportedInProfile),
            "UnknownAttribute" => Ok(Self::UnknownAttribute),
            "DuplicateId" => Ok(Self::DuplicateId),
            "MissingReferenceDefinition" => Ok(Self::MissingReferenceDefinition),
            "PartialImplementation" => Ok(Self::PartialImplementation),
            "PrefixRequired" => Ok(Self::PrefixRequired),
            "BehindFlag" => Ok(Self::BehindFlag),
            "UnusedSuppression" => Ok(Self::UnusedSuppression),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
