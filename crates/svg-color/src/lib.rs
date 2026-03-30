//! SVG and CSS color parsing, extraction, and presentation helpers.

/// Color extraction from SVG/XML trees and inline CSS.
pub mod extract;
/// CSS named-color lookup table.
pub mod named_colors;
/// Color parsing and color-space conversion helpers.
pub mod parse;
/// Color presentation formatting for editor UIs.
pub mod present;
/// Shared color metadata types.
pub mod types;

pub use extract::{colors as extract_colors, colors_from_tree as extract_colors_from_tree};
pub use present::color_presentations;
pub use types::{ColorInfo, ColorKind};
