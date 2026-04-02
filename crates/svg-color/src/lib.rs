//! SVG and CSS color parsing, extraction, and presentation helpers.
//!
//! # Examples
//!
//! ```
//! let svg = b"<svg><rect fill=\"red\"/></svg>";
//! let colors = svg_color::extract_colors(svg);
//! assert!(!colors.is_empty());
//! ```

/// Color extraction from SVG/XML trees and inline CSS.
pub(crate) mod extract;
/// CSS named-color lookup table.
pub(crate) mod named_colors;
/// Color parsing and color-space conversion helpers.
pub(crate) mod parse;
/// Color presentation formatting for editor UIs.
pub(crate) mod present;
/// Shared color metadata types.
pub(crate) mod types;

pub use extract::{colors as extract_colors, colors_from_tree as extract_colors_from_tree};
pub use present::color_presentations;
pub use types::{ColorInfo, ColorKind};
