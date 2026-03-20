pub mod extract;
pub mod named_colors;
pub mod parse;
pub mod present;
pub mod types;

pub use extract::{extract_colors, extract_colors_from_tree};
pub use present::color_presentations;
pub use types::{ColorInfo, ColorKind};
