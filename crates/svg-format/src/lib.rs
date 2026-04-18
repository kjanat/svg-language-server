//! Deterministic structural formatting for SVG documents.
//!
//! # Examples
//!
//! ```rust
//! let input = r#"<svg><rect x="0"  y="0" /></svg>"#;
//! let formatted = svg_format::format(input);
//! assert!(formatted.contains("<rect"));
//! ```

mod tag_parse;
mod text_content;

use tag_parse::{
    ParsedAttribute, ParsedAttributeValue, ParsedTag, canonical_group_key, parse_tag,
    reorder_attributes,
};
use text_content::{
    collapse_whitespace, decode_xml_entities, dedent_block, encode_xml_entities,
    is_text_content_element, normalize_text_content_with_entities, strip_cdata_wrapper,
};
use tree_sitter::{Node, Parser};

/// Attribute ordering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "kebab-case"))]
pub enum AttributeSort {
    /// Keep original source order.
    None,
    /// SVG-aware canonical grouping/order.
    #[default]
    Canonical,
    /// Sort attributes alphabetically by name.
    Alphabetical,
}

/// Attribute wrapping mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "kebab-case"))]
pub enum AttributeLayout {
    /// Wrap only when inline width exceeds threshold (or source was already multiline).
    #[default]
    Auto,
    /// Always keep attributes in one line.
    SingleLine,
    /// Always wrap attributes into multiple lines (if any attributes exist).
    MultiLine,
}

/// Quoting strategy for attribute values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "kebab-case"))]
pub enum QuoteStyle {
    /// Preserve original quote style where present.
    #[default]
    Preserve,
    /// Normalize quoted values to double quotes.
    Double,
    /// Normalize quoted values to single quotes.
    Single,
}

/// Indentation strategy for wrapped attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "kebab-case"))]
pub enum WrappedAttributeIndent {
    /// Add one normal indentation unit.
    OneLevel,
    /// Align to the column after `<tag ` so wrapped attributes line up visually.
    #[default]
    AlignToTagName,
}

/// How blank lines between sibling elements are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "kebab-case"))]
pub enum BlankLines {
    /// Strip all blank lines between siblings.
    Remove,
    /// Keep blank lines from source verbatim.
    Preserve,
    /// Collapse 2+ blank lines to exactly 1.
    #[default]
    Truncate,
    /// Force exactly 1 blank line between every sibling.
    Insert,
}

/// How the formatter handles whitespace in text nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "kebab-case"))]
pub enum TextContentMode {
    /// Collapse runs of whitespace into single spaces, trim lines, skip blanks.
    Collapse,
    /// Preserve content structure; dedent then re-indent to SVG depth.
    #[default]
    Maintain,
    /// Trim each line, remove blank lines, re-indent to SVG depth.
    Prettify,
}

/// The language of embedded content found within an SVG element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedLanguage {
    /// CSS inside `<style>`.
    Css,
    /// JavaScript inside `<script>`.
    JavaScript,
    /// HTML/XHTML inside `<foreignObject>`.
    Html,
}

/// A request to format embedded content within an SVG document.
pub struct EmbeddedContent<'a> {
    /// The language of the embedded content.
    pub language: EmbeddedLanguage,
    /// The raw content text (common indent removed).
    pub content: &'a str,
    /// The nesting depth in the SVG tree where this content lives.
    pub indent_depth: usize,
}

/// Formatter configuration for SVG pretty-printing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatOptions {
    /// Number of spaces per indentation level when `insert_spaces` is true.
    pub indent_width: usize,
    /// Whether indentation should use spaces (true) or tabs (false).
    pub insert_spaces: bool,
    /// Maximum inline tag width before switching to multi-line attributes.
    pub max_inline_tag_width: usize,
    /// Attribute ordering mode.
    pub attribute_sort: AttributeSort,
    /// Attribute wrapping mode.
    pub attribute_layout: AttributeLayout,
    /// Maximum number of attributes emitted per wrapped line.
    pub attributes_per_line: usize,
    /// Emit a space before `/>` in self-closing tags.
    pub space_before_self_close: bool,
    /// Preferred quote style for attribute values.
    pub quote_style: QuoteStyle,
    /// Indentation style for wrapped attributes.
    pub wrapped_attribute_indent: WrappedAttributeIndent,
    /// How text-node whitespace is handled.
    pub text_content: TextContentMode,
    /// How blank lines between sibling elements are handled.
    pub blank_lines: BlankLines,
    /// Comment prefixes that trigger ignore directives.
    ///
    /// For each prefix `p`, the formatter recognizes:
    /// - `<!-- p-ignore -->` — skip formatting the next sibling
    /// - `<!-- p-ignore-file -->` — skip the entire file (detected anywhere)
    /// - `<!-- p-ignore-start -->` / `<!-- p-ignore-end -->` — skip a range
    ///
    /// Defaults to `["svg-format"]`.
    pub ignore_prefixes: Vec<String>,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            indent_width: 2,
            insert_spaces: true,
            max_inline_tag_width: 100,
            attribute_sort: AttributeSort::Canonical,
            attribute_layout: AttributeLayout::Auto,
            attributes_per_line: 1,
            space_before_self_close: true,
            quote_style: QuoteStyle::Preserve,
            wrapped_attribute_indent: WrappedAttributeIndent::AlignToTagName,
            text_content: TextContentMode::Maintain,
            blank_lines: BlankLines::Truncate,
            ignore_prefixes: vec!["svg-format".to_string()],
        }
    }
}

/// Format an SVG source string with default options.
#[must_use]
pub fn format(source: &str) -> String {
    format_with_options(source, FormatOptions::default())
}

/// Format an SVG source string with explicit options.
#[must_use]
pub fn format_with_options(source: &str, options: FormatOptions) -> String {
    format_with_host(source, options, &mut |_| None)
}

/// Format an SVG source string, delegating embedded content to a callback.
///
/// The callback receives [`EmbeddedContent`] for `<style>`, `<script>`, and
/// `<foreignObject>` blocks. Return `Some(formatted)` to use the formatted
/// result, or `None` to fall back to the default text-handling behavior.
#[must_use]
pub fn format_with_host(
    source: &str,
    options: FormatOptions,
    format_embedded: &mut dyn FnMut(EmbeddedContent<'_>) -> Option<String>,
) -> String {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_svg::LANGUAGE.into())
        .is_err()
    {
        return normalize_line_endings(source);
    }

    let Some(tree) = parser.parse(source.as_bytes(), None) else {
        return normalize_line_endings(source);
    };

    if tree.root_node().has_error() {
        return normalize_line_endings(source);
    }

    // Check for ignore-file directive in actual comment nodes only.
    if has_ignore_file_comment(
        tree.root_node(),
        source.as_bytes(),
        &options.ignore_prefixes,
    ) {
        return normalize_line_endings(source);
    }

    let mut formatter = Formatter::new(source.as_bytes(), options);
    formatter.format_node(tree.root_node(), 0, format_embedded);
    formatter.finish(source)
}

/// Normalize CRLF and bare CR to LF.
///
/// [`format_with_host`] guarantees pure-LF output so callers can safely
/// translate line endings with a blanket `replace('\n', target)` without
/// double-counting CRs that the source happened to contain.
fn normalize_line_endings(source: &str) -> String {
    if !source.contains('\r') {
        return source.to_owned();
    }
    source.replace("\r\n", "\n").replace('\r', "\n")
}

/// Walk the AST looking for `<!-- {prefix}-ignore-file -->` in comment nodes.
fn has_ignore_file_comment(node: Node<'_>, source: &[u8], prefixes: &[String]) -> bool {
    if node.kind() == "comment" {
        let inner = node
            .child_by_field_name("text")
            .and_then(|t| std::str::from_utf8(&source[t.byte_range()]).ok())
            .map_or("", str::trim);
        if prefixes.iter().any(|p| inner == format!("{p}-ignore-file")) {
            return true;
        }
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| has_ignore_file_comment(child, source, prefixes))
}

/// Greedily pack SVG path-data segments (or `points` coordinate pairs)
/// onto wrapped lines separated by `\n`, breaking at segment or pair
/// boundaries so the value fits within `budget` characters per line
/// when possible. Returns `Some(value)` with embedded newlines for the
/// caller to re-emit under continuation alignment, or `None` if the
/// value fits as-is, is empty, or can't be parsed.
///
/// The formatter never reformats minified path-data on its own merit;
/// `wrap_path_data` is only invoked when the tag would otherwise
/// overflow `max_inline_tag_width` and we're choosing to break at
/// semantic boundaries instead of leaving a 12kB single-line blob.
fn wrap_path_data(name: &str, raw: &str, budget: usize) -> Option<String> {
    if budget == 0 || raw.chars().count() <= budget {
        return None;
    }
    if raw.trim().is_empty() {
        return None;
    }
    if raw.contains('\n') {
        // Source-preserved embedded newlines take the existing
        // continuation path — don't re-wrap.
        return None;
    }
    match name {
        "d" => wrap_d_value(raw, budget),
        "points" => wrap_points_value(raw, budget),
        _ => None,
    }
}

fn wrap_d_value(raw: &str, budget: usize) -> Option<String> {
    // Wrap in a synthetic `<svg><path d="..."/></svg>` so the grammar's
    // path-data scanner recognizes the attribute value as structured
    // segments (the grammar only activates for attributes inside an
    // SVG tag). Path data never contains `"`, so the quoting is
    // unambiguous; parse errors mean we leave the value untouched.
    const PREFIX: &str = "<svg><path d=\"";
    const SUFFIX: &str = "\"/></svg>";
    let wrapper = format!("{PREFIX}{raw}{SUFFIX}");
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_svg::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(wrapper.as_bytes(), None)?;
    if tree.root_node().has_error() {
        return None;
    }

    let mut segments: Vec<(usize, usize, &str)> = Vec::new();
    collect_path_segments(tree.root_node(), wrapper.as_bytes(), &mut segments);
    if segments.is_empty() {
        return None;
    }

    let prefix_len = PREFIX.len();
    let segment_strs: Vec<&str> = segments
        .iter()
        .map(|&(start, end, _kind)| &raw[start - prefix_len..end - prefix_len])
        .collect();
    let segment_kinds: Vec<&str> = segments.iter().map(|&(_, _, kind)| kind).collect();

    pack_segments(&segment_strs, &segment_kinds, budget, true)
}

fn wrap_points_value(raw: &str, budget: usize) -> Option<String> {
    // `<polyline/polygon points="…">` has no grammar children for the
    // coordinate pairs — split on whitespace into pairs manually. A pair
    // is an `x,y` (comma-separated) or two whitespace-separated numbers;
    // this implementation preserves the inter-pair separator style the
    // source uses by splitting only on runs of whitespace between pairs.
    let tokens: Vec<&str> = raw.split_ascii_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }
    // Re-group tokens into pairs. If a token already contains a comma
    // (e.g. `10,20`) it's a full pair; otherwise two consecutive tokens
    // form a pair.
    let mut pairs: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        if t.contains(',') {
            pairs.push(t.to_string());
            i += 1;
        } else if i + 1 < tokens.len() {
            pairs.push(format!("{t},{}", tokens[i + 1]));
            i += 2;
        } else {
            pairs.push(t.to_string());
            i += 1;
        }
    }
    if pairs.len() <= 1 {
        return None;
    }
    let pair_refs: Vec<&str> = pairs.iter().map(String::as_str).collect();
    let kinds = vec!["pair"; pair_refs.len()];
    pack_segments(&pair_refs, &kinds, budget, false)
}

/// Greedy packer: emit segments separated by single spaces, wrapping
/// to a new line at `\n` when adding the next segment would exceed
/// `budget`. When `prefer_subpath_breaks` is set, a `moveto_segment`
/// forces a new line if the current line is already half full — this
/// keeps `M...` subpath starts from dangling at the end of a line.
fn pack_segments(
    segments: &[&str],
    kinds: &[&str],
    budget: usize,
    prefer_subpath_breaks: bool,
) -> Option<String> {
    if segments.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(segments.iter().map(|s| s.len() + 1).sum::<usize>());
    let mut line_width = 0usize;
    let half_budget = budget / 2;
    for (i, seg) in segments.iter().enumerate() {
        let w = seg.chars().count();
        let is_moveto_split = prefer_subpath_breaks
            && i > 0
            && kinds[i] == "moveto_segment"
            && line_width >= half_budget;
        if line_width == 0 {
            out.push_str(seg);
            line_width = w;
        } else if !is_moveto_split && line_width + 1 + w <= budget {
            out.push(' ');
            out.push_str(seg);
            line_width += 1 + w;
        } else {
            out.push('\n');
            out.push_str(seg);
            line_width = w;
        }
    }
    if out.contains('\n') { Some(out) } else { None }
}

fn collect_path_segments(
    node: Node<'_>,
    source: &[u8],
    segments: &mut Vec<(usize, usize, &'static str)>,
) {
    const SEGMENT_KINDS: &[&str] = &[
        "moveto_segment",
        "lineto_segment",
        "closepath_segment",
        "curveto_segment",
        "smooth_curveto_segment",
        "quadratic_bezier_curveto_segment",
        "smooth_quadratic_bezier_curveto_segment",
        "elliptical_arc_segment",
        "horizontal_lineto_segment",
        "vertical_lineto_segment",
        "implicit_lineto_segment",
    ];
    let kind = node.kind();
    if let Some(&matched) = SEGMENT_KINDS.iter().find(|k| **k == kind) {
        let range = node.byte_range();
        // Use the raw slice to drop any trailing whitespace captured
        // inside the segment — keeps packed output compact.
        let text = std::str::from_utf8(&source[range.clone()])
            .unwrap_or("")
            .trim_end();
        let end = range.start + text.len();
        segments.push((range.start, end, matched));
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_path_segments(child, source, segments);
    }
}

/// Partition attributes into contiguous runs sharing the same canonical
/// group key. Returned as lists of indices into the input slice so the
/// caller can emit them without reallocating attribute data.
fn partition_by_canonical_group(attributes: &[ParsedAttribute]) -> Vec<Vec<usize>> {
    if attributes.is_empty() {
        return Vec::new();
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = vec![0];
    let mut current_key = canonical_group_key(&attributes[0].name);
    for (i, attr) in attributes.iter().enumerate().skip(1) {
        let key = canonical_group_key(&attr.name);
        if key == current_key {
            current.push(i);
        } else {
            groups.push(std::mem::take(&mut current));
            current.push(i);
            current_key = key;
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

/// If a group's rendered width on a single wrapped line would exceed
/// `budget`, fall back to one attribute per line within that group.
/// Otherwise emit the group as a single chunk.
///
/// Rendered width here uses the raw `<name>=<quoted-value>` form with a
/// single space separator. This is an approximation of the final
/// `render_attribute_aligned` output for single-line values — exact
/// enough to decide overflow since quote style doesn't change column
/// width.
fn split_group_if_overflow(
    attributes: &[ParsedAttribute],
    group: Vec<usize>,
    budget: usize,
) -> Vec<Vec<usize>> {
    let width: usize = group
        .iter()
        .map(|&i| approximate_attribute_width(&attributes[i]))
        .sum::<usize>()
        + group.len().saturating_sub(1);
    if width <= budget || group.len() <= 1 {
        vec![group]
    } else {
        group.into_iter().map(|i| vec![i]).collect()
    }
}

/// Rough column width of `name=(quote)value(quote)`. `value` and quotes
/// contribute their literal `chars().count()`; a bare attribute has
/// just the name width.
fn approximate_attribute_width(attribute: &ParsedAttribute) -> usize {
    let name_width = attribute.name.chars().count();
    attribute.value.as_ref().map_or(name_width, |v| {
        let quote_width = if v.original_quote.is_some() { 2 } else { 0 };
        name_width + 1 + quote_width + v.raw.chars().count()
    })
}

/// Decide which attributes ride the tag line and which become the head
/// of the wrapped remainder. Pops trailing attributes off the first
/// chunk until it fits within `line_budget`, with a hard minimum of one
/// attribute on the tag line (per W3 sample style).
///
/// `tag_line_prefix_width` is the column count consumed by `<tagname `
/// plus leading indent — i.e. everything before the first attribute.
/// Returns `(first_chunk_indices, rest_chunks)` where `rest_chunks`
/// contains any popped attrs as a new chunk prepended to the remaining
/// wrapped lines.
fn split_first_chunk_for_tag_line(
    attributes: &[ParsedAttribute],
    chunks: &mut Vec<Vec<usize>>,
    tag_line_prefix_width: usize,
    line_budget: usize,
    mut render: impl FnMut(&ParsedAttribute) -> String,
) -> (Vec<usize>, Vec<Vec<usize>>) {
    if chunks.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let first = chunks.remove(0);
    let mut kept: Vec<usize> = Vec::with_capacity(first.len());
    let mut popped: Vec<usize> = Vec::new();
    for &idx in &first {
        let candidate = render(&attributes[idx]);
        let tentative = if kept.is_empty() {
            tag_line_prefix_width + candidate.chars().count()
        } else {
            // existing width + space separator + new attr
            let existing: usize = tag_line_prefix_width
                + kept
                    .iter()
                    .map(|&i| render(&attributes[i]).chars().count() + 1)
                    .sum::<usize>()
                - 1;
            existing + 1 + candidate.chars().count()
        };
        if !kept.is_empty() && tentative > line_budget {
            popped.push(idx);
        } else {
            kept.push(idx);
        }
    }
    // Once we start popping, any later attrs in this group also get
    // pushed into the remainder — preserve source order by appending.
    for &idx in &first {
        if !kept.contains(&idx) && !popped.contains(&idx) {
            popped.push(idx);
        }
    }
    let rest: Vec<Vec<usize>> = if popped.is_empty() {
        std::mem::take(chunks)
    } else {
        std::iter::once(popped)
            .chain(std::mem::take(chunks))
            .collect()
    };
    (kept, rest)
}

struct Formatter<'a> {
    source: &'a [u8],
    options: FormatOptions,
    out: String,
}

impl<'a> Formatter<'a> {
    const fn new(source: &'a [u8], options: FormatOptions) -> Self {
        Self {
            source,
            options,
            out: String::new(),
        }
    }

    fn finish(mut self, original: &str) -> String {
        while self.out.ends_with('\n') {
            self.out.pop();
        }
        if original.ends_with('\n') {
            self.out.push('\n');
        }
        self.out
    }

    fn format_node(
        &mut self,
        node: Node<'_>,
        depth: usize,
        fmt: &mut dyn FnMut(EmbeddedContent<'_>) -> Option<String>,
    ) {
        match node.kind() {
            "svg_root_element" | "element" => self.format_element_like(node, depth, fmt),
            "start_tag" => self.write_tag_node(node, depth, false),
            "self_closing_tag" => self.write_tag_node(node, depth, true),
            "style_text_double" | "style_text_single" | "script_text_double"
            | "script_text_single" => {
                self.write_preserved_block_text(node, depth);
            }
            "text" | "raw_text" => {
                self.write_text_node(node, depth);
            }
            "end_tag"
            | "comment"
            | "cdata_section"
            | "doctype"
            | "processing_instruction"
            | "xml_declaration"
            | "entity_reference"
            | "erroneous_end_tag" => {
                let text = self.node_text(node).trim().to_string();
                self.write_line(depth, &text);
            }
            _ => self.format_children(node, depth, fmt),
        }
    }

    fn format_children(
        &mut self,
        node: Node<'_>,
        depth: usize,
        fmt: &mut dyn FnMut(EmbeddedContent<'_>) -> Option<String>,
    ) {
        let mut cursor = node.walk();
        let mut prev_end: Option<usize> = None;
        let mut prev_was_comment = false;
        let mut ignore_next = false;
        let mut in_ignore_range = false;
        for child in node.named_children(&mut cursor) {
            if self.handle_ignore(
                child,
                &mut in_ignore_range,
                &mut ignore_next,
                &mut prev_was_comment,
                &mut prev_end,
            ) {
                continue;
            }

            if let Some(end) = prev_end {
                self.emit_gap(end, child.start_byte(), prev_was_comment);
            }
            self.format_node(child, depth, fmt);
            prev_was_comment = child.kind() == "comment";
            prev_end = Some(child.end_byte());
        }
    }

    fn format_element_like(
        &mut self,
        node: Node<'_>,
        depth: usize,
        fmt: &mut dyn FnMut(EmbeddedContent<'_>) -> Option<String>,
    ) {
        let mut cursor = node.walk();
        let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
        if children.is_empty() {
            return;
        }

        // Self-closing form: <rect .../>
        if children.len() == 1 && children[0].kind() == "self_closing_tag" {
            self.format_node(children[0], depth, fmt);
            return;
        }

        // Detect the element's tag name for embedded content handling.
        let tag_name: String = children
            .iter()
            .find(|c| c.kind() == "start_tag")
            .and_then(|tag| {
                let text = self.node_text(*tag).trim();
                text.strip_prefix('<')
                    .and_then(|s| s.split(|c: char| c.is_whitespace() || c == '>').next())
                    .map(str::to_string)
            })
            .unwrap_or_default();

        let embedded_lang = match tag_name.as_str() {
            "style" => Some(EmbeddedLanguage::Css),
            "script" => Some(EmbeddedLanguage::JavaScript),
            "foreignObject" => Some(EmbeddedLanguage::Html),
            _ => None,
        };

        // For foreignObject, try to format the entire inner content as HTML.
        if embedded_lang == Some(EmbeddedLanguage::Html)
            && self
                .try_format_foreign_object(&children, depth, fmt)
                .is_some()
        {
            return;
        }

        if let Some((start, end)) = text_content_entity_bounds(&children, &tag_name) {
            self.format_text_content_element(start, end, depth);
            return;
        }

        let mut prev_end: Option<usize> = None;
        let mut prev_was_comment = false;
        let mut ignore_next = false;
        let mut in_ignore_range = false;
        for child in children {
            if self.handle_ignore(
                child,
                &mut in_ignore_range,
                &mut ignore_next,
                &mut prev_was_comment,
                &mut prev_end,
            ) {
                continue;
            }

            match child.kind() {
                "start_tag" | "end_tag" => {
                    self.format_node(child, depth, fmt);
                }
                "style_text_double" | "style_text_single" | "script_text_double"
                | "script_text_single" => {
                    if !self.node_text(child).trim().is_empty() {
                        self.write_preserved_block_text(child, depth + 1);
                    }
                    prev_was_comment = false;
                    prev_end = Some(child.end_byte());
                }
                "text" | "raw_text" => {
                    if self.node_text(child).trim().is_empty() {
                        continue;
                    }
                    if let Some(end) = prev_end {
                        self.emit_gap(end, child.start_byte(), prev_was_comment);
                    }
                    // Try embedded formatting for style/script raw_text.
                    if let Some(lang) = embedded_lang
                        && lang != EmbeddedLanguage::Html
                        && self.try_format_embedded_text(child, lang, depth + 1, fmt)
                    {
                        prev_was_comment = false;
                        prev_end = Some(child.end_byte());
                        continue;
                    }
                    if matches!(self.options.text_content, TextContentMode::Maintain)
                        && matches!(
                            embedded_lang,
                            Some(EmbeddedLanguage::Css | EmbeddedLanguage::JavaScript)
                        )
                    {
                        self.write_preserved_embedded_text(child, depth + 1);
                    } else {
                        self.write_text_node(child, depth + 1);
                    }
                    prev_was_comment = false;
                    prev_end = Some(child.end_byte());
                }
                _ => {
                    if let Some(end) = prev_end {
                        self.emit_gap(end, child.start_byte(), prev_was_comment);
                    }
                    self.format_node(child, depth + 1, fmt);
                    prev_was_comment = child.kind() == "comment";
                    prev_end = Some(child.end_byte());
                }
            }
        }
    }

    /// Format a text-content element whose children include entity
    /// references mixed with text. Extracts the raw source between start
    /// and end tags, normalizes whitespace into a single line, and inlines
    /// with the tags when it fits.
    ///
    /// Called only when `entity_reference` nodes are present — the whitespace
    /// around them is a formatting artifact, not meaningful content, so we
    /// always normalize regardless of [`TextContentMode`].
    fn format_text_content_element(&mut self, start: Node<'_>, end: Node<'_>, depth: usize) {
        let raw = std::str::from_utf8(&self.source[start.end_byte()..end.start_byte()])
            .unwrap_or_default();
        let normalized = normalize_text_content_with_entities(raw);
        let end_text = self.node_text(end).trim().to_string();

        // Render the start tag (capture output for potential inline rewrite).
        let out_before = self.out.len();
        self.write_tag_node(start, depth, false);
        let tag_output = self.out[out_before..].to_string();

        if normalized.is_empty() {
            self.write_line(depth, &end_text);
            return;
        }

        // Try to keep everything on one line: <tag>content</tag>
        let tag_str = tag_output.trim_end_matches('\n');
        if !tag_str.contains('\n') {
            let tag_inline = tag_str.trim_start();
            let candidate = format!("{tag_inline}{normalized}{end_text}");
            if self.indent(depth).len() + candidate.len() <= self.options.max_inline_tag_width {
                self.out.truncate(out_before);
                self.write_line(depth, &candidate);
                return;
            }
        }

        // Doesn't fit inline — write normalized content on its own line.
        self.write_line(depth + 1, &normalized);
        self.write_line(depth, &end_text);
    }

    fn write_tag_node(&mut self, node: Node<'_>, depth: usize, self_closing: bool) {
        let raw = self.node_text(node).trim().to_string();
        let Some(mut tag) = parse_tag(&raw, self_closing) else {
            self.write_line(depth, &raw);
            return;
        };
        reorder_attributes(&mut tag.attributes, self.options.attribute_sort);
        let rendered_attributes: Vec<String> = tag
            .attributes
            .iter()
            .map(|attribute| self.render_attribute(attribute))
            .collect();
        let inline = self.render_inline_tag(&tag.name, &rendered_attributes, self_closing);

        if !self.should_break_into_multiline(&raw, &inline, &rendered_attributes) {
            self.write_line(depth, &inline);
            return;
        }
        if rendered_attributes.is_empty() {
            self.emit_attributeless_multiline(depth, &tag.name, self_closing);
            return;
        }
        self.emit_multiline_tag(depth, &mut tag, self_closing);
    }

    /// Build the one-line rendering of a tag with all attributes inline.
    /// Used as the candidate both for the Auto-layout width check and as
    /// the direct output when no wrapping is needed.
    fn render_inline_tag(
        &self,
        name: &str,
        rendered_attributes: &[String],
        self_closing: bool,
    ) -> String {
        let mut inline = format!("<{name}");
        if !rendered_attributes.is_empty() {
            inline.push(' ');
            inline.push_str(&rendered_attributes.join(" "));
        }
        if self_closing {
            inline.push_str(self.self_closing_suffix());
        } else {
            inline.push('>');
        }
        inline
    }

    /// Decide whether the attribute layout forces (or permits) breaking
    /// a tag across multiple lines. `SingleLine` never breaks;
    /// `MultiLine` always does when any attributes exist; `Auto` breaks
    /// on source-side newlines or inline-width overflow.
    fn should_break_into_multiline(
        &self,
        raw: &str,
        inline: &str,
        rendered_attributes: &[String],
    ) -> bool {
        match self.options.attribute_layout {
            AttributeLayout::SingleLine => false,
            AttributeLayout::MultiLine => !rendered_attributes.is_empty(),
            AttributeLayout::Auto => {
                raw.contains('\n') || inline.len() > self.options.max_inline_tag_width
            }
        }
    }

    /// Emit an attributeless tag that still needs breaking onto its own
    /// line (e.g. forced by `MultiLine` layout). The open bracket goes
    /// on one line, closer on the next — mirrors the W3 convention.
    fn emit_attributeless_multiline(&mut self, depth: usize, tag_name: &str, self_closing: bool) {
        self.write_line(depth, &format!("<{tag_name}"));
        if self_closing {
            self.write_line(depth, self.self_closing_suffix());
        } else {
            self.write_line(depth, ">");
        }
    }

    /// Full multi-line tag emission: apply the path-data auto-wrap pass,
    /// choose a chunking strategy (canonical groups vs fixed chunks),
    /// then either ride the first chunk on the tag line
    /// (`AlignToTagName`) or place the opening bracket alone on line 1
    /// (`OneLevel`) and emit each remaining chunk on its own wrapped
    /// line.
    fn emit_multiline_tag(&mut self, depth: usize, tag: &mut ParsedTag, self_closing: bool) {
        let wrapped_prefix = self.wrapped_attribute_prefix(depth, &tag.name);
        self.auto_wrap_path_data_values(&mut tag.attributes, &wrapped_prefix);

        // Force one-attribute-per-line when any value carries internal
        // newlines (typical of `d="..."` path data in W3 samples). The
        // continuation-alignment logic in `render_attribute_aligned`
        // assumes a known start column for the attribute name; packing
        // multiple such attributes per wrapped line would break that.
        let has_multiline_value = tag
            .attributes
            .iter()
            .any(|a| a.value.as_ref().is_some_and(|v| v.raw.contains('\n')));

        // AlignToTagName pairs naturally with "first attribute inline on
        // the tag line": the wrapped-prefix width equals the column
        // where the first attribute would sit inline (indent + "<tag "),
        // so `render_attribute_aligned` produces correct continuation
        // alignment for the first attr's multi-line values *and* the
        // same prefix aligns subsequent wrapped attrs under the first.
        // OneLevel cannot do this cleanly — its column math differs —
        // so we keep the existing "<tag alone on line 1" layout there.
        let first_inline = matches!(
            self.options.wrapped_attribute_indent,
            WrappedAttributeIndent::AlignToTagName,
        );
        let closer = if self_closing {
            self.self_closing_suffix()
        } else {
            ">"
        };

        let chunks = self.build_wrap_chunks(&tag.attributes, &wrapped_prefix, has_multiline_value);

        if first_inline {
            self.emit_first_inline_layout(depth, tag, chunks, &wrapped_prefix, closer);
        } else {
            self.emit_one_level_layout(depth, tag, &chunks, &wrapped_prefix, closer);
        }
    }

    /// Auto-wrap minified `d="…"` path data and `<polyline/polygon>
    /// points="…"` values at semantic boundaries (M/L/C segments,
    /// coordinate pairs) when a single inline line would overflow
    /// `max_inline_tag_width`. Values that already carry source
    /// newlines skip this pass and keep their preserved layout.
    fn auto_wrap_path_data_values(&self, attributes: &mut [ParsedAttribute], wrapped_prefix: &str) {
        for attribute in attributes {
            let Some(value) = attribute.value.as_mut() else {
                continue;
            };
            let name_width = attribute.name.chars().count();
            let available = self
                .options
                .max_inline_tag_width
                .saturating_sub(wrapped_prefix.len() + name_width + 2);
            if let Some(wrapped) = wrap_path_data(&attribute.name, &value.raw, available) {
                value.raw = wrapped;
            }
        }
    }

    /// Partition attributes into chunks that will each occupy one
    /// wrapped line.
    ///
    /// - Canonical sort + no multiline values: one wrapped line per
    ///   canonical group (identity / geometry / drawing / refs /
    ///   presentation / other / namespaces / version). If a group's
    ///   rendered width exceeds budget, fall back to one-per-line
    ///   within that group.
    /// - Otherwise: chunks of `attributes_per_line` (existing
    ///   behavior; multiline values already force `per_line=1` because
    ///   the continuation-alignment helper assumes a known column).
    fn build_wrap_chunks(
        &self,
        attributes: &[ParsedAttribute],
        wrapped_prefix: &str,
        has_multiline_value: bool,
    ) -> Vec<Vec<usize>> {
        let budget = self
            .options
            .max_inline_tag_width
            .saturating_sub(wrapped_prefix.len());
        if matches!(self.options.attribute_sort, AttributeSort::Canonical) && !has_multiline_value {
            partition_by_canonical_group(attributes)
                .into_iter()
                .flat_map(|group| split_group_if_overflow(attributes, group, budget))
                .collect()
        } else {
            let per_line = if has_multiline_value {
                1
            } else {
                self.options.attributes_per_line.max(1)
            };
            (0..attributes.len())
                .collect::<Vec<_>>()
                .chunks(per_line)
                .map(<[usize]>::to_vec)
                .collect()
        }
    }

    /// `AlignToTagName` layout: the first chunk rides the tag line
    /// with `<tag ` prefix. If it would overflow the budget, pop
    /// attributes off its tail until it fits; popped attrs become a
    /// new wrapped chunk at the front of the rest. At minimum the
    /// first attribute stays on the tag line — matching W3 SVG sample
    /// style.
    fn emit_first_inline_layout(
        &mut self,
        depth: usize,
        tag: &ParsedTag,
        mut chunks: Vec<Vec<usize>>,
        wrapped_prefix: &str,
        closer: &str,
    ) {
        let (first_chunk_indices, rest_chunks) = split_first_chunk_for_tag_line(
            &tag.attributes,
            &mut chunks,
            self.indent(depth).len() + tag.name.chars().count() + 2, // "<" + name + " "
            self.options.max_inline_tag_width,
            |attr| self.render_attribute_aligned(attr, wrapped_prefix),
        );
        let first_rendered: Vec<String> = first_chunk_indices
            .iter()
            .map(|&i| self.render_attribute_aligned(&tag.attributes[i], wrapped_prefix))
            .collect();
        let mut tag_line = format!(
            "{}<{} {}",
            self.indent(depth),
            tag.name,
            first_rendered.join(" ")
        );
        if rest_chunks.is_empty() {
            tag_line.push_str(closer);
        }
        tag_line.push('\n');
        self.out.push_str(&tag_line);
        self.emit_wrapped_chunks(tag, &rest_chunks, wrapped_prefix, closer);
    }

    /// `OneLevel` layout: the opening bracket sits alone on line 1 at
    /// `depth` indent; every chunk wraps onto its own line at
    /// `depth + 1` indent (the `wrapped_prefix`).
    fn emit_one_level_layout(
        &mut self,
        depth: usize,
        tag: &ParsedTag,
        chunks: &[Vec<usize>],
        wrapped_prefix: &str,
        closer: &str,
    ) {
        self.write_line(depth, &format!("<{}", tag.name));
        self.emit_wrapped_chunks(tag, chunks, wrapped_prefix, closer);
    }

    /// Emit pre-computed chunks one-per-line at `wrapped_prefix`,
    /// appending `closer` to the final chunk's line.
    fn emit_wrapped_chunks(
        &mut self,
        tag: &ParsedTag,
        chunks: &[Vec<usize>],
        wrapped_prefix: &str,
        closer: &str,
    ) {
        for (index, chunk) in chunks.iter().enumerate() {
            let rendered: Vec<String> = chunk
                .iter()
                .map(|&i| self.render_attribute_aligned(&tag.attributes[i], wrapped_prefix))
                .collect();
            let mut line = rendered.join(" ");
            if index == chunks.len() - 1 {
                line.push_str(closer);
            }
            self.write_prefixed_line(wrapped_prefix, &line);
        }
    }

    const fn self_closing_suffix(&self) -> &'static str {
        if self.options.space_before_self_close {
            " />"
        } else {
            "/>"
        }
    }

    fn render_attribute(&self, attribute: &ParsedAttribute) -> String {
        attribute.value.as_ref().map_or_else(
            || attribute.name.clone(),
            |value| format!("{}={}", attribute.name, self.render_attribute_value(value)),
        )
    }

    fn render_attribute_value(&self, value: &ParsedAttributeValue) -> String {
        match self.options.quote_style {
            QuoteStyle::Preserve => match value.original_quote {
                Some('\'') => format!("'{}'", value.raw),
                Some('"') => format!("\"{}\"", value.raw),
                Some(other) => format!("{other}{}{other}", value.raw),
                None => value.raw.clone(),
            },
            QuoteStyle::Double => format!("\"{}\"", value.raw.replace('"', "&quot;")),
            QuoteStyle::Single => format!("'{}'", value.raw.replace('\'', "&apos;")),
        }
    }

    /// Render an attribute, aligning any continuation lines of a multi-line
    /// value to the column directly under the first value character (i.e.
    /// just after the opening quote).
    ///
    /// `prefix` is the whitespace string that will precede this attribute
    /// on its line (the wrapped-attribute indent, which may mix tabs and
    /// spaces). Continuation lines are emitted as `prefix` plus spaces
    /// spanning `name=` and the opening quote, so the visual alignment
    /// holds regardless of the caller's tab-width setting — mixing pure
    /// spaces with a tab-indented prefix would misalign at any tab width
    /// other than one.
    ///
    /// For values without embedded newlines this delegates to
    /// [`Self::render_attribute_value`]; for multi-line values it strips
    /// each continuation line's original leading whitespace and re-indents
    /// it, matching W3 SVG sample style where `<path d="M … " ` wrapping
    /// preserves logical path-command groupings under a stable column.
    fn render_attribute_aligned(&self, attribute: &ParsedAttribute, prefix: &str) -> String {
        let Some(value) = attribute.value.as_ref() else {
            return attribute.name.clone();
        };
        if !value.raw.contains('\n') {
            return format!("{}={}", attribute.name, self.render_attribute_value(value));
        }

        let quote = match self.options.quote_style {
            QuoteStyle::Preserve => value.original_quote.unwrap_or('"'),
            QuoteStyle::Double => '"',
            QuoteStyle::Single => '\'',
        };
        let name_width = attribute.name.chars().count();
        let mut pad = String::with_capacity(prefix.len() + name_width + 2);
        pad.push_str(prefix);
        // `name=` + opening quote → name_width + 2 spaces of alignment.
        for _ in 0..name_width + 2 {
            pad.push(' ');
        }

        let mut result = String::with_capacity(value.raw.len() + pad.len() * 2 + 8);
        let mut lines = value.raw.split('\n');
        if let Some(first) = lines.next() {
            result.push_str(&attribute.name);
            result.push('=');
            result.push(quote);
            result.push_str(first);
        }
        for line in lines {
            result.push('\n');
            result.push_str(&pad);
            result.push_str(line.trim_start());
        }
        result.push(quote);
        result
    }

    fn wrapped_attribute_prefix(&self, depth: usize, tag_name: &str) -> String {
        match self.options.wrapped_attribute_indent {
            WrappedAttributeIndent::OneLevel => self.indent(depth + 1),
            WrappedAttributeIndent::AlignToTagName => {
                let mut prefix = self.indent(depth);
                // Align to the column right after `<tag ` in a hypothetical one-line form.
                prefix.push_str(&" ".repeat(tag_name.chars().count() + 2));
                prefix
            }
        }
    }

    fn write_prefixed_line(&mut self, prefix: &str, text: &str) {
        self.out.push_str(prefix);
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn write_text_node(&mut self, node: Node<'_>, depth: usize) {
        let text = self.node_text(node).to_string();
        self.write_text_str(&text, depth);
    }

    fn write_text_str(&mut self, text: &str, depth: usize) {
        if text.trim().is_empty() {
            return;
        }

        match self.options.text_content {
            TextContentMode::Collapse => {
                for line in text.lines() {
                    let collapsed = collapse_whitespace(line);
                    if collapsed.is_empty() {
                        continue;
                    }
                    self.write_line(depth, &collapsed);
                }
            }
            TextContentMode::Maintain => {
                self.write_preserved_str(text, depth);
            }
            TextContentMode::Prettify => {
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    self.write_line(depth, trimmed);
                }
            }
        }
    }

    fn write_preserved_block_text(&mut self, node: Node<'_>, depth: usize) {
        let text = self.node_text(node).to_string();
        self.write_preserved_str(&text, depth);
    }

    fn write_preserved_embedded_text(&mut self, node: Node<'_>, depth: usize) {
        let text = self.node_text(node).to_string();
        self.write_embedded_preserved_str(&text, depth);
    }

    fn write_preserved_str(&mut self, text: &str, depth: usize) {
        if text.trim().is_empty() {
            return;
        }

        let lines: Vec<&str> = text.lines().collect();
        let first_non_empty = lines.iter().position(|line| !line.trim().is_empty());
        let last_non_empty = lines.iter().rposition(|line| !line.trim().is_empty());
        let (Some(start), Some(end)) = (first_non_empty, last_non_empty) else {
            return;
        };

        let block = &lines[start..=end];
        let min_leading = block
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.chars().take_while(|c| c.is_whitespace()).count())
            .min()
            .unwrap_or(0);

        for line in block {
            let without_common_indent = line.chars().skip(min_leading).collect::<String>();
            self.write_line(depth, without_common_indent.trim_end());
        }
    }

    fn write_embedded_preserved_str(&mut self, text: &str, depth: usize) {
        if text.trim().is_empty() {
            return;
        }

        let lines: Vec<&str> = text.lines().collect();
        let first_non_empty = lines.iter().position(|line| !line.trim().is_empty());
        let last_non_empty = lines.iter().rposition(|line| !line.trim().is_empty());
        let (Some(start), Some(end)) = (first_non_empty, last_non_empty) else {
            return;
        };

        let block = &lines[start..=end];
        let min_leading = block
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.chars().take_while(|c| c.is_whitespace()).count())
            .min()
            .unwrap_or(0);

        let mut consecutive_blank = 0usize;
        for line in block {
            let without_common_indent = line.chars().skip(min_leading).collect::<String>();
            let trimmed = without_common_indent.trim_end();
            if trimmed.is_empty() {
                consecutive_blank += 1;
                if self.should_emit_embedded_blank(consecutive_blank) {
                    self.out.push('\n');
                }
            } else {
                consecutive_blank = 0;
                self.write_line(depth, trimmed);
            }
        }
    }

    /// Try to format embedded text (style/script `raw_text`) via the callback.
    /// Returns `true` if the callback produced a result.
    ///
    /// Handles CDATA-wrapped payloads specially: the `<![CDATA[`/`]]>` markers
    /// are stripped before the host formatter sees the content (the CSS/JS
    /// parsers reject them at column 0 as syntax errors — see W3 SVG path
    /// samples). When CDATA was present, XML entity decoding/encoding is
    /// skipped — inside a CDATA section, `&amp;` is literal, not escaped —
    /// and the wrapper is re-emitted on output to preserve the XML-safety
    /// semantics the author chose.
    fn try_format_embedded_text(
        &mut self,
        node: Node<'_>,
        language: EmbeddedLanguage,
        depth: usize,
        fmt: &mut dyn FnMut(EmbeddedContent<'_>) -> Option<String>,
    ) -> bool {
        let raw = self.node_text(node).to_string();
        let (payload, cdata_wrapped) = strip_cdata_wrapper(&raw).map_or_else(
            || (decode_xml_entities(dedent_block(&raw)), false),
            |inner| (dedent_block(inner), true),
        );
        if payload.is_empty() {
            return false;
        }
        let req = EmbeddedContent {
            language,
            content: &payload,
            indent_depth: depth,
        };
        fmt(req).is_some_and(|formatted| {
            if cdata_wrapped {
                self.write_cdata_block(&formatted, depth);
            } else {
                let encoded = encode_xml_entities(&formatted);
                self.write_indented_block(&encoded, depth);
            }
            true
        })
    }

    /// Write a formatted host block wrapped in a CDATA section.
    ///
    /// The opening `<![CDATA[` sits at `depth`, payload lines are indented
    /// at `depth + 1` via [`Self::write_indented_block`], and the closing
    /// `]]>` returns to `depth`. Matches the nested-tag indentation scheme
    /// used by surrounding element children.
    fn write_cdata_block(&mut self, text: &str, depth: usize) {
        self.write_line(depth, "<![CDATA[");
        self.write_indented_block(text, depth + 1);
        self.write_line(depth, "]]>");
    }

    /// Try to format `foreignObject` inner content via the callback.
    /// On success, writes the full element (start tag, formatted content, end tag).
    fn try_format_foreign_object(
        &mut self,
        children: &[Node<'_>],
        depth: usize,
        fmt: &mut dyn FnMut(EmbeddedContent<'_>) -> Option<String>,
    ) -> Option<()> {
        let start_tag = children.iter().find(|c| c.kind() == "start_tag")?;
        let end_tag = children.iter().find(|c| c.kind() == "end_tag")?;

        let content_start = start_tag.end_byte();
        let content_end = end_tag.start_byte();
        if content_start >= content_end {
            return None;
        }

        let raw = std::str::from_utf8(&self.source[content_start..content_end]).ok()?;
        let content = dedent_block(raw);
        if content.is_empty() {
            return None;
        }

        let req = EmbeddedContent {
            language: EmbeddedLanguage::Html,
            content: &content,
            indent_depth: depth + 1,
        };
        let formatted = fmt(req)?;

        // Write start tag, formatted content, end tag.
        self.write_tag_node(*start_tag, depth, false);
        self.write_indented_block(&formatted, depth + 1);
        let end_text = self.node_text(*end_tag).trim().to_string();
        self.write_line(depth, &end_text);
        Some(())
    }

    /// Write pre-formatted text, re-indented to the given depth.
    /// Preserves the content's internal indentation structure.
    /// Leading/trailing blank lines are stripped; interior consecutive
    /// blank lines are normalized per the `blank_lines` option.
    fn write_indented_block(&mut self, text: &str, depth: usize) {
        let lines: Vec<&str> = text.lines().collect();
        let Some(first) = lines.iter().position(|l| !l.trim().is_empty()) else {
            return;
        };
        let last = lines
            .iter()
            .rposition(|l| !l.trim().is_empty())
            .unwrap_or(first);
        let block = &lines[first..=last];

        let indent = self.indent(depth);
        let mut consecutive_blank = 0usize;
        for line in block {
            if line.trim().is_empty() {
                consecutive_blank += 1;
                if self.should_emit_embedded_blank(consecutive_blank) {
                    self.out.push('\n');
                }
            } else {
                consecutive_blank = 0;
                self.out.push_str(&indent);
                self.out.push_str(line);
                self.out.push('\n');
            }
        }
    }

    /// Process ignore directives and whitespace-skip logic for a child node.
    ///
    /// Returns `true` if the child was fully handled (caller should `continue`).
    fn handle_ignore(
        &mut self,
        child: Node<'_>,
        in_ignore_range: &mut bool,
        ignore_next: &mut bool,
        prev_was_comment: &mut bool,
        prev_end: &mut Option<usize>,
    ) -> bool {
        let mut skip_ignore_self = false;

        if child.kind() == "comment" {
            // Inside an ignore range, only look for the end marker.
            // All other directives are preserved verbatim.
            if *in_ignore_range {
                if self.is_ignore_directive(child, "ignore-end") {
                    self.write_source_span(*prev_end, child.end_byte());
                    *in_ignore_range = false;
                    *prev_was_comment = true;
                    *prev_end = Some(child.end_byte());
                    return true;
                }
                // Not an end marker — fall through to the in_ignore_range
                // raw-write below. Don't set ignore_next or skip_ignore_self.
            } else {
                if self.is_ignore_directive(child, "ignore-start") {
                    *in_ignore_range = true;
                    if let Some(end) = *prev_end {
                        self.emit_gap(end, child.start_byte(), *prev_was_comment);
                    }
                    self.write_source_span(Some(child.start_byte()), child.end_byte());
                    *prev_was_comment = true;
                    *prev_end = Some(child.end_byte());
                    return true;
                }
                if self.is_ignore_directive(child, "ignore") {
                    *ignore_next = true;
                    skip_ignore_self = true;
                }
            }
        }

        // Skip whitespace-only text — but not inside an ignore range,
        // where we need to preserve everything verbatim.
        if !*in_ignore_range
            && matches!(child.kind(), "text" | "raw_text")
            && self.node_text(child).trim().is_empty()
        {
            return true;
        }

        if !skip_ignore_self && *in_ignore_range {
            // Inside an ignore range: write from prev_end through this node,
            // preserving the original gap + content verbatim.
            self.write_source_span(*prev_end, child.end_byte());
            *prev_was_comment = child.kind() == "comment";
            *prev_end = Some(child.end_byte());
            return true;
        }

        if !skip_ignore_self && *ignore_next {
            // Single-element ignore: write only the node bytes.
            // The gap before it was already emitted by the previous
            // write_line/emit_gap, so don't re-emit it.
            self.write_source_span(Some(child.start_byte()), child.end_byte());
            if !self.out.ends_with('\n') {
                self.out.push('\n');
            }
            *ignore_next = false;
            *prev_was_comment = child.kind() == "comment";
            *prev_end = Some(child.end_byte());
            return true;
        }

        false
    }

    /// Check if a comment node matches `<!-- {prefix}-{suffix} -->`.
    ///
    /// Uses the tree-sitter `text` field on the comment node to get
    /// the inner content without manual `<!--`/`-->` stripping.
    fn is_ignore_directive(&self, node: Node<'_>, suffix: &str) -> bool {
        let inner = node
            .child_by_field_name("text")
            .map_or("", |t| self.node_text(t).trim());
        self.options
            .ignore_prefixes
            .iter()
            .any(|prefix| inner == format!("{prefix}-{suffix}"))
    }

    /// Write a source span verbatim, from `from` (or start of node if None)
    /// through `to`. Preserves original whitespace, gaps, and content exactly.
    fn write_source_span(&mut self, from: Option<usize>, to: usize) {
        let start = from.unwrap_or(to);
        if start < to {
            self.out
                .push_str(std::str::from_utf8(&self.source[start..to]).unwrap_or_default());
        }
    }

    /// Count blank lines in the source gap between two byte positions.
    fn source_blank_lines(&self, from: usize, to: usize) -> usize {
        if from >= to {
            return 0;
        }
        let gap = std::str::from_utf8(&self.source[from..to]).unwrap_or_default();
        let newlines = gap.chars().filter(|&c| c == '\n').count();
        newlines.saturating_sub(1)
    }

    /// Emit blank lines between siblings based on the `blank_lines` option.
    ///
    /// When `prev_was_comment` is true and mode is `Insert`, the gap is
    /// skipped — comments attach downward to the element they annotate.
    fn emit_gap(&mut self, prev_end: usize, next_start: usize, prev_was_comment: bool) {
        let source_gaps = self.source_blank_lines(prev_end, next_start);
        let count = match self.options.blank_lines {
            BlankLines::Remove => 0,
            BlankLines::Preserve => source_gaps,
            BlankLines::Truncate => source_gaps.min(1),
            BlankLines::Insert => usize::from(!prev_was_comment),
        };
        for _ in 0..count {
            self.out.push('\n');
        }
    }

    /// Whether an embedded-content blank line should be emitted.
    ///
    /// `consecutive` is how many blank lines in a row have been seen so far
    /// (1 = first blank, 2 = second, etc.). `Truncate` collapses runs of 2+
    /// to 1, `Remove` strips all, and `Preserve`/`Insert` pass through
    /// (those modes only meaningfully apply to sibling element gaps).
    const fn should_emit_embedded_blank(&self, consecutive: usize) -> bool {
        match self.options.blank_lines {
            BlankLines::Remove => false,
            BlankLines::Truncate => consecutive <= 1,
            BlankLines::Preserve | BlankLines::Insert => true,
        }
    }

    fn node_text(&self, node: Node<'_>) -> &str {
        std::str::from_utf8(&self.source[node.byte_range()]).unwrap_or_default()
    }

    fn write_line(&mut self, depth: usize, text: &str) {
        self.out.push_str(&self.indent(depth));
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn indent(&self, depth: usize) -> String {
        if self.options.insert_spaces {
            " ".repeat(depth.saturating_mul(self.options.indent_width))
        } else {
            "\t".repeat(depth)
        }
    }
}

fn text_content_entity_bounds<'a>(
    children: &[Node<'a>],
    tag_name: &str,
) -> Option<(Node<'a>, Node<'a>)> {
    if !is_text_content_element(tag_name)
        || !children
            .iter()
            .any(|child| child.kind() == "entity_reference")
    {
        return None;
    }

    let start = children
        .iter()
        .find(|child| child.kind() == "start_tag")
        .copied()?;
    let end = children
        .iter()
        .find(|child| child.kind() == "end_tag")
        .copied()?;
    let all_inline = children
        .iter()
        .filter(|child| !matches!(child.kind(), "start_tag" | "end_tag"))
        .all(|child| matches!(child.kind(), "text" | "raw_text" | "entity_reference"));

    all_inline.then_some((start, end))
}
