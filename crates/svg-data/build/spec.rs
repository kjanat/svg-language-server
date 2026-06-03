//! Deterministic, hermetic extractor for per-element **descriptions** scraped
//! from the vendored svgwg chapter HTML.
//!
//! For each SVG element the spec documents, the chapter HTML opens the element's
//! section with a definition anchor — most often a heading `<h2 id="RectElement">`
//! — immediately followed by a short lead paragraph describing the element. This
//! module locates that anchor + lead `<p>`, strips the markup, and normalizes the
//! whitespace into a single clean sentence-or-two description. The result feeds
//! the snapshot `title` audit (`tests/descriptions_audit.rs`) and, via
//! `build.rs`, the baked catalog descriptions.
//!
//! ## Why raw scanning instead of a DOM walk
//!
//! The vendored chapter files are pre-publish svgwg sources: they contain
//! bespoke `<edit:*>` processing directives (`<edit:elementsummary/>`,
//! `<edit:with element='rect'>`, …) that are **not** valid HTML5. Lenient HTML5
//! parsers (including `tl`) mis-nest the surrounding content around these tags,
//! which makes "the lead `<p>` immediately after the anchor, in document order"
//! unreliable to express against the reconstructed DOM. A deterministic byte/
//! string scan over the raw source — anchored on the canonical `id="XxxElement"`
//! markers, skipping editorial noise containers and `<edit:*>` directives — is
//! both simpler and more faithful to document order here. `tl` is still used to
//! turn the *chosen* paragraph's inner HTML into clean text (entity decoding,
//! nested-tag stripping).
//!
//! ## Determinism
//!
//! - Chapter files are read in a fixed, sorted order.
//! - Within a file, anchors are resolved in a fixed **priority tier** order
//!   (canonical `id="XxxElement"` first, then element-name headings, then
//!   `<edit:with element='…'>`), and within a tier by ascending document offset.
//! - The first description found for an element wins; later anchors never
//!   overwrite it. The output is a [`BTreeMap`], so iteration order is stable.
//! - No I/O beyond reading the passed-in vendored files; no network.

use std::{collections::BTreeMap, fs, path::Path};

use regex::Regex;
use tl::ParserOptions;

/// Chapter HTML files (under the vendored `master/` directory) that carry
/// element definition sections, in a fixed order. Files absent from the vendor
/// are silently skipped, so a partial vendor still extracts what it has.
///
/// Note: `filters.html` and the animations `Overview.html` are intentionally
/// **not** listed — they are not part of the vendored chapter set, so filter
/// and animation elements have no locatable prose here and are reported as gaps
/// by the audit rather than fabricated.
const CHAPTER_FILES: &[&str] = &[
    "embedded.html",
    "interact.html",
    "linking.html",
    "masking.html",
    "painting.html",
    "paths.html",
    "pservers.html",
    "shapes.html",
    "struct.html",
    "text.html",
];

/// How far past an anchor we are willing to look for the lead paragraph before
/// giving up. Lead descriptions sit within the first kilobyte or two of the
/// section; searching further risks wandering into examples or sub-sections.
const MAX_SECTION_WINDOW: usize = 12_000;

/// Minimum length (in characters, post-normalization) for a paragraph to be
/// accepted as a description. Filters out stray short fragments and labels.
const MIN_DESCRIPTION_LEN: usize = 20;

/// Extract per-element lead descriptions from every vendored chapter file under
/// `master`.
///
/// Returns a map of `element name → clean, whitespace-normalized description`.
/// Elements whose prose cannot be located (e.g. filter/animation elements not
/// covered by the vendored chapters) are simply absent — never fabricated.
#[must_use]
pub fn extract_chapter_descriptions(master: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for file in CHAPTER_FILES {
        let Ok(html) = fs::read_to_string(master.join(file)) else {
            continue;
        };
        extract_file(&html, &mut out);
    }
    out
}

/// Resolve every anchor in a single chapter file (tiered, document-ordered) and
/// record the first description found for each element.
fn extract_file(html: &str, out: &mut BTreeMap<String, String>) {
    for anchor in collect_anchors(html) {
        if out.contains_key(&anchor.element) {
            continue;
        }
        if let Some(desc) = anchor_description(html, &anchor) {
            out.insert(anchor.element.clone(), desc);
        }
    }
}

/// Resolve an anchor to its element description.
///
/// For a [`AnchorKind::SelfParagraph`] anchor (`<p id="XxxElement">…</p>`) the
/// description is the anchor paragraph's *own* content. For a
/// [`AnchorKind::FollowingParagraph`] anchor (a heading / `<div>` / directive)
/// the description is the lead `<p>` that follows it.
fn anchor_description(html: &str, anchor: &Anchor) -> Option<String> {
    match anchor.kind {
        AnchorKind::SelfParagraph => {
            let body = &html[anchor.body_start..];
            let close = body.find("</p>")?;
            let text = paragraph_text(&body[..close]);
            (text.chars().count() >= MIN_DESCRIPTION_LEN).then_some(text)
        }
        AnchorKind::FollowingParagraph => lead_description(&html[anchor.body_start..]),
    }
}

/// A located element-definition anchor.
struct Anchor {
    element: String,
    /// Byte offset at which the searchable section body begins (just past the
    /// anchor's open tag).
    body_start: usize,
    kind: AnchorKind,
}

/// Where an anchor's description lives relative to the anchor markup.
#[derive(Clone, Copy)]
enum AnchorKind {
    /// The anchor *is* the descriptive paragraph (`<p id="XxxElement">…</p>`).
    SelfParagraph,
    /// The description is the lead `<p>` following the anchor.
    FollowingParagraph,
}

/// Collect anchors across all priority tiers, each tier ordered by ascending
/// document offset, tier 1 (canonical) entirely before tier 2, etc.
///
/// Tiers, highest priority first:
/// 1. `id="XxxElement"` on a heading / `<div>` / `<p>` — the canonical
///    element-definition anchor.
/// 2. A heading carrying exactly one `<span class="element-name">'name'</span>`
///    — covers sections whose id does not follow the `XxxElement` convention
///    (e.g. the `'a'` element under `id="Links"`).
/// 3. `<edit:with element='name'>` — a processing directive that also names its
///    element; a last-resort anchor.
fn collect_anchors(html: &str) -> Vec<Anchor> {
    let mut anchors = Vec::new();
    push_sorted(&mut anchors, id_element_anchors(html));
    push_sorted(&mut anchors, element_name_heading_anchors(html));
    push_sorted(&mut anchors, edit_with_anchors(html));
    anchors
}

/// Sort a tier's anchors by document offset and append them after the anchors
/// already collected from higher-priority tiers.
fn push_sorted(acc: &mut Vec<Anchor>, mut tier: Vec<Anchor>) {
    tier.sort_by_key(|anchor| anchor.body_start);
    acc.extend(tier);
}

/// Tier 1: `id="XxxElement"` anchors on a heading, `<div>`, or `<p>`.
fn id_element_anchors(html: &str) -> Vec<Anchor> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r#"(?s)<(h[1-6]|div|p)\s+id=(?:'([A-Za-z]+Element)'|"([A-Za-z]+Element)")[^>]*>"#,
        )
        .unwrap_or_else(|err| unreachable!("id-element anchor regex is valid: {err}"))
    });
    let mut anchors = Vec::new();
    for caps in re.captures_iter(html) {
        let Some(whole) = caps.get(0) else { continue };
        let Some(tag) = caps.get(1).map(|m| m.as_str()) else {
            continue;
        };
        // The id is captured in group 2 (single-quoted) or group 3
        // (double-quoted); the `regex` crate has no backreferences, so the two
        // quote styles are alternated rather than matched against an open quote.
        let Some(raw_id) = caps.get(2).or_else(|| caps.get(3)).map(|m| m.as_str()) else {
            continue;
        };
        if is_non_element_id(raw_id) {
            continue;
        }
        let Some(element) = element_name_from_id(raw_id) else {
            continue;
        };
        // `<p id="XxxElement">` carries the lead description in its own body
        // (e.g. `linearGradient`, `stop`); heading / `<div>` anchors are
        // followed by the lead paragraph.
        let kind = if tag.eq_ignore_ascii_case("p") {
            AnchorKind::SelfParagraph
        } else {
            AnchorKind::FollowingParagraph
        };
        anchors.push(Anchor {
            element,
            body_start: whole.end(),
            kind,
        });
    }
    anchors
}

/// Tier 2: headings carrying exactly one `<span class="element-name">'name'</span>`.
///
/// Headings that name two elements (e.g. the shared `'desc'` / `'title'`
/// section) are skipped — there is no single unambiguous lead paragraph for
/// them, so they are left to the audit's gap handling rather than mis-assigned.
fn element_name_heading_anchors(html: &str) -> Vec<Anchor> {
    static HEADING: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static NAME: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let heading = HEADING.get_or_init(|| {
        Regex::new(r"(?s)<h[1-6][^>]*>(.*?)</h[1-6]>")
            .unwrap_or_else(|err| unreachable!("heading regex is valid: {err}"))
    });
    let name = NAME.get_or_init(|| {
        Regex::new(r#"class=['"]element-name['"]>\s*['\u{2018}]([A-Za-z]+)['\u{2019}]"#)
            .unwrap_or_else(|err| unreachable!("element-name regex is valid: {err}"))
    });
    let mut anchors = Vec::new();
    for caps in heading.captures_iter(html) {
        let (Some(whole), Some(inner)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        if inner.as_str().matches("element-name").count() != 1 {
            continue;
        }
        if let Some(found) = name.captures(inner.as_str())
            && let Some(element) = found.get(1)
        {
            anchors.push(Anchor {
                element: element.as_str().to_string(),
                body_start: whole.end(),
                kind: AnchorKind::FollowingParagraph,
            });
        }
    }
    anchors
}

/// Tier 3: `<edit:with element='name'>` processing directives.
fn edit_with_anchors(html: &str) -> Vec<Anchor> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"<edit:with element='([^']+)'>")
            .unwrap_or_else(|err| unreachable!("edit:with regex is valid: {err}"))
    });
    let mut anchors = Vec::new();
    for caps in re.captures_iter(html) {
        let (Some(whole), Some(element)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        anchors.push(Anchor {
            element: element.as_str().to_string(),
            body_start: whole.end(),
            kind: AnchorKind::FollowingParagraph,
        });
    }
    anchors
}

/// Find the lead descriptive paragraph at the start of a section body.
///
/// The body is first truncated at the next element-definition anchor so we never
/// borrow another element's prose. Editorial noise containers
/// (`<div class="annotation|note|example|svg2-requirement">…</div>`) are removed
/// so the genuine lead paragraph — which often follows a SVG-2-requirement table
/// or an `<edit:elementsummary/>` — is the first `<p>` we see. Paragraphs whose
/// own class marks them as noise, label-only paragraphs (e.g. "Attribute
/// definitions:"), and too-short fragments are skipped.
fn lead_description(body: &str) -> Option<String> {
    let window = &body[..body.len().min(MAX_SECTION_WINDOW)];
    let bounded = truncate_at_next_anchor(window);
    let cleaned = remove_noise_containers(bounded);

    for paragraph in paragraphs(&cleaned) {
        if paragraph.class.as_deref().is_some_and(class_is_noise) {
            continue;
        }
        let text = paragraph_text(paragraph.inner);
        if text.chars().count() < MIN_DESCRIPTION_LEN {
            continue;
        }
        if is_label(&text) {
            continue;
        }
        // A paragraph that merely ends with a colon is an introduction to a
        // list, not a description — unless it is the "…defined as follows:"
        // lead the spec uses for a couple of elements (e.g. `view`).
        if text.trim_end().ends_with(':') && !text.to_ascii_lowercase().contains("follows") {
            continue;
        }
        return Some(text);
    }
    None
}

/// Truncate a section body at the next `id="XxxElement"` anchor so an element's
/// lead-paragraph search never crosses into the following element's section.
fn truncate_at_next_anchor(body: &str) -> &str {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"(?s)<(?:h[1-6]|p|div)\s+id=['"][A-Za-z]+Element['"]"#)
            .unwrap_or_else(|err| unreachable!("next-anchor regex is valid: {err}"))
    });
    re.find(body).map_or(body, |found| &body[..found.start()])
}

/// Remove editorial noise containers (`<div class="annotation|note|example|
/// svg2-requirement">…</div>`) so the lead descriptive `<p>` is the first one
/// the scanner encounters.
///
/// svgwg's annotation/requirement/example blocks are flat (not nested inside one
/// another) in the chapter sources, so a non-greedy single pass is sufficient
/// and deterministic.
fn remove_noise_containers(body: &str) -> String {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r#"(?s)<div\s+class=['"][^'"]*(?:annotation|note|example|svg2-requirement)[^'"]*['"][^>]*>.*?</div>"#,
        )
        .unwrap_or_else(|err| unreachable!("noise-container regex is valid: {err}"))
    });
    re.replace_all(body, " ").into_owned()
}

/// A `<p>` block: the value of its `class` attribute (if any) and its raw inner
/// HTML.
struct Paragraph<'a> {
    class: Option<String>,
    inner: &'a str,
}

/// Iterate the `<p>…</p>` blocks of a body in document order.
fn paragraphs(body: &str) -> Vec<Paragraph<'_>> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?s)<p\b([^>]*)>(.*?)</p>")
            .unwrap_or_else(|err| unreachable!("paragraph regex is valid: {err}"))
    });
    re.captures_iter(body)
        .filter_map(|caps| {
            let inner = caps.get(2)?.as_str();
            let class = caps.get(1).and_then(|attrs| class_value(attrs.as_str()));
            Some(Paragraph { class, inner })
        })
        .collect()
}

/// Extract the value of a `class` attribute from an open-tag attribute string.
fn class_value(attrs: &str) -> Option<String> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"class=['"]([^'"]*)['"]"#)
            .unwrap_or_else(|err| unreachable!("class-value regex is valid: {err}"))
    });
    re.captures(attrs)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

/// `true` if a paragraph's `class` marks it as editorial noise rather than
/// descriptive prose.
fn class_is_noise(class: &str) -> bool {
    const NOISE: &[&str] = &["note", "annotation", "example", "svg2-requirement"];
    NOISE.iter().any(|needle| class.contains(needle))
}

/// `true` if a paragraph is just a section label (e.g. "Attribute definitions:")
/// rather than a description.
fn is_label(text: &str) -> bool {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)^(attribute|element|property)\s+definitions?:?\s*$")
            .unwrap_or_else(|err| unreachable!("label regex is valid: {err}"))
    });
    re.is_match(text.trim())
}

/// Turn a paragraph's inner HTML into clean, whitespace-normalized text.
///
/// `tl` parses the fragment (entity-decoding and dropping nested `<a>`, `<code>`,
/// `<em>` … wrappers); the resulting text is whitespace-collapsed and the
/// typographic single quotes the spec uses around element/attribute references
/// are normalized to straight quotes, matching the snapshot transcription style.
fn paragraph_text(inner_html: &str) -> String {
    let text = tl::parse(inner_html, ParserOptions::default()).map_or_else(
        |_| strip_tags_fallback(inner_html),
        |dom| {
            let parser = dom.parser();
            dom.children()
                .iter()
                .map(|handle| {
                    handle
                        .get(parser)
                        .map(|node| node.inner_text(parser).into_owned())
                        .unwrap_or_default()
                })
                .collect::<String>()
        },
    );
    normalize_text(&text)
}

/// Plain tag-stripping fallback for the (unexpected) case where `tl` cannot
/// parse a paragraph fragment.
fn strip_tags_fallback(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

/// Collapse whitespace runs to single spaces, trim, and normalize typographic
/// single quotes to straight quotes.
fn normalize_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(['\u{2018}', '\u{2019}'], "'")
}

/// Convert a `PascalCase` element id (sans the `Element` suffix) to the actual
/// element name. `Rect` → `rect`, `LinearGradient` → `linearGradient`.
fn element_name_from_id(raw_id: &str) -> Option<String> {
    let stem = raw_id.strip_suffix("Element")?;
    if stem.is_empty() {
        return None;
    }
    // An all-uppercase stem is an acronym (`SVGElement` → `svg`); lowercase it
    // wholesale rather than only its first character (which would yield `sVG`).
    if stem.chars().all(|ch| ch.is_ascii_uppercase()) {
        return Some(stem.to_ascii_lowercase());
    }
    let mut chars = stem.chars();
    let first = chars.next()?;
    Some(format!("{}{}", first.to_lowercase(), chars.as_str()))
}

/// `true` for `id="…Element"` values that are **not** element-definition
/// anchors: `WebIDL` interface ids (`InterfaceSVGRectElement`), glossary term
/// definitions (`TermShapeElement`), and the `PointAssociatedElement` umbrella.
fn is_non_element_id(raw_id: &str) -> bool {
    raw_id.contains("InterfaceSVG")
        || raw_id.starts_with("Term")
        || raw_id == "PointAssociatedElement"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_to_element_name() {
        assert_eq!(element_name_from_id("RectElement").as_deref(), Some("rect"));
        assert_eq!(
            element_name_from_id("LinearGradientElement").as_deref(),
            Some("linearGradient")
        );
        assert_eq!(element_name_from_id("Foo").as_deref(), None);
    }

    #[test]
    fn rejects_non_element_ids() {
        assert!(is_non_element_id("InterfaceSVGRectElement"));
        assert!(is_non_element_id("TermShapeElement"));
        assert!(is_non_element_id("PointAssociatedElement"));
        assert!(!is_non_element_id("RectElement"));
    }

    #[test]
    fn extracts_simple_heading_lead() {
        let html = "<h2 id=\"RectElement\">The <span class=\"element-name\">'rect'</span> element</h2>\
            <edit:with element='rect'>\
            <p>The <a>'rect'</a> element defines a rectangle which is axis-aligned \
            with the current user coordinate system.</p>";
        let mut out = BTreeMap::new();
        extract_file(html, &mut out);
        assert_eq!(
            out.get("rect").map(String::as_str),
            Some(
                "The 'rect' element defines a rectangle which is axis-aligned with the current user coordinate system."
            )
        );
    }

    #[test]
    fn skips_annotation_and_label_paragraphs() {
        let html = "<h3 id=\"MarkerElement\">The <span class=\"element-name\">'marker'</span> element</h3>\
            <edit:elementsummary name='marker'/>\
            <p>The <a>'marker element'</a> element defines the graphics that are to \
            be used for drawing markers on a shape.</p>\
            <p id=\"MarkerAttributes\"><em>Attribute definitions:</em></p>";
        let mut out = BTreeMap::new();
        extract_file(html, &mut out);
        assert_eq!(
            out.get("marker").map(String::as_str),
            Some(
                "The 'marker element' element defines the graphics that are to be used for drawing markers on a shape."
            )
        );
    }

    #[test]
    fn canonical_anchor_wins_over_element_name_heading() {
        // A non-canonical element-name heading appears first in the document,
        // but the canonical `id="GElement"` anchor must win.
        let html = "<h2 id=\"Grouping\">Grouping: the <span class=\"element-name\">'g'</span> element</h2>\
            <p>Some earlier prose that is not the canonical lead.</p>\
            <h3 id=\"GElement\">The <span class=\"element-name\">'g'</span> element</h3>\
            <p>The <a>'g'</a> element is a container used to group other SVG elements.</p>";
        let mut out = BTreeMap::new();
        extract_file(html, &mut out);
        assert_eq!(
            out.get("g").map(String::as_str),
            Some("The 'g' element is a container used to group other SVG elements.")
        );
    }

    #[test]
    fn missing_prose_is_absent_not_fabricated() {
        let html = "<div id=\"AElement\">\
            <edit:elementsummary name='a'/>\
            </div>\
            <p><em>Attribute definitions:</em></p>";
        let mut out = BTreeMap::new();
        extract_file(html, &mut out);
        assert!(!out.contains_key("a"));
    }
}
