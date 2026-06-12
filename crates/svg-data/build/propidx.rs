//! Deterministic extractor for the SVG 1.1 property index (`propidx.html`).
//!
//! The W3C SVG 1.1 recommendations ship a "list of all properties" table whose
//! columns are `Name`, `Values`, `Initial value`, … For the *keyword* enum
//! grammars (`enum-display`, `enum-pointer-events`, …) the `Values` column is
//! the authoritative source: it is a CSS value-definition-syntax string such as
//! `nonzero | evenodd | inherit`. This module parses that table into
//! `(property-name, values-syntax)` rows; [`value_syntax`](super::value_syntax)
//! turns each row into an ordered keyword set (or rejects it as a non-enum).
//!
//! Both vendored snapshots are supported despite differing markup:
//!
//! - **REC 2003-01-14** — the name cell carries `span.propinst-<name>` and the
//!   property name is rendered between straight quotes (`'display'`).
//! - **REC 2011-08-16** — the name cell carries `span.prop-name` and the
//!   property name is rendered between typographic quotes (`‘display’`,
//!   U+2018/U+2019).
//!
//! Both forms are handled uniformly by reading the *text* of the first two
//! `<td>` cells of every `<tbody>` row and stripping the surrounding quotes
//! from the name, so the extractor never depends on the per-edition class
//! scheme. The parse is pure and offline: it operates on a vendored HTML string
//! and performs no I/O.

use tl::{Node, NodeHandle, Parser, ParserOptions, VDom};

/// A single property-index row: the property name and the raw text of its
/// `Values` column (HTML entities still encoded, e.g. `&lt;length&gt;`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropIdxRow {
    /// Property name with surrounding quotes stripped (e.g. `pointer-events`).
    pub name: String,
    /// Raw inner text of the `Values` cell, whitespace-collapsed; HTML entities
    /// are left encoded for the value-syntax tokenizer to decode.
    pub values: String,
}

/// Parse a vendored `propidx.html` into property-index rows.
///
/// Returns one [`PropIdxRow`] per `<tbody> <tr>` whose first cell yields a
/// non-empty property name. Rows without at least two `<td>` cells, or whose
/// name cell is empty after quote-stripping, are skipped — they are table
/// chrome, not properties.
pub fn parse_propidx(html: &str) -> Result<Vec<PropIdxRow>, String> {
    // Normalize away `<br>` line breaks before parsing. In the `Values` column a
    // `<br/>` is purely visual (it wraps a long `a | b | c` alternation across
    // two rendered lines) and carries no grammar meaning. It also breaks tl's
    // lenient HTML5 reconstruction: a mid-cell `<br/>` causes the text *after*
    // it — and the row's remaining `<td>` cells — to be detached from the cell,
    // corrupting extraction for `pointer-events`, `display`, `font-stretch`, …
    // Replacing each `<br …>` with a space yields the same value definition
    // (alternatives are whitespace-insensitive) and lets the rows parse cleanly.
    let normalized = strip_br_tags(html);
    let dom = tl::parse(&normalized, ParserOptions::default())
        .map_err(|err| format!("propidx: HTML parse failed: {err}"))?;
    let parser = dom.parser();

    // The rows are collected by a recursive `tr` query scoped to the property
    // table's `<tbody>` rather than by walking direct children. The 2011 markup
    // is single-line and tl's lenient HTML5 nesting parses most rows as *nested*
    // inside a preceding row's `<td>`, so only ~20 of 60 rows are direct
    // children of `<tbody>`. A recursive query recovers all rows in document
    // order; each `tr`'s own **direct** `td` children are still its own cells.
    // Scoping to `<tbody>` also drops the `<thead>` header row for free.
    let row_handles = tbody_row_handles(&dom, parser)
        .ok_or_else(|| "propidx: no <tbody> found in property table".to_string())?;

    let mut rows = Vec::new();
    for tr in row_handles {
        let cells: Vec<NodeHandle> = direct_children(tr, parser, "td");
        let (Some(name_cell), Some(values_cell)) = (cells.first(), cells.get(1)) else {
            continue;
        };
        let name = clean_property_name(&cell_text(*name_cell, parser));
        if name.is_empty() {
            continue;
        }
        let values = collapse_ws(&bounded_cell_text(*values_cell, parser));
        rows.push(PropIdxRow { name, values });
    }

    if rows.is_empty() {
        return Err("propidx: property table produced zero rows".to_string());
    }
    Ok(rows)
}

/// Collect every `<tr>` handle inside the property table's first `<tbody>`, in
/// document order. Returns `None` if the page has no `<tbody>`.
fn tbody_row_handles<'dom>(
    dom: &'dom VDom<'dom>,
    parser: &'dom Parser<'dom>,
) -> Option<Vec<NodeHandle>> {
    let tbody = dom.query_selector("tbody")?.next()?;
    let tag = tbody.get(parser)?.as_tag()?;
    let rows = tag
        .query_selector(parser, "tr")?
        .collect::<Vec<NodeHandle>>();
    Some(rows)
}

/// Collect the direct element children of `parent` whose tag name matches
/// `tag_name` (ASCII case-insensitive), in order.
fn direct_children(parent: NodeHandle, parser: &Parser<'_>, tag_name: &str) -> Vec<NodeHandle> {
    let Some(tag) = parent.get(parser).and_then(Node::as_tag) else {
        return Vec::new();
    };
    tag.children()
        .top()
        .as_slice()
        .iter()
        .copied()
        .filter(|handle| {
            handle
                .get(parser)
                .and_then(Node::as_tag)
                .is_some_and(|child| child.name().as_utf8_str().eq_ignore_ascii_case(tag_name))
        })
        .collect()
}

/// Inner text of a cell node. `tl` leaves HTML entities encoded, which is
/// exactly what the value-syntax tokenizer expects (so `&lt;color&gt;` stays
/// distinguishable from bare keyword text).
fn cell_text(handle: NodeHandle, parser: &Parser<'_>) -> String {
    handle
        .get(parser)
        .map(|node| node.inner_text(parser).into_owned())
        .unwrap_or_default()
}

/// Text of a cell, **stopping at the first nested `<tr>`**.
///
/// The 2011 markup contains `<br/>` inside several `Values` cells (e.g.
/// `pointer-events`, `display`). tl's lenient HTML5 parser mis-nests the rows
/// that *follow* such a cell **inside** it, so a naive [`cell_text`] /
/// `inner_text` would splice the entire rest of the table into the cell. This
/// walker accumulates text in document order but halts at the first descendant
/// `<tr>` — the boundary of the genuine cell content — so the captured `Values`
/// string is exactly the property's own value definition.
fn bounded_cell_text(handle: NodeHandle, parser: &Parser<'_>) -> String {
    let mut out = String::new();
    let mut stopped = false;
    append_text_until_row(handle, parser, &mut out, &mut stopped);
    out
}

/// Depth-first text accumulation that stops at the first `<tr>`. `stopped` short
/// -circuits the rest of the traversal once a row boundary is hit.
fn append_text_until_row(
    handle: NodeHandle,
    parser: &Parser<'_>,
    out: &mut String,
    stopped: &mut bool,
) {
    if *stopped {
        return;
    }
    let Some(node) = handle.get(parser) else {
        return;
    };
    match node {
        Node::Raw(bytes) => out.push_str(&bytes.as_utf8_str()),
        Node::Comment(_) => {}
        Node::Tag(tag) => {
            if tag.name().as_utf8_str().eq_ignore_ascii_case("tr") {
                *stopped = true;
                return;
            }
            for &child in tag.children().top().as_slice() {
                append_text_until_row(child, parser, out, stopped);
                if *stopped {
                    return;
                }
            }
        }
    }
}

/// Replace every `<br …>` / `<br/>` / `<br>` tag (any casing) with a single
/// space. Deterministic single pass over the bytes; non-`<br` `<` characters are
/// preserved verbatim.
fn strip_br_tags(html: &str) -> String {
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'<' && is_br_at(bytes, idx) {
            // Skip to and past the closing `>`.
            if let Some(offset) = bytes[idx..].iter().position(|&byte| byte == b'>') {
                out.push(' ');
                idx += offset + 1;
            } else {
                // Unterminated `<br` — emit the rest unchanged and stop.
                out.push_str(&html[idx..]);
                break;
            }
        } else {
            // Push one full UTF-8 char so multibyte content stays intact. `idx`
            // is always on a char boundary and `idx < bytes.len()`, so a char
            // always exists; `\0` is a neutral fallback if that invariant ever
            // breaks.
            let ch = html[idx..].chars().next().unwrap_or('\0');
            out.push(ch);
            idx += ch.len_utf8();
        }
    }
    out
}

/// `true` if a `<br` tag opens at `idx` (i.e. `<br` followed by `>`, `/`, or
/// ASCII whitespace — so it is the `br` element, not `<break>` or similar).
///
/// Inspects raw bytes (`bytes`/`rest`) rather than chars: `bytes` is the
/// UTF-8-validated source's `as_bytes()`, and every comparison here is against
/// ASCII (`b'b'`, `b'r'`, delimiters). Multibyte UTF-8 lead/continuation bytes
/// are all ≥ 0x80, so they can never match these ASCII checks — the byte-level
/// scan is intentional and safe, not a latent multibyte bug.
fn is_br_at(bytes: &[u8], idx: usize) -> bool {
    let rest = &bytes[idx..];
    if rest.len() < 3 {
        return false;
    }
    rest[1].eq_ignore_ascii_case(&b'b')
        && rest[2].eq_ignore_ascii_case(&b'r')
        && rest
            .get(3)
            .is_none_or(|&byte| byte == b'>' || byte == b'/' || byte.is_ascii_whitespace())
}

/// Collapse all runs of ASCII/Unicode whitespace to single spaces and trim.
fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Strip the quotes the spec renders around property names and collapse
/// whitespace. Handles straight (`'`), typographic (`‘ ’`), and any leading
/// `&nbsp;` artifacts. Returns an empty string for non-property cells.
fn clean_property_name(text: &str) -> String {
    collapse_ws(text)
        .trim_matches(|ch: char| {
            ch == '\'' || ch == '\u{2018}' || ch == '\u{2019}' || ch.is_whitespace()
        })
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_encoded_entities_while_stripping_quotes() {
        assert_eq!(clean_property_name(" 'display' "), "display");
        assert_eq!(
            clean_property_name("\u{2018}pointer-events\u{2019}"),
            "pointer-events"
        );
        // `tl::inner_text` leaves entities encoded, so a literal `&nbsp;` string
        // (not a real U+00A0) has no quote/whitespace chars to trim and is
        // preserved verbatim — only actual whitespace, incl. U+00A0, is stripped.
        assert_eq!(clean_property_name("&nbsp;"), "&nbsp;");
    }

    #[test]
    fn parses_minimal_table() {
        let html = "<table><tbody>\
            <tr><td><span class=\"prop-name\">\u{2018}fill-rule\u{2019}</span></td>\
            <td>nonzero | evenodd | inherit</td><td>nonzero</td></tr>\
            </tbody></table>";
        let rows = parse_propidx(html).unwrap_or_else(|err| panic!("parse failed: {err}"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "fill-rule");
        assert_eq!(rows[0].values, "nonzero | evenodd | inherit");
    }
}
