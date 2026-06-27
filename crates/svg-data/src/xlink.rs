//! Canonicalization of legacy `xlink:` attribute names.

use std::borrow::Cow;

/// Map a legacy `xlink:`-namespaced attribute to its canonical SVG 2 name.
///
/// Only `xlink:href` has a clean canonical replacement (`href`); other
/// `xlink:*` attributes are returned unchanged. Returns a [`Cow`] so callers can
/// take ownership without forcing an allocation on the common (unchanged) path.
///
/// # Examples
///
/// ```rust
/// assert_eq!(svg_data::xlink::canonical_svg_attribute_name("xlink:href"), "href");
/// assert_eq!(svg_data::xlink::canonical_svg_attribute_name("fill"), "fill");
/// ```
#[must_use]
pub fn canonical_svg_attribute_name(name: &str) -> Cow<'_, str> {
    match name {
        "xlink:href" => Cow::Borrowed("href"),
        other => Cow::Borrowed(other),
    }
}
