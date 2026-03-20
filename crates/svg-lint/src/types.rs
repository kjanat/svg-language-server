use std::ops::Range;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    InvalidChild,
    MissingRequiredAttr,
    DeprecatedElement,
    DeprecatedAttribute,
    UnknownElement,
    UnknownAttribute,
    DuplicateId,
}
