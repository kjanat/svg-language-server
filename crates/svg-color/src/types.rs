use std::ops::Range;

/// A discovered color occurrence in source text.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorInfo {
    /// Red channel in `[0.0, 1.0]`.
    pub r: f32,
    /// Green channel in `[0.0, 1.0]`.
    pub g: f32,
    /// Blue channel in `[0.0, 1.0]`.
    pub b: f32,
    /// Alpha channel in `[0.0, 1.0]`.
    pub a: f32,
    /// Byte range of the color literal in the original source.
    pub byte_range: Range<usize>,
    /// Start line in zero-based row coordinates.
    pub start_row: usize,
    /// Start column in bytes within `start_row`.
    pub start_col: usize,
    /// End line in zero-based row coordinates.
    pub end_row: usize,
    /// End column in bytes within `end_row`.
    pub end_col: usize,
    /// Syntactic form the color was originally written in.
    pub kind: ColorKind,
}

/// The original representation kind of a parsed color literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorKind {
    /// Hex notation such as `#fff` or `#ff00cc`.
    Hex,
    /// Functional notation such as `rgb(...)` or `oklch(...)`.
    Functional,
    /// Named-color notation such as `red`.
    Named,
}
