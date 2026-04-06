/// Definition of an SVG element.
#[derive(Debug, Clone)]
pub struct ElementDef {
    /// Element tag name, for example `rect`.
    pub name: &'static str,
    /// Short human-readable description.
    pub description: &'static str,
    /// MDN reference URL for the element.
    pub mdn_url: &'static str,
    /// Whether the element is deprecated.
    pub deprecated: bool,
    /// Whether the element is experimental.
    pub experimental: bool,
    /// Primary specification URL when known.
    pub spec_url: Option<&'static str>,
    /// Baseline support status when known.
    pub baseline: Option<BaselineStatus>,
    /// Per-browser desktop support data when known.
    pub browser_support: Option<BrowserSupport>,
    /// Structural child-content model for the element.
    pub content_model: ContentModel,
    /// Attributes that must be present for valid usage.
    pub required_attrs: &'static [&'static str],
    /// Element-specific attributes accepted by this element.
    pub attrs: &'static [&'static str],
    /// Whether the element also accepts SVG global attributes.
    pub global_attrs: bool,
}

/// Whether an element is a container, void, or text-content element.
#[derive(Debug, Clone)]
pub enum ContentModel {
    /// The element accepts children from the listed categories.
    Children(&'static [ElementCategory]),
    /// The element accepts foreign-namespace content such as HTML.
    Foreign,
    /// The element is empty and must not have children.
    Void,
    /// The element primarily contains inline text content.
    Text,
}

/// Definition of an SVG attribute.
#[derive(Debug, Clone)]
pub struct AttributeDef {
    /// Attribute name, for example `fill`.
    pub name: &'static str,
    /// Short human-readable description.
    pub description: &'static str,
    /// MDN reference URL for the attribute.
    pub mdn_url: &'static str,
    /// Whether the attribute is deprecated.
    pub deprecated: bool,
    /// Whether the attribute is experimental.
    pub experimental: bool,
    /// Primary specification URL when known.
    pub spec_url: Option<&'static str>,
    /// Baseline support status when known.
    pub baseline: Option<BaselineStatus>,
    /// Per-browser desktop support data when known.
    pub browser_support: Option<BrowserSupport>,
    /// High-level value-shape description for completion and hover logic.
    pub values: AttributeValues,
    /// Elements the attribute applies to; `*` means global applicability.
    pub elements: &'static [&'static str],
}

/// What kind of values an attribute accepts.
#[derive(Debug, Clone)]
pub enum AttributeValues {
    /// One of the listed keywords.
    Enum(&'static [&'static str]),
    /// Free-form text with no constrained grammar.
    FreeText,
    /// A CSS/SVG color value.
    Color,
    /// A length value with optional units.
    Length,
    /// A URL or fragment reference.
    Url,
    /// A numeric value or percentage.
    NumberOrPercentage,
    /// A transform list, optionally constrained to the listed function names.
    Transform(&'static [&'static str]),
    /// A `min-x min-y width height` viewBox tuple.
    ViewBox,
    /// A `preserveAspectRatio` value split into alignment and meet/slice parts.
    PreserveAspectRatio {
        /// Allowed alignment keywords.
        alignments: &'static [&'static str],
        /// Allowed `meet` / `slice` keywords.
        meet_or_slice: &'static [&'static str],
    },
    /// A point-list value such as `x,y x,y`.
    Points,
    /// SVG path-data syntax.
    PathData,
}

/// Baseline browser support status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineStatus {
    /// Supported broadly across current browsers since the given year.
    Widely {
        /// First calendar year in which the feature was considered widely available.
        since: u16,
    },
    /// Newly in Baseline since the given year.
    Newly {
        /// First calendar year in which the feature entered Baseline.
        since: u16,
    },
    /// Not part of Baseline support.
    Limited,
}

/// Browser support status for a single browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserVersion {
    /// The feature is supported, but the first version is unknown.
    Unknown,
    /// The feature is supported starting with the given version.
    Version(&'static str),
}

/// Per-browser support data for the four major desktop browsers.
///
/// `None` means the browser does not support the feature.
/// `Some(BrowserVersion::Unknown)` means support is known but the first
/// version is not.
/// `Some(BrowserVersion::Version("85"))` means support was added in that
/// version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserSupport {
    /// Chrome desktop support data.
    pub chrome: Option<BrowserVersion>,
    /// Edge desktop support data.
    pub edge: Option<BrowserVersion>,
    /// Firefox desktop support data.
    pub firefox: Option<BrowserVersion>,
    /// Safari desktop support data.
    pub safari: Option<BrowserVersion>,
}

/// SVG element categories for content model grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementCategory {
    /// General grouping/container elements.
    Container,
    /// Shape elements such as `rect` and `circle`.
    Shape,
    /// Text-content elements.
    Text,
    /// Gradient elements.
    Gradient,
    /// Filter container elements.
    Filter,
    /// Descriptive metadata elements.
    Descriptive,
    /// Structural elements such as `svg`, `g`, and `defs`.
    Structural,
    /// Animation elements.
    Animation,
    /// Paint-server elements such as gradients and patterns.
    PaintServer,
    /// Clipping and masking elements.
    ClipMask,
    /// Filter light-source elements.
    LightSource,
    /// Filter primitive elements.
    FilterPrimitive,
    /// Transfer-function elements inside component transfer filters.
    TransferFunction,
    /// `feMergeNode`-style merge children.
    MergeNode,
    /// Motion-path helper elements.
    MotionPath,
    /// Elements that never render directly.
    NeverRendered,
}
