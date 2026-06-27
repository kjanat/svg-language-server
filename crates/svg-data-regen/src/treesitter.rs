//! Tree-sitter grammar projection derived from the catalog plus webref CSS.
//!
//! `catalog.tree-sitter.json` is parser projection config, not raw spec truth.
//! Policy is allowed here when the grammar needs a stable, finite set of bucket
//! choices (`css_text` vs `keyword`, dedicated attribute ownership, opaque
//! timing attrs, etc.) that cannot be represented directly in upstream specs.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    catalog::{
        CatalogAttribute, CatalogAttributeValues, CatalogCssGrammarNodeKind, CatalogPackageSource,
        CatalogTreeSitterAttributeBuckets, CatalogTreeSitterDocument, CatalogTreeSitterSources,
        CatalogTreeSitterTokens,
    },
    fetch,
    paths::PathsGrammarFacts,
    util::{boxed, compile_regex, normalize_html_ws},
};

static TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| compile_regex(r"[A-Za-z][A-Za-z0-9-]*"));
static TYPE_REF_RE: LazyLock<Regex> = LazyLock::new(|| compile_regex(r"<([^>]+)>"));
static TAG_RE: LazyLock<Regex> = LazyLock::new(|| compile_regex("(?is)<[^>]+>"));
static METRIC_RE: LazyLock<Regex> = LazyLock::new(|| compile_regex(r"Metric\s*::=\s*([^\n]+)"));
static QUOTED_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| compile_regex(r#""([^"]+)""#));

const WEBREF_CSS_PACKAGE: &str = "@webref/css";
const WEBREF_CSS_DATA_PATH: &str = "css.json";
const SVG_CLOCK_VALUE_SYNTAX_URL: &str = "https://svgwg.org/specs/animations/";

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

const EXPECTED_LENGTH_UNITS: &[&str] = &[
    "em", "ex", "px", "cm", "mm", "in", "pt", "pc", "Q", "rem", "ch", "vh", "vw", "vmin", "vmax",
];
const EXPECTED_ANGLE_UNITS: &[&str] = &["deg", "grad", "rad", "turn"];
const EXPECTED_TIME_UNITS: &[&str] = &["h", "min", "s", "ms"];
const EXPECTED_COLOR_SPACES: &[&str] = &[
    "srgb-linear",
    "srgb",
    "display-p3",
    "a98-rgb",
    "prophoto-rgb",
    "rec2020",
    "xyz-d50",
    "xyz-d65",
    "xyz",
];
const EXPECTED_COLOR_INTERPOLATION_SPACES: &[&str] = &[
    "srgb-linear",
    "srgb",
    "display-p3",
    "a98-rgb",
    "prophoto-rgb",
    "rec2020",
    "xyz-d50",
    "xyz-d65",
    "xyz",
    "hsl",
    "hwb",
    "lab",
    "lch",
    "oklab",
    "oklch",
];
const EXPECTED_HUE_INTERPOLATION_METHODS: &[&str] =
    &["shorter", "longer", "increasing", "decreasing"];
const EXPECTED_PATH_DATA_ATTRIBUTES: &[&str] = &["d", "path"];
const EXPECTED_COLOR_ATTRIBUTES: &[&str] = &[
    "color",
    "fill",
    "flood-color",
    "lighting-color",
    "stop-color",
    "stroke",
];
const EXPECTED_NUMBER_OR_PERCENTAGE_ATTRIBUTES: &[&str] = &[
    "opacity",
    "fill-opacity",
    "stroke-opacity",
    "stop-opacity",
    "flood-opacity",
];
const EXPECTED_FUNCTIONAL_IRI_ATTRIBUTES: &[&str] = &[
    "clip-path",
    "mask",
    "filter",
    "marker-start",
    "marker-mid",
    "marker-end",
    "cursor",
];
const EXPECTED_POINTS_ATTRIBUTES: &[&str] = &["points"];
const EXPECTED_VIEW_BOX_ATTRIBUTES: &[&str] = &["viewBox"];
const EXPECTED_LENGTH_LIST_ATTRIBUTES: &[&str] = &["dx", "dy"];
const OPAQUE_TIMING_ATTRIBUTE_NAMES: &[&str] = &["begin", "end", "max", "min"];

/// CSS primitive types that terminate value-grammar resolution. This is the
/// projection's intentional output alphabet, not a heuristic: every reachable
/// CSS type is collapsed down to one of these primitives, and the downstream
/// tree-sitter grammar only has rules for these shapes. Their syntaxes either
/// have no further CSS type references (`number`, `percentage`, `length`,
/// `color`) or are deliberately treated as opaque terminals so resolution stops
/// (`url`, `length-percentage`) rather than expanding into their sub-grammar.
/// `validate_projection_inputs` asserts upstream `@webref/css` still defines
/// each one, so a rename upstream fails the build loudly instead of silently
/// reclassifying attributes.
const TERMINAL_PRIMITIVE_TYPES: &[&str] = &[
    "url",
    "color",
    "length",
    "length-percentage",
    "number",
    // `<integer>` is a numeric primitive (the grammar's `number` token already
    // accepts integers). Stopping resolution here keeps `<integer>` attributes
    // (`numOctaves`, `targetX`, …) in the `number` bucket instead of expanding
    // into the undefined `<number-token>` and falling to the `css_text`
    // catch-all.
    "integer",
    "percentage",
];

/// Attribute names that `grammars/tree-sitter-svg/grammar.js` handles with
/// dedicated, bespoke parser rules instead of generated bucket lists. This is an
/// intentional grammar/catalog coupling: each name here has a hand-written rule
/// in `grammar.js` (e.g. `transform`, `preserveAspectRatio`, `keySplines`) whose
/// shape cannot be derived from spec value grammars, so the projection must drop
/// these names from the generated buckets to avoid emitting a second, conflicting
/// rule. It is parser-projection config, not spec-derived data; keep it in sync
/// with `grammar.js`.
const GRAMMAR_DEDICATED_ATTRIBUTE_NAMES: &[&str] = &[
    "class",
    "clip",
    "d",
    "dur",
    "enable-background",
    "gradientTransform",
    "href",
    "id",
    "keySplines",
    "keyTimes",
    "offset",
    "path",
    "patternTransform",
    "preserveAspectRatio",
    "repeatCount",
    "repeatDur",
    "rotate",
    "style",
    "transform",
    "xlink:href",
];

/// Inputs needed to project catalog value grammars into parser buckets.
pub struct GrammarProjectionInputs {
    source: CatalogPackageSource,
    css: WebrefCssIndex,
    pub paths: PathsGrammarFacts,
    css_unit_pages: Vec<String>,
    length_units: Vec<String>,
    angle_units: Vec<String>,
    time_units: Vec<String>,
    color_spaces: Vec<String>,
    color_interpolation_spaces: Vec<String>,
    hue_interpolation_methods: Vec<String>,
}

impl GrammarProjectionInputs {
    pub(crate) fn transform_functions(&self) -> Vec<String> {
        self.css.transform_function_names()
    }

    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        let css = WebrefCssIndex::from_features(
            [(
                "types",
                vec![
                    WebrefFeature::new("url", "<url()>"),
                    WebrefFeature::new("image", "<url> | <gradient>"),
                    WebrefFeature::new("mask-reference", "none | <image>"),
                    WebrefFeature::new("mask-layer", "<mask-reference>#"),
                    WebrefFeature::new("length-percentage", "<length> | <percentage>"),
                    WebrefFeature::new(
                        "transform-function",
                        "matrix() | translate() | scale() | rotate() | skewX() | skewY()",
                    ),
                    WebrefFeature::new("transform-list", "<transform-function>#"),
                ],
            )],
            [("opacity", "<number> | <percentage>")],
        );
        Self {
            source: CatalogPackageSource {
                name: WEBREF_CSS_PACKAGE.to_owned(),
                version: "test".to_owned(),
                url: "test://webref/css.json".to_owned(),
            },
            css,
            paths: PathsGrammarFacts {
                url: "test://paths.html".to_owned(),
                path_data_property: "d".to_owned(),
                path_command_letters: [
                    "A", "C", "H", "L", "M", "Q", "S", "T", "V", "Z", "a", "c", "h", "l", "m", "q",
                    "s", "t", "v", "z",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            },
            css_unit_pages: Vec::new(),
            length_units: std::iter::once("px").map(str::to_owned).collect(),
            angle_units: std::iter::once("deg").map(str::to_owned).collect(),
            time_units: std::iter::once("s").map(str::to_owned).collect(),
            color_spaces: std::iter::once("srgb").map(str::to_owned).collect(),
            color_interpolation_spaces: ["srgb", "hsl"].into_iter().map(str::to_owned).collect(),
            hue_interpolation_methods: std::iter::once("shorter").map(str::to_owned).collect(),
        }
    }
}

/// Fetch all external inputs used by the grammar projection.
///
/// # Errors
/// Returns an error if a package or linked spec page cannot be fetched or
/// parsed.
pub fn fetch_grammar_projection_inputs(
    repo_slug: &str,
    commit_sha: &str,
) -> Fallible<GrammarProjectionInputs> {
    let paths = crate::paths::fetch_paths_grammar_facts(repo_slug, commit_sha)?;
    let source = package_source(WEBREF_CSS_PACKAGE, WEBREF_CSS_DATA_PATH)?;
    let css_json: WebrefCssJson =
        serde_json::from_str(&fetch::url_text(&source.url, "application/json")?)?;
    let css = WebrefCssIndex::from_json(&css_json);

    let (length_units, length_page) = css_units(&css, "length")?;
    let (angle_units, angle_page) = css_units(&css, "angle")?;
    // SVG clock values (used for the `time_units` token set) carry the metric
    // units SVG animation timing accepts; CSS `<time>` units are a stricter
    // subset (`s`, `ms`). Fetch the CSS set both for its provenance page URL and
    // to cross-check the two sources still agree.
    let (css_time_units, time_page) = css_units(&css, "time")?;
    let time_units = svg_clock_units()?;
    assert_subset(
        "CSS <time> units",
        &css_time_units,
        "SVG clock units",
        &time_units,
    )?;

    let color_spaces = css
        .color_space_keywords("colorspace-params")
        .into_iter()
        .collect();
    let color_interpolation_spaces = css
        .color_space_keywords("color-space")
        .into_iter()
        .collect();
    let hue_interpolation_methods = css
        .keywords_before_literal_suffix("hue-interpolation-method", "hue")
        .into_iter()
        .collect();

    let css_unit_pages = unique_sorted([length_page, angle_page, time_page]);

    let inputs = GrammarProjectionInputs {
        source,
        css,
        paths,
        css_unit_pages,
        length_units,
        angle_units,
        time_units,
        color_spaces,
        color_interpolation_spaces,
        hue_interpolation_methods,
    };
    validate_projection_inputs(&inputs)?;
    Ok(inputs)
}

/// Build the tree-sitter grammar projection document.
///
/// `validate` runs the full-catalog bucket assertions
/// ([`validate_attribute_buckets`]); the real binary passes `true`. Callers
/// exercising partial attribute lists (unit tests) pass `false` so the
/// completeness asserts do not spuriously fail on a deliberately small input.
pub fn build_tree_sitter_document(
    attributes: &[CatalogAttribute],
    inputs: &GrammarProjectionInputs,
    validate: bool,
) -> Fallible<CatalogTreeSitterDocument> {
    let mut buckets = CatalogTreeSitterAttributeBuckets::default();
    for attribute in attributes {
        match attribute_bucket(attribute, &inputs.css, &inputs.paths) {
            Some(AttributeBucket::Keyword) => buckets.keyword.push(attribute.name.clone()),
            Some(AttributeBucket::Color) => buckets.color.push(attribute.name.clone()),
            Some(AttributeBucket::Length) => buckets.length.push(attribute.name.clone()),
            Some(AttributeBucket::LengthList) => buckets.length_list.push(attribute.name.clone()),
            Some(AttributeBucket::LengthListOrNone) => {
                buckets.length_list_or_none.push(attribute.name.clone());
            }
            Some(AttributeBucket::Number) => buckets.number.push(attribute.name.clone()),
            Some(AttributeBucket::NumberOptionalNumber) => {
                buckets.number_optional_number.push(attribute.name.clone());
            }
            Some(AttributeBucket::NumberList) => buckets.number_list.push(attribute.name.clone()),
            Some(AttributeBucket::NumberOrPercentage) => {
                buckets.number_or_percentage.push(attribute.name.clone());
            }
            Some(AttributeBucket::CoordinatePairList) => {
                buckets.coordinate_pair_list.push(attribute.name.clone());
            }
            Some(AttributeBucket::PathData) => buckets.path_data.push(attribute.name.clone()),
            Some(AttributeBucket::ViewBox) => buckets.view_box.push(attribute.name.clone()),
            Some(AttributeBucket::FunctionalIri) => {
                buckets.functional_iri.push(attribute.name.clone());
            }
            Some(AttributeBucket::CssText) => buckets.css_text.push(attribute.name.clone()),
            None => {}
        }
    }
    buckets.sort();
    if validate {
        validate_attribute_buckets(&buckets)?;
    }
    remove_grammar_dedicated_attributes(&mut buckets);

    Ok(CatalogTreeSitterDocument {
        schema_version: crate::schema::CATALOG_SCHEMA_VERSION,
        sources: CatalogTreeSitterSources {
            webref_css: inputs.source.clone(),
            css_unit_pages: inputs.css_unit_pages.clone(),
            svg_clock_value_syntax: SVG_CLOCK_VALUE_SYNTAX_URL.to_owned(),
            paths_html: inputs.paths.url.clone(),
        },
        attribute_buckets: buckets,
        tokens: CatalogTreeSitterTokens {
            length_units: inputs.length_units.clone(),
            angle_units: inputs.angle_units.clone(),
            time_units: inputs.time_units.clone(),
            color_spaces: inputs.color_spaces.clone(),
            color_interpolation_spaces: inputs.color_interpolation_spaces.clone(),
            hue_interpolation_methods: inputs.hue_interpolation_methods.clone(),
            path_command_letters: inputs.paths.path_command_letters.clone(),
        },
    })
}

impl CatalogTreeSitterAttributeBuckets {
    fn sort(&mut self) {
        self.keyword.sort();
        self.color.sort();
        self.length.sort();
        self.length_list.sort();
        self.length_list_or_none.sort();
        self.number.sort();
        self.number_optional_number.sort();
        self.number_list.sort();
        self.number_or_percentage.sort();
        self.coordinate_pair_list.sort();
        self.path_data.sort();
        self.view_box.sort();
        self.functional_iri.sort();
        self.css_text.sort();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttributeBucket {
    Keyword,
    Color,
    Length,
    LengthList,
    LengthListOrNone,
    Number,
    NumberOptionalNumber,
    NumberList,
    NumberOrPercentage,
    CoordinatePairList,
    PathData,
    ViewBox,
    FunctionalIri,
    CssText,
}

fn attribute_bucket(
    attribute: &CatalogAttribute,
    css: &WebrefCssIndex,
    paths: &PathsGrammarFacts,
) -> Option<AttributeBucket> {
    if OPAQUE_TIMING_ATTRIBUTE_NAMES.contains(&attribute.name.as_str()) {
        return Some(AttributeBucket::CssText);
    }

    let canonical = attribute_values_bucket(attribute, &attribute.values, css, paths);
    let canonical_bucket = canonical?;
    let mut buckets = vec![canonical_bucket];
    for element_values in &attribute.element_values {
        buckets.push(attribute_values_bucket(
            attribute,
            &element_values.values,
            css,
            paths,
        )?);
    }
    buckets
        .iter()
        .copied()
        .find(|candidate| {
            buckets
                .iter()
                .all(|bucket| bucket_accepts(*candidate, *bucket))
        })
        .or(Some(AttributeBucket::CssText))
}

fn bucket_accepts(superset: AttributeBucket, subset: AttributeBucket) -> bool {
    superset == subset
        || matches!(
            (superset, subset),
            (AttributeBucket::CssText, _)
                | (
                    AttributeBucket::LengthList,
                    AttributeBucket::Length
                        | AttributeBucket::Number
                        | AttributeBucket::NumberOptionalNumber
                        | AttributeBucket::NumberOrPercentage,
                )
                | (
                    AttributeBucket::Length | AttributeBucket::NumberOrPercentage,
                    AttributeBucket::Number,
                )
                | (AttributeBucket::Color, AttributeBucket::Keyword)
                | (
                    AttributeBucket::NumberList,
                    AttributeBucket::CoordinatePairList
                )
        )
}

fn attribute_values_bucket(
    attribute: &CatalogAttribute,
    values: &CatalogAttributeValues,
    css: &WebrefCssIndex,
    paths: &PathsGrammarFacts,
) -> Option<AttributeBucket> {
    match values {
        CatalogAttributeValues::Enum { .. } => Some(AttributeBucket::Keyword),
        CatalogAttributeValues::Color => Some(AttributeBucket::Color),
        CatalogAttributeValues::Length => Some(AttributeBucket::Length),
        CatalogAttributeValues::NumberOrPercentage => Some(AttributeBucket::NumberOrPercentage),
        CatalogAttributeValues::Url => Some(AttributeBucket::FunctionalIri),
        CatalogAttributeValues::Integer => Some(AttributeBucket::Number),
        CatalogAttributeValues::PathData => Some(AttributeBucket::PathData),
        CatalogAttributeValues::SemicolonNumberList => Some(AttributeBucket::NumberList),
        CatalogAttributeValues::CoordinatePair | CatalogAttributeValues::CoordinatePairList => {
            Some(AttributeBucket::CoordinatePairList)
        }
        CatalogAttributeValues::Boolean
        | CatalogAttributeValues::TokenList
        | CatalogAttributeValues::CommaTokenList
        | CatalogAttributeValues::UrlTokenList
        | CatalogAttributeValues::LanguageTag
        | CatalogAttributeValues::MediaType
        | CatalogAttributeValues::MediaQueryList
        | CatalogAttributeValues::CssDeclarationList
        | CatalogAttributeValues::Id
        | CatalogAttributeValues::ReferrerPolicy
        | CatalogAttributeValues::SuggestedFileName => Some(AttributeBucket::CssText),
        CatalogAttributeValues::CssGrammar { graph, .. } => {
            let analysis = GrammarAnalysis::from_graph(graph);
            if analysis.is_path_data(attribute, paths) {
                return Some(AttributeBucket::PathData);
            }
            if analysis.is_view_box() {
                return Some(AttributeBucket::ViewBox);
            }
            if analysis.is_coordinate_pair_list() {
                return Some(AttributeBucket::CoordinatePairList);
            }
            if analysis
                .types
                .iter()
                .any(|name| css.resolves_to_type(name, "url", &mut BTreeSet::new()))
            {
                return Some(AttributeBucket::FunctionalIri);
            }
            if analysis.is_number_optional_number(css) {
                return Some(AttributeBucket::NumberOptionalNumber);
            }
            if analysis.is_number_list(css) {
                return Some(AttributeBucket::NumberList);
            }
            if analysis.is_number_or_percentage(css) {
                return Some(AttributeBucket::NumberOrPercentage);
            }
            if analysis.is_number(css) {
                // The spec's prose number-list grammars (`list of <number>` for
                // `feColorMatrix/values` and `feFuncR/tableValues`, `<list of
                // numbers>` for `feConvolveMatrix/kernelMatrix`) are canonicalized
                // to a real `<number>+` production during scraping
                // (`number_list_prose_production` in `chapter::…`), so they reach
                // `is_number_list` above and never fall through to this scalar
                // bucket. Genuine single-number attributes (`bias`, `divisor`, …)
                // land here.
                return Some(AttributeBucket::Number);
            }
            if analysis.is_length_number_list() {
                return Some(AttributeBucket::LengthList);
            }
            if analysis.is_length_like(css) {
                if analysis.is_list_like(css) && analysis.keywords.contains("none") {
                    Some(AttributeBucket::LengthListOrNone)
                } else if analysis.is_list_like(css) {
                    Some(AttributeBucket::LengthList)
                } else {
                    Some(AttributeBucket::Length)
                }
            } else if analysis.resolves_to_keywords_only(css) {
                // A pure-enum attribute expressed as a CSS grammar (e.g. `mode`
                // resolving through `<blend-mode>`): keep it as keyword tokens
                // rather than losing the structure to the css_text catch-all.
                // Mirrors the keyword fallback in `attribute_bucket_for_syntax`.
                Some(AttributeBucket::Keyword)
            } else {
                Some(AttributeBucket::CssText)
            }
        }
        CatalogAttributeValues::FreeText => attribute
            .presentation_attribute
            .as_deref()
            .and_then(|property| attribute_bucket_for_syntax(property, css)),
        CatalogAttributeValues::Transform { .. } => None,
    }
}

fn attribute_bucket_for_syntax(name: &str, css: &WebrefCssIndex) -> Option<AttributeBucket> {
    let analysis = GrammarAnalysis::from_syntax(css.syntax(name)?);
    if analysis
        .types
        .iter()
        .any(|name| css.resolves_to_type(name, "url", &mut BTreeSet::new()))
    {
        return Some(AttributeBucket::FunctionalIri);
    }
    if analysis.is_colorish(css) {
        return Some(AttributeBucket::Color);
    }
    if analysis.is_number_or_percentage(css) {
        return Some(AttributeBucket::NumberOrPercentage);
    }
    if analysis.is_number_list(css) {
        return Some(AttributeBucket::NumberList);
    }
    if analysis.is_number(css) {
        return Some(AttributeBucket::Number);
    }
    if analysis.is_length_like(css) {
        if analysis.is_list_like(css) && analysis.keywords.contains("none") {
            Some(AttributeBucket::LengthListOrNone)
        } else if analysis.is_list_like(css) {
            Some(AttributeBucket::LengthList)
        } else {
            Some(AttributeBucket::Length)
        }
    } else if !analysis.keywords.is_empty() {
        Some(AttributeBucket::Keyword)
    } else {
        None
    }
}

#[derive(Default, Debug)]
struct GrammarAnalysis {
    types: BTreeSet<String>,
    keywords: BTreeSet<String>,
    operators: BTreeSet<String>,
    is_list_like: bool,
}

impl GrammarAnalysis {
    fn from_syntax(syntax: &str) -> Self {
        let mut analysis = Self {
            types: syntax_type_refs(syntax),
            keywords: syntax_keywords(syntax),
            operators: syntax_operators(syntax),
            is_list_like: syntax.contains('#') || syntax.contains('+') || syntax.contains('*'),
        };
        analysis
            .keywords
            .retain(|keyword| !analysis.types.contains(keyword));
        analysis
    }

    fn from_graph(graph: &crate::catalog::CatalogCssGrammarGraph) -> Self {
        let mut analysis = Self::default();
        for node in &graph.nodes {
            let Some(text) = node.text.as_deref() else {
                continue;
            };
            match node.kind {
                CatalogCssGrammarNodeKind::Type => {
                    if let Some(name) = css_type_name(text) {
                        analysis.is_list_like |= type_token_is_list_like(text);
                        analysis.types.insert(name);
                    }
                }
                CatalogCssGrammarNodeKind::Keyword => {
                    analysis.keywords.insert(text.to_ascii_lowercase());
                }
                CatalogCssGrammarNodeKind::Operator => {
                    analysis.is_list_like |= matches!(text, "#" | "+" | "*");
                    analysis.operators.insert(text.to_owned());
                }
                CatalogCssGrammarNodeKind::Function
                | CatalogCssGrammarNodeKind::Group
                | CatalogCssGrammarNodeKind::Root => {}
            }
        }
        analysis
    }

    fn is_number(&self, css: &WebrefCssIndex) -> bool {
        !self.types.is_empty()
            && self.keywords.is_empty()
            && !self.is_list_like(css)
            && self
                .types
                .iter()
                .all(|name| css.resolves_to_number(name, &mut BTreeSet::new()))
    }

    /// Whether the grammar is the CSS `<number-optional-number>` shape: a single
    /// type reference to a webref type whose syntax accepts one or two numbers.
    /// Anchored to the webref type named `number-optional-number` resolving
    /// (`<number> <number>?`), not to a hardcoded SVG attribute-name list.
    fn is_number_optional_number(&self, css: &WebrefCssIndex) -> bool {
        self.keywords.is_empty()
            && self.types.len() == 1
            && self
                .types
                .iter()
                .all(|name| css.is_number_optional_number_type(name))
    }

    /// Whether the grammar is a pure keyword enumeration. True when the grammar
    /// has no operators that would carry a non-keyword payload and every type it
    /// references resolves, through webref, to a keyword-only production (e.g.
    /// `mode` referencing `<blend-mode>`). A grammar with no type references but a
    /// genuine bare enum shape (`a | b | c`, or a single keyword) also qualifies.
    /// Numeric or otherwise value-bearing types (`<integer>`, `<xml-name>`, …)
    /// disqualify it, so those stay in the `css_text` catch-all rather than
    /// losing their value shape to the keyword bucket.
    fn resolves_to_keywords_only(&self, css: &WebrefCssIndex) -> bool {
        if self.types.is_empty() {
            return self.is_bare_keyword_enum();
        }
        self.types
            .iter()
            .all(|name| css.is_keyword_only_type(name, &mut BTreeSet::new()))
    }

    /// Whether a type-free grammar is a genuine bare keyword enumeration rather
    /// than scraped spec prose. A real enum is alternatives joined by `|`/`||`
    /// (e.g. `anonymous | use-credentials`), or a single keyword. Spec
    /// placeholders and cross-references (`(see below)`, `Language-Tag [ABNF]`,
    /// `space-separated valid non-empty URL tokens [HTML]`) arrive as several
    /// juxtaposed keyword tokens with no alternation operator; treating those as
    /// enums would route values like `text/javascript` through the keyword rule
    /// and break parsing, so they fall through to the `css_text` catch-all.
    fn is_bare_keyword_enum(&self) -> bool {
        match self.keywords.len() {
            0 => false,
            1 => true,
            _ => self
                .operators
                .iter()
                .any(|operator| operator == "|" || operator == "||"),
        }
    }

    fn is_number_list(&self, css: &WebrefCssIndex) -> bool {
        !self.types.is_empty()
            && self.keywords.is_empty()
            && self.is_list_like(css)
            && self
                .types
                .iter()
                .all(|name| css.resolves_to_number(name, &mut BTreeSet::new()))
    }

    fn is_number_or_percentage(&self, css: &WebrefCssIndex) -> bool {
        !self.types.is_empty()
            && self
                .types
                .iter()
                .all(|name| css.resolves_to_number_or_percentage(name, &mut BTreeSet::new()))
    }

    fn is_length_like(&self, css: &WebrefCssIndex) -> bool {
        !self.types.is_empty()
            && self.types.iter().all(|name| {
                css.resolves_to_length(name, &mut BTreeSet::new())
                    || css.resolves_to_number(name, &mut BTreeSet::new())
                    || css.resolves_to_number_or_percentage(name, &mut BTreeSet::new())
            })
    }

    fn is_length_number_list(&self) -> bool {
        self.is_list_like
            && self.types == BTreeSet::from(["length-percentage".to_owned(), "number".to_owned()])
    }

    fn is_color(&self, css: &WebrefCssIndex) -> bool {
        !self.types.is_empty()
            && self
                .types
                .iter()
                .all(|name| css.resolves_to_type(name, "color", &mut BTreeSet::new()))
    }

    fn is_colorish(&self, css: &WebrefCssIndex) -> bool {
        self.is_color(css)
            || (!self.types.is_empty()
                && self
                    .types
                    .iter()
                    .all(|name| css.resolves_to_color_type(name, &mut BTreeSet::new())))
    }

    /// A `points` list (polyline/polygon), matched on the SVG value grammar's
    /// own `<points>` type token. `points` is an SVG-defined value type with no
    /// upstream CSS production to enumerate, so unlike the colour-space tokens
    /// this is anchored to the parsed attribute grammar, not a derivable seed.
    fn is_coordinate_pair_list(&self) -> bool {
        self.types.contains("points")
    }

    /// A `viewBox`: the SVG value grammar's four positional coordinate-system
    /// slots `<min-x> <min-y> <width> <height>`. These are SVG-local value types
    /// (no upstream CSS production), so the shape is matched against the parsed
    /// attribute grammar directly — grammar-anchored, not a derivable seed.
    fn is_view_box(&self) -> bool {
        self.types
            == BTreeSet::from([
                "min-x".to_owned(),
                "min-y".to_owned(),
                "width".to_owned(),
                "height".to_owned(),
            ])
    }

    fn is_path_data(&self, attribute: &CatalogAttribute, paths: &PathsGrammarFacts) -> bool {
        if attribute.name == paths.path_data_property {
            return true;
        }
        path_data_keywords(self)
    }

    fn is_list_like(&self, css: &WebrefCssIndex) -> bool {
        self.is_list_like
            || self
                .types
                .iter()
                .any(|name| css.is_list_like_type(name, &mut BTreeSet::new()))
    }
}

fn type_token_is_list_like(token: &str) -> bool {
    token
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .contains('#')
}

fn css_type_name(token: &str) -> Option<String> {
    let token = token.trim().trim_start_matches('<').trim_end_matches('>');
    let token = token
        .trim_matches('\'')
        .split_once('[')
        .map_or(token, |(name, _range)| name)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches(['?', '#', '+', '*'])
        .trim_matches('\'')
        .to_ascii_lowercase();
    (!token.is_empty()).then_some(token)
}

#[derive(Default)]
struct WebrefCssIndex {
    types: BTreeMap<String, WebrefFeature>,
    properties: BTreeMap<String, WebrefFeature>,
}

impl WebrefCssIndex {
    fn from_json(json: &WebrefCssJson) -> Self {
        Self {
            types: index_features(&json.types),
            properties: index_features(&json.properties),
        }
    }

    #[cfg(test)]
    fn from_features(
        categories: impl IntoIterator<Item = (&'static str, Vec<WebrefFeature>)>,
        properties: impl IntoIterator<Item = (&'static str, &'static str)>,
    ) -> Self {
        let mut index = Self::default();
        for (category, features) in categories {
            if category == "types" {
                index.types = index_features(&features);
            }
        }
        index.properties = properties
            .into_iter()
            .map(|(name, value)| (name.to_owned(), WebrefFeature::new(name, value)))
            .collect();
        index
    }

    /// Whether `@webref/css` defines a type entry for `name`. Terminal
    /// primitives (`number`, `length`, …) have an entry with no expansion, so
    /// presence is checked against the type map rather than via `syntax`.
    fn defines_type(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }

    fn syntax(&self, name: &str) -> Option<&str> {
        self.types
            .get(name)
            .or_else(|| self.properties.get(name))
            .and_then(WebrefFeature::syntax)
    }

    fn href(&self, name: &str) -> Option<&str> {
        self.types
            .get(name)
            .and_then(|feature| feature.href.as_deref())
    }

    /// Colour-space keyword identifiers enumerated by `production`, derived from
    /// webref's own grammar rather than a hardcoded type-name seed. Each
    /// alternative is either a colour-space category listed directly (as in
    /// `color-space = <rectangular-color-space> | <polar-color-space> | …`) or a
    /// `<…-params>` wrapper that prefixes channel values (as in `colorspace-params`
    /// for the `color()` function); in the latter case the colour space is the
    /// wrapper's leading type reference, so channel keywords like `none` are not
    /// collected. `bare_keywords` excludes type-reference names, so a category
    /// name such as `xyz-space` never leaks in as a value. A category added
    /// upstream is discovered automatically.
    fn color_space_keywords(&self, production: &str) -> BTreeSet<String> {
        let Some(syntax) = self.syntax(production) else {
            return BTreeSet::new();
        };
        let mut keywords = BTreeSet::new();
        for reference in ordered_type_refs(syntax) {
            let space = if reference.ends_with("-params") {
                self.syntax(&reference)
                    .and_then(|inner| ordered_type_refs(inner).into_iter().next())
            } else {
                Some(reference)
            };
            if let Some(space) = space {
                keywords.extend(self.bare_keywords(&space, &mut BTreeSet::new()));
            }
        }
        keywords
    }

    fn resolves_to_type(&self, name: &str, target: &str, seen: &mut BTreeSet<String>) -> bool {
        self.terminal_types(name, seen).contains(target)
    }

    /// Whether `name` participates in CSS color syntax. A type is "colorish" if
    /// it resolves to the `<color>` terminal, or if it is reachable within the
    /// transitive type-reference expansion of webref's own `<color>` production
    /// (its branches `<color-base>`, `<system-color>`, `<device-cmyk()>`,
    /// `<light-dark-color>`, `<contrast-color()>`, etc.). Derived from the
    /// upstream `<color>` graph, not a hardcoded type-name list.
    fn resolves_to_color_type(&self, name: &str, seen: &mut BTreeSet<String>) -> bool {
        if self.resolves_to_type(name, "color", seen) {
            return true;
        }
        let normalized = name.trim_end_matches("()");
        self.color_graph_type_names()
            .iter()
            .any(|reachable| reachable.trim_end_matches("()") == normalized)
    }

    /// Type names reachable from webref's `<color>` production, including the
    /// `color` root itself so direct uses classify as colorish.
    fn color_graph_type_names(&self) -> BTreeSet<String> {
        let mut names = self.reachable_type_names("color", &mut BTreeSet::new());
        names.insert("color".to_owned());
        names
    }

    fn keywords_before_literal_suffix(&self, type_name: &str, suffix: &str) -> BTreeSet<String> {
        let Some(syntax) = self.syntax(type_name) else {
            return BTreeSet::new();
        };
        let Some(suffix_index) = syntax.rfind(suffix) else {
            return BTreeSet::new();
        };
        let head = syntax[..suffix_index].trim();
        let group = head
            .strip_prefix('[')
            .and_then(|inner| inner.strip_suffix(']'))
            .unwrap_or(head);
        syntax_keywords(group)
    }

    fn transform_function_names(&self) -> Vec<String> {
        let mut names = self.function_names_preserve(
            "transform-function",
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
        );
        if names.is_empty() {
            names = self.function_names_preserve(
                "transform-list",
                &mut BTreeSet::new(),
                &mut BTreeSet::new(),
            );
        }
        names
    }

    fn function_names_preserve(
        &self,
        name: &str,
        seen_types: &mut BTreeSet<String>,
        seen_names: &mut BTreeSet<String>,
    ) -> Vec<String> {
        if !seen_types.insert(name.to_owned()) {
            return Vec::new();
        }
        let Some(syntax) = self.syntax(name) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        for candidate in function_names_in_syntax(syntax) {
            if seen_names.insert(candidate.clone()) {
                names.push(candidate);
            }
        }
        for reference in ordered_type_refs(syntax) {
            names.extend(self.function_names_preserve(&reference, seen_types, seen_names));
        }
        names
    }

    fn resolves_to_length(&self, name: &str, seen: &mut BTreeSet<String>) -> bool {
        let terminals = self.terminal_types(name, seen);
        terminals.contains("length") || terminals.contains("length-percentage")
    }

    fn resolves_to_number(&self, name: &str, seen: &mut BTreeSet<String>) -> bool {
        let terminals = self.terminal_types(name, seen);
        // `<number>` and `<integer>` both project onto the grammar's single
        // `number` value rule, so either terminal alone is a number bucket.
        terminals.len() == 1 && (terminals.contains("number") || terminals.contains("integer"))
    }

    fn resolves_to_number_or_percentage(&self, name: &str, seen: &mut BTreeSet<String>) -> bool {
        let terminals = self.terminal_types(name, seen);
        !terminals.is_empty()
            && terminals
                .iter()
                .all(|terminal| matches!(terminal.as_str(), "number" | "percentage"))
            && terminals.contains("percentage")
    }

    fn terminal_types(&self, name: &str, seen: &mut BTreeSet<String>) -> BTreeSet<String> {
        if TERMINAL_PRIMITIVE_TYPES.contains(&name) {
            return std::iter::once(name.to_owned()).collect();
        }
        if !seen.insert(name.to_owned()) {
            return BTreeSet::new();
        }
        let Some(syntax) = self.syntax(name) else {
            return BTreeSet::new();
        };
        syntax_type_refs(syntax)
            .into_iter()
            .flat_map(|referenced| self.terminal_types(&referenced, seen))
            .collect()
    }

    /// Whether `name` is the webref `number-optional-number` type, confirmed by
    /// the upstream definition resolving (its syntax is `<number> <number>?`).
    /// The build fails loudly in `validate_projection_inputs` if upstream ever
    /// drops or renames this type, so the name check stays anchored to live data.
    fn is_number_optional_number_type(&self, name: &str) -> bool {
        name == "number-optional-number" && self.syntax(name).is_some()
    }

    /// Whether `name` resolves to a pure keyword enumeration: it is defined, is
    /// not a terminal primitive, every type it references is itself keyword-only,
    /// and its expansion contributes at least one bare keyword. Numeric and
    /// otherwise value-bearing types (`<integer>`, whose syntax references the
    /// undefined `<number-token>`; `<xml-name>`, undefined) are rejected so they
    /// are not misclassified as keyword enumerations.
    fn is_keyword_only_type(&self, name: &str, seen: &mut BTreeSet<String>) -> bool {
        if TERMINAL_PRIMITIVE_TYPES.contains(&name) {
            return false;
        }
        if !seen.insert(name.to_owned()) {
            // A cycle contributes no new disqualifying type; treat as neutral.
            return true;
        }
        let Some(syntax) = self.syntax(name) else {
            return false;
        };
        let referenced = syntax_type_refs(syntax);
        if !referenced
            .iter()
            .all(|child| self.is_keyword_only_type(child, seen))
        {
            return false;
        }
        !self.bare_keywords(name, &mut BTreeSet::new()).is_empty()
    }

    /// Keyword tokens reachable from `name`, excluding any token that appears
    /// inside a `<type-reference>` (those are type names, not keyword values).
    fn bare_keywords(&self, name: &str, seen: &mut BTreeSet<String>) -> BTreeSet<String> {
        if !seen.insert(name.to_owned()) {
            return BTreeSet::new();
        }
        let Some(syntax) = self.syntax(name) else {
            return BTreeSet::new();
        };
        let stripped = TYPE_REF_RE.replace_all(syntax, " ");
        let mut keywords = syntax_keywords(&stripped);
        for referenced in syntax_type_refs(syntax) {
            keywords.extend(self.bare_keywords(&referenced, seen));
        }
        keywords
    }

    /// Transitive set of type names reachable from `name` through its syntax's
    /// type references (raw graph expansion, not stopped at terminals).
    fn reachable_type_names(&self, name: &str, seen: &mut BTreeSet<String>) -> BTreeSet<String> {
        if !seen.insert(name.to_owned()) {
            return BTreeSet::new();
        }
        let Some(syntax) = self.syntax(name) else {
            return BTreeSet::new();
        };
        let mut reachable = BTreeSet::new();
        for referenced in syntax_type_refs(syntax) {
            reachable.insert(referenced.clone());
            reachable.extend(self.reachable_type_names(&referenced, seen));
        }
        reachable
    }

    fn is_list_like_type(&self, name: &str, seen: &mut BTreeSet<String>) -> bool {
        if !seen.insert(name.to_owned()) {
            return false;
        }
        let Some(syntax) = self.syntax(name) else {
            return false;
        };
        syntax.contains('#')
            || syntax.contains('+')
            || syntax_type_refs(syntax)
                .into_iter()
                .any(|referenced| self.is_list_like_type(&referenced, seen))
    }
}

fn index_features(features: &[WebrefFeature]) -> BTreeMap<String, WebrefFeature> {
    let mut indexed = BTreeMap::new();
    for (index, feature) in features.iter().enumerate() {
        let duplicate = feature.for_.is_some()
            && features
                .iter()
                .enumerate()
                .any(|(other_index, other)| other_index != index && other.name == feature.name);
        let id = if duplicate {
            format!(
                "{} for {}",
                feature.name,
                feature
                    .for_
                    .as_ref()
                    .and_then(|scopes| scopes.first())
                    .map_or("", String::as_str)
            )
        } else {
            feature.name.clone()
        };
        indexed.insert(id, feature.clone());
    }
    indexed
}

#[derive(Debug, Deserialize)]
struct WebrefCssJson {
    #[serde(default)]
    types: Vec<WebrefFeature>,
    #[serde(default)]
    properties: Vec<WebrefFeature>,
}

#[derive(Clone, Debug, Deserialize)]
struct WebrefFeature {
    name: String,
    #[serde(default, rename = "for")]
    for_: Option<Vec<String>>,
    #[serde(default)]
    href: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    syntax: Option<String>,
}

impl WebrefFeature {
    #[cfg(test)]
    fn new(name: &str, value: &str) -> Self {
        Self {
            name: name.to_owned(),
            for_: None,
            href: None,
            value: Some(value.to_owned()),
            syntax: None,
        }
    }

    fn syntax(&self) -> Option<&str> {
        self.value.as_deref().or(self.syntax.as_deref())
    }
}

/// Whether the parsed grammar facts describe SVG path data. Anchored to the
/// grammar graph rather than the attribute name: the only path-data value space
/// outside the canonical `d` property (handled via `paths.path_data_property`)
/// presents in the catalog as the literal keyword set `{path, data}` (the
/// `<path-data>` production), with no other keywords or types. The primary,
/// data-driven signal is the path property name; this keyword shape is the
/// secondary anchor for aliases that reuse the same grammar.
fn path_data_keywords(analysis: &GrammarAnalysis) -> bool {
    analysis.types.is_empty()
        && analysis.keywords.len() == 2
        && analysis.keywords.contains("data")
        && analysis.keywords.contains("path")
}

fn syntax_keywords(syntax: &str) -> BTreeSet<String> {
    TOKEN_RE
        .find_iter(syntax)
        .map(|token| token.as_str())
        .map(str::to_owned)
        .collect()
}

fn syntax_operators(syntax: &str) -> BTreeSet<String> {
    syntax
        .chars()
        .filter(|ch| matches!(ch, '|' | ',' | '?' | '*' | '+' | '#' | '!'))
        .map(|ch| ch.to_string())
        .collect()
}

fn function_names_in_syntax(syntax: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, ch) in syntax.char_indices() {
        if ch != '(' {
            continue;
        }
        let mut start = index;
        while let Some((previous_index, previous)) = syntax[..start].char_indices().next_back() {
            if previous.is_ascii_alphanumeric() || previous == '-' {
                start = previous_index;
            } else {
                break;
            }
        }
        if start == index {
            continue;
        }
        let candidate = &syntax[start..index];
        if candidate
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic())
            && seen.insert(candidate.to_owned())
        {
            names.push(candidate.to_owned());
        }
    }
    names
}

fn syntax_type_refs(syntax: &str) -> BTreeSet<String> {
    TYPE_REF_RE
        .captures_iter(syntax)
        .filter_map(|capture| capture.get(1))
        .filter_map(|matched| css_type_name(matched.as_str()))
        .collect()
}

/// Type references in `syntax` in order of appearance, unlike `syntax_type_refs`
/// which dedupes into a sorted set. Used where the *first* reference matters
/// (e.g. the leading colour space of a `<…-params>` production).
fn ordered_type_refs(syntax: &str) -> Vec<String> {
    TYPE_REF_RE
        .captures_iter(syntax)
        .filter_map(|capture| capture.get(1))
        .filter_map(|matched| css_type_name(matched.as_str()))
        .collect()
}

fn package_source(package: &str, path: &str) -> Fallible<CatalogPackageSource> {
    let version = npm_latest_version(package)?;
    let url = format!("https://unpkg.com/{package}@{version}/{path}");
    Ok(CatalogPackageSource {
        name: package.to_owned(),
        version,
        url,
    })
}

/// Resolve the npm `latest` dist-tag for `package`.
///
/// NOTE: this makes regeneration output depend on whatever `@webref/css` is
/// "latest" at run time, so two runs on different days can differ. The crate has
/// no version-pinning convention (`compat.rs` resolves bcd/web-features the same
/// way); the resolved version is recorded into the catalog's package provenance
/// so a given committed catalog stays reproducible from its own metadata.
/// TODO: pin `@webref/css` (and the other npm sources) to an explicit version if
/// fully deterministic regeneration from a clean checkout is required.
fn npm_latest_version(package: &str) -> Fallible<String> {
    let registry_package = package.replace('/', "%2f");
    let url = format!("https://registry.npmjs.org/{registry_package}");
    let json: Value = serde_json::from_str(&fetch::url_text(&url, "application/json")?)?;
    let version = json
        .pointer("/dist-tags/latest")
        .and_then(Value::as_str)
        .ok_or_else(|| boxed("npm package metadata missing dist-tags.latest"))?;
    Ok(version.to_owned())
}

fn css_units(css: &WebrefCssIndex, type_name: &str) -> Fallible<(Vec<String>, String)> {
    let href = css
        .href(type_name)
        .ok_or_else(|| boxed(format!("@webref/css missing href for type `{type_name}`")))?;
    let page = page_url(href);
    let html = fetch::url_text(&page, "text/html")?;
    let units = dfn_values_for(&html, &format!("<{type_name}>"));
    if units.is_empty() {
        return Err(boxed(format!(
            "CSS unit extraction found no `{type_name}` units in {page}"
        )));
    }
    Ok((units, page))
}

fn dfn_values_for(html: &str, dfn_for: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = html[offset..].find("<dfn") {
        let start = offset + relative_start;
        let Some(open_end) = tag_open_end(html, start) else {
            break;
        };
        let tag = &html[start..open_end];
        let Some(relative_close) = html[open_end..].find("</dfn>") else {
            offset = open_end;
            continue;
        };
        let close = open_end + relative_close;
        if tag_attr(tag, "data-dfn-type").as_deref() != Some("value") {
            offset = close + "</dfn>".len();
            continue;
        }
        if tag_attr(tag, "data-dfn-for").as_deref() != Some(dfn_for) {
            offset = close + "</dfn>".len();
            continue;
        }
        let content = &html[open_end..close];
        values.push(strip_tags(content));
        offset = close + "</dfn>".len();
    }
    unique_preserve(values)
}

fn tag_open_end(html: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, ch) in html[start..].char_indices() {
        match (quote, ch) {
            (Some(current), found) if found == current => quote = None,
            (None, '"' | '\'') => quote = Some(ch),
            (None, '>') => return Some(start + offset + ch.len_utf8()),
            _ => {}
        }
    }
    None
}

fn tag_attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let start = tag.find(&needle)? + needle.len();
    let value = tag[start..].trim_start();
    if let Some(rest) = value.strip_prefix('"') {
        return rest.split_once('"').map(|(value, _)| value.to_owned());
    }
    if let Some(rest) = value.strip_prefix('\'') {
        return rest.split_once('\'').map(|(value, _)| value.to_owned());
    }
    let end = value
        .find(|ch: char| ch.is_whitespace() || ch == '>')
        .unwrap_or(value.len());
    Some(value[..end].to_owned())
}

fn strip_tags(html: &str) -> String {
    let stripped = TAG_RE.replace_all(html, "");
    normalize_html_ws(stripped.as_ref())
}

fn svg_clock_units() -> Fallible<Vec<String>> {
    let html = fetch::url_text(SVG_CLOCK_VALUE_SYNTAX_URL, "text/html")?;
    let heading = html
        .find("id=\"ClockValueSyntax\"")
        .ok_or_else(|| boxed("SVG Animations missing ClockValueSyntax heading"))?;
    let pre_start = html[heading..]
        .find("<pre")
        .map(|offset| heading + offset)
        .ok_or_else(|| boxed("SVG Animations missing ClockValueSyntax pre block"))?;
    let pre_open = html[pre_start..]
        .find('>')
        .map(|offset| pre_start + offset + 1)
        .ok_or_else(|| boxed("SVG Animations clock pre block has no opening tag"))?;
    let pre_end = html[pre_open..]
        .find("</pre>")
        .map(|offset| pre_open + offset)
        .ok_or_else(|| boxed("SVG Animations clock pre block has no closing tag"))?;
    let pre = strip_tags(&html[pre_open..pre_end]);
    let metric = METRIC_RE
        .captures(&pre)
        .and_then(|capture| capture.get(1))
        .ok_or_else(|| boxed("SVG Animations clock grammar missing Metric production"))?
        .as_str();
    let units = QUOTED_TOKEN_RE
        .captures_iter(metric)
        .filter_map(|capture| capture.get(1))
        .map(|matched| matched.as_str().to_owned())
        .collect::<Vec<_>>();
    if units.is_empty() {
        return Err(boxed("SVG Animations clock Metric production had no units"));
    }
    Ok(units)
}

fn page_url(url: &str) -> String {
    url.split_once('#')
        .map_or(url, |(page, _fragment)| page)
        .to_owned()
}

fn unique_sorted(values: impl IntoIterator<Item = String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn unique_preserve(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn remove_grammar_dedicated_attributes(buckets: &mut CatalogTreeSitterAttributeBuckets) {
    let dedicated: BTreeSet<&str> = GRAMMAR_DEDICATED_ATTRIBUTE_NAMES.iter().copied().collect();
    let retain = |values: &mut Vec<String>| {
        values.retain(|name| !dedicated.contains(name.as_str()));
    };
    retain(&mut buckets.keyword);
    retain(&mut buckets.color);
    retain(&mut buckets.length);
    retain(&mut buckets.length_list);
    retain(&mut buckets.length_list_or_none);
    retain(&mut buckets.number);
    retain(&mut buckets.number_optional_number);
    retain(&mut buckets.number_list);
    retain(&mut buckets.number_or_percentage);
    retain(&mut buckets.coordinate_pair_list);
    retain(&mut buckets.view_box);
    retain(&mut buckets.functional_iri);
    retain(&mut buckets.css_text);
}

fn validate_projection_inputs(inputs: &GrammarProjectionInputs) -> Fallible<()> {
    assert_webref_defines_terminal_primitives(&inputs.css)?;
    assert_contains_all("length units", &inputs.length_units, EXPECTED_LENGTH_UNITS)?;
    assert_contains_all("angle units", &inputs.angle_units, EXPECTED_ANGLE_UNITS)?;
    assert_contains_all("time units", &inputs.time_units, EXPECTED_TIME_UNITS)?;
    assert_contains_all("color spaces", &inputs.color_spaces, EXPECTED_COLOR_SPACES)?;
    assert_contains_all(
        "color interpolation spaces",
        &inputs.color_interpolation_spaces,
        EXPECTED_COLOR_INTERPOLATION_SPACES,
    )?;
    assert_contains_all(
        "hue interpolation methods",
        &inputs.hue_interpolation_methods,
        EXPECTED_HUE_INTERPOLATION_METHODS,
    )?;
    Ok(())
}

fn validate_attribute_buckets(buckets: &CatalogTreeSitterAttributeBuckets) -> Fallible<()> {
    assert_bucket_contains("color", &buckets.color, EXPECTED_COLOR_ATTRIBUTES)?;
    assert_bucket_contains(
        "number_or_percentage",
        &buckets.number_or_percentage,
        EXPECTED_NUMBER_OR_PERCENTAGE_ATTRIBUTES,
    )?;
    assert_bucket_contains(
        "functional_iri",
        &buckets.functional_iri,
        EXPECTED_FUNCTIONAL_IRI_ATTRIBUTES,
    )?;
    assert_bucket_contains(
        "coordinate_pair_list",
        &buckets.coordinate_pair_list,
        EXPECTED_POINTS_ATTRIBUTES,
    )?;
    assert_bucket_contains("view_box", &buckets.view_box, EXPECTED_VIEW_BOX_ATTRIBUTES)?;
    assert_bucket_contains(
        "length_list",
        &buckets.length_list,
        EXPECTED_LENGTH_LIST_ATTRIBUTES,
    )?;
    assert_bucket_contains(
        "path_data",
        &buckets.path_data,
        EXPECTED_PATH_DATA_ATTRIBUTES,
    )?;
    Ok(())
}

/// Fail the build if upstream `@webref/css` no longer defines a terminal
/// primitive type or the `number-optional-number` type the projection depends
/// on, so an upstream rename surfaces loudly instead of silently reclassifying
/// attributes.
fn assert_webref_defines_terminal_primitives(css: &WebrefCssIndex) -> Fallible<()> {
    let mut missing: Vec<&str> = TERMINAL_PRIMITIVE_TYPES
        .iter()
        .copied()
        .filter(|name| !css.defines_type(name))
        .collect();
    if !css.defines_type("number-optional-number") {
        missing.push("number-optional-number");
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(boxed(format!(
        "@webref/css no longer defines expected CSS type(s): {}",
        missing.join(", ")
    )))
}

/// Fail the build when `subset` is not wholly contained in `superset`, used to
/// cross-check two independently-fetched unit sources agree.
fn assert_subset(
    subset_label: &str,
    subset: &[String],
    superset_label: &str,
    superset: &[String],
) -> Fallible<()> {
    let superset: BTreeSet<&str> = superset.iter().map(String::as_str).collect();
    let extra: Vec<&str> = subset
        .iter()
        .map(String::as_str)
        .filter(|value| !superset.contains(value))
        .collect();
    if extra.is_empty() {
        return Ok(());
    }
    Err(boxed(format!(
        "{subset_label} not contained in {superset_label}: {}",
        extra.join(", ")
    )))
}

fn assert_contains_all(label: &str, extracted: &[String], expected: &[&str]) -> Fallible<()> {
    let extracted: BTreeSet<&str> = extracted.iter().map(String::as_str).collect();
    let missing: Vec<&str> = expected
        .iter()
        .copied()
        .filter(|value| !extracted.contains(value))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(boxed(format!(
        "grammar projection {label} missing expected values: {}",
        missing.join(", ")
    )))
}

fn assert_bucket_contains(bucket: &str, extracted: &[String], expected: &[&str]) -> Fallible<()> {
    let extracted: BTreeSet<&str> = extracted.iter().map(String::as_str).collect();
    let missing: Vec<&str> = expected
        .iter()
        .copied()
        .filter(|value| !extracted.contains(value))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(boxed(format!(
        "grammar projection attribute bucket `{bucket}` missing expected attributes: {}",
        missing.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{
        CatalogAttributeApplicability, CatalogAttributeElementValues, CatalogCssGrammarEdge,
        CatalogCssGrammarEdgeKind, CatalogCssGrammarGraph, CatalogCssGrammarNode,
        CatalogCssGrammarNodeKind,
    };

    fn panic_projection(result: Fallible<CatalogTreeSitterDocument>) -> CatalogTreeSitterDocument {
        match result {
            Ok(document) => document,
            Err(error) => panic!("projection: {error}"),
        }
    }

    #[test]
    fn resolves_nested_url_types_for_functional_iri_bucket() {
        let inputs = GrammarProjectionInputs::for_tests();
        let attribute = attribute("mask", css("<mask-layer#>"));
        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(document.attribute_buckets.functional_iri, ["mask"]);
    }

    #[test]
    fn resolves_color_presentation_attribute_bucket() {
        let css = WebrefCssIndex::from_features(
            [],
            [(
                "color",
                "<color-base> | currentColor | <system-color> | <contrast-color()> | \
                 <device-cmyk()> | <light-dark-color>",
            )],
        );
        let inputs = GrammarProjectionInputs::for_tests();
        let inputs = GrammarProjectionInputs { css, ..inputs };
        let mut attribute = attribute("color", CatalogAttributeValues::FreeText);
        attribute.presentation_attribute = Some("color".to_owned());
        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(document.attribute_buckets.color, ["color"]);
    }

    #[test]
    fn resolves_number_optional_number_grammar_to_dedicated_bucket() {
        let attribute = attribute("stdDeviation", css("<number-optional-number>"));
        let index = WebrefCssIndex::from_features(
            [(
                "types",
                vec![WebrefFeature::new(
                    "number-optional-number",
                    "<number> <number>?",
                )],
            )],
            [],
        );
        let inputs = GrammarProjectionInputs::for_tests();
        let inputs = GrammarProjectionInputs {
            css: index,
            ..inputs
        };
        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(
            document.attribute_buckets.number_optional_number,
            ["stdDeviation"]
        );
        assert!(document.attribute_buckets.number.is_empty());
    }

    #[test]
    fn resolves_keyword_only_css_grammar_to_keyword_bucket() {
        let attribute = attribute("mode", css("<blend-mode>"));
        let index = WebrefCssIndex::from_features(
            [(
                "types",
                vec![WebrefFeature::new(
                    "blend-mode",
                    "normal | multiply | screen | overlay",
                )],
            )],
            [],
        );
        let inputs = GrammarProjectionInputs::for_tests();
        let inputs = GrammarProjectionInputs {
            css: index,
            ..inputs
        };
        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(document.attribute_buckets.keyword, ["mode"]);
        assert!(document.attribute_buckets.css_text.is_empty());
    }

    #[test]
    fn resolves_d_property_to_path_data_bucket_from_paths_facts() {
        let inputs = GrammarProjectionInputs::for_tests();
        let attribute = CatalogAttribute {
            name: "d".to_owned(),
            description: None,
            mdn_url: None,
            spec_url: Some("https://example.test/paths.html#DProperty".to_owned()),
            deprecated: false,
            experimental: false,
            standard_track: None,
            animatable: true,
            presentation_attribute: None,
            baseline: None,
            browser_support: None,
            element_compat: Vec::new(),
            element_values: Vec::new(),
            value_overrides: Vec::new(),
            values: css("none | <string>"),
            applicability: CatalogAttributeApplicability::None,
        };
        assert_eq!(
            attribute_bucket(&attribute, &inputs.css, &inputs.paths),
            Some(AttributeBucket::PathData)
        );

        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(document.attribute_buckets.path_data, ["d"]);
    }

    #[test]
    fn routes_timing_mini_language_attributes_to_css_text_bucket() {
        let inputs = GrammarProjectionInputs::for_tests();
        let attributes = OPAQUE_TIMING_ATTRIBUTE_NAMES
            .iter()
            .map(|name| {
                attribute(
                    name,
                    CatalogAttributeValues::Enum {
                        values: vec!["begin-value-list".to_owned()],
                    },
                )
            })
            .collect::<Vec<_>>();
        let document = panic_projection(build_tree_sitter_document(&attributes, &inputs, false));
        assert_eq!(
            document.attribute_buckets.css_text,
            ["begin", "end", "max", "min"]
        );
        assert!(document.attribute_buckets.keyword.is_empty());
    }

    #[test]
    fn resolves_property_refs_for_number_or_percentage_bucket() {
        let inputs = GrammarProjectionInputs::for_tests();
        let attribute = attribute("fill-opacity", css("<''opacity''>"));
        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(
            document.attribute_buckets.number_or_percentage,
            ["fill-opacity"]
        );
    }

    #[test]
    fn resolves_integer_grammar_to_number_bucket() {
        // `<integer>` is a numeric primitive; the grammar's `number` token
        // accepts integers, so an `<integer>` attribute (`numOctaves`) belongs in
        // the `number` bucket, not the `css_text` catch-all.
        let attribute = attribute("numOctaves", css("<integer>"));
        let index = WebrefCssIndex::from_features(
            [("types", vec![WebrefFeature::new("integer", "<integer>")])],
            [],
        );
        let inputs = GrammarProjectionInputs::for_tests();
        let inputs = GrammarProjectionInputs {
            css: index,
            ..inputs
        };
        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(document.attribute_buckets.number, ["numOctaves"]);
        assert!(document.attribute_buckets.css_text.is_empty());
    }

    #[test]
    fn routes_juxtaposed_keyword_prose_to_css_text_not_keyword() {
        // A grammar of juxtaposed keyword tokens with no alternation operator is
        // not a real enum and must not land in the keyword bucket, where the
        // single-token keyword value rule would reject multi-token real values.
        // (Catalog resolution now rewrites genuine spec see-references upstream,
        // so this guards the routing rule itself with a neutral synthetic input.)
        let attribute = attribute(
            "x_prose",
            prose_css("alpha beta gamma", ["alpha", "beta", "gamma"]),
        );
        let inputs = GrammarProjectionInputs::for_tests();
        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(document.attribute_buckets.css_text, ["x_prose"]);
        assert!(document.attribute_buckets.keyword.is_empty());
    }

    #[test]
    fn routes_bare_alternation_enum_to_keyword_bucket() {
        // A genuine bare keyword enum (`anonymous | use-credentials`) has
        // alternation operators between its keywords, so it stays in the keyword
        // bucket even without a type reference.
        let attribute = attribute(
            "crossorigin",
            alternation_css(["anonymous", "use-credentials"]),
        );
        let inputs = GrammarProjectionInputs::for_tests();
        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(document.attribute_buckets.keyword, ["crossorigin"]);
        assert!(document.attribute_buckets.css_text.is_empty());
    }

    #[test]
    fn divergent_element_values_fall_back_to_css_text_bucket() {
        let mut attribute = attribute("operator", alternation_css(["over", "in"]));
        attribute
            .element_values
            .push(CatalogAttributeElementValues {
                element: "feMorphology".to_owned(),
                values: css("<length-percentage>"),
            });
        let inputs = GrammarProjectionInputs::for_tests();
        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(document.attribute_buckets.css_text, ["operator"]);
        assert!(document.attribute_buckets.keyword.is_empty());
        assert!(document.attribute_buckets.length.is_empty());
    }

    #[test]
    fn compatible_element_values_keep_structured_superset_bucket() {
        let mut attribute = attribute("dx", length_number_list_css());
        attribute
            .element_values
            .push(CatalogAttributeElementValues {
                element: "feOffset".to_owned(),
                values: css("<number>"),
            });
        let inputs = GrammarProjectionInputs::for_tests();
        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(document.attribute_buckets.length_list, ["dx"]);
        assert!(document.attribute_buckets.css_text.is_empty());
    }

    #[test]
    fn compatible_element_values_can_widen_canonical_bucket() {
        let mut attribute = attribute("dx", css("<number>"));
        attribute
            .element_values
            .push(CatalogAttributeElementValues {
                element: "text".to_owned(),
                values: length_number_list_css(),
            });
        let inputs = GrammarProjectionInputs::for_tests();
        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(document.attribute_buckets.length_list, ["dx"]);
        assert!(document.attribute_buckets.css_text.is_empty());
    }

    #[test]
    fn color_bucket_accepts_keyword_scoped_values() {
        let mut attribute = attribute("fill", CatalogAttributeValues::Color);
        attribute
            .element_values
            .push(CatalogAttributeElementValues {
                element: "animate".to_owned(),
                values: CatalogAttributeValues::Enum {
                    values: vec!["freeze".to_owned(), "remove".to_owned()],
                },
            });
        let inputs = GrammarProjectionInputs::for_tests();
        let document = panic_projection(build_tree_sitter_document(&[attribute], &inputs, false));
        assert_eq!(document.attribute_buckets.color, ["fill"]);
        assert!(document.attribute_buckets.css_text.is_empty());
    }

    fn attribute(name: &str, values: CatalogAttributeValues) -> CatalogAttribute {
        CatalogAttribute {
            name: name.to_owned(),
            description: None,
            mdn_url: None,
            spec_url: None,
            deprecated: false,
            experimental: false,
            standard_track: None,
            animatable: false,
            presentation_attribute: None,
            baseline: None,
            browser_support: None,
            element_compat: Vec::new(),
            element_values: Vec::new(),
            value_overrides: Vec::new(),
            values,
            applicability: CatalogAttributeApplicability::None,
        }
    }

    fn css(grammar: &str) -> CatalogAttributeValues {
        CatalogAttributeValues::CssGrammar {
            grammar: grammar.to_owned(),
            graph: graph_for_types([grammar]),
        }
    }

    /// A grammar of juxtaposed keyword tokens with no operators, mirroring how
    /// the scraper captures spec prose (`(see in attribute)`).
    fn prose_css<const N: usize>(grammar: &str, keywords: [&str; N]) -> CatalogAttributeValues {
        CatalogAttributeValues::CssGrammar {
            grammar: grammar.to_owned(),
            graph: graph_of_kinds(
                keywords
                    .into_iter()
                    .map(|text| (CatalogCssGrammarNodeKind::Keyword, text)),
            ),
        }
    }

    /// A genuine bare keyword enum: keywords separated by `|` alternation
    /// operators (`anonymous | use-credentials`).
    fn alternation_css<const N: usize>(keywords: [&str; N]) -> CatalogAttributeValues {
        let mut tokens: Vec<(CatalogCssGrammarNodeKind, &str)> = Vec::new();
        for (index, keyword) in keywords.into_iter().enumerate() {
            if index > 0 {
                tokens.push((CatalogCssGrammarNodeKind::Operator, "|"));
            }
            tokens.push((CatalogCssGrammarNodeKind::Keyword, keyword));
        }
        let grammar = keywords.as_slice().join(" | ");
        CatalogAttributeValues::CssGrammar {
            grammar,
            graph: graph_of_kinds(tokens),
        }
    }

    fn length_number_list_css() -> CatalogAttributeValues {
        CatalogAttributeValues::CssGrammar {
            grammar: "[ [ <length-percentage> | <number> ]+ ]#".to_owned(),
            graph: graph_of_kinds([
                (CatalogCssGrammarNodeKind::Type, "<length-percentage>"),
                (CatalogCssGrammarNodeKind::Operator, "|"),
                (CatalogCssGrammarNodeKind::Type, "<number>"),
                (CatalogCssGrammarNodeKind::Operator, "+"),
                (CatalogCssGrammarNodeKind::Operator, "#"),
            ]),
        }
    }

    fn graph_of_kinds<'a>(
        tokens: impl IntoIterator<Item = (CatalogCssGrammarNodeKind, &'a str)>,
    ) -> CatalogCssGrammarGraph {
        let mut nodes = vec![CatalogCssGrammarNode {
            id: 0,
            kind: CatalogCssGrammarNodeKind::Root,
            text: None,
        }];
        let mut edges = Vec::new();
        for (index, (kind, text)) in tokens.into_iter().enumerate() {
            let id = u16::try_from(index + 1).unwrap_or_else(|_| panic!("test graph id fits"));
            nodes.push(CatalogCssGrammarNode {
                id,
                kind,
                text: Some(text.to_owned()),
            });
            edges.push(CatalogCssGrammarEdge {
                from: 0,
                to: id,
                kind: CatalogCssGrammarEdgeKind::Contains,
            });
        }
        CatalogCssGrammarGraph {
            root: 0,
            nodes,
            edges,
        }
    }

    fn graph_for_types<const N: usize>(types: [&str; N]) -> CatalogCssGrammarGraph {
        let mut nodes = vec![CatalogCssGrammarNode {
            id: 0,
            kind: CatalogCssGrammarNodeKind::Root,
            text: None,
        }];
        let mut edges = Vec::new();
        for (index, token) in types.into_iter().enumerate() {
            let id = u16::try_from(index + 1).unwrap_or_else(|_| panic!("test graph id fits"));
            nodes.push(CatalogCssGrammarNode {
                id,
                kind: CatalogCssGrammarNodeKind::Type,
                text: Some(token.to_owned()),
            });
            edges.push(CatalogCssGrammarEdge {
                from: 0,
                to: id,
                kind: CatalogCssGrammarEdgeKind::Contains,
            });
        }
        CatalogCssGrammarGraph {
            root: 0,
            nodes,
            edges,
        }
    }
}
