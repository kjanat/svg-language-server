//! Extract permalink anchors, term definitions, and example references from a
//! chapter or appendix HTML page.
//!
//! Chapter source HTML carries the prose, the `id` anchors that element and
//! attribute hrefs point at, the `<dfn>` term definitions, and `<edit:example>`
//! references. (The rendered element-summary tables are injected at publish
//! time from `definitions.xml`, so the structural content model is extracted
//! from there, not here.) This module turns one page into a typed record.

use std::borrow::Cow;

use serde::Serialize;
use tl::{HTMLTag, Parser, ParserOptions};

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
}

/// Extract anchors, definitions, and examples from a chapter's HTML.
///
/// # Errors
/// Returns an error if the HTML cannot be parsed.
pub fn extract_chapter(name: &str, html: &str) -> Fallible<Chapter> {
    let dom = tl::parse(html, ParserOptions::default())?;
    let parser = dom.parser();
    let mut chapter = Chapter {
        name: name.to_owned(),
        anchors: Vec::new(),
        dfns: Vec::new(),
        examples: Vec::new(),
        properties: Vec::new(),
        term_definitions: Vec::new(),
    };

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
            "table" if has_class(tag, "propdef") => {
                if let Some(property) = extract_propdef(tag, parser) {
                    chapter.properties.push(property);
                }
            }
            "dl" if has_class(tag, "definitions") => {
                extract_definition_list(tag, parser, &mut chapter.term_definitions);
            }
            _ => {}
        }
    }

    Ok(chapter)
}

/// Pair the `<dt>`/`<dd>` children of a definition list into term definitions,
/// walking direct children in document order so each term keeps its own
/// description.
fn extract_definition_list(dl: &HTMLTag, parser: &Parser, out: &mut Vec<TermDefinition>) {
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
                        description: normalize_ws(&child.inner_text(parser)),
                    });
                }
            }
            _ => {}
        }
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
        let Some(label) = first_text(row, parser, "th") else {
            continue;
        };
        let cell = first_text(row, parser, "td").unwrap_or_default();
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

/// Whether a value token is a bare keyword (letters, digits, hyphens only).
fn is_keyword_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// Whether `tag` carries `class` among its space-separated class list.
fn has_class(tag: &HTMLTag, class: &str) -> bool {
    attr(tag, "class").is_some_and(|classes| classes.split_whitespace().any(|each| each == class))
}

/// The normalized inner text of the first descendant matching `selector`.
fn first_text(tag: &HTMLTag, parser: &Parser, selector: &str) -> Option<String> {
    let handle = tag.query_selector(parser, selector)?.next()?;
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

/// Decode HTML entities, then collapse whitespace runs into single spaces.
///
/// `tl`'s `inner_text` returns text verbatim (entities undecoded), so value
/// grammars come back as `auto | &lt;length-percentage&gt;`; this restores them
/// to `auto | <length-percentage>`.
fn normalize_ws(text: &str) -> String {
    decode_entities(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Decode the HTML entities the spec text uses (named basics plus numeric
/// references) in a single pass, leaving unrecognized `&...;` runs verbatim.
fn decode_entities(input: &str) -> Cow<'_, str> {
    if !input.contains('&') {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        if let Some(semi) = after.find(';')
            && let Some(decoded) = decode_entity(&after[1..semi])
        {
            out.push(decoded);
            rest = &after[semi + 1..];
            continue;
        }
        out.push('&');
        rest = &after[1..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Decode one entity body (the text between `&` and `;`).
fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{00A0}'),
        _ => {
            let code = entity.strip_prefix('#')?;
            let value = match code.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => code.parse().ok()?,
            };
            char::from_u32(value)
        }
    }
}
