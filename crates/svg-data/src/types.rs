/// Definition of an SVG element.
#[derive(Debug, Clone)]
pub struct ElementDef {
    pub name: &'static str,
    pub description: &'static str,
    pub mdn_url: &'static str,
    pub deprecated: bool,
    pub baseline: Option<BaselineStatus>,
    pub content_model: ContentModel,
    pub required_attrs: &'static [&'static str],
    pub attrs: &'static [&'static str],
    pub global_attrs: bool,
}

/// Whether an element is a container, void, or text-content element.
#[derive(Debug, Clone)]
pub enum ContentModel {
    Children(&'static [ElementCategory]),
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
    pub baseline: Option<BaselineStatus>,
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
}
