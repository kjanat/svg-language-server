//! Deterministic extractor for the published SVG 2 Candidate Recommendation
//! (`CR-SVG2-20181004`) index pages.
//!
//! Unlike the SVG 2 Editor's Draft, whose machine-readable grammar lives in
//! `definitions*.xml`, the dated CR has no such vendored grammar. Its
//! authoritative machine-readable artifacts are the *rendered* appendix index
//! tables the W3C ships with the publication:
//!
//! - **`eltindex.html`** — Appendix F element index: a `<ul class="element-index">`
//!   whose every `<li>` names one element (`<span class="element-name">…<span>NAME</span></span>`).
//! - **`attindex.html`** — the attribute index `<table class="attrtable">`: one
//!   `<tbody>` row per attribute scope, with three cells —
//!   `<th>` the attribute name (`<span class="attr-name">…<span>NAME</span></span>`),
//!   a `<td>` of the comma-separated elements the attribute may be specified on
//!   (each `<span class="element-name">…<span>ELEM</span></span>`), and a `<td>`
//!   carrying a `✓` (U+2713) iff the attribute is animatable.
//! - **`propidx.html`** — the property index `<table class="proptable">`: one
//!   `<tbody>` row per property, with the property name in a `<th>`
//!   (`<a class="property">NAME</a>`) and the CSS value-definition syntax in the
//!   first `<td>`. Reused by [`value_syntax`](super::value_syntax) to recover
//!   pure-keyword enums, exactly as `build/propidx.rs` does for SVG 1.1.
//!
//! The parse is pure and offline: it operates on the vendored HTML strings and
//! performs no I/O. It is deliberately faithful to *what the rendered index
//! exposes* — notably, the index does **not** carry the `attributecategory`
//! groups the ED `definitions*.xml` does, so the CR inventory's attributes are
//! classified only where the index itself marks them (it does not), and the
//! animatable flag is recorded as raw provenance instead. See
//! [`crate::inventory`] for how this feeds the baked CR inventory.

use std::collections::{BTreeMap, BTreeSet};

use tl::{Node, NodeHandle, Parser, ParserOptions, VDom};

/// One attribute scope row from `attindex.html`: the attribute name, the
/// elements it may be specified on (in document order), and whether the row's
/// animatable cell carried a `✓`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttIndexRow {
    /// Attribute name, e.g. `fill` or `aria-label`.
    pub name: String,
    /// Elements the attribute may be specified on, in document order.
    pub elements: Vec<String>,
    /// `true` iff the animatable cell carried a `✓`.
    pub animatable: bool,
}

/// One property row from `propidx.html`: the property name and the raw text of
/// its `Values` cell (HTML entities left encoded for the value tokenizer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropIndexRow {
    /// Property name, e.g. `fill` or `pointer-events`.
    pub name: String,
    /// Raw inner text of the `Values` cell, whitespace-collapsed; HTML entities
    /// (`&lt;`, `&gt;`) are left encoded for [`super::value_syntax`].
    pub values: String,
}

/// The merged element-x-attribute inventory derived from `eltindex.html` +
/// `attindex.html`.
///
/// Attribute rows that share a name (an attribute listed under several scopes —
/// the index lists, e.g., a presentation attribute once per applicability set)
/// are merged: their element scopes are unioned and their animatable flags
/// OR-ed, so each distinct attribute name yields exactly one [`AttrFacts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrInventory {
    /// Every element named in `eltindex.html`, sorted.
    pub elements: BTreeSet<String>,
    /// Per-attribute facts, keyed by attribute name (sorted by the map).
    pub attributes: BTreeMap<String, AttrFacts>,
    /// Every resolved `(element, attribute)` edge, sorted and de-duplicated.
    pub edges: BTreeSet<(String, String)>,
}

/// Merged facts for one attribute name across all its `attindex.html` rows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AttrFacts {
    /// Union of every element scope the attribute appeared under, sorted.
    pub element_scope: BTreeSet<String>,
    /// `true` iff *any* row for this attribute marked it animatable.
    pub animatable: bool,
}

/// Parse `eltindex.html` into the sorted set of element names.
///
/// # Errors
///
/// Returns an error string if the HTML fails to parse or the element-index list
/// yields zero elements.
pub fn parse_eltindex(html: &str) -> Result<BTreeSet<String>, String> {
    let dom = tl::parse(html, ParserOptions::default())
        .map_err(|err| format!("eltindex: HTML parse failed: {err}"))?;
    let parser = dom.parser();

    let mut elements = BTreeSet::new();
    for ul in query_all(&dom, parser, "ul") {
        let Some(ul_tag) = ul.get(parser).and_then(Node::as_tag) else {
            continue;
        };
        if !has_class(ul_tag, "element-index") {
            continue;
        }
        let Some(spans) = ul_tag.query_selector(parser, "span") else {
            continue;
        };
        for handle in spans {
            let Some(tag) = handle.get(parser).and_then(Node::as_tag) else {
                continue;
            };
            if !has_class(tag, "element-name") {
                continue;
            }
            if let Some(name) = inner_element_name(handle, parser) {
                elements.insert(name);
            }
        }
    }

    if elements.is_empty() {
        return Err("eltindex: element index produced zero elements".to_string());
    }
    Ok(elements)
}

/// Parse `attindex.html` into per-attribute scope rows, in document order.
///
/// # Errors
///
/// Returns an error string if the HTML fails to parse or the attribute table
/// yields zero rows.
pub fn parse_attindex(html: &str) -> Result<Vec<AttIndexRow>, String> {
    let dom = tl::parse(html, ParserOptions::default())
        .map_err(|err| format!("attindex: HTML parse failed: {err}"))?;
    let parser = dom.parser();

    let row_handles = tbody_rows(&dom, parser)
        .ok_or_else(|| "attindex: no <tbody> found in attribute table".to_string())?;

    let mut rows = Vec::new();
    for tr in row_handles {
        // The attribute name lives in the row's `<th>`; the element scope and
        // animatable flag in its two `<td>` cells. A row missing the `<th>` is
        // table chrome, not an attribute.
        let header_cells = direct_children(tr, parser, "th");
        let Some(name_cell) = header_cells.first() else {
            continue;
        };
        let Some(name) = attr_name_in(*name_cell, parser) else {
            continue;
        };
        let data_cells = direct_children(tr, parser, "td");
        let elements = data_cells
            .first()
            .map(|cell| element_names_in(*cell, parser))
            .unwrap_or_default();
        let animatable = data_cells
            .get(1)
            .is_some_and(|cell| cell_has_checkmark(*cell, parser));
        rows.push(AttIndexRow {
            name,
            elements,
            animatable,
        });
    }

    if rows.is_empty() {
        return Err("attindex: attribute table produced zero rows".to_string());
    }
    Ok(rows)
}

/// Parse `propidx.html` into property rows.
///
/// The CR property index puts the property name in a `<th>` (`<a class="property">`)
/// and the value definition in the row's first `<td>`. The value cell may wrap
/// datatype tokens in `<a>` links (e.g. `<a …>&lt;percentage></a>`); the inner
/// text is taken verbatim (tags stripped, entities preserved) so
/// [`super::value_syntax::keyword_enum`] sees the same `&lt;…>` tokens it does
/// for SVG 1.1.
///
/// # Errors
///
/// Returns an error string if the HTML fails to parse or the property table
/// yields zero rows.
pub fn parse_propidx(html: &str) -> Result<Vec<PropIndexRow>, String> {
    let dom = tl::parse(html, ParserOptions::default())
        .map_err(|err| format!("propidx: HTML parse failed: {err}"))?;
    let parser = dom.parser();

    let row_handles = tbody_rows(&dom, parser)
        .ok_or_else(|| "propidx: no <tbody> found in property table".to_string())?;

    let mut rows = Vec::new();
    for tr in row_handles {
        let Some(name) = property_name_in(tr, parser) else {
            continue;
        };
        let values_cell = direct_children(tr, parser, "td");
        let Some(values_cell) = values_cell.first() else {
            continue;
        };
        let values = collapse_ws(&cell_text(*values_cell, parser));
        if values.is_empty() {
            continue;
        }
        rows.push(PropIndexRow { name, values });
    }

    if rows.is_empty() {
        return Err("propidx: property table produced zero rows".to_string());
    }
    Ok(rows)
}

/// Build the merged CR element-x-attribute inventory from the element index and
/// the attribute index rows.
///
/// Attribute rows sharing a name are merged (scopes unioned, animatable flags
/// OR-ed). Every `(element, attribute)` pair an attribute row names becomes an
/// edge. The returned [`CrInventory::elements`] is exactly the `eltindex.html`
/// set; the attribute-index also references this same set (verified by the
/// inventory test's no-dangling-edge assertion).
pub fn build_inventory(
    elements: BTreeSet<String>,
    rows: &[AttIndexRow],
) -> Result<CrInventory, String> {
    let mut attributes: BTreeMap<String, AttrFacts> = BTreeMap::new();
    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
    for row in rows {
        let facts = attributes.entry(row.name.clone()).or_default();
        facts.animatable |= row.animatable;
        for element in &row.elements {
            if !elements.contains(element) {
                return Err(format!(
                    "tr_index: attribute `{}` references unknown element `{element}`",
                    row.name
                ));
            }
            facts.element_scope.insert(element.clone());
            edges.insert((element.clone(), row.name.clone()));
        }
    }
    Ok(CrInventory {
        elements,
        attributes,
        edges,
    })
}

/// Recursively collect every element handle matching `selector`, in document
/// order. tl's lenient HTML5 reconstruction nests rows unexpectedly, so a
/// recursive query (not a direct-children walk) is required to recover them all.
fn query_all<'dom>(
    dom: &'dom VDom<'dom>,
    parser: &'dom Parser<'dom>,
    selector: &str,
) -> Vec<NodeHandle> {
    dom.query_selector(selector)
        .map_or_else(Vec::new, Iterator::collect::<Vec<NodeHandle>>)
        .into_iter()
        .filter(|handle: &NodeHandle| handle.get(parser).and_then(Node::as_tag).is_some())
        .collect()
}

/// Collect every `<tr>` handle inside the page's first index-table `<tbody>`, in
/// document order. Returns `None` if the page has no matching index table.
fn tbody_rows<'dom>(dom: &'dom VDom<'dom>, parser: &'dom Parser<'dom>) -> Option<Vec<NodeHandle>> {
    let table = dom.query_selector("table")?.find(|handle| {
        handle
            .get(parser)
            .and_then(Node::as_tag)
            .is_some_and(|tag| has_class(tag, "attrtable") || has_class(tag, "proptable"))
    })?;
    let table_tag = table.get(parser)?.as_tag()?;
    let tbody = table_tag.query_selector(parser, "tbody")?.next()?;
    let tag = tbody.get(parser)?.as_tag()?;
    Some(
        tag.query_selector(parser, "tr")?
            .collect::<Vec<NodeHandle>>(),
    )
}

/// Direct element children of `parent` whose tag name matches `tag_name`
/// (ASCII case-insensitive), in order.
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

/// `true` iff `tag`'s `class` attribute contains the whitespace-delimited
/// `class_name`.
fn has_class(tag: &tl::HTMLTag<'_>, class_name: &str) -> bool {
    tag.attributes()
        .class()
        .map(|value| value.as_utf8_str())
        .is_some_and(|classes| classes.split_whitespace().any(|cls| cls == class_name))
}

/// Innermost element name for a `span.element-name` / `span.attr-name` handle:
/// the index renders the bare name inside an inner `<span>` (under the link),
/// so the *last* descendant `<span>`'s text is the clean name without the
/// surrounding typographic quotes.
fn inner_element_name(handle: NodeHandle, parser: &Parser<'_>) -> Option<String> {
    let tag = handle.get(parser).and_then(Node::as_tag)?;
    let inner = tag
        .query_selector(parser, "span")?
        .filter_map(|child| child.get(parser).and_then(Node::as_tag))
        .last()?;
    let text = collapse_ws(&inner.inner_text(parser));
    if text.is_empty() { None } else { Some(text) }
}

/// Attribute name carried by a `<th>` cell: the text of the inner `<span>`
/// nested under `span.attr-name`.
fn attr_name_in(cell: NodeHandle, parser: &Parser<'_>) -> Option<String> {
    let tag = cell.get(parser).and_then(Node::as_tag)?;
    for span in tag.query_selector(parser, "span")? {
        let Some(span_tag) = span.get(parser).and_then(Node::as_tag) else {
            continue;
        };
        if has_class(span_tag, "attr-name") {
            return inner_element_name(span, parser);
        }
    }
    None
}

/// Element names listed in a `<td>` scope cell, in document order. Each element
/// is a `span.element-name`; duplicates within the cell are collapsed by the
/// caller's edge set, but order is preserved here for the row record.
fn element_names_in(cell: NodeHandle, parser: &Parser<'_>) -> Vec<String> {
    let Some(tag) = cell.get(parser).and_then(Node::as_tag) else {
        return Vec::new();
    };
    let Some(spans) = tag.query_selector(parser, "span") else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for span in spans {
        let Some(span_tag) = span.get(parser).and_then(Node::as_tag) else {
            continue;
        };
        if !has_class(span_tag, "element-name") {
            continue;
        }
        if let Some(name) = inner_element_name(span, parser) {
            names.push(name);
        }
    }
    names
}

/// `true` iff the cell's text contains the `✓` (U+2713) animatable marker.
fn cell_has_checkmark(cell: NodeHandle, parser: &Parser<'_>) -> bool {
    cell.get(parser)
        .map(|node| node.inner_text(parser))
        .is_some_and(|text| text.contains('\u{2713}'))
}

/// Property name in a propidx row's `<th><a class="property">…</a>`.
fn property_name_in(tr: NodeHandle, parser: &Parser<'_>) -> Option<String> {
    let header = direct_children(tr, parser, "th");
    let header = header.first()?;
    let tag = header.get(parser).and_then(Node::as_tag)?;
    for anchor in tag.query_selector(parser, "a")? {
        let Some(anchor_tag) = anchor.get(parser).and_then(Node::as_tag) else {
            continue;
        };
        if has_class(anchor_tag, "property") {
            let text = collapse_ws(&anchor_tag.inner_text(parser));
            return if text.is_empty() { None } else { Some(text) };
        }
    }
    None
}

/// Inner text of a cell. tl leaves HTML entities encoded, which is exactly what
/// [`super::value_syntax`] expects for `&lt;datatype>` tokens.
fn cell_text(handle: NodeHandle, parser: &Parser<'_>) -> String {
    handle
        .get(parser)
        .map(|node| node.inner_text(parser).into_owned())
        .unwrap_or_default()
}

/// Collapse all runs of whitespace to single spaces and trim. Also strips the
/// typographic single quotes (U+2018/U+2019) the index wraps names in.
fn collapse_ws(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|ch: char| ch == '\u{2018}' || ch == '\u{2019}' || ch == '\'')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eltindex_extracts_names() {
        let html = "<ul class=\"element-index\">\
            <li><span class=\"element-name\">\u{2018}<a href=\"#\"><span>rect</span></a>\u{2019}</span></li>\
            <li><span class=\"element-name\">\u{2018}<a href=\"#\"><span>circle</span></a>\u{2019}</span></li>\
            </ul>";
        let elements = parse_eltindex(html).unwrap_or_else(|err| panic!("{err}"));
        assert!(elements.contains("rect"));
        assert!(elements.contains("circle"));
        assert_eq!(elements.len(), 2);
    }

    #[test]
    fn attindex_extracts_scope_and_animatable() {
        let html = "<table class=\"attrtable\"><tbody>\
            <tr><th><span class=\"attr-name\"><a href=\"#\"><span>fill</span></a></span></th>\
            <td><span class=\"element-name\"><a href=\"#\"><span>circle</span></a></span>, \
            <span class=\"element-name\"><a href=\"#\"><span>rect</span></a></span></td>\
            <td>\u{2713}</td></tr>\
            </tbody></table>";
        let rows = parse_attindex(html).unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "fill");
        assert_eq!(rows[0].elements, vec!["circle", "rect"]);
        assert!(rows[0].animatable);
    }

    #[test]
    fn attindex_empty_animatable_cell_is_not_animatable() {
        let html = "<table class=\"attrtable\"><tbody>\
            <tr><th><span class=\"attr-name\"><a href=\"#\"><span>id</span></a></span></th>\
            <td><span class=\"element-name\"><a href=\"#\"><span>rect</span></a></span></td>\
            <td></td></tr>\
            </tbody></table>";
        let rows = parse_attindex(html).unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].animatable);
    }

    #[test]
    fn propidx_extracts_name_and_values() {
        let html = "<table class=\"proptable\"><tbody>\
            <tr><th><a class=\"property\" href=\"#\">fill-rule</a></th>\
            <td>nonzero | evenodd</td><td>nonzero</td></tr>\
            </tbody></table>";
        let rows = parse_propidx(html).unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "fill-rule");
        assert_eq!(rows[0].values, "nonzero | evenodd");
    }

    #[test]
    fn build_inventory_merges_rows_and_collects_edges() {
        let rows = vec![
            AttIndexRow {
                name: "fill".to_string(),
                elements: vec!["circle".to_string()],
                animatable: false,
            },
            AttIndexRow {
                name: "fill".to_string(),
                elements: vec!["rect".to_string()],
                animatable: true,
            },
        ];
        let elements: BTreeSet<String> = ["circle".to_string(), "rect".to_string()]
            .into_iter()
            .collect();
        let inventory = build_inventory(elements, &rows).unwrap_or_else(|err| panic!("{err}"));
        let fill = inventory
            .attributes
            .get("fill")
            .unwrap_or_else(|| panic!("fill should be present"));
        assert!(fill.animatable, "animatable flag should OR across rows");
        assert_eq!(fill.element_scope.len(), 2);
        assert_eq!(inventory.edges.len(), 2);
    }
}
