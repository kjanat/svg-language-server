//! Deterministic structural formatting for SVG documents.

mod tag_parse;
mod text_content;

use tag_parse::{ParsedAttribute, ParsedAttributeValue, parse_tag, reorder_attributes};
use text_content::{
    collapse_whitespace, dedent_block, is_text_content_element,
    normalize_text_content_with_entities,
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
    #[default]
    OneLevel,
    /// Align to the column after `<tag ` so wrapped attributes line up visually.
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
            insert_spaces: false,
            max_inline_tag_width: 100,
            attribute_sort: AttributeSort::Canonical,
            attribute_layout: AttributeLayout::Auto,
            attributes_per_line: 1,
            space_before_self_close: true,
            quote_style: QuoteStyle::Preserve,
            wrapped_attribute_indent: WrappedAttributeIndent::OneLevel,
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
        return source.to_owned();
    }

    let Some(tree) = parser.parse(source.as_bytes(), None) else {
        return source.to_owned();
    };

    if tree.root_node().has_error() {
        return source.to_owned();
    }

    // Check for ignore-file directive in actual comment nodes only.
    if has_ignore_file_comment(
        tree.root_node(),
        source.as_bytes(),
        &options.ignore_prefixes,
    ) {
        return source.to_owned();
    }

    let mut formatter = Formatter::new(source.as_bytes(), options);
    formatter.format_node(tree.root_node(), 0, format_embedded);
    formatter.finish(source)
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
                    self.write_text_node(child, depth + 1);
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

        let mut inline = format!("<{}", tag.name);
        if !rendered_attributes.is_empty() {
            inline.push(' ');
            inline.push_str(&rendered_attributes.join(" "));
        }
        if self_closing {
            inline.push_str(self.self_closing_suffix());
        } else {
            inline.push('>');
        }

        let multiline = match self.options.attribute_layout {
            AttributeLayout::SingleLine => false,
            AttributeLayout::MultiLine => !rendered_attributes.is_empty(),
            AttributeLayout::Auto => {
                raw.contains('\n') || inline.len() > self.options.max_inline_tag_width
            }
        };
        if !multiline {
            self.write_line(depth, &inline);
            return;
        }

        self.write_line(depth, &format!("<{}", tag.name));
        if rendered_attributes.is_empty() {
            if self_closing {
                self.write_line(depth, self.self_closing_suffix());
            } else {
                self.write_line(depth, ">");
            }
            return;
        }

        let per_line = self.options.attributes_per_line.max(1);
        let wrapped_prefix = self.wrapped_attribute_prefix(depth, &tag.name);
        let chunks = rendered_attributes.chunks(per_line).collect::<Vec<_>>();
        for (index, chunk) in chunks.iter().enumerate() {
            let mut line = chunk.join(" ");
            if index == chunks.len() - 1 {
                if self_closing {
                    line.push_str(self.self_closing_suffix());
                } else {
                    line.push('>');
                }
            }
            self.write_prefixed_line(&wrapped_prefix, &line);
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

    /// Try to format embedded text (style/script `raw_text`) via the callback.
    /// Returns `true` if the callback produced a result.
    fn try_format_embedded_text(
        &mut self,
        node: Node<'_>,
        language: EmbeddedLanguage,
        depth: usize,
        fmt: &mut dyn FnMut(EmbeddedContent<'_>) -> Option<String>,
    ) -> bool {
        let raw = self.node_text(node).to_string();
        let content = dedent_block(&raw);
        if content.is_empty() {
            return false;
        }
        let req = EmbeddedContent {
            language,
            content: &content,
            indent_depth: depth,
        };
        fmt(req).is_some_and(|formatted| {
            self.write_indented_block(&formatted, depth);
            true
        })
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
    fn write_indented_block(&mut self, text: &str, depth: usize) {
        let indent = self.indent(depth);
        for line in text.lines() {
            if line.trim().is_empty() {
                self.out.push('\n');
            } else {
                self.out.push_str(&indent);
                self.out.push_str(line);
            }
            self.out.push('\n');
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
