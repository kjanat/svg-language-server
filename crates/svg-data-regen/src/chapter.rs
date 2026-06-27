//! Extract permalink anchors, term definitions, and example references from a
//! chapter or appendix HTML page.
//!
//! Chapter source HTML carries the prose, the `id` anchors that element and
//! attribute hrefs point at, the `<dfn>` term definitions, and `<edit:example>`
//! references. (The rendered element-summary tables are injected at publish
//! time from `definitions.xml`, so the structural content model is extracted
//! from there, not here.) This module turns one page into a typed record.

use std::collections::BTreeMap;

use serde::Serialize;
use tl::{HTMLTag, Parser, ParserOptions};

use crate::util::{is_keyword_token, normalize_html_ws as normalize_ws};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// An `id` anchor: a permalink target within a chapter.
#[derive(Debug, Clone, Serialize)]
pub struct Anchor {
    /// The fragment id (the part after `#` in a permalink).
    pub id: String,
    /// The HTML tag carrying the id (`h2`, `dfn`, `dt`, ...).
    pub tag: String,
    /// The heading text, captured for section headings (`h1`..`h6`).
    pub text: Option<String>,
}

/// A `<dfn>` term definition.
#[derive(Debug, Clone, Serialize)]
pub struct Dfn {
    /// The definition's anchor id, when it has one.
    pub id: Option<String>,
    /// The defined term's text.
    pub term: String,
    /// The `data-dfn-type` (e.g. `dfn`, `element`, `attribute`), when set.
    pub kind: Option<String>,
}

/// An `<edit:example>` reference to an example asset.
#[derive(Debug, Clone, Serialize)]
pub struct Example {
    /// The referenced example file (`href`).
    pub href: Option<String>,
    /// The `image` flag (`yes`/`no`), when set.
    pub image: Option<String>,
    /// The `link` flag (`yes`/`no`), when set.
    pub link: Option<String>,
}

/// A property definition table (`<table class="propdef">`): the value space and
/// metadata for a single CSS-style property or presentation attribute.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyValueDef {
    /// Property name (the `Name:` row).
    pub name: String,
    /// The bearer element this definition is scoped to, from the source
    /// `data-dfn-for` element attribute. Element-attrdef rows carry the element
    /// they belong to (e.g. `feComposite`); property/global tables that have no
    /// single bearer leave this `None`.
    pub dfn_for: Option<String>,
    /// The property's definition anchor id, when its `<dfn>` has one.
    pub id: Option<String>,
    /// The raw value grammar (the `Value:` row), e.g. `start | middle | end`.
    pub value: Option<String>,
    /// The bare keyword alternatives parsed out of the value grammar (the enum
    /// members); `<type>` references and bracketed groups are excluded.
    pub keywords: Vec<String>,
    /// The initial value (`Initial:` row).
    pub initial: Option<String>,
    /// Which elements the property applies to (`Applies to:` row).
    pub applies_to: Option<String>,
    /// Whether the property is inherited (`Inherited:` row).
    pub inherited: Option<String>,
    /// The computed value description (`Computed value:` row).
    pub computed_value: Option<String>,
    /// The animation type (`Animation type:` row).
    pub animation_type: Option<String>,
}

/// A term defined in a `<dl class="definitions">` list, paired with the prose
/// description from its `<dd>`. This is the spec's own glossary: the reliable,
/// structured source of per-entity descriptions.
#[derive(Debug, Clone, Serialize)]
pub struct TermDefinition {
    /// The defined term (the `<dt>`'s `<dfn>` text).
    pub term: String,
    /// The definition's anchor id, when its `<dfn>` has one.
    pub id: Option<String>,
    /// The `data-dfn-type` (`dfn`, `element`, `attribute`, ...), when set.
    pub kind: Option<String>,
    /// The description prose from the paired `<dd>`.
    pub description: String,
}

/// Prose attached to an anchor id, usually the first meaningful paragraph
/// after a section heading, property table, or attribute definition row.
#[derive(Debug, Clone, Serialize)]
pub struct AnchorDescription {
    /// The anchor id the prose describes.
    pub id: String,
    /// Human-readable description prose.
    pub description: String,
}

/// Everything extracted from one chapter/appendix page.
#[derive(Debug, Clone, Serialize)]
pub struct Chapter {
    /// The chapter's source name (e.g. `struct`), backing `<name>.html`.
    pub name: String,
    /// Every `id` anchor on the page.
    pub anchors: Vec<Anchor>,
    /// Term definitions (anchors only).
    pub dfns: Vec<Dfn>,
    /// Example references.
    pub examples: Vec<Example>,
    /// Property value-definition tables.
    pub properties: Vec<PropertyValueDef>,
    /// Glossary term definitions paired with their descriptions.
    pub term_definitions: Vec<TermDefinition>,
    /// Prose descriptions keyed by spec anchor id.
    pub anchor_descriptions: Vec<AnchorDescription>,
}

/// Category membership used to expand the spec's publish-time `<edit:*category>`
/// macros (which are empty in source HTML) back into prose. Keyed by category
/// name; values are the member element/attribute names, in document order.
#[derive(Debug, Default)]
pub struct MacroIndex {
    /// Element-category name to its member element names.
    pub element_categories: BTreeMap<String, Vec<String>>,
    /// Attribute-category name to its member attribute names.
    pub attribute_categories: BTreeMap<String, Vec<String>>,
}

/// Extract anchors, definitions, examples, properties, and term definitions
/// from a chapter's HTML. `macros` supplies category membership so that the
/// publish-time `<edit:*category>` placeholders in descriptions are expanded
/// rather than dropped.
///
/// # Errors
/// Returns an error if the HTML cannot be parsed.
pub fn extract_chapter(name: &str, html: &str, macros: &MacroIndex) -> Fallible<Chapter> {
    let dom = tl::parse(html, ParserOptions::default())?;
    let parser = dom.parser();
    let mut chapter = Chapter {
        name: name.to_owned(),
        anchors: Vec::new(),
        dfns: Vec::new(),
        examples: Vec::new(),
        properties: Vec::new(),
        term_definitions: Vec::new(),
        anchor_descriptions: Vec::new(),
    };
    let mut pending_description_ids: Vec<String> = Vec::new();
    let mut section_intro: Option<String> = None;

    for node in dom.nodes() {
        let Some(tag) = node.as_tag() else {
            continue;
        };
        let tag_name = tag.name().as_utf8_str();

        if let Some(id) = attr(tag, "id") {
            let text = if is_heading(&tag_name) {
                Some(normalize_ws(&tag.inner_text(parser)))
            } else {
                None
            };
            chapter.anchors.push(Anchor {
                id,
                tag: tag_name.clone().into_owned(),
                text,
            });
        }

        if is_heading(&tag_name)
            && let Some(id) = attr(tag, "id")
        {
            pending_description_ids = vec![id];
            section_intro = None;
            continue;
        }

        if has_class(tag, "propdef") {
            let ids = dfn_ids(tag, parser);
            if !ids.is_empty() {
                pending_description_ids = ids;
            }
        }

        if tag_name == "edit:elementsummary" {
            pending_description_ids.clear();
        }

        if handle_paragraph_description(
            tag,
            parser,
            macros,
            &mut pending_description_ids,
            &mut section_intro,
            &mut chapter.anchor_descriptions,
        ) {
            continue;
        }

        match tag_name.as_ref() {
            "dfn" => chapter.dfns.push(Dfn {
                id: attr(tag, "id"),
                term: normalize_ws(&tag.inner_text(parser)),
                kind: attr(tag, "data-dfn-type"),
            }),
            "edit:example" => chapter.examples.push(Example {
                href: attr(tag, "href"),
                image: attr(tag, "image"),
                link: attr(tag, "link"),
            }),
            "table" if has_class(tag, "attrdef") => {
                chapter
                    .properties
                    .extend(extract_attrdef_table(tag, parser));
            }
            "table" if has_class(tag, "propdef") => {
                if let Some(property) = extract_propdef(tag, parser) {
                    chapter.properties.push(property);
                }
            }
            "dl" if has_class(tag, "definitions") => {
                extract_definition_list(tag, parser, macros, &mut chapter.term_definitions);
            }
            "dl" if is_attribute_definition_list(tag) => {
                extract_attribute_definition_list(
                    tag,
                    parser,
                    macros,
                    &mut chapter.anchor_descriptions,
                );
                chapter
                    .properties
                    .extend(extract_svg2_attrdef_list(tag, parser));
            }
            _ => {}
        }
    }

    append_html_derived_properties(html, &dom, &mut chapter.properties);

    Ok(chapter)
}

fn append_html_derived_properties(
    html: &str,
    dom: &tl::VDom,
    properties: &mut Vec<PropertyValueDef>,
) {
    properties.extend(extract_legacy_adef_dt_assignments(html));
    properties.extend(extract_animate_motion_coordinate_values(dom));
}

/// Extract only CSS/SVG property-definition tables from an HTML page.
///
/// This is the fast path for external CSS specs: unlike [`extract_chapter`],
/// it does not walk prose anchors, examples, dfn panels, or descriptions.
pub fn extract_property_definitions(html: &str) -> Vec<PropertyValueDef> {
    let mut properties = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = html[offset..].find("<table") {
        let start = offset + relative_start;
        let Some(relative_head_end) = html[start..].find('>') else {
            break;
        };
        let head_end = start + relative_head_end + 1;
        let Some(relative_end) = html[head_end..].find("</table>") else {
            offset = head_end;
            continue;
        };
        let end = head_end + relative_end + "</table>".len();
        if table_head_mentions_propdef(&html[start..head_end]) {
            let table_html = &html[start..end];
            if let Some(property) = extract_raw_propdef(table_html) {
                properties.push(property);
            }
        }
        offset = end;
    }
    properties.extend(extract_css2_propinfo_definitions(html));
    properties.extend(extract_raw_element_attrdef_assignments(html));
    properties
}

fn table_head_mentions_propdef(head: &str) -> bool {
    head.to_ascii_lowercase().contains("propdef")
}

fn extract_raw_propdef(table_html: &str) -> Option<PropertyValueDef> {
    let mut name = None;
    let mut id = None;
    let mut value = None;
    let mut initial = None;
    let mut applies_to = None;
    let mut inherited = None;
    let mut computed_value = None;
    let mut animation_type = None;

    for row in raw_rows(table_html) {
        let cells = raw_cells(row);
        let Some(label) = cells.first() else {
            continue;
        };
        let cell = cells.get(1).cloned().unwrap_or_default();
        match label
            .trim_end_matches(':')
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "name" => {
                name = Some(cell);
                id = first_raw_dfn_id(row);
            }
            "value" => value = Some(cell),
            "initial" => initial = Some(cell),
            "applies to" => applies_to = Some(cell),
            "inherited" => inherited = Some(cell),
            "computed value" => computed_value = Some(cell),
            "animation type" => animation_type = Some(cell),
            _ => {}
        }
    }

    let name = name?;
    let keywords = value.as_deref().map(value_keywords).unwrap_or_default();
    Some(PropertyValueDef {
        name,
        dfn_for: None,
        id,
        value,
        keywords,
        initial,
        applies_to,
        inherited,
        computed_value,
        animation_type,
    })
}

fn extract_css2_propinfo_definitions(html: &str) -> Vec<PropertyValueDef> {
    let mut properties = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = html[offset..].find("<div") {
        let start = offset + relative_start;
        let Some(relative_head_end) = html[start..].find('>') else {
            break;
        };
        let head_end = start + relative_head_end + 1;
        let Some(relative_end) = html[head_end..].find("</div>") else {
            offset = head_end;
            continue;
        };
        let end = head_end + relative_end + "</div>".len();
        let block_html = &html[start..end];
        if table_head_mentions_propdef(&html[start..head_end])
            && block_html.contains("class=\"propinfo\"")
            && let Some(property) = extract_raw_propinfo_propdef(block_html)
        {
            properties.push(property);
        }
        offset = end;
    }
    properties
}

fn extract_raw_propinfo_propdef(block_html: &str) -> Option<PropertyValueDef> {
    let id = first_raw_propdef_anchor(block_html)?;
    let name = id.strip_prefix("propdef-")?.to_owned();
    let mut value = None;
    let mut initial = None;
    let mut applies_to = None;
    let mut inherited = None;
    let mut computed_value = None;
    let mut animation_type = None;

    for row in raw_rows(block_html) {
        let cells = raw_cells(row);
        let Some(label) = cells.first() else {
            continue;
        };
        let cell = cells.get(1).cloned().unwrap_or_default();
        match label
            .trim_end_matches(':')
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "value" => value = Some(cell),
            "initial" => initial = Some(cell),
            "applies to" => applies_to = Some(cell),
            "inherited" => inherited = Some(cell),
            "computed value" => computed_value = Some(cell),
            "animation type" => animation_type = Some(cell),
            _ => {}
        }
    }

    let keywords = value.as_deref().map(value_keywords).unwrap_or_default();
    Some(PropertyValueDef {
        name,
        dfn_for: None,
        id: Some(id),
        value,
        keywords,
        initial,
        applies_to,
        inherited,
        computed_value,
        animation_type,
    })
}

fn extract_legacy_adef_dt_assignments(html: &str) -> Vec<PropertyValueDef> {
    let mut definitions = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = html[offset..].find("<dt") {
        let start = offset + relative_start;
        let Some(open_end) = raw_tag_open_end(html, start) else {
            break;
        };
        let Some(relative_close) = html[open_end..].find("</dt>") else {
            offset = open_end;
            continue;
        };
        let close = open_end + relative_close;
        let block = &html[open_end..close];
        offset = close + "</dt>".len();
        if !is_inline_legacy_adef_dt(block) {
            continue;
        }
        let Some(name) = legacy_adef_name(block) else {
            continue;
        };
        let Some(value) = legacy_adef_assignment_value(block) else {
            continue;
        };
        let keywords = value_keywords(&value);
        definitions.push(PropertyValueDef {
            name,
            dfn_for: None,
            id: raw_attr(&html[start..open_end], "id"),
            value: Some(value),
            keywords,
            initial: None,
            applies_to: None,
            inherited: None,
            computed_value: None,
            animation_type: None,
        });
    }
    definitions
}

fn legacy_adef_name(block: &str) -> Option<String> {
    let marker = r#"class="adef""#;
    let start = block.find(marker)? + marker.len();
    let after_open = block[start..].find('>')? + start + 1;
    let close = block[after_open..].find("</span>")? + after_open;
    let name = normalize_ws(&strip_tags(&block[after_open..close]));
    (!name.is_empty()).then_some(name)
}

fn is_inline_legacy_adef_dt(block: &str) -> bool {
    block.contains(r#"class="adef""#) && (block.contains("</span> =") || block.contains("</span>="))
}

fn legacy_adef_assignment_value(block: &str) -> Option<String> {
    let marker = r#"class="adef""#;
    let start = block.find(marker)? + marker.len();
    let after_open = block[start..].find('>')? + start + 1;
    let close = block[after_open..].find("</span>")? + after_open + "</span>".len();
    let after_name = &block[close..];
    let equals = after_name.find('=')?;
    let value = after_name[equals + 1..].trim_start();
    let quote = value.chars().next().filter(|ch| matches!(ch, '"' | '\''))?;
    let body = &value[quote.len_utf8()..];
    let end = raw_quoted_html_value_end(body, quote)?;
    let value = attrdef_assignment_value_from_raw_html(&body[..end]);
    (!value.is_empty()).then_some(value)
}

fn extract_raw_element_attrdef_assignments(html: &str) -> Vec<PropertyValueDef> {
    let mut definitions = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = html[offset..].find("<dfn") {
        let start = offset + relative_start;
        let Some(open_end) = raw_tag_open_end(html, start) else {
            break;
        };
        let tag_head = &html[start..open_end];
        let Some(relative_close) = html[open_end..].find("</dfn>") else {
            offset = open_end;
            continue;
        };
        let close = open_end + relative_close;
        offset = close + "</dfn>".len();

        if raw_attr(tag_head, "data-dfn-type").as_deref() != Some("element-attr") {
            continue;
        }
        let name = normalize_ws(&strip_tags(&html[open_end..close]));
        if name.is_empty() {
            continue;
        }
        let Some(value) = raw_attrdef_assignment_value(&html[offset..]) else {
            continue;
        };
        let keywords = value_keywords(&value);
        definitions.push(PropertyValueDef {
            name,
            dfn_for: raw_attr(tag_head, "data-dfn-for"),
            id: raw_attr(tag_head, "id"),
            value: Some(value),
            keywords,
            initial: None,
            applies_to: None,
            inherited: None,
            computed_value: None,
            animation_type: None,
        });
    }
    definitions
}

fn raw_attrdef_assignment_value(after_dfn: &str) -> Option<String> {
    let limit = find_first(after_dfn, &["<dd", "<dt", "\n"]).unwrap_or(after_dfn.len());
    let head = &after_dfn[..limit];
    let equals = head.find('=')?;
    let value = head[equals + 1..].trim_start();
    let quote = value.chars().next().filter(|ch| matches!(ch, '"' | '\''))?;
    let body = &value[quote.len_utf8()..];
    let end = raw_quoted_html_value_end(body, quote)?;
    let value = attrdef_assignment_value_from_raw_html(&body[..end]);
    (!value.is_empty()).then_some(value)
}

/// Extract an attribute's value grammar from the still-entity-encoded HTML of a
/// `<dfn …> = "…"` assignment.
///
/// `raw` MUST be the pre-decode HTML: real markup tags appear as literal `<…>`
/// while CSS type references the spec writes inline are entity-escaped (`&lt;…>`)
/// or wrapped in a `class="css production"` anchor. Decoding first would erase
/// that distinction, making an `<em>`/`<code>` markup wrapper indistinguishable
/// from a `<length>` production and corrupting keyword grammars (e.g. `<em>R | G
/// | B | A</em>`) into the bare tag name. Working on `raw` keeps both apart.
fn attrdef_assignment_value_from_raw_html(raw: &str) -> String {
    let stripped = normalize_ws(&crate::util::decode_html_entities(&strip_markup_tags(raw)));
    if let Some(production) = number_list_prose_production(&stripped) {
        return production;
    }
    match css_production_assignment_value(raw) {
        Some(production) if production == stripped => production,
        _ => stripped,
    }
}

/// Canonicalize the SVG spec's prose "list of numbers" idiom into a real CSS
/// production (`<number>+`) so downstream graph/classification treats it as the
/// number list it is, rather than losing the list shape.
///
/// The spec writes this grammar several prose ways, all of which otherwise
/// degrade to a scalar `<number>` (the `class="css production"` anchor path
/// keeps only the inner type reference, discarding the "list of" prose) or to
/// garbled keyword tokens (`<list of numbers>` survives entity-decoding as the
/// mangled `<list> of numbers>`). Verbatim from drafts.csswg.org/filter-effects-1:
///
/// * `list of <number>s`   — `feColorMatrix/values`.
/// * `(list of <number>s)` — `feComponentTransfer/tableValues` (paren-wrapped).
/// * `<list of numbers>`   — `feConvolveMatrix/kernelMatrix` (whole-phrase prose).
///
/// All denote one-or-more whitespace/comma-separated numbers. Matching is on the
/// grammar *shape* (the "list of" phrasing wrapping the `number` type), not on
/// any attribute name, so new spec attributes with the same prose are covered
/// automatically.
fn number_list_prose_production(stripped: &str) -> Option<String> {
    let mut inner = stripped.trim().to_ascii_lowercase();
    // Drop surrounding parentheses (`(list of <number>s)`).
    if let Some(unparen) = inner
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
    {
        inner = unparen.trim().to_owned();
    }
    // Drop the angle brackets of the whole-phrase form (`<list of numbers>`).
    if let Some(unwrapped) = inner
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
    {
        inner = unwrapped.trim().to_owned();
    }
    let rest = inner.strip_prefix("list of ")?.trim();
    // The remaining noun is `number`/`numbers`, optionally as the `<number>`
    // production and/or pluralized (`<number>s`).
    let noun = rest
        .strip_prefix('<')
        .and_then(|body| body.strip_suffix('>').or_else(|| body.strip_suffix(">s")))
        .unwrap_or(rest);
    matches!(noun, "number" | "numbers").then(|| "<number>+".to_owned())
}

/// The single `class="css production"` type reference of an assignment whose
/// whole value is one production anchor (e.g. `&lt;length-percentage>`), read
/// from the still-encoded `raw` HTML so the anchor's escaped `&lt;…>` body is
/// recovered intact rather than colliding with later markup tags.
fn css_production_assignment_value(raw: &str) -> Option<String> {
    let marker = r#"class="css production""#;
    let start = raw.find(marker)? + marker.len();
    let after_open = raw[start..].find('>')? + start + 1;
    let close = raw[after_open..].find('<')? + after_open;
    let value = normalize_ws(&crate::util::decode_html_entities(&raw[after_open..close]));
    value.starts_with('<').then_some(value)
}

/// Strip HTML markup tags from `html`, treating `>` as a tag terminator only
/// while inside a tag. Unlike [`strip_tags`], a `>` encountered outside a tag is
/// preserved as text, so an entity-escaped CSS production (`&lt;list of
/// numbers>`) keeps its closing `>` after the surrounding `<em>`/`<a>` wrappers
/// are removed. Markup tags always open with a literal `<`; escaped productions
/// never do, so the two are never confused.
fn strip_markup_tags(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match (in_tag, ch) {
            (false, '<') => in_tag = true,
            (true, '>') => in_tag = false,
            (false, _) => text.push(ch),
            (true, _) => {}
        }
    }
    text
}

fn raw_quoted_html_value_end(value: &str, quote: char) -> Option<usize> {
    let mut in_tag = false;
    let mut tag_quote = None;
    for (offset, ch) in value.char_indices() {
        match (in_tag, tag_quote, ch) {
            (true, Some(current), found) if found == current => tag_quote = None,
            (true, None, '"' | '\'') => tag_quote = Some(ch),
            (true, None, '>') => in_tag = false,
            (false, _, '<') => in_tag = true,
            (false, _, found) if found == quote => return Some(offset),
            _ => {}
        }
    }
    None
}

fn raw_tag_open_end(html: &str, start: usize) -> Option<usize> {
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

fn raw_rows(table_html: &str) -> Vec<&str> {
    let mut rows = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = table_html[offset..].find("<tr") {
        let start = offset + relative_start;
        let Some(relative_head_end) = table_html[start..].find('>') else {
            break;
        };
        let content_start = start + relative_head_end + 1;
        let relative_end = find_first(&table_html[content_start..], &["<tr", "</table>"])
            .unwrap_or(table_html.len() - content_start);
        let end = content_start + relative_end;
        rows.push(&table_html[content_start..end]);
        offset = end;
    }
    rows
}

fn raw_cells(row_html: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = find_first(&row_html[offset..], &["<th", "<td"]) {
        let start = offset + relative_start;
        let Some(relative_head_end) = row_html[start..].find('>') else {
            break;
        };
        let content_start = start + relative_head_end + 1;
        let relative_end = find_first(
            &row_html[content_start..],
            &["<th", "<td", "</tr", "<tr", "</table>"],
        )
        .unwrap_or(row_html.len() - content_start);
        let end = content_start + relative_end;
        cells.push(normalize_ws(&strip_tags(&row_html[content_start..end])));
        offset = end;
    }
    cells
}

fn find_first(haystack: &str, needles: &[&str]) -> Option<usize> {
    needles
        .iter()
        .filter_map(|needle| haystack.find(needle))
        .min()
}

fn strip_tags(html: &str) -> String {
    // This is not a general HTML tokenizer. It is only used on controlled W3C
    // snippets where `<` inside attribute values is not expected in the text
    // fragments we strip.
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    text
}

fn first_raw_dfn_id(html: &str) -> Option<String> {
    let start = html.find("<dfn")?;
    let end = start + html[start..].find('>')?;
    raw_attr(&html[start..end], "id")
}

fn first_raw_propdef_anchor(html: &str) -> Option<String> {
    let mut offset = 0;
    while let Some(relative_start) = html[offset..].find("<a") {
        let start = offset + relative_start;
        let Some(relative_end) = html[start..].find('>') else {
            break;
        };
        let end = start + relative_end;
        let tag_head = &html[start..end];
        if let Some(name) = raw_attr(tag_head, "name")
            && name.starts_with("propdef-")
        {
            return Some(name);
        }
        if let Some(id) = raw_attr(tag_head, "id")
            && id.starts_with("propdef-")
        {
            return Some(id);
        }
        offset = end + 1;
    }
    None
}

fn raw_attr(tag_head: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let start = tag_head.find(&needle)? + needle.len();
    let value = tag_head[start..].trim_start();
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

/// Pair the `<dt>`/`<dd>` children of a definition list into term definitions,
/// walking direct children in document order so each term keeps its own
/// description.
fn extract_definition_list(
    dl: &HTMLTag,
    parser: &Parser,
    macros: &MacroIndex,
    out: &mut Vec<TermDefinition>,
) {
    let mut pending: Option<Dfn> = None;
    for handle in dl.children().top().iter() {
        let Some(child) = handle.get(parser).and_then(|node| node.as_tag()) else {
            continue;
        };
        match child.name().as_utf8_str().as_ref() {
            "dt" => pending = Some(term_of(child, parser)),
            "dd" => {
                if let Some(dfn) = pending.take() {
                    out.push(TermDefinition {
                        term: dfn.term,
                        id: dfn.id,
                        kind: dfn.kind,
                        description: description_text(child, parser, macros),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Pair `<dt>` rows containing attribute `<dfn>` ids with their following
/// `<dd>` prose and assign that prose to each id in the row.
fn extract_attribute_definition_list(
    dl: &HTMLTag,
    parser: &Parser,
    macros: &MacroIndex,
    out: &mut Vec<AnchorDescription>,
) {
    let mut pending_ids = Vec::new();
    for handle in dl.children().top().iter() {
        let Some(child) = handle.get(parser).and_then(|node| node.as_tag()) else {
            continue;
        };
        match child.name().as_utf8_str().as_ref() {
            "dt" => pending_ids = description_anchor_ids(child, parser),
            "dd" if !pending_ids.is_empty() => {
                let description = attribute_description_text(child, parser, macros);
                if !description.is_empty() {
                    let ids = std::mem::take(&mut pending_ids);
                    out.extend(ids.into_iter().map(|id| AnchorDescription {
                        id,
                        description: description.clone(),
                    }));
                }
            }
            _ => {}
        }
    }
}

fn handle_paragraph_description(
    tag: &HTMLTag,
    parser: &Parser,
    macros: &MacroIndex,
    pending_ids: &mut Vec<String>,
    section_intro: &mut Option<String>,
    out: &mut Vec<AnchorDescription>,
) -> bool {
    if tag.name().as_utf8_str().as_ref() != "p" {
        return false;
    }

    let description = description_text(tag, parser, macros);
    let usable = is_description_paragraph(tag, &description);
    if pending_ids.is_empty() {
        if section_intro.is_none() && usable {
            *section_intro = Some(description);
        }
        return true;
    }

    if usable {
        if section_intro.is_none() {
            *section_intro = Some(description.clone());
        }
        append_anchor_descriptions(pending_ids, &description, out);
    } else if let Some(description) = section_intro.clone() {
        append_anchor_descriptions(pending_ids, &description, out);
    }
    true
}

fn append_anchor_descriptions(
    pending_ids: &mut Vec<String>,
    description: &str,
    out: &mut Vec<AnchorDescription>,
) {
    let ids = std::mem::take(pending_ids);
    out.extend(ids.into_iter().map(|id| AnchorDescription {
        id,
        description: description.to_owned(),
    }));
}

fn is_attribute_definition_list(tag: &HTMLTag) -> bool {
    has_class(tag, "attrdef-list") || has_class(tag, "attrdef-list-svg2")
}

fn is_description_paragraph(tag: &HTMLTag, description: &str) -> bool {
    if description.is_empty()
        || has_class(tag, "annotation")
        || has_class(tag, "caption")
        || has_class(tag, "definition")
        || has_class(tag, "prod")
    {
        return false;
    }
    !matches!(
        description.trim(),
        "where:" | "Values have the following meanings:"
    ) && !description.ends_with(" is defined as follows:")
}

fn description_anchor_ids(tag: &HTMLTag, parser: &Parser) -> Vec<String> {
    let ids = dfn_ids(tag, parser);
    if !ids.is_empty() {
        return ids;
    }
    attr(tag, "id").into_iter().collect()
}

fn attribute_description_text(dd: &HTMLTag, parser: &Parser, macros: &MacroIndex) -> String {
    for handle in dd.children().top().iter() {
        let Some(child) = handle.get(parser).and_then(|node| node.as_tag()) else {
            continue;
        };
        if child.name().as_utf8_str().as_ref() == "p" && !has_class(child, "annotation") {
            let description = description_text(child, parser, macros);
            if !description.is_empty() {
                return description;
            }
        }
    }
    description_text(dd, parser, macros)
}

/// Build a tag's text content, expanding `<edit:elementcategory>` and
/// `<edit:attributecategory>` placeholders into their member lists (which
/// `inner_text` would otherwise drop, leaving dangling "Specifically:" prose).
fn description_text(tag: &HTMLTag, parser: &Parser, macros: &MacroIndex) -> String {
    let mut buffer = String::new();
    collect_text(tag, parser, macros, &mut buffer);
    normalize_ws(&buffer)
}

/// Append `tag`'s descendant text to `buffer`, substituting category macros.
fn collect_text(tag: &HTMLTag, parser: &Parser, macros: &MacroIndex, buffer: &mut String) {
    for handle in tag.children().top().iter() {
        let Some(node) = handle.get(parser) else {
            continue;
        };
        if let Some(child) = node.as_tag() {
            match child.name().as_utf8_str().as_ref() {
                "edit:elementcategory" => {
                    push_members(&macros.element_categories, child, buffer);
                }
                "edit:attributecategory" => {
                    push_members(&macros.attribute_categories, child, buffer);
                }
                _ => collect_text(child, parser, macros, buffer),
            }
        } else if let Some(raw) = node.as_raw() {
            buffer.push_str(&raw.as_utf8_str());
        }
    }
}

/// Append the comma-joined members of the category named by `tag`'s `name`.
fn push_members(members: &BTreeMap<String, Vec<String>>, tag: &HTMLTag, buffer: &mut String) {
    if let Some(name) = attr(tag, "name")
        && let Some(names) = members.get(&name)
    {
        buffer.push_str(&names.join(", "));
    }
}

/// The term a `<dt>` defines: its inner `<dfn>` when present, else the `<dt>`'s
/// own text (so no term is dropped).
fn term_of(dt: &HTMLTag, parser: &Parser) -> Dfn {
    if let Some(handle) = dt
        .query_selector(parser, "dfn")
        .and_then(|mut hits| hits.next())
        && let Some(dfn) = handle.get(parser).and_then(|node| node.as_tag())
    {
        return Dfn {
            id: attr(dfn, "id"),
            term: normalize_ws(&dfn.inner_text(parser)),
            kind: attr(dfn, "data-dfn-type"),
        };
    }
    Dfn {
        id: None,
        term: normalize_ws(&dt.inner_text(parser)),
        kind: None,
    }
}

fn dfn_ids(tag: &HTMLTag, parser: &Parser) -> Vec<String> {
    let Some(hits) = tag.query_selector(parser, "dfn") else {
        return Vec::new();
    };
    hits.filter_map(|handle| handle.get(parser).and_then(|node| node.as_tag()))
        .filter_map(|dfn| attr(dfn, "id"))
        .collect()
}

/// Extract a single property value-definition table into a [`PropertyValueDef`].
///
/// Returns `None` when the table has no `Name:` row (so it is not a real
/// property definition).
fn extract_propdef(table: &HTMLTag, parser: &Parser) -> Option<PropertyValueDef> {
    let mut name = None;
    let mut id = None;
    let mut value = None;
    let mut initial = None;
    let mut applies_to = None;
    let mut inherited = None;
    let mut computed_value = None;
    let mut animation_type = None;

    let rows = table.query_selector(parser, "tr")?;
    for handle in rows {
        let Some(row) = handle.get(parser).and_then(|node| node.as_tag()) else {
            continue;
        };
        let Some((label, cell)) = propdef_row_label_and_value(row, parser) else {
            continue;
        };
        match label
            .trim_end_matches(':')
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "name" => {
                name = Some(cell);
                id = first_attr(row, parser, "dfn", "id");
            }
            "value" => value = Some(cell),
            "initial" => initial = Some(cell),
            "applies to" => applies_to = Some(cell),
            "inherited" => inherited = Some(cell),
            "computed value" => computed_value = Some(cell),
            "animation type" => animation_type = Some(cell),
            _ => {}
        }
    }

    let name = name?;
    let keywords = value.as_deref().map(value_keywords).unwrap_or_default();
    Some(PropertyValueDef {
        name,
        dfn_for: None,
        id,
        value,
        keywords,
        initial,
        applies_to,
        inherited,
        computed_value,
        animation_type,
    })
}

/// Extract SVG attribute definition tables that use a horizontal
/// `Name / Value / Initial value / Animatable` layout.
fn extract_attrdef_table(table: &HTMLTag, parser: &Parser) -> Vec<PropertyValueDef> {
    let Some(rows) = table.query_selector(parser, "tr") else {
        return Vec::new();
    };
    let mut header: Option<AttrdefHeader> = None;
    let mut definitions = Vec::new();

    for handle in rows {
        let Some(row) = handle.get(parser).and_then(|node| node.as_tag()) else {
            continue;
        };
        let th = row_cell_texts(row, parser, "th");
        if !th.is_empty() {
            header = AttrdefHeader::from_cells(&th);
            continue;
        }

        let Some(header) = header else {
            continue;
        };
        let Some(td_handles) = row_cell_handles(row, parser, "td") else {
            continue;
        };
        if td_handles.len() <= header.name || td_handles.len() <= header.value {
            continue;
        }
        let names = td_handles
            .get(header.name)
            .and_then(|handle| handle.get(parser))
            .and_then(|node| node.as_tag())
            .map_or_else(Vec::new, |cell| attrdef_names(cell, parser));
        if names.is_empty() {
            continue;
        }

        let value = row_cell_text(&td_handles, parser, header.value);
        let initial = header
            .initial
            .and_then(|index| row_cell_text(&td_handles, parser, index));
        let animation_type = header
            .animatable
            .and_then(|index| row_cell_text(&td_handles, parser, index));
        definitions.extend(names.into_iter().map(|name| {
            property_from_attrdef(
                name.name,
                name.dfn_for,
                name.id,
                value.clone(),
                initial.clone(),
                animation_type.clone(),
            )
        }));
    }

    definitions
}

/// Extract SVG 2 attribute definition lists of the form:
///
/// `<dt id=...><span class=adef>name</span></dt><dd><dl class=attrdef-svg2>...`.
fn extract_svg2_attrdef_list(dl: &HTMLTag, parser: &Parser) -> Vec<PropertyValueDef> {
    let mut pending_names = Vec::new();
    let mut definitions = Vec::new();

    for handle in dl.children().top().iter() {
        let Some(child) = handle.get(parser).and_then(|node| node.as_tag()) else {
            continue;
        };
        match child.name().as_utf8_str().as_ref() {
            "dt" => pending_names = attrdef_names(child, parser),
            "dd" if !pending_names.is_empty() => {
                if let Some(definition) = extract_svg2_attrdef_metadata(child, parser) {
                    definitions.extend(pending_names.iter().cloned().map(|name| {
                        property_from_attrdef(
                            name.name,
                            name.dfn_for,
                            name.id,
                            Some(definition.value.clone()),
                            definition.initial.clone(),
                            definition.animation_type.clone(),
                        )
                    }));
                }
                pending_names.clear();
            }
            _ => {}
        }
    }

    definitions
}

#[derive(Clone, Copy)]
struct AttrdefHeader {
    name: usize,
    value: usize,
    initial: Option<usize>,
    animatable: Option<usize>,
}

impl AttrdefHeader {
    fn from_cells(cells: &[String]) -> Option<Self> {
        let mut name = None;
        let mut value = None;
        let mut initial = None;
        let mut animatable = None;
        for (index, cell) in cells.iter().enumerate() {
            match cell
                .trim_end_matches(':')
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "name" => name = Some(index),
                "value" => value = Some(index),
                "initial value" | "initial" => initial = Some(index),
                "animatable" => animatable = Some(index),
                _ => {}
            }
        }
        Some(Self {
            name: name?,
            value: value?,
            initial,
            animatable,
        })
    }
}

struct Svg2AttrdefMetadata {
    value: String,
    initial: Option<String>,
    animation_type: Option<String>,
}

#[derive(Clone)]
struct AttrdefName {
    name: String,
    dfn_for: Option<String>,
    id: Option<String>,
}

fn extract_svg2_attrdef_metadata(dd: &HTMLTag, parser: &Parser) -> Option<Svg2AttrdefMetadata> {
    let mut lists = dd.query_selector(parser, "dl")?;
    let list = lists.find_map(|handle| {
        handle
            .get(parser)
            .and_then(|node| node.as_tag())
            .filter(|tag| has_class(tag, "attrdef-svg2"))
    })?;
    let mut pending_label = None;
    let mut value = None;
    let mut initial = None;
    let mut animation_type = None;

    for handle in list.children().top().iter() {
        let Some(child) = handle.get(parser).and_then(|node| node.as_tag()) else {
            continue;
        };
        match child.name().as_utf8_str().as_ref() {
            "dt" => pending_label = Some(normalize_ws(&child.inner_text(parser))),
            "dd" => {
                let Some(label) = pending_label.take() else {
                    continue;
                };
                let text = normalize_ws(&child.inner_text(parser));
                match label
                    .trim_end_matches(':')
                    .trim()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "value" => value = Some(text),
                    "initial value" | "initial" => initial = Some(text),
                    "animatable" => animation_type = Some(text),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    Some(Svg2AttrdefMetadata {
        value: value?,
        initial,
        animation_type,
    })
}

fn row_cell_handles(row: &HTMLTag, parser: &Parser, selector: &str) -> Option<Vec<tl::NodeHandle>> {
    Some(row.query_selector(parser, selector)?.collect())
}

fn row_cell_texts(row: &HTMLTag, parser: &Parser, selector: &str) -> Vec<String> {
    row_cell_handles(row, parser, selector)
        .unwrap_or_default()
        .iter()
        .filter_map(|handle| handle_text(*handle, parser))
        .collect()
}

fn row_cell_text(handles: &[tl::NodeHandle], parser: &Parser, index: usize) -> Option<String> {
    handles
        .get(index)
        .and_then(|handle| handle_text(*handle, parser))
        .filter(|text| !text.is_empty())
}

fn attrdef_names(tag: &HTMLTag, parser: &Parser) -> Vec<AttrdefName> {
    let mut names = Vec::new();
    if let Some(handles) = tag.query_selector(parser, "dfn") {
        let mut inherited_dfn_for = attr(tag, "data-dfn-for");
        names.extend(handles.filter_map(|handle| {
            let dfn = handle.get(parser)?.as_tag()?;
            let dfn_for = attr(dfn, "data-dfn-for").or_else(|| inherited_dfn_for.clone());
            if dfn_for.is_some() {
                inherited_dfn_for.clone_from(&dfn_for);
            }
            let id = attr(dfn, "id").or_else(|| attr(tag, "id"));
            let name = normalize_attr_name(&dfn.inner_text(parser)).or_else(|| {
                id.as_deref()
                    .and_then(|id| attr_name_from_dfn_id(id, dfn_for.as_deref()))
            })?;
            Some(AttrdefName { name, dfn_for, id })
        }));
    }
    if names.is_empty()
        && let Some(handles) = tag.query_selector(parser, "span")
    {
        names.extend(handles.filter_map(|handle| {
            let span = handle.get(parser)?.as_tag()?;
            if !has_class(span, "adef") {
                return None;
            }
            let name = normalize_attr_name(&span.inner_text(parser))?;
            Some(AttrdefName {
                name,
                dfn_for: attr(span, "data-dfn-for").or_else(|| attr(tag, "data-dfn-for")),
                id: attr(span, "id").or_else(|| attr(tag, "id")),
            })
        }));
    }
    if names.is_empty() {
        names.extend(
            split_attr_names(&normalize_ws(&tag.inner_text(parser))).map(|name| {
                let id = attr(tag, "id");
                AttrdefName {
                    name,
                    dfn_for: attr(tag, "data-dfn-for"),
                    id,
                }
            }),
        );
    }
    names
}

fn split_attr_names(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(',').filter_map(normalize_attr_name)
}

fn normalize_attr_name(name: &str) -> Option<String> {
    let name = name
        .trim()
        .trim_matches('\'')
        .trim_matches('"')
        .trim()
        .to_owned();
    (!name.is_empty()).then_some(name)
}

fn attr_name_from_dfn_id(id: &str, dfn_for: Option<&str>) -> Option<String> {
    let mut name = id.strip_suffix("Attribute")?;
    let prefix = dfn_for.map(element_dfn_prefix);
    if let Some(prefix) = prefix.as_deref()
        && let Some(stripped) = name.strip_prefix(prefix)
    {
        name = stripped;
    }
    lower_first(name)
}

fn element_dfn_prefix(element: &str) -> String {
    let mut chars = element.chars();
    let Some(first) = chars.next() else {
        return "Element".to_owned();
    };
    let mut prefix = first.to_ascii_uppercase().to_string();
    prefix.extend(chars);
    prefix.push_str("Element");
    prefix
}

fn lower_first(name: &str) -> Option<String> {
    let mut chars = name.chars();
    let first = chars.next()?.to_ascii_lowercase();
    let mut lowered = first.to_string();
    lowered.extend(chars);
    Some(lowered)
}

fn property_from_attrdef(
    name: String,
    dfn_for: Option<String>,
    id: Option<String>,
    value: Option<String>,
    initial: Option<String>,
    animation_type: Option<String>,
) -> PropertyValueDef {
    let keywords = value.as_deref().map(value_keywords).unwrap_or_default();
    PropertyValueDef {
        name,
        dfn_for,
        id,
        value,
        keywords,
        initial,
        applies_to: None,
        inherited: None,
        computed_value: None,
        animation_type,
    }
}

fn extract_animate_motion_coordinate_values(dom: &tl::VDom) -> Vec<PropertyValueDef> {
    let parser = dom.parser();
    let mut definitions = Vec::new();
    for node in dom.nodes() {
        let Some(tag) = node.as_tag() else {
            continue;
        };
        if tag.name().as_utf8_str().as_ref() != "p" {
            continue;
        }
        let text = normalize_ws(&tag.inner_text(parser));
        let lower = text.to_ascii_lowercase();
        if !lower.contains("animatemotion")
            || !lower.contains("from")
            || !lower.contains("by")
            || !lower.contains("to")
            || !lower.contains("values")
            || !lower.contains("x, y coordinate pairs")
        {
            continue;
        }
        definitions.extend(["from", "to", "by"].into_iter().map(|name| {
            synthetic_attrdef(
                name,
                "animateMotion",
                Some("x, y coordinate pair".to_owned()),
            )
        }));
        definitions.push(synthetic_attrdef(
            "values",
            "animateMotion",
            Some("semicolon-separated x, y coordinate pairs".to_owned()),
        ));
        break;
    }
    definitions
}

fn synthetic_attrdef(name: &str, dfn_for: &str, value: Option<String>) -> PropertyValueDef {
    let keywords = value.as_deref().map(value_keywords).unwrap_or_default();
    PropertyValueDef {
        name: name.to_owned(),
        dfn_for: Some(dfn_for.to_owned()),
        id: None,
        value,
        keywords,
        initial: None,
        applies_to: None,
        inherited: None,
        computed_value: None,
        animation_type: None,
    }
}

/// The bare keyword alternatives in a value grammar (the enum members).
///
/// Splits on the CSS `|` alternation and keeps only bare identifier tokens,
/// dropping `<type>` references, functional notation, and bracketed groups. The
/// full grammar is retained separately, so this is a convenience view, not the
/// source of truth.
fn value_keywords(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(str::trim)
        .filter(|token| is_keyword_token(token))
        .map(str::to_owned)
        .collect()
}

/// Whether `tag` carries `class` among its space-separated class list.
fn has_class(tag: &HTMLTag, class: &str) -> bool {
    attr(tag, "class").is_some_and(|classes| classes.split_whitespace().any(|each| each == class))
}

fn propdef_row_label_and_value(row: &HTMLTag, parser: &Parser) -> Option<(String, String)> {
    if let Some(label) = first_text(row, parser, "th") {
        return Some((label, first_text(row, parser, "td").unwrap_or_default()));
    }

    let mut cells = row.query_selector(parser, "td")?;
    let label = handle_text(cells.next()?, parser)?;
    let value = cells
        .next()
        .and_then(|handle| handle_text(handle, parser))
        .unwrap_or_default();
    Some((label, value))
}

/// The normalized inner text of the first descendant matching `selector`.
fn first_text(tag: &HTMLTag, parser: &Parser, selector: &str) -> Option<String> {
    let handle = tag.query_selector(parser, selector)?.next()?;
    handle_text(handle, parser)
}

fn handle_text(handle: tl::NodeHandle, parser: &Parser) -> Option<String> {
    let found = handle.get(parser)?.as_tag()?;
    Some(normalize_ws(&found.inner_text(parser)))
}

/// The value of `attr_key` on the first descendant matching `selector`.
fn first_attr(tag: &HTMLTag, parser: &Parser, selector: &str, attr_key: &str) -> Option<String> {
    let handle = tag.query_selector(parser, selector)?.next()?;
    let found = handle.get(parser)?.as_tag()?;
    attr(found, attr_key)
}

/// Whether a tag name is an HTML heading (`h1`..`h6`).
fn is_heading(name: &str) -> bool {
    matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

/// The value of attribute `key` on `tag`, if present with a value.
fn attr(tag: &HTMLTag, key: &str) -> Option<String> {
    match tag.attributes().get(key) {
        Some(Some(value)) => Some(value.as_utf8_str().into_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"<h2 id="Shapes">Basic <span>Shapes</span></h2>
<p>Shapes are graphical elements.</p>
<dl class="definitions">
  <dt><dfn id="term-shape" data-dfn-type="dfn">shape</dfn></dt>
  <dd>A graphics element with a defined outline.</dd>
</dl>
<table class="propdef def">
  <tr><th>Name:</th><td><dfn id="TextAnchor" data-dfn-type="property">text-anchor</dfn></td></tr>
  <tr><th>Value:</th><td>start | middle | end</td></tr>
  <tr><th>Initial:</th><td>start</td></tr>
  <tr><th>Inherited:</th><td>yes</td></tr>
</table>
<p>The <a>'text-anchor'</a> property aligns text.</p>
<dl class="attrdef-list">
  <dt><table><tr><td><dfn id="DemoAttribute">demo</dfn></td></tr></table></dt>
  <dd>The demo attribute controls demo behavior.</dd>
</dl>
<dl class="attrdef-list-svg2">
  <dt id="DirectAttribute"><span class="adef">direct</span></dt>
  <dd><p>The direct attribute uses its dt id.</p><dl><dt>Value</dt><dd>number</dd></dl></dd>
</dl>
<edit:example href='images/x.svg' image='no'/>"#;

    #[test]
    fn value_keywords_keeps_only_bare_keywords() {
        assert_eq!(
            value_keywords("start | middle | end"),
            ["start", "middle", "end"]
        );
        assert_eq!(value_keywords("auto | <length-percentage>"), ["auto"]);
        assert_eq!(value_keywords("<paint>"), Vec::<String>::new());
        assert_eq!(value_keywords("nonzero | evenodd"), ["nonzero", "evenodd"]);
    }

    #[test]
    fn decode_entities_handles_named_and_numeric() {
        assert_eq!(
            crate::util::decode_html_entities("a&lt;b&gt;c").as_ref(),
            "a<b>c"
        );
        assert_eq!(
            crate::util::decode_html_entities("&amp;&quot;").as_ref(),
            "&\""
        );
        assert_eq!(crate::util::decode_html_entities("x&#65;y").as_ref(), "xAy");
        assert_eq!(
            crate::util::decode_html_entities("x&#x41;y").as_ref(),
            "xAy"
        );
        assert_eq!(
            crate::util::decode_html_entities("plain text").as_ref(),
            "plain text"
        );
        // Unrecognized entity body is left verbatim.
        assert_eq!(
            crate::util::decode_html_entities("a&bogus;b").as_ref(),
            "a&bogus;b"
        );
    }

    #[test]
    fn extracts_chapter_entities() -> Result<(), Box<dyn std::error::Error>> {
        let ch = extract_chapter("shapes", HTML, &MacroIndex::default())?;

        // Heading anchor keeps its (entity/whitespace-normalized) text.
        let shapes = ch
            .anchors
            .iter()
            .find(|a| a.id == "Shapes")
            .ok_or("no Shapes anchor")?;
        assert_eq!(shapes.tag, "h2");
        assert_eq!(shapes.text.as_deref(), Some("Basic Shapes"));

        assert_eq!(ch.examples.len(), 1);
        assert_eq!(ch.examples[0].href.as_deref(), Some("images/x.svg"));
        assert_eq!(ch.examples[0].image.as_deref(), Some("no"));

        assert_eq!(ch.properties.len(), 1);
        let prop = &ch.properties[0];
        assert_eq!(prop.name, "text-anchor");
        assert_eq!(prop.id.as_deref(), Some("TextAnchor"));
        assert_eq!(prop.value.as_deref(), Some("start | middle | end"));
        assert_eq!(prop.keywords, ["start", "middle", "end"]);
        assert_eq!(prop.initial.as_deref(), Some("start"));
        assert_eq!(prop.inherited.as_deref(), Some("yes"));

        assert_eq!(ch.term_definitions.len(), 1);
        let term = &ch.term_definitions[0];
        assert_eq!(term.term, "shape");
        assert_eq!(term.id.as_deref(), Some("term-shape"));
        assert_eq!(term.kind.as_deref(), Some("dfn"));
        assert_eq!(
            term.description,
            "A graphics element with a defined outline."
        );

        let descriptions: std::collections::BTreeMap<_, _> = ch
            .anchor_descriptions
            .iter()
            .map(|description| (description.id.as_str(), description.description.as_str()))
            .collect();
        assert_eq!(
            descriptions.get("Shapes").copied(),
            Some("Shapes are graphical elements.")
        );
        assert_eq!(
            descriptions.get("TextAnchor").copied(),
            Some("The 'text-anchor' property aligns text.")
        );
        assert_eq!(
            descriptions.get("DemoAttribute").copied(),
            Some("The demo attribute controls demo behavior.")
        );
        assert_eq!(
            descriptions.get("DirectAttribute").copied(),
            Some("The direct attribute uses its dt id.")
        );
        Ok(())
    }

    #[test]
    fn property_descriptions_skip_value_explanation_boilerplate()
    -> Result<(), Box<dyn std::error::Error>> {
        let html = r#"<h3 id="DashSection">Dash section</h3>
<table class="propdef">
  <tr><th>Name:</th><td><dfn id="DashProperty">stroke-dasharray</dfn></td></tr>
  <tr><th>Value:</th><td>none | &lt;dasharray&gt;</td></tr>
</table>
<p>where:</p>
<p class="definition prod">&lt;dasharray&gt; = [ &lt;number&gt;+ ]#</p>
<p>The 'stroke-dasharray' property controls the pattern of dashes and gaps.</p>
<h3 id="AnchorSection">Anchor section</h3>
<p>The 'text-anchor' property aligns text relative to a point.</p>
<table class="propdef">
  <tr><th>Name:</th><td><dfn id="AnchorProperty">text-anchor</dfn></td></tr>
  <tr><th>Value:</th><td>start | middle | end</td></tr>
</table>
<p>Values have the following meanings:</p>"#;
        let ch = extract_chapter("text", html, &MacroIndex::default())?;
        let descriptions: std::collections::BTreeMap<_, _> = ch
            .anchor_descriptions
            .iter()
            .map(|description| (description.id.as_str(), description.description.as_str()))
            .collect();

        assert_eq!(
            descriptions.get("DashProperty").copied(),
            Some("The 'stroke-dasharray' property controls the pattern of dashes and gaps.")
        );
        assert_eq!(
            descriptions.get("AnchorProperty").copied(),
            Some("The 'text-anchor' property aligns text relative to a point.")
        );
        Ok(())
    }

    #[test]
    fn decodes_entities_in_value_grammar() -> Result<(), Box<dyn std::error::Error>> {
        let html = r#"<table class="propdef">
  <tr><th>Name:</th><td><dfn id="P">inline-size</dfn></td></tr>
  <tr><th>Value:</th><td>auto | <a>&lt;length-percentage&gt;</a></td></tr>
</table>"#;
        let ch = extract_chapter("text", html, &MacroIndex::default())?;
        assert_eq!(ch.properties.len(), 1);
        assert_eq!(
            ch.properties[0].value.as_deref(),
            Some("auto | <length-percentage>")
        );
        assert_eq!(ch.properties[0].keywords, ["auto"]);
        Ok(())
    }

    #[test]
    fn extracts_legacy_css_propdef_tables_with_td_labels() -> Result<(), Box<dyn std::error::Error>>
    {
        let html = r#"<table class="propdef">
  <tr><td>Name:</td><td><dfn id="propdef-font-style">font-style</dfn></td></tr>
  <tr><td>Value:</td><td>normal | italic | oblique</td></tr>
  <tr><td>Animation type:</td><td>no</td></tr>
</table>"#;
        let ch = extract_chapter("css-fonts-3", html, &MacroIndex::default())?;
        assert_eq!(ch.properties.len(), 1);
        let prop = &ch.properties[0];
        assert_eq!(prop.name, "font-style");
        assert_eq!(prop.id.as_deref(), Some("propdef-font-style"));
        assert_eq!(prop.value.as_deref(), Some("normal | italic | oblique"));
        assert_eq!(prop.keywords, ["normal", "italic", "oblique"]);
        assert_eq!(prop.animation_type.as_deref(), Some("no"));
        Ok(())
    }

    #[test]
    fn extracts_bikeshed_propdef_tables_with_optional_cell_closures() {
        let html = r#"
<table class="data"><tr><th>Other<td>ignored</table>
<table class="def propdef" data-link-for-hint="clip-rule">
  <tbody>
    <tr>
      <th>Name:
      <td><dfn class="css" id="propdef-clip-rule">clip-rule</dfn>
    <tr class="value">
      <th>Value:
      <td class="prod">nonzero | evenodd
    <tr>
      <th>Animation type:
      <td>discrete
  </table>"#;
        let properties = extract_property_definitions(html);

        assert_eq!(properties.len(), 1);
        let prop = &properties[0];
        assert_eq!(prop.name, "clip-rule");
        assert_eq!(prop.id.as_deref(), Some("propdef-clip-rule"));
        assert_eq!(prop.value.as_deref(), Some("nonzero | evenodd"));
        assert_eq!(prop.keywords, ["nonzero", "evenodd"]);
        assert_eq!(prop.animation_type.as_deref(), Some("discrete"));
    }

    #[test]
    fn extracts_css2_propinfo_blocks() {
        let html = r#"
<div class="propdef">
<dl><dt>
<span class="index-def" title="'display'"><a name="propdef-display" class="propdef-title"><strong>'display'</strong></a></span>
<dd>
<table class="propinfo" cellspacing=0 cellpadding=0>
<tr valign=baseline><td><em>Value:</em>&nbsp;&nbsp;<td>inline | block | list-item | inline-block |
table | inline-table | table-row-group | table-header-group |
table-footer-group | table-row | table-column-group | table-column |
table-cell | table-caption | none | <a href="cascade.html#value-def-inherit" class="noxref"><span class="value-inst-inherit">inherit</span></a>
<tr valign=baseline><td><em>Initial:</em>&nbsp;&nbsp;<td>inline
<tr valign=baseline><td><em>Inherited:</em>&nbsp;&nbsp;<td>no
</table>
</dl>
</div>"#;
        let properties = extract_property_definitions(html);

        assert_eq!(properties.len(), 1);
        let prop = &properties[0];
        assert_eq!(prop.name, "display");
        assert_eq!(prop.id.as_deref(), Some("propdef-display"));
        assert!(prop.keywords.contains(&"inline-block".to_owned()));
        assert!(prop.keywords.contains(&"inherit".to_owned()));
        assert!(!prop.keywords.contains(&"run-in".to_owned()));
        assert_eq!(prop.initial.as_deref(), Some("inline"));
        assert_eq!(prop.inherited.as_deref(), Some("no"));
    }

    #[test]
    fn extracts_legacy_adef_dt_assignments() {
        let html = r#"
<dt id="RadialGradientElementFXAttribute">
  <span class="adef">fx</span> =
  "<span class="attr-value"><a>&lt;length&gt;</a></span>"
</dt>
"#;
        let properties = extract_legacy_adef_dt_assignments(html);

        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].name, "fx");
        assert_eq!(
            properties[0].id.as_deref(),
            Some("RadialGradientElementFXAttribute")
        );
        assert_eq!(properties[0].value.as_deref(), Some("<length>"));
    }

    #[test]
    fn extracts_bikeshed_element_attrdef_assignments() {
        let html = r#"
<dl>
  <dt data-md><dfn class="dfn-paneled" data-dfn-for="mask" data-dfn-type="element-attr" data-export id="element-attrdef-mask-x"><code>x</code></dfn> = "<a class="css production" data-link-type="type" href="https://drafts.csswg.org/css-values-4/#typedef-length-percentage">&lt;length-percentage></a>"
  <dd data-md><p>The x-axis coordinate.</p>
</dl>
"#;
        let properties = extract_property_definitions(html);

        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].name, "x");
        assert_eq!(properties[0].dfn_for.as_deref(), Some("mask"));
        assert_eq!(properties[0].id.as_deref(), Some("element-attrdef-mask-x"));
        assert_eq!(properties[0].value.as_deref(), Some("<length-percentage>"));
    }

    /// Filter attrdef values whose grammar is wrapped in inline markup
    /// (`<em>`, `<span>`, `<dfn><code>…`) must keep their real keyword/type
    /// grammar rather than collapsing to the wrapper tag name. The snippets are
    /// verbatim from drafts.csswg.org/filter-effects-1 (with whitespace trimmed
    /// to one attrdef per `<dt>`).
    #[test]
    fn markup_wrapped_attrdef_values_keep_their_grammar() {
        let html = r##"
<dl>
  <dt data-md><dfn class="dfn-paneled" data-dfn-for="feComposite" data-dfn-type="element-attr" data-export id="element-attrdef-fecomposite-operator"><code>operator</code></dfn> = "<span><dfn class="dfn-paneled" data-dfn-for="feComposite/operator" data-dfn-type="attr-value" data-export id="attr-valuedef-fecomposite-operator-over"><code>over</code></dfn> | <dfn data-dfn-type="attr-value" id="attr-valuedef-fecomposite-operator-in"><code>in</code></dfn> | <dfn data-dfn-type="attr-value" id="attr-valuedef-fecomposite-operator-out"><code>out</code></dfn> | <dfn data-dfn-type="attr-value" id="attr-valuedef-fecomposite-operator-atop"><code>atop</code></dfn> | <dfn data-dfn-type="attr-value" id="attr-valuedef-fecomposite-operator-xor"><code>xor</code></dfn> | <dfn data-dfn-type="attr-value" id="attr-valuedef-fecomposite-operator-lighter"><code>lighter</code></dfn> | <dfn data-dfn-type="attr-value" id="attr-valuedef-fecomposite-operator-arithmetic"><code>arithmetic</code></dfn></span>"
  <dd data-md><p>The compositing operation.</p>
  <dt data-md><dfn class="dfn-paneled" data-dfn-for="feDisplacementMap" data-dfn-type="element-attr" data-export id="element-attrdef-fedisplacementmap-xchannelselector"><code>xChannelSelector</code></dfn> = "<em>R | G | B | A</em>"
  <dd data-md><p>Which channel selects the x displacement.</p>
  <dt data-md><dfn class="dfn-paneled" data-dfn-for="feBlend" data-dfn-type="element-attr" data-export id="element-attrdef-feblend-in2"><code>in2</code></dfn> = "<em>(see <a data-link-type="element-attr" href="#element-attrdef-filter-primitive-in" id="ref-for-element-attrdef-filter-primitive-in">in</a> attribute)</em>"
  <dd data-md><p>The second input.</p>
  <dt data-md><dfn class="dfn-paneled" data-dfn-for="feTurbulence" data-dfn-type="element-attr" data-export id="element-attrdef-feturbulence-stitchtiles"><code>stitchTiles</code></dfn> = "<em>stitch | noStitch</em>"
  <dd data-md><p>Tile stitching behavior.</p>
  <dt data-md><dfn class="dfn-paneled" data-dfn-for="feConvolveMatrix" data-dfn-type="element-attr" data-export id="element-attrdef-feconvolvematrix-kernelmatrix"><code>kernelMatrix</code></dfn> = "<em>&lt;list of numbers></em>"
  <dd data-md><p>The convolution kernel.</p>
  <dt data-md><dfn class="dfn-paneled" data-dfn-for="filter" data-dfn-type="element-attr" data-export id="element-attrdef-filter-filterunits"><code>filterUnits</code></dfn> = "<dfn class="dfn-paneled" data-dfn-type="attr-value" id="attr-valuedef-filterunits-userspaceonuse"><code>userSpaceOnUse</code></dfn> | <dfn data-dfn-type="attr-value" id="attr-valuedef-filterunits-objectboundingbox"><code>objectBoundingBox</code></dfn>"
  <dd data-md><p>The filter region coordinate system.</p>
</dl>
"##;
        let values: std::collections::BTreeMap<_, _> = extract_property_definitions(html)
            .into_iter()
            .map(|property| (property.name, property.value))
            .collect();

        assert_eq!(
            values.get("operator").and_then(Option::as_deref),
            Some("over | in | out | atop | xor | lighter | arithmetic")
        );
        assert_eq!(
            values.get("xChannelSelector").and_then(Option::as_deref),
            Some("R | G | B | A")
        );
        assert_eq!(
            values.get("in2").and_then(Option::as_deref),
            Some("(see in attribute)")
        );
        assert_eq!(
            values.get("stitchTiles").and_then(Option::as_deref),
            Some("stitch | noStitch")
        );
        // The spec's `<list of numbers>` prose canonicalizes to a real CSS
        // number-list production rather than surviving as garbled keyword
        // tokens (`<list> of numbers>`) that would lose the list shape.
        assert_eq!(
            values.get("kernelMatrix").and_then(Option::as_deref),
            Some("<number>+")
        );
        assert_eq!(
            values.get("filterUnits").and_then(Option::as_deref),
            Some("userSpaceOnUse | objectBoundingBox")
        );
    }

    /// A whole-value `class="css production"` anchor resolves to its single
    /// type reference, while the spec's `list of <number>s` prose (and its
    /// paren-wrapped variant) keeps its list shape (canonicalized to `<number>+`)
    /// rather than being truncated to the bare `<number>` production by the
    /// anchor-extraction path.
    #[test]
    fn css_production_anchor_attrdef_values_resolve_to_type_refs() {
        let html = r##"
<dl>
  <dt data-md><dfn class="dfn-paneled" data-dfn-type="element-attr" data-export id="element-attrdef-filter-filterres"><code>filterRes</code></dfn> = "<a class="css production" data-link-type="type" href="#typedef-number-optional-number">&lt;number-optional-number></a>"
  <dd data-md><p>The filter resolution.</p>
  <dt data-md><dfn class="dfn-paneled" data-dfn-type="element-attr" data-export id="element-attrdef-fecolormatrix-values"><code>values</code></dfn> = "<em>list of <a class="css production" data-link-type="type" href="#typedef-number">&lt;number></a>s</em>"
  <dd data-md><p>The matrix values.</p>
  <dt data-md><dfn class="dfn-paneled" data-dfn-type="element-attr" data-export id="element-attrdef-fecomponenttransfer-tablevalues"><code>tableValues</code></dfn> = "<em>(list of <a class="css production" data-link-type="type" href="#typedef-number">&lt;number></a>s)</em>"
  <dd data-md><p>The transfer function table.</p>
  <dt data-md><dfn class="dfn-paneled" data-dfn-type="element-attr" data-export id="element-attrdef-mask-width"><code>width</code></dfn> = "auto | <a class="css production" data-link-type="type" href="#typedef-length-percentage">&lt;length-percentage></a>"
  <dd data-md><p>The mask width.</p>
</dl>
"##;
        let values: std::collections::BTreeMap<_, _> = extract_property_definitions(html)
            .into_iter()
            .map(|property| (property.name, property.value))
            .collect();

        assert_eq!(
            values.get("filterRes").and_then(Option::as_deref),
            Some("<number-optional-number>")
        );
        // `list of <number>s` (verbatim spec phrasing, plural trailing `s`)
        // keeps its list shape instead of truncating to the bare `<number>`
        // production that the `class="css production"` anchor path would extract.
        assert_eq!(
            values.get("values").and_then(Option::as_deref),
            Some("<number>+")
        );
        // The paren-wrapped variant (`(list of <number>s)`) canonicalizes too.
        assert_eq!(
            values.get("tableValues").and_then(Option::as_deref),
            Some("<number>+")
        );
        assert_eq!(
            values.get("width").and_then(Option::as_deref),
            Some("auto | <length-percentage>")
        );
    }

    #[test]
    fn expands_category_macro_in_description() -> Result<(), Box<dyn std::error::Error>> {
        let html = r"<dl class='definitions'>
  <dt><dfn id='c' data-dfn-type='dfn'>container element</dfn></dt>
  <dd>An element. Specifically: <edit:elementcategory name='container'/>.</dd>
</dl>";
        let mut macros = MacroIndex::default();
        macros.element_categories.insert(
            "container".to_owned(),
            vec!["svg".to_owned(), "g".to_owned(), "defs".to_owned()],
        );
        let ch = extract_chapter("struct", html, &macros)?;
        assert_eq!(ch.term_definitions.len(), 1);
        assert_eq!(
            ch.term_definitions[0].description,
            "An element. Specifically: svg, g, defs."
        );
        Ok(())
    }
}
