use std::{ops::Range, str::FromStr};

/// A single diagnostic produced by the SVG linter.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgDiagnostic {
    pub byte_range: Range<usize>,
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
}

/// Diagnostic severity levels (mirrors LSP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

/// Machine-readable diagnostic codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    InvalidChild,
    MissingRequiredAttr,
    DeprecatedElement,
    DeprecatedAttribute,
    ExperimentalElement,
    ExperimentalAttribute,
    UnknownElement,
    UnknownAttribute,
    DuplicateId,
    MissingReferenceDefinition,
    UnusedSuppression,
}

impl DiagnosticCode {
    #[must_use]
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
