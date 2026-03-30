use std::{ops::Range, str::FromStr};

/// A single diagnostic produced by the SVG linter.
#[derive(Debug, Clone, PartialEq)]
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
    #[must_use]
    /// Return the stable string representation used in diagnostics and comments.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidChild => "InvalidChild",
            Self::MissingRequiredAttr => "MissingRequiredAttr",
            Self::DeprecatedElement => "DeprecatedElement",
            Self::DeprecatedAttribute => "DeprecatedAttribute",
            Self::ExperimentalElement => "ExperimentalElement",
            Self::ExperimentalAttribute => "ExperimentalAttribute",
            Self::UnknownElement => "UnknownElement",
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
