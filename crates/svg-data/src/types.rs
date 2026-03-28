/// Definition of an SVG element.
#[derive(Debug, Clone)]
pub struct ElementDef {
    pub name: &'static str,
    pub description: &'static str,
    pub mdn_url: &'static str,
    pub deprecated: bool,
    pub experimental: bool,
    pub spec_url: Option<&'static str>,
    pub baseline: Option<BaselineStatus>,
    pub browser_support: Option<BrowserSupport>,
    pub content_model: ContentModel,
    pub required_attrs: &'static [&'static str],
    pub attrs: &'static [&'static str],
    pub global_attrs: bool,
}

/// Whether an element is a container, void, or text-content element.
#[derive(Debug, Clone)]
pub enum ContentModel {
    Children(&'static [ElementCategory]),
    Foreign,
    Void,
    Text,
}

/// Definition of an SVG attribute.
#[derive(Debug, Clone)]
pub struct AttributeDef {
    pub name: &'static str,
    pub description: &'static str,
    pub mdn_url: &'static str,
    pub deprecated: bool,
    pub experimental: bool,
    pub spec_url: Option<&'static str>,
    pub baseline: Option<BaselineStatus>,
    pub browser_support: Option<BrowserSupport>,
    pub values: AttributeValues,
    pub elements: &'static [&'static str],
}

/// What kind of values an attribute accepts.
#[derive(Debug, Clone)]
pub enum AttributeValues {
    Enum(&'static [&'static str]),
    FreeText,
    Color,
    Length,
    Url,
    NumberOrPercentage,
    Transform(&'static [&'static str]),
    ViewBox,
    PreserveAspectRatio {
        alignments: &'static [&'static str],
        meet_or_slice: &'static [&'static str],
    },
    Points,
    PathData,
}

/// Baseline browser support status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineStatus {
    Widely { since: u16 },
    Newly { since: u16 },
    Limited,
}

/// Per-browser `version_added` for the four major desktop browsers.
///
/// `None` means the browser does not support the feature.
/// `Some("85")` means support was added in that version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserSupport {
    pub chrome: Option<&'static str>,
    pub edge: Option<&'static str>,
    pub firefox: Option<&'static str>,
    pub safari: Option<&'static str>,
}

/// SVG element categories for content model grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementCategory {
    Container,
    Shape,
    Text,
    Gradient,
    Filter,
    Descriptive,
    Structural,
    Animation,
    PaintServer,
    ClipMask,
    LightSource,
    FilterPrimitive,
    TransferFunction,
    MergeNode,
    MotionPath,
    NeverRendered,
}
