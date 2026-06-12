//! Shared build-time attribute-classification taxonomy.
//!
//! Both the SVG 2 Editor's Draft reader ([`spec_xml`](super::spec_xml)) and the
//! SVG 1.1 flat-DTD reader ([`dtd`](super::dtd)) normalize their respective
//! upstream attribute-collection group names into one [`Classification`] set,
//! so both editions expose attributes under a single bucket taxonomy. Factored
//! into its own module so a consumer that only needs the taxonomy (e.g. the
//! `dtd` reader, or a test target exercising it) does not have to pull in the
//! full `spec_xml` parser machinery.

/// Spec-derived classification of an attribute, normalized from the upstream
/// `attributecategory` group(s) it was declared under.
///
/// An attribute may carry **several** classifications (the SVG 2 ED lists
/// `onunload`, for instance, under both the `document event` and `window
/// event` groups). The mapping is derived purely from the upstream category
/// name string by [`Classification::from_category`]; the raw string is kept
/// alongside for provenance (see [`super::spec_xml::AttributeFacts::raw_categories`]).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Classification {
    /// The `core` attributecategory (`id`, `class`, `style`, `lang`,
    /// `tabindex`, `xml:space`, …).
    Core,
    /// The `presentation` attributecategory (the CSS-property-backed
    /// presentation attributes such as `fill`, `stroke`, `opacity`).
    Presentation,
    /// The `aria` attributecategory (the `aria-*` family plus `role`).
    Aria,
    /// Any of the event-handler categories (`global event`, `document
    /// event`, `window event`, `animation event`) — the `on*` handler
    /// attributes.
    EventHandler,
    /// The `deprecated xlink` attributecategory (`xlink:href`,
    /// `xlink:title`).
    Xlink,
    /// The `conditional processing` attributecategory
    /// (`requiredExtensions`, `systemLanguage`).
    ConditionalProcessing,
    /// Any classification whose upstream category does not map to one of
    /// the buckets above (e.g. `filter primitive`, `transfer function
    /// element`, the non-event `animation *` categories). The wrapped
    /// string is the raw upstream category name, preserved verbatim so no
    /// spec datum is dropped.
    Other(String),
}

impl Classification {
    /// Normalize one raw upstream `attributecategory` name into a
    /// [`Classification`]. Deterministic and total: every category string
    /// maps to exactly one variant, with [`Classification::Other`] carrying
    /// the verbatim name for anything outside the named buckets.
    pub fn from_category(category: &str) -> Self {
        match category {
            "core" => Self::Core,
            "presentation" => Self::Presentation,
            "aria" => Self::Aria,
            "global event" | "document event" | "window event" | "animation event" => {
                Self::EventHandler
            }
            "deprecated xlink" => Self::Xlink,
            "conditional processing" => Self::ConditionalProcessing,
            other => Self::Other(other.to_string()),
        }
    }
}
