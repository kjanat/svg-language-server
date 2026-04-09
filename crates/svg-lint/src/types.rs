use std::{collections::HashMap, ops::Range, str::FromStr};

use svg_data::SpecSnapshotId;

/// Runtime override flags for deprecated/experimental status.
///
/// When present in a [`LintOverrides`] map, these override the baked-in
/// `svg_data` catalog values for a given element or attribute name.
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
#[derive(Debug, Clone, Default)]
pub struct LintOverrides {
    /// Element name → override flags.
    pub elements: HashMap<String, CompatFlags>,
    /// Attribute name → override flags.
    pub attributes: HashMap<String, CompatFlags>,
}

/// A single diagnostic produced by the SVG linter.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LintOptions {
    /// Selected pinned SVG snapshot.
    pub profile: SpecSnapshotId,
}

impl Default for LintOptions {
    fn default() -> Self {
        Self {
            profile: SpecSnapshotId::Svg2EditorsDraft20250914,
        }
    }
}

/// Diagnostic severity levels (mirrors LSP).
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
        Self::ExperimentalElement,
        Self::ExperimentalAttribute,
        Self::UnknownElement,
        Self::UnsupportedInProfile,
        Self::UnknownAttribute,
        Self::DuplicateId,
        Self::MissingReferenceDefinition,
        Self::UnusedSuppression,
    ];

    /// Return the stable string representation used in diagnostics and comments.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidChild => "InvalidChild",
            Self::MissingRequiredAttr => "MissingRequiredAttr",
            Self::DeprecatedElement => "DeprecatedElement",
            Self::DeprecatedAttribute => "DeprecatedAttribute",
            Self::ExperimentalElement => "ExperimentalElement",
            Self::ExperimentalAttribute => "ExperimentalAttribute",
            Self::UnknownElement => "UnknownElement",
            Self::UnsupportedInProfile => "UnsupportedInProfile",
            Self::UnknownAttribute => "UnknownAttribute",
            Self::DuplicateId => "DuplicateId",
            Self::MissingReferenceDefinition => "MissingReferenceDefinition",
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
            "ExperimentalElement" => Ok(Self::ExperimentalElement),
            "ExperimentalAttribute" => Ok(Self::ExperimentalAttribute),
            "UnknownElement" => Ok(Self::UnknownElement),
            "UnsupportedInProfile" => Ok(Self::UnsupportedInProfile),
            "UnknownAttribute" => Ok(Self::UnknownAttribute),
            "DuplicateId" => Ok(Self::DuplicateId),
            "MissingReferenceDefinition" => Ok(Self::MissingReferenceDefinition),
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
