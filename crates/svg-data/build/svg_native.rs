//! SVG **Native profile** constraint extractor.
//!
//! Parses the vendored Bikeshed spec source
//! (`data/sources/svg-native/index.bs`) into the typed
//! [`SvgNative`](crate::profile::SvgNative) constraint dataset
//! committed at `data/profiles/svg-native.json`.
//!
//! SVG Native is a **profile** (a reductive subset of SVG 2's Secure Static
//! Mode), *not* a version snapshot. This extractor produces the structured
//! constraint data only; wiring it into the LSP profile axis is a separate
//! follow-on.
//!
//! ## Why parse the Bikeshed `.bs` source rather than the rendered HTML
//!
//! The `.bs` source uses unambiguous, machine-friendly reference syntax that
//! survives 1:1 into the rendered HTML but is far cleaner to match:
//!
//! | Bikeshed token | Meaning                | Extracted kind |
//! |----------------|------------------------|----------------|
//! | `<{name}>`     | element reference      | `Element`      |
//! | `'name'`       | attribute / property   | `Attribute`/`Property` |
//! | `''value''`    | value-keyword ref      | `Value`        |
//! | `Title {#id}`  | section heading        | section anchor |
//!
//! The rendered `index.html` is also vendored as a cross-check, but the `.bs`
//! is the authoritative, lower-noise parse target.
//!
//! ## Extraction strategy (deterministic, order-stable)
//!
//! 1. Split the document into sections keyed by their `{#anchor}` ids.
//! 2. Within each section, apply **targeted** pattern rules (not one giant
//!    regex): "X is not supported", explicit bullet allowlists for units /
//!    transform-bearing elements / image formats, and the handful of
//!    individually-phrased supported-only rules (`gradientUnits`, `viewBox`,
//!    `preserveAspectRatio`).
//! 3. Normalise every name (strip Bikeshed `<{ }>` / `' '` / `'' ''` markup,
//!    backticks, link wrappers).
//! 4. Sort constraints by `(kind, name, section)` and de-duplicate so the JSON
//!    is reproducible byte-for-byte across runs.
//!
//! Prose extraction is heuristic by nature. Sections whose constraints cannot
//! be captured structurally with confidence are recorded as
//! [`CoverageGap`](crate::profile::CoverageGap)s rather than silently dropped.

use regex::Regex;

use crate::profile::{
    Constraint, ConstraintKind, ConstraintRule, ConstraintScope, CoverageGap, ProvenancePin,
    SvgNative,
};

/// Errors from compiling the extractor's static regex set.
#[derive(Debug)]
pub enum ExtractError {
    Regex(regex::Error),
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Regex(error) => write!(f, "svg_native regex compile failed: {error}"),
        }
    }
}

impl std::error::Error for ExtractError {}

impl From<regex::Error> for ExtractError {
    fn from(error: regex::Error) -> Self {
        Self::Regex(error)
    }
}

/// A `{#anchor}`-delimited section of the Bikeshed document.
struct Section {
    /// The section anchor id (without the `#`), e.g. `painting`.
    anchor: String,
    /// The section body text (heading line excluded).
    body: String,
}

/// The extractor's compiled regex set, built once.
struct ExtractRegexes {
    /// `Heading Text {#anchor}` line — captures the anchor id.
    heading: Regex,
    /// Bikeshed element reference `<{name}>`.
    element_ref: Regex,
    /// Bikeshed value reference `''value''` (incl. `''calc()''` etc).
    value_ref: Regex,
    /// Bikeshed attribute / property reference `'name'` — NOT preceded by a
    /// second quote (so it never re-captures the inner half of a `''value''`).
    attr_ref: Regex,
    /// A `data:`-URL / HTML-comment line we deliberately skip.
    comment_line: Regex,
    /// Leads the transform-bearing elements bullet block.
    lead_transform: Regex,
    /// Leads the preserveAspectRatio-bearing elements bullet block.
    lead_par: Regex,
}

impl ExtractRegexes {
    fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            heading: Regex::new(r"(?m)^.*\{#([A-Za-z0-9_-]+)\}\s*$")?,
            element_ref: Regex::new(r"<\{([A-Za-z][A-Za-z0-9:_-]*)\}>")?,
            // Two single quotes around a token, tolerating `()` for functions.
            value_ref: Regex::new(r"''([A-Za-z][A-Za-z0-9_-]*(?:\(\))?)''")?,
            // A single-quoted token that is not part of a `''…''` pair. The
            // leading `(?:^|[^'])` guard and trailing `(?:[^']|$)` guard keep
            // it from matching inside a value ref.
            attr_ref: Regex::new(r"(?:^|[^'])'([A-Za-z][A-Za-z0-9:_-]*)'(?:[^']|$)")?,
            comment_line: Regex::new(r"^\s*<!--")?,
            lead_transform: Regex::new(r"'transform'.*only supported")?,
            lead_par: Regex::new(r"'preserveAspectRatio'.*only supported")?,
        })
    }
}

/// Split the raw Bikeshed source into `{#anchor}` sections.
///
/// Lines before the first anchored heading (the metadata/boilerplate preamble)
/// are dropped — they carry no constraints. Each section body runs from just
/// after its heading line up to the next heading.
fn split_sections(re: &ExtractRegexes, content: &str) -> Vec<Section> {
    let mut headings: Vec<(usize, usize, String)> = Vec::new();
    for caps in re.heading.captures_iter(content) {
        let (Some(whole), Some(anchor)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        headings.push((whole.start(), whole.end(), anchor.as_str().to_string()));
    }

    let mut sections = Vec::new();
    for (index, (_, body_start, anchor)) in headings.iter().enumerate() {
        let body_end = headings
            .get(index + 1)
            .map_or(content.len(), |(next_start, _, _)| *next_start);
        let Some(body) = content.get(*body_start..body_end) else {
            continue;
        };
        sections.push(Section {
            anchor: anchor.clone(),
            body: body.to_string(),
        });
    }
    sections
}

/// Strip a leading underline row (`===` / `---`) that Setext headings leave at
/// the top of a section body, so per-line matching starts on real prose.
fn strip_underline(body: &str) -> &str {
    let trimmed = body.trim_start_matches(['\n', '\r']);
    if let Some(rest) = trimmed.split_once('\n') {
        let (first, tail) = rest;
        if !first.is_empty() && first.chars().all(|c| c == '=' || c == '-') {
            return tail;
        }
    }
    body
}

/// Collect the bullet items (`- item`) that follow a line matching `lead` in
/// `body`, until the first non-bullet, non-blank line.
fn bullet_block_after(body: &str, lead: impl Fn(&str) -> bool) -> Option<Vec<&str>> {
    let mut lines = body.lines();
    while let Some(line) = lines.next() {
        if lead(line) {
            let mut items = Vec::new();
            for following in lines.by_ref() {
                let trimmed = following.trim();
                if let Some(item) = trimmed.strip_prefix("- ") {
                    items.push(item);
                } else if trimmed.is_empty() {
                    // Blank line inside / ending the list.
                    if items.is_empty() {
                        continue;
                    }
                    break;
                } else {
                    break;
                }
            }
            if !items.is_empty() {
                return Some(items);
            }
        }
    }
    None
}

/// Pull the single element name out of a `<{name}>` bullet item.
fn element_in(re: &ExtractRegexes, item: &str) -> Option<String> {
    re.element_ref
        .captures(item)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Lines we never treat as constraint prose: HTML comments and the heading
/// underline are already handled; this also drops the `Note:` / `ISSUE:`
/// editorial lines so their inline refs don't become spurious constraints.
fn is_skippable(re: &ExtractRegexes, line: &str) -> bool {
    let trimmed = line.trim_start();
    // `comment_line` is `^\s*<!--`, so it already covers HTML comments; only the
    // `Note:` / `ISSUE:` editorial leaders need the extra checks.
    re.comment_line.is_match(line) || trimmed.starts_with("Note:") || trimmed.starts_with("ISSUE:")
}

/// Does this prose line assert non-support ("not supported")?
fn asserts_unsupported(line: &str) -> bool {
    line.contains("not supported") || line.contains("are not supported")
}

/// Extract every `not supported` constraint from a section body, classifying
/// each referenced name by its Bikeshed markup.
fn unsupported_from_section(re: &ExtractRegexes, section: &Section, out: &mut Vec<Constraint>) {
    for line in strip_underline(&section.body).lines() {
        if is_skippable(re, line) || !asserts_unsupported(line) {
            continue;
        }
        collect_unsupported_refs(re, line, &section.anchor, out);
    }
}

/// Push one [`ConstraintRule::Unsupported`] constraint per distinct markup
/// reference found on a "not supported" `line`.
fn collect_unsupported_refs(
    re: &ExtractRegexes,
    line: &str,
    section: &str,
    out: &mut Vec<Constraint>,
) {
    for caps in re.element_ref.captures_iter(line) {
        if let Some(name) = caps.get(1) {
            if element_ref_is_contextual(name.as_str(), line) {
                continue;
            }
            out.push(Constraint {
                kind: classify_element_ref(name.as_str(), line),
                name: name.as_str().to_string(),
                rule: ConstraintRule::Unsupported,
                section: section.to_string(),
            });
        }
    }
    for caps in re.value_ref.captures_iter(line) {
        if let Some(name) = caps.get(1) {
            out.push(Constraint {
                kind: ConstraintKind::Value,
                name: name.as_str().to_string(),
                rule: ConstraintRule::Unsupported,
                section: section.to_string(),
            });
        }
    }
    for caps in re.attr_ref.captures_iter(line) {
        if let Some(name) = caps.get(1) {
            out.push(Constraint {
                kind: classify_attr(name.as_str()),
                name: name.as_str().to_string(),
                rule: ConstraintRule::Unsupported,
                section: section.to_string(),
            });
        }
    }
}

/// A curated set of names the spec calls *properties* (presentation
/// properties) rather than plain attributes. Everything else single-quoted is
/// recorded as [`ConstraintKind::Attribute`].
///
/// The distinction matters for the oracle (the task lists `display`, `color`,
/// `pointer-events`, `vector-effect`, `paint-order`, `color-interpolation` as
/// *properties*). These names are the CSS/presentation properties the SVG
/// Native spec explicitly enumerates as unsupported.
const PROPERTY_NAMES: &[&str] = &[
    "alignment-baseline",
    "all",
    "baseline-shift",
    "color",
    "color-interpolation",
    "direction",
    "display",
    "dominant-baseline",
    "font",
    "font-family",
    "font-feature-settings",
    "font-kerning",
    "font-size",
    "font-size-adjust",
    "font-stretch",
    "font-style",
    "font-variant",
    "font-weight",
    "glyph-orientation-horizontal",
    "glyph-orientation-vertical",
    "inline-size",
    "kerning",
    "letter-spacing",
    "marker",
    "marker-end",
    "marker-mid",
    "marker-start",
    "overflow",
    "paint-order",
    "pointer-events",
    "text-align",
    "text-align-last",
    "text-anchor",
    "text-decoration",
    "text-decoration-color",
    "text-decoration-fill",
    "text-decoration-line",
    "text-decoration-stroke",
    "text-decoration-style",
    "text-indent",
    "text-orientation",
    "text-overflow",
    "text-rendering",
    "vector-effect",
    "vertical-align",
    "white-space",
    "will-change",
    "word-spacing",
    "writing-mode",
];

/// Classify a single-quoted name as a presentation [`Property`] or a plain
/// [`Attribute`].
///
/// [`Property`]: ConstraintKind::Property
/// [`Attribute`]: ConstraintKind::Attribute
fn classify_attr(name: &str) -> ConstraintKind {
    if PROPERTY_NAMES.contains(&name) {
        ConstraintKind::Property
    } else {
        ConstraintKind::Attribute
    }
}

/// Classify a `<{name}>` dfn reference as an [`Element`] or, in the one case
/// the spec uses element-dfn syntax for a CSS property, a [`Property`].
///
/// SVG Native writes `The <{color}> property is not supported` — `color` is a
/// presentation property, not an element, but Bikeshed links it with the
/// element-style `<{ }>` dfn syntax. The disambiguator is prose-driven: a
/// `<{name}> property` phrasing on the same line marks a property. Elements
/// like `<{font}>` (listed among text/font *facilities*, never followed by the
/// bare word "property") stay elements.
///
/// [`Element`]: ConstraintKind::Element
/// [`Property`]: ConstraintKind::Property
fn classify_element_ref(name: &str, line: &str) -> ConstraintKind {
    let phrase = format!("<{{{name}}}> property");
    if line.contains(&phrase) {
        ConstraintKind::Property
    } else {
        ConstraintKind::Element
    }
}

/// `true` when a `<{name}>` reference on a "not supported" line is merely
/// *contextual* — the host of an unsupported attribute, or a partial/positive
/// qualification — rather than the element being declared unsupported itself.
///
/// Two spec phrasings otherwise trip the flat element matcher and emit a
/// spurious unsupported *element*:
/// - `The 'pathLength' attribute on the <{path}> element is not supported` — the
///   attribute is unsupported; `path` is only its host and stays supported.
/// - `The root element must be an <{svg}> element, and all other <{svg}>
///   elements are not supported` — the root `<svg>` is supported; only nested
///   `<svg>` are not. The flat model can't express "nested-only", so `svg` is
///   left off the unsupported list rather than wrongly rejecting the root.
fn element_ref_is_contextual(name: &str, line: &str) -> bool {
    [
        format!("attribute on the <{{{name}}}>"),
        format!("all other <{{{name}}}>"),
        format!("must be an <{{{name}}}>"),
        format!("must be a <{{{name}}}>"),
    ]
    .iter()
    .any(|phrase| line.contains(phrase))
}

/// Build the four supported-only allowlists plus the per-section enumerated
/// removals that flat `not supported` matching misses.
fn supported_only_and_lists(re: &ExtractRegexes, sections: &[Section], out: &mut Vec<Constraint>) {
    for section in sections {
        let body = strip_underline(&section.body);

        // Units allowlist (coords section): bullet list of `''unit''` / `(unitless)`.
        if let Some(items) = bullet_block_after(body, |line| {
            line.contains("Only the following units are supported")
        }) {
            let mut units: Vec<String> = items
                .iter()
                .filter_map(|item| unit_token(re, item))
                .collect();
            units.sort();
            units.dedup();
            if !units.is_empty() {
                out.push(Constraint {
                    kind: ConstraintKind::Feature,
                    name: "length-unit".to_string(),
                    rule: ConstraintRule::SupportedOnly {
                        scope: ConstraintScope::Units { names: units },
                    },
                    section: section.anchor.clone(),
                });
            }
        }

        // transform-bearing elements (coords section).
        if let Some(items) = bullet_block_after(body, |line| re.lead_transform.is_match(line)) {
            let mut names: Vec<String> = items
                .iter()
                .filter_map(|item| element_in(re, item))
                .collect();
            names.sort();
            names.dedup();
            if !names.is_empty() {
                out.push(Constraint {
                    kind: ConstraintKind::Property,
                    name: "transform".to_string(),
                    rule: ConstraintRule::SupportedOnly {
                        scope: ConstraintScope::Elements { names },
                    },
                    section: section.anchor.clone(),
                });
            }
        }

        // preserveAspectRatio-bearing elements (coords section).
        if let Some(items) = bullet_block_after(body, |line| re.lead_par.is_match(line)) {
            let mut names: Vec<String> = items
                .iter()
                .filter_map(|item| element_in(re, item))
                .collect();
            names.sort();
            names.dedup();
            if !names.is_empty() {
                out.push(Constraint {
                    kind: ConstraintKind::Attribute,
                    name: "preserveAspectRatio".to_string(),
                    rule: ConstraintRule::SupportedOnly {
                        scope: ConstraintScope::Elements { names },
                    },
                    section: section.anchor.clone(),
                });
            }
        }
    }
}

/// Extract `''px''` → `px`, or `(unitless)` → `unitless`, from a unit bullet.
fn unit_token(re: &ExtractRegexes, item: &str) -> Option<String> {
    if let Some(caps) = re.value_ref.captures(item) {
        return caps.get(1).map(|m| m.as_str().to_string());
    }
    if item.trim() == "(unitless)" {
        return Some("unitless".to_string());
    }
    None
}

/// Add the manually-phrased supported-only rules that aren't bullet lists:
/// `gradientUnits` (only `userSpaceOnUse`), `viewBox` (only on `svg`), and the
/// image-format allowlist (`JPEG`/`PNG`, APNG rendered static).
fn singleton_rules(sections: &[Section], out: &mut Vec<Constraint>) {
    let section_of = |needle: &str| -> Option<&str> {
        sections
            .iter()
            .find(|s| s.body.contains(needle))
            .map(|s| s.anchor.as_str())
    };

    // gradientUnits — only userSpaceOnUse.
    if let Some(anchor) = section_of("userSpaceOnUse") {
        out.push(Constraint {
            kind: ConstraintKind::Attribute,
            name: "gradientUnits".to_string(),
            rule: ConstraintRule::SupportedOnly {
                scope: ConstraintScope::Values {
                    names: vec!["userSpaceOnUse".to_string()],
                },
            },
            section: anchor.to_string(),
        });
    }

    // viewBox — only on svg.
    if let Some(anchor) = section_of("'viewBox' attribute is only supported") {
        out.push(Constraint {
            kind: ConstraintKind::Attribute,
            name: "viewBox".to_string(),
            rule: ConstraintRule::SupportedOnly {
                scope: ConstraintScope::Elements {
                    names: vec!["svg".to_string()],
                },
            },
            section: anchor.to_string(),
        });
    }

    // image formats — JPEG / PNG (APNG rendered static).
    if let Some(anchor) = section_of("base64-encoded") {
        out.push(Constraint {
            kind: ConstraintKind::Feature,
            name: "image-format".to_string(),
            rule: ConstraintRule::SupportedOnly {
                scope: ConstraintScope::ImageFormats {
                    names: vec!["JPEG".to_string(), "PNG".to_string()],
                },
            },
            section: anchor.to_string(),
        });
    }
}

/// Capability-level (`Feature`) removals the markup-ref scan can't name —
/// phrased as bare prose ("SVG Native does not support masking", "All external
/// resource loading is forbidden", "must not contain any … XML DTD subset",
/// "XSL Processing is not supported", percentage / relative lengths).
fn feature_rules(sections: &[Section], out: &mut Vec<Constraint>) {
    // (needle, feature-slug)
    const PROSE_FEATURES: &[(&str, &str)] = &[
        ("does not support masking", "masking"),
        (
            "All external resource loading is forbidden",
            "external-resource-loading",
        ),
        ("XSL Processing is not supported", "xsl-processing"),
        (
            "must not contain any of the external or internal XML DTD subset",
            "xml-dtd-subset",
        ),
        (
            "Percentage length values are not supported",
            "percentage-length",
        ),
        ("Relative units</a> are not supported", "relative-length"),
        (
            "HTML elements in SVG subtrees are not supported",
            "html-in-svg",
        ),
        (
            "All scripting and interactivity facilities are not supported",
            "scripting-interactivity",
        ),
        (
            "All text and fonts facilities are not supported",
            "text-and-fonts",
        ),
        (
            "Conditional processesing attributes and elements are not supported",
            "conditional-processing",
        ),
        (
            "global CSS keywords are not supported",
            "css-global-keywords",
        ),
    ];

    for (needle, slug) in PROSE_FEATURES {
        if let Some(anchor) = sections
            .iter()
            .find(|s| s.body.contains(needle))
            .map(|s| s.anchor.as_str())
        {
            out.push(Constraint {
                kind: ConstraintKind::Feature,
                name: (*slug).to_string(),
                rule: ConstraintRule::Unsupported,
                section: anchor.to_string(),
            });
        }
    }
}

/// Sections whose constraints are prose-only or enumerated as free-text lists
/// inside a single paragraph, where structured per-name extraction is partial.
/// Recorded as explicit coverage gaps so consumers know the dataset is not a
/// total capture of that prose.
fn coverage_gaps(sections: &[Section]) -> Vec<CoverageGap> {
    let mut gaps = Vec::new();

    // The `commonattributes` section lists attribute *groups* by English name
    // ("aria attributes", "conditional processing attributes", "all core
    // attributes except for 'id'", …) — these are not individual SVG names and
    // cannot be expanded without a curated group→names map.
    if sections.iter().any(|s| s.anchor == "commonattributes") {
        gaps.push(CoverageGap {
            section: "commonattributes".to_string(),
            reason: "Lists attribute GROUPS by prose name (aria / conditional-processing / core \
                     except id / event-attribute families / deprecated xlink). These are not \
                     individual SVG attribute names; expanding them needs a curated group→names \
                     map, deferred."
                .to_string(),
        });
    }

    // The `text` section enumerates ~50 names in one comma-separated paragraph;
    // every name with `<{ }>` / `' '` markup IS captured by the ref scan, but
    // the bare phrase "the subproperties of 'font-variant'" expands to names
    // not literally present — flag the residual.
    if sections.iter().any(|s| s.anchor == "text") {
        gaps.push(CoverageGap {
            section: "text".to_string(),
            reason: "Captures every explicitly-marked text/font element & property, but the prose \
                     phrase \"the subproperties of 'font-variant'\" denotes additional names not \
                     spelled out literally; those sub-properties are not individually enumerated."
                .to_string(),
        });
    }

    gaps
}

/// Run the full SVG Native constraint extraction over the vendored `index.bs`.
///
/// `bikeshed_source` is the raw bytes of `data/sources/svg-native/index.bs`.
/// The `pin` carries provenance copied from the vendored `PROVENANCE.toml`.
pub fn extract_svg_native(
    bikeshed_source: &str,
    pin: ProvenancePin,
) -> Result<SvgNative, ExtractError> {
    let re = ExtractRegexes::new()?;
    let sections = split_sections(&re, bikeshed_source);

    let mut constraints: Vec<Constraint> = Vec::new();

    for section in &sections {
        unsupported_from_section(&re, section, &mut constraints);
    }
    supported_only_and_lists(&re, &sections, &mut constraints);
    singleton_rules(&sections, &mut constraints);
    feature_rules(&sections, &mut constraints);

    // Stable, schema-independent ordering by (kind, name, section), then drop
    // exact duplicates so the committed JSON is reproducible byte-for-byte.
    constraints.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.section.cmp(&b.section))
    });
    constraints.dedup();

    Ok(SvgNative {
        schema_version: 1,
        profile: "SvgNative".to_string(),
        source_pin: pin,
        constraints,
        coverage_gaps: coverage_gaps(&sections),
    })
}
