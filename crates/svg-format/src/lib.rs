//! Deterministic structural formatting for SVG documents.

use tree_sitter::{Node, Parser};

/// Attribute ordering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
pub enum WrappedAttributeIndent {
    /// Add one normal indentation unit.
    #[default]
    OneLevel,
    /// Align to the column after `<tag ` so wrapped attributes line up visually.
    AlignToTagName,
}

/// How blank lines between sibling elements are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    fn new(source: &'a [u8], options: FormatOptions) -> Self {
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
            "source_file" => self.format_children(node, depth, fmt),
            "svg_root_element" | "element" => self.format_element_like(node, depth, fmt),
            "start_tag" => self.write_tag_node(node, depth, false),
            "self_closing_tag" => self.write_tag_node(node, depth, true),
            "end_tag" => {
                let text = self.node_text(node).trim().to_string();
                self.write_line(depth, &text);
            }
            "style_text_double" | "style_text_single" | "script_text_double"
            | "script_text_single" => {
                self.write_preserved_block_text(node, depth);
            }
            "text" | "raw_text" => {
                self.write_text_node(node, depth);
            }
            "comment"
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

        // Text-content elements with entity references (&lt; &amp; etc.):
        // extract raw content between tags as a unified text block so entity
        // references aren't split onto separate lines.
        if is_text_content_element(&tag_name) {
            let has_entity_refs = children.iter().any(|c| c.kind() == "entity_reference");

            if has_entity_refs {
                let start_node = children.iter().find(|c| c.kind() == "start_tag").copied();
                let end_node = children.iter().find(|c| c.kind() == "end_tag").copied();
                let all_inline = children
                    .iter()
                    .filter(|c| !matches!(c.kind(), "start_tag" | "end_tag"))
                    .all(|c| matches!(c.kind(), "text" | "raw_text" | "entity_reference"));

                if let (Some(start), Some(end)) = (start_node, end_node)
                    && all_inline
                {
                    self.format_text_content_element(start, end, depth);
                    return;
                }
            }
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
                "start_tag" => {
                    self.format_node(child, depth, fmt);
                }
                "end_tag" => {
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
    /// Called only when entity_reference nodes are present — the whitespace
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

    fn self_closing_suffix(&self) -> &'static str {
        if self.options.space_before_self_close {
            " />"
        } else {
            "/>"
        }
    }

    fn render_attribute(&self, attribute: &ParsedAttribute) -> String {
        if let Some(value) = &attribute.value {
            format!("{}={}", attribute.name, self.render_attribute_value(value))
        } else {
            attribute.name.clone()
        }
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

    /// Try to format embedded text (style/script raw_text) via the callback.
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
        if let Some(formatted) = fmt(req) {
            self.write_indented_block(&formatted, depth);
            true
        } else {
            false
        }
    }

    /// Try to format foreignObject inner content via the callback.
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
            .map(|t| self.node_text(t).trim())
            .unwrap_or("");
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
            BlankLines::Insert => {
                if prev_was_comment {
                    0
                } else {
                    1
                }
            }
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

struct ParsedTag {
    name: String,
    attributes: Vec<ParsedAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedAttribute {
    name: String,
    value: Option<ParsedAttributeValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedAttributeValue {
    raw: String,
    original_quote: Option<char>,
}

fn reorder_attributes(attributes: &mut [ParsedAttribute], mode: AttributeSort) {
    match mode {
        AttributeSort::None => {}
        AttributeSort::Alphabetical => {
            attributes.sort_by_key(|attribute| attribute.name.to_ascii_lowercase());
        }
        AttributeSort::Canonical => {
            attributes.sort_by_key(|attribute| canonical_attribute_sort_key(&attribute.name));
        }
    }
}

fn canonical_attribute_sort_key(name: &str) -> (u8, u16, String) {
    let lowered = name.to_ascii_lowercase();

    if lowered == "xmlns" {
        return (0, 0, lowered);
    }
    if lowered.starts_with("xmlns:") {
        return (0, 1, lowered);
    }
    if lowered == "id" {
        return (1, 0, lowered);
    }
    if lowered == "class" {
        return (1, 1, lowered);
    }
    if let Some(order) = canonical_geometry_order(&lowered) {
        return (2, order, lowered);
    }
    (3, u16::MAX, lowered)
}

fn canonical_geometry_order(name: &str) -> Option<u16> {
    // Common SVG geometry/presentation progression before fallback alphabetical ordering.
    let order = [
        "x",
        "y",
        "x1",
        "y1",
        "x2",
        "y2",
        "cx",
        "cy",
        "r",
        "rx",
        "ry",
        "width",
        "height",
        "viewbox",
        "preserveaspectratio",
        "href",
        "xlink:href",
        "d",
        "points",
        "transform",
        "fill",
        "stroke",
        "stroke-width",
        "style",
    ];
    order
        .iter()
        .position(|candidate| *candidate == name)
        .map(|i| i as u16)
}

fn parse_tag(raw: &str, self_closing: bool) -> Option<ParsedTag> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('<') {
        return None;
    }

    let inner = if self_closing {
        if let Some(stripped) = trimmed.strip_suffix("/>") {
            stripped
        } else {
            trimmed.strip_suffix(" />")?
        }
    } else {
        trimmed.strip_suffix('>')?
    };
    let inner = inner.strip_prefix('<')?.trim();
    if inner.is_empty() {
        return None;
    }

    let mut i = 0usize;
    let bytes = inner.as_bytes();
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i == 0 {
        return None;
    }

    let name = inner[..i].to_string();
    let mut attrs = Vec::new();
    let mut j = i;

    while j < bytes.len() {
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() {
            break;
        }

        let start = j;
        while j < bytes.len() && !bytes[j].is_ascii_whitespace() && bytes[j] != b'=' {
            j += 1;
        }
        if start == j {
            break;
        }

        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }

        if j < bytes.len() && bytes[j] == b'=' {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                let quote = bytes[j];
                j += 1;
                while j < bytes.len() {
                    if bytes[j] == quote {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
            } else {
                while j < bytes.len() && !bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
            }
        }

        let attribute = inner[start..j].trim();
        if !attribute.is_empty() {
            attrs.push(parse_attribute(attribute));
        }
    }

    Some(ParsedTag {
        name,
        attributes: attrs,
    })
}

fn parse_attribute(attribute: &str) -> ParsedAttribute {
    let trimmed = attribute.trim();
    if let Some((name, raw_value)) = trimmed.split_once('=') {
        let name = name.trim().to_string();
        let raw_value = raw_value.trim();
        let value = if let Some(inner) = raw_value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        {
            ParsedAttributeValue {
                raw: inner.to_string(),
                original_quote: Some('"'),
            }
        } else if let Some(inner) = raw_value
            .strip_prefix('\'')
            .and_then(|value| value.strip_suffix('\''))
        {
            ParsedAttributeValue {
                raw: inner.to_string(),
                original_quote: Some('\''),
            }
        } else {
            ParsedAttributeValue {
                raw: raw_value.to_string(),
                original_quote: None,
            }
        };

        ParsedAttribute {
            name,
            value: Some(value),
        }
    } else {
        ParsedAttribute {
            name: trimmed.to_string(),
            value: None,
        }
    }
}

/// Remove common leading whitespace from a block of text,
/// trimming leading/trailing blank lines.
fn dedent_block(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let first_non_empty = lines.iter().position(|l| !l.trim().is_empty());
    let last_non_empty = lines.iter().rposition(|l| !l.trim().is_empty());
    let (Some(start), Some(end)) = (first_non_empty, last_non_empty) else {
        return String::new();
    };

    let block = &lines[start..=end];
    let min_indent = block
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);

    block
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                ""
            } else {
                let skip: usize = l.chars().take(min_indent).map(char::len_utf8).sum();
                &l[skip..]
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Collapse runs of whitespace into single spaces and trim.
/// SVG elements whose content is whitespace-sensitive inline text.
///
/// For these elements the formatter preserves raw content between the start
/// and end tags as a single text block instead of formatting each child node
/// on its own line (which would break entity references like `&lt;` apart
/// from surrounding text).
fn is_text_content_element(tag_name: &str) -> bool {
    matches!(tag_name, "text" | "tspan" | "textPath" | "title" | "desc")
}

#[derive(Clone, Copy)]
enum TextContentToken<'a> {
    Text(&'a str),
    Entity(&'a str),
    Whitespace(&'a str),
}

fn normalize_text_content_with_entities(text: &str) -> String {
    let mut tokens = Vec::new();
    let mut offset = 0;
    while offset < text.len() {
        let rest = &text[offset..];
        let Some(ch) = rest.chars().next() else {
            break;
        };

        if ch.is_whitespace() {
            let start = offset;
            offset += ch.len_utf8();
            while offset < text.len() {
                let Some(next) = text[offset..].chars().next() else {
                    break;
                };
                if !next.is_whitespace() {
                    break;
                }
                offset += next.len_utf8();
            }
            tokens.push(TextContentToken::Whitespace(&text[start..offset]));
            continue;
        }

        if ch == '&'
            && let Some(len) = entity_reference_len(rest)
        {
            tokens.push(TextContentToken::Entity(&text[offset..offset + len]));
            offset += len;
            continue;
        }

        let start = offset;
        offset += ch.len_utf8();
        while offset < text.len() {
            let Some(next) = text[offset..].chars().next() else {
                break;
            };
            if next.is_whitespace() {
                break;
            }
            if next == '&' && entity_reference_len(&text[offset..]).is_some() {
                break;
            }
            offset += next.len_utf8();
        }
        tokens.push(TextContentToken::Text(&text[start..offset]));
    }

    let mut normalized = String::new();
    for (index, token) in tokens.iter().enumerate() {
        match token {
            TextContentToken::Text(text) | TextContentToken::Entity(text) => {
                normalized.push_str(text);
            }
            TextContentToken::Whitespace(space) => {
                let prev = tokens[..index]
                    .iter()
                    .rev()
                    .find(|token| !matches!(token, TextContentToken::Whitespace(_)));
                let next = tokens[index + 1..]
                    .iter()
                    .find(|token| !matches!(token, TextContentToken::Whitespace(_)));

                let (Some(prev), Some(next)) = (prev, next) else {
                    continue;
                };

                if should_strip_entity_boundary_space(*prev, *next, space) {
                    continue;
                }

                if !normalized.ends_with(' ') {
                    normalized.push(' ');
                }
            }
        }
    }

    normalized.trim().to_string()
}

fn should_strip_entity_boundary_space(
    prev: TextContentToken<'_>,
    next: TextContentToken<'_>,
    whitespace: &str,
) -> bool {
    if !whitespace.contains(['\n', '\r']) {
        return false;
    }

    matches!(prev, TextContentToken::Entity(entity) if is_open_angle_entity(entity))
        && matches!(next, TextContentToken::Text(_))
        || matches!(prev, TextContentToken::Text(_))
            && matches!(next, TextContentToken::Entity(entity) if is_close_angle_entity(entity))
}

fn entity_reference_len(text: &str) -> Option<usize> {
    let end = text.find(';')?;
    let candidate = &text[..=end];
    let body = &candidate[1..candidate.len() - 1];
    if body.is_empty() {
        return None;
    }

    let valid = if let Some(hex) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
        !hex.is_empty() && hex.chars().all(|ch| ch.is_ascii_hexdigit())
    } else if let Some(decimal) = body.strip_prefix('#') {
        !decimal.is_empty() && decimal.chars().all(|ch| ch.is_ascii_digit())
    } else {
        body.chars().all(|ch| ch.is_ascii_alphanumeric())
    };

    valid.then_some(candidate.len())
}

fn is_open_angle_entity(entity: &str) -> bool {
    matches!(
        entity.to_ascii_lowercase().as_str(),
        "&lt;" | "&#60;" | "&#x3c;"
    )
}

fn is_close_angle_entity(entity: &str) -> bool {
    matches!(
        entity.to_ascii_lowercase().as_str(),
        "&gt;" | "&#62;" | "&#x3e;"
    )
}

fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_ws = true; // treat start as whitespace to trim leading
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                result.push(' ');
            }
            prev_ws = true;
        } else {
            result.push(ch);
            prev_ws = false;
        }
    }
    // trim trailing space
    if result.ends_with(' ') {
        result.pop();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_nested_elements() {
        let input = r#"<svg><g><rect/></g></svg>"#;
        let expected = "<svg>\n\t<g>\n\t\t<rect />\n\t</g>\n</svg>";
        assert_eq!(format(input), expected);
    }

    #[test]
    fn formats_multiline_attributes_consistently() {
        let input = r#"<svg><linearGradient id="sky" x1="0%" y1="0%" x2="0%" y2="100%"></linearGradient></svg>"#;
        let options = FormatOptions {
            max_inline_tag_width: 24,
            ..Default::default()
        };
        let expected = "<svg>\n\t<linearGradient\n\t\tid=\"sky\"\n\t\tx1=\"0%\"\n\t\ty1=\"0%\"\n\t\tx2=\"0%\"\n\t\ty2=\"100%\">\n\t</linearGradient>\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn canonical_attribute_ordering() {
        let input = r#"<svg><rect y="2" width="4" class="hero" id="x" x="1" height="5"/></svg>"#;
        let expected = "<svg>\n\t<rect id=\"x\" class=\"hero\" x=\"1\" y=\"2\" width=\"4\" height=\"5\" />\n</svg>";
        assert_eq!(format(input), expected);
    }

    #[test]
    fn preserves_style_block_content_shape() {
        // Default text_content is Maintain — preserves relative indentation.
        // .b has 2 extra spaces of indentation relative to .a in the source,
        // which is preserved in the output.
        let input = r#"<svg><style>
  .a { fill: red; }
    .b { stroke: blue; }
</style></svg>"#;
        let expected = "<svg>\n\t<style>\n\t\t.a { fill: red; }\n\t\t  .b { stroke: blue; }\n\t</style>\n</svg>";
        assert_eq!(format(input), expected);
    }

    #[test]
    fn attribute_sort_none_preserves_input_order() {
        let input = r#"<svg><rect y="2" width="4" class="hero" id="x" x="1" height="5"/></svg>"#;
        let options = FormatOptions {
            attribute_sort: AttributeSort::None,
            ..Default::default()
        };
        let expected = "<svg>\n\t<rect y=\"2\" width=\"4\" class=\"hero\" id=\"x\" x=\"1\" height=\"5\" />\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn attribute_sort_alphabetical_orders_by_name() {
        let input = r#"<svg><rect y="2" width="4" class="hero" id="x" x="1" height="5"/></svg>"#;
        let options = FormatOptions {
            attribute_sort: AttributeSort::Alphabetical,
            ..Default::default()
        };
        let expected = "<svg>\n\t<rect class=\"hero\" height=\"5\" id=\"x\" width=\"4\" x=\"1\" y=\"2\" />\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn quote_style_double_normalizes_quotes() {
        let input = r#"<svg><rect class='hero' id='x'/></svg>"#;
        let options = FormatOptions {
            quote_style: QuoteStyle::Double,
            ..Default::default()
        };
        let expected = "<svg>\n\t<rect id=\"x\" class=\"hero\" />\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn quote_style_single_normalizes_quotes() {
        let input = r#"<svg><rect class="hero" id="x"/></svg>"#;
        let options = FormatOptions {
            quote_style: QuoteStyle::Single,
            ..Default::default()
        };
        let expected = "<svg>\n\t<rect id='x' class='hero' />\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn attribute_layout_single_line_ignores_width_trigger() {
        let input = r#"<svg><linearGradient id="sky" x1="0%" y1="0%" x2="0%" y2="100%"></linearGradient></svg>"#;
        let options = FormatOptions {
            attribute_layout: AttributeLayout::SingleLine,
            max_inline_tag_width: 10,
            ..Default::default()
        };
        let expected = "<svg>\n\t<linearGradient id=\"sky\" x1=\"0%\" y1=\"0%\" x2=\"0%\" y2=\"100%\">\n\t</linearGradient>\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn space_before_self_close_false_removes_spacing() {
        let input = r#"<svg><rect id="x"/></svg>"#;
        let options = FormatOptions {
            space_before_self_close: false,
            ..Default::default()
        };
        let expected = "<svg>\n\t<rect id=\"x\"/>\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn wrapped_attribute_indent_align_to_tag_name() {
        let input = r#"<svg><linearGradient id="sky" x1="0%" y1="0%"></linearGradient></svg>"#;
        let options = FormatOptions {
            attribute_layout: AttributeLayout::MultiLine,
            wrapped_attribute_indent: WrappedAttributeIndent::AlignToTagName,
            ..Default::default()
        };
        let aligned = format!("\t{}", " ".repeat("linearGradient".len() + 2));
        let expected = format!(
            "<svg>\n\t<linearGradient\n{aligned}id=\"sky\"\n{aligned}x1=\"0%\"\n{aligned}y1=\"0%\">\n\t</linearGradient>\n</svg>"
        );
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn parse_error_returns_original_source() {
        let input = r#"<svg><path d="m0 0 l"/></svg>"#;
        assert_eq!(format(input), input);
    }

    #[test]
    fn text_content_maintain_preserves_relative_indentation() {
        let input = "<svg><text>\n  hello\n    world\n</text></svg>";
        let options = FormatOptions {
            text_content: TextContentMode::Maintain,
            ..Default::default()
        };
        let expected = "<svg>\n\t<text>\n\t\thello\n\t\t  world\n\t</text>\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn text_content_collapse_collapses_whitespace() {
        let input = "<svg><text>\n  hello   world  \n    foo    bar  \n</text></svg>";
        let options = FormatOptions {
            text_content: TextContentMode::Collapse,
            ..Default::default()
        };
        let expected = "<svg>\n\t<text>\n\t\thello world\n\t\tfoo bar\n\t</text>\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn text_content_prettify_trims_and_reindents() {
        let input = "<svg><text>\n  hello  \n    world  \n</text></svg>";
        let options = FormatOptions {
            text_content: TextContentMode::Prettify,
            ..Default::default()
        };
        let expected = "<svg>\n\t<text>\n\t\thello\n\t\tworld\n\t</text>\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn text_content_default_is_maintain() {
        assert_eq!(
            FormatOptions::default().text_content,
            TextContentMode::Maintain
        );
    }

    #[test]
    fn format_with_host_delegates_style_content() {
        let input = "<svg><style>.a{fill:red}</style></svg>";
        let mut called_lang = None;
        let mut called_content = None;
        let result = format_with_host(input, FormatOptions::default(), &mut |req| {
            called_lang = Some(req.language);
            called_content = Some(req.content.to_string());
            Some(".a {\n  fill: red;\n}".to_string())
        });
        assert_eq!(called_lang, Some(EmbeddedLanguage::Css));
        assert_eq!(called_content.as_deref(), Some(".a{fill:red}"));
        // Re-indented CSS at depth 2 (inside <svg><style>)
        assert_eq!(
            result,
            "<svg>\n\t<style>\n\t\t.a {\n\t\t  fill: red;\n\t\t}\n\t</style>\n</svg>"
        );
    }

    #[test]
    fn format_with_host_falls_back_when_callback_returns_none() {
        let input = "<svg><style>.a { fill: red; }</style></svg>";
        let result = format_with_host(input, FormatOptions::default(), &mut |_| None);
        let fallback = format_with_options(input, FormatOptions::default());
        assert_eq!(result, fallback);
    }

    #[test]
    fn format_with_host_delegates_script_content() {
        let input = "<svg><script>alert(1)</script></svg>";
        let mut called_lang = None;
        format_with_host(input, FormatOptions::default(), &mut |req| {
            called_lang = Some(req.language);
            None
        });
        assert_eq!(called_lang, Some(EmbeddedLanguage::JavaScript));
    }

    #[test]
    fn format_with_host_delegates_foreign_object_content() {
        let input = r#"<svg><foreignObject width="200" height="200"><div>hello</div></foreignObject></svg>"#;
        let mut called_lang = None;
        let mut called_content = None;
        format_with_host(input, FormatOptions::default(), &mut |req| {
            called_lang = Some(req.language);
            called_content = Some(req.content.to_string());
            None
        });
        assert_eq!(called_lang, Some(EmbeddedLanguage::Html));
        assert!(called_content.unwrap().contains("<div>hello</div>"));
    }

    #[test]
    fn format_with_host_foreign_object_with_formatted_html() {
        let input = r#"<svg><foreignObject width="200" height="200"><div>hello</div></foreignObject></svg>"#;
        let result = format_with_host(input, FormatOptions::default(), &mut |req| {
            if req.language == EmbeddedLanguage::Html {
                Some("<div>\n  hello\n</div>".to_string())
            } else {
                None
            }
        });
        assert_eq!(
            result,
            "<svg>\n\t<foreignObject width=\"200\" height=\"200\">\n\t\t<div>\n\t\t  hello\n\t\t</div>\n\t</foreignObject>\n</svg>"
        );
    }

    #[test]
    fn blank_lines_remove_strips_all_gaps() {
        let input = "<svg>\n\t<rect />\n\n\t<!--legend-->\n\t<circle />\n</svg>";
        let options = FormatOptions {
            blank_lines: BlankLines::Remove,
            ..Default::default()
        };
        let expected = "<svg>\n\t<rect />\n\t<!--legend-->\n\t<circle />\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn blank_lines_preserve_keeps_source_gaps() {
        let input = "<svg>\n\t<rect />\n\n\n\t<!--legend-->\n\t<circle />\n</svg>";
        let options = FormatOptions {
            blank_lines: BlankLines::Preserve,
            ..Default::default()
        };
        let expected = "<svg>\n\t<rect />\n\n\n\t<!--legend-->\n\t<circle />\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn blank_lines_truncate_collapses_multiple() {
        let input = "<svg>\n\t<rect />\n\n\n\n\t<!--legend-->\n\t<circle />\n</svg>";
        let options = FormatOptions {
            blank_lines: BlankLines::Truncate,
            ..Default::default()
        };
        let expected = "<svg>\n\t<rect />\n\n\t<!--legend-->\n\t<circle />\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn blank_lines_truncate_keeps_single() {
        let input = "<svg>\n\t<rect />\n\n\t<!--legend-->\n\t<circle />\n</svg>";
        let options = FormatOptions {
            blank_lines: BlankLines::Truncate,
            ..Default::default()
        };
        let expected = "<svg>\n\t<rect />\n\n\t<!--legend-->\n\t<circle />\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn blank_lines_insert_adds_gaps() {
        let input = "<svg><rect/><circle/></svg>";
        let options = FormatOptions {
            blank_lines: BlankLines::Insert,
            ..Default::default()
        };
        let expected = "<svg>\n\t<rect />\n\n\t<circle />\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn blank_lines_insert_comments_attach_downward() {
        let input = "<svg><rect/><!--legend--><circle/></svg>";
        let options = FormatOptions {
            blank_lines: BlankLines::Insert,
            ..Default::default()
        };
        // Blank line before comment, but NOT between comment and circle.
        let expected = "<svg>\n\t<rect />\n\n\t<!--legend-->\n\t<circle />\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn blank_lines_insert_normalizes_multiple_to_one() {
        let input = "<svg>\n\t<rect />\n\n\n\n\t<circle />\n</svg>";
        let options = FormatOptions {
            blank_lines: BlankLines::Insert,
            ..Default::default()
        };
        let expected = "<svg>\n\t<rect />\n\n\t<circle />\n</svg>";
        assert_eq!(format_with_options(input, options), expected);
    }

    #[test]
    fn blank_lines_default_is_truncate() {
        assert_eq!(FormatOptions::default().blank_lines, BlankLines::Truncate);
    }

    #[test]
    fn ignore_file_skips_formatting() {
        let input = "<svg><rect y=\"2\" x=\"1\"/>\n<!-- svg-format-ignore-file -->\n</svg>";
        assert_eq!(format(input), input);
    }

    #[test]
    fn ignore_next_skips_one_sibling() {
        let input = "<svg>\n<!-- svg-format-ignore -->\n<rect y=\"2\" x=\"1\"/>\n<circle cx=\"1\" cy=\"2\" r=\"3\"/>\n</svg>";
        let result = format(input);
        // rect keeps original attr order (y before x), circle gets sorted
        assert!(result.contains("y=\"2\" x=\"1\""));
        assert!(result.contains("<circle cx=\"1\" cy=\"2\" r=\"3\" />"));
    }

    #[test]
    fn ignore_range_preserves_content() {
        let input = "<svg>\n<rect id=\"a\"/>\n<!-- svg-format-ignore-start -->\n<rect y=\"2\" x=\"1\"/>\n<circle r=\"3\" cx=\"1\" cy=\"2\"/>\n<!-- svg-format-ignore-end -->\n<rect y=\"2\" x=\"1\" id=\"b\"/>\n</svg>";
        let result = format(input);
        // Inside range: preserved verbatim
        assert!(result.contains("<rect y=\"2\" x=\"1\"/>"));
        assert!(result.contains("<circle r=\"3\" cx=\"1\" cy=\"2\"/>"));
        // Outside range: formatted
        assert!(result.contains("<rect id=\"b\" x=\"1\" y=\"2\" />"));
    }

    #[test]
    fn custom_ignore_prefix_works() {
        let input = "<svg><!-- custom-ignore-file --><rect/></svg>";
        let options = FormatOptions {
            ignore_prefixes: vec!["custom".to_string()],
            ..Default::default()
        };
        assert_eq!(format_with_options(input, options), input);
    }

    #[test]
    fn ignore_file_only_matches_comments_not_text() {
        // The string "svg-format-ignore-file" inside a <text> should NOT
        // trigger file-level ignore — only an actual comment does.
        let input = "<svg><text>svg-format-ignore-file</text></svg>";
        let result = format(input);
        // Should be formatted (not returned as-is)
        assert_ne!(result, input);
    }

    #[test]
    fn ignore_range_preserves_gaps_verbatim() {
        // Blank lines and indentation inside ignore range must survive.
        let input = "<svg>\n<!-- svg-format-ignore-start -->\n<rect y=\"2\"\n      x=\"1\"/>\n\n<circle r=\"3\"/>\n<!-- svg-format-ignore-end -->\n</svg>";
        let result = format(input);
        // The exact content between start/end markers should be preserved.
        assert!(result.contains("<rect y=\"2\"\n      x=\"1\"/>\n\n<circle r=\"3\"/>"));
    }

    #[test]
    fn ignore_next_preserves_inline_text() {
        let input = "<svg>\n<!-- svg-format-ignore -->\n<text>  spaced  </text>\n</svg>";
        let result = format(input);
        assert!(result.contains("<text>  spaced  </text>"));
    }

    #[test]
    fn ignore_range_outside_svg_with_blank_lines_is_idempotent() {
        // Reproduces diagnostics-errors.svg: ignore range after </svg>
        // with blank lines between comment groups.
        let input = "\
</svg>
<!-- dprint-ignore-start -->
<!-- comment A -->

<!-- comment B -->
<!-- comment C -->

<!-- comment D -->
<!-- comment E -->
<!-- comment F -->

<!-- comment G -->
<!-- comment H -->
<!-- comment I -->
<!-- comment J -->
<!-- dprint-ignore-end -->
";
        let opts = FormatOptions {
            ignore_prefixes: vec!["dprint".into()],
            blank_lines: BlankLines::Insert,
            ..Default::default()
        };
        let pass1 = format_with_options(input, opts.clone());
        let pass2 = format_with_options(&pass1, opts.clone());
        assert_eq!(
            pass1, pass2,
            "not idempotent:\n--- pass1:\n{pass1}\n--- pass2:\n{pass2}"
        );
    }

    #[test]
    fn ignore_range_inside_svg_with_blank_lines_is_idempotent() {
        let input = "\
<svg>
\t<rect />
\t<!-- dprint-ignore-start -->
\t<rect y=\"2\" x=\"1\"/>

\t<circle r=\"3\"/>
\t<!-- dprint-ignore-end -->
\t<rect />
</svg>
";
        let opts = FormatOptions {
            ignore_prefixes: vec!["dprint".into()],
            blank_lines: BlankLines::Insert,
            ..Default::default()
        };
        let pass1 = format_with_options(input, opts.clone());
        let pass2 = format_with_options(&pass1, opts.clone());
        assert_eq!(
            pass1, pass2,
            "not idempotent:\n--- pass1:\n{pass1}\n--- pass2:\n{pass2}"
        );
    }

    #[test]
    fn ignore_range_preserves_exact_source_bytes() {
        // The exact bytes between ignore-start and ignore-end must be
        // preserved, including blank lines, indentation, and spacing.
        let input = "\
<svg>
<!-- dprint-ignore-start -->
<rect y=\"2\"
      x=\"1\"/>

<circle r=\"3\"/>
<!-- dprint-ignore-end -->
</svg>
";
        let opts = FormatOptions {
            ignore_prefixes: vec!["dprint".into()],
            ..Default::default()
        };
        let result = format_with_options(input, opts);
        assert!(
            result.contains("<rect y=\"2\"\n      x=\"1\"/>\n\n<circle r=\"3\"/>"),
            "source bytes not preserved:\n{result}"
        );
    }

    #[test]
    fn ignore_range_with_insert_blank_lines_is_stable() {
        // Insert mode should not add blank lines inside an ignore range.
        let input = "\
<svg>
\t<rect />
\t<!-- dprint-ignore-start -->
\t<rect y=\"2\" x=\"1\"/>
\t<circle r=\"3\"/>
\t<!-- dprint-ignore-end -->
\t<rect />
</svg>
";
        let opts = FormatOptions {
            ignore_prefixes: vec!["dprint".into()],
            blank_lines: BlankLines::Insert,
            ..Default::default()
        };
        let pass1 = format_with_options(input, opts.clone());
        // No blank line should be inserted between the two ignored elements.
        assert!(
            pass1.contains("<rect y=\"2\" x=\"1\"/>\n\t<circle r=\"3\"/>"),
            "insert mode modified ignored content:\n{pass1}"
        );
    }

    // ── Edge-case ignore directive tests ────────────────────────────

    #[test]
    fn two_consecutive_ignore_next_skip_two_siblings() {
        let input = "<svg>\n<!-- svg-format-ignore -->\n<rect y=\"2\" x=\"1\"/>\n<!-- svg-format-ignore -->\n<circle r=\"3\" cx=\"1\" cy=\"2\"/>\n<ellipse ry=\"1\" rx=\"2\"/>\n</svg>";
        let result = format(input);
        // Both rect and circle should be unformatted (original attr order)
        assert!(
            result.contains("y=\"2\" x=\"1\""),
            "first ignored element was formatted:\n{result}"
        );
        assert!(
            result.contains("r=\"3\" cx=\"1\" cy=\"2\""),
            "second ignored element was formatted:\n{result}"
        );
        // ellipse should be formatted (canonical order)
        assert!(
            result.contains("<ellipse rx=\"2\" ry=\"1\" />"),
            "non-ignored element was not formatted:\n{result}"
        );
    }

    #[test]
    fn ignore_end_without_start_is_harmless() {
        // A stray ignore-end should not crash or alter behavior.
        let input = "<svg>\n<!-- svg-format-ignore-end -->\n<rect y=\"2\" x=\"1\"/>\n</svg>";
        let result = format(input);
        // rect should still be formatted (canonical order)
        assert!(
            result.contains("<rect x=\"1\" y=\"2\" />"),
            "formatting was suppressed by stray ignore-end:\n{result}"
        );
    }

    #[test]
    fn ignore_start_without_end_ignores_rest_of_siblings() {
        let input = "<svg>\n<rect id=\"a\"/>\n<!-- svg-format-ignore-start -->\n<rect y=\"2\" x=\"1\"/>\n<circle r=\"3\" cx=\"1\" cy=\"2\"/>\n</svg>";
        let result = format(input);
        // rect id=a should be formatted
        assert!(
            result.contains("<rect id=\"a\" />"),
            "element before ignore-start was not formatted:\n{result}"
        );
        // Both elements after ignore-start should be unformatted
        assert!(
            result.contains("y=\"2\" x=\"1\""),
            "element after unclosed ignore-start was formatted:\n{result}"
        );
        assert!(
            result.contains("r=\"3\" cx=\"1\" cy=\"2\""),
            "element after unclosed ignore-start was formatted:\n{result}"
        );
    }

    #[test]
    fn nested_ignore_start_is_harmless() {
        // A second ignore-start inside an active range should not break anything.
        let input = "<svg>\n<!-- svg-format-ignore-start -->\n<rect y=\"2\" x=\"1\"/>\n<!-- svg-format-ignore-start -->\n<circle r=\"3\" cx=\"1\" cy=\"2\"/>\n<!-- svg-format-ignore-end -->\n<ellipse ry=\"1\" rx=\"2\"/>\n</svg>";
        let result = format(input);
        // rect and circle inside range should be unformatted
        assert!(
            result.contains("y=\"2\" x=\"1\""),
            "inner content was formatted:\n{result}"
        );
        assert!(
            result.contains("r=\"3\" cx=\"1\" cy=\"2\""),
            "inner content was formatted:\n{result}"
        );
        // ellipse after ignore-end should be formatted
        assert!(
            result.contains("<ellipse rx=\"2\" ry=\"1\" />"),
            "element after ignore-end was not formatted:\n{result}"
        );
    }

    #[test]
    fn ignore_next_inside_ignore_range_is_preserved_verbatim() {
        let input = "<svg>\n<!-- svg-format-ignore-start -->\n<!-- svg-format-ignore -->\n<rect y=\"2\" x=\"1\"/>\n<!-- svg-format-ignore-end -->\n</svg>";
        let result = format(input);
        // The ignore directive comment inside the range should be preserved
        assert!(
            result.contains("<!-- svg-format-ignore -->"),
            "inner directive was stripped:\n{result}"
        );
        assert!(
            result.contains("y=\"2\" x=\"1\""),
            "inner content was formatted:\n{result}"
        );
    }

    #[test]
    fn ignore_next_inside_range_does_not_leak_after_end() {
        // An ignore directive inside a range must not leak ignore_next
        // state past the ignore-end, causing the next sibling to be skipped.
        let input = "<svg>\n<!-- svg-format-ignore-start -->\n<!-- svg-format-ignore -->\n<rect y=\"2\" x=\"1\"/>\n<!-- svg-format-ignore-end -->\n<ellipse ry=\"1\" rx=\"2\"/>\n</svg>";
        let result = format(input);
        // ellipse after ignore-end must be formatted (canonical order)
        assert!(
            result.contains("<ellipse rx=\"2\" ry=\"1\" />"),
            "ignore_next leaked past ignore-end:\n{result}"
        );
    }

    #[test]
    fn ignore_directives_work_inside_nested_elements() {
        let input = "<svg>\n<g>\n<!-- svg-format-ignore -->\n<rect y=\"2\" x=\"1\"/>\n<circle r=\"3\" cx=\"1\" cy=\"2\"/>\n</g>\n</svg>";
        let result = format(input);
        // rect inside <g> should be unformatted
        assert!(
            result.contains("y=\"2\" x=\"1\""),
            "ignored element inside <g> was formatted:\n{result}"
        );
        // circle should be formatted
        assert!(
            result.contains("<circle cx=\"1\" cy=\"2\" r=\"3\" />"),
            "non-ignored element inside <g> was not formatted:\n{result}"
        );
    }

    #[test]
    fn ignore_next_with_insert_puts_blank_line_before_comment() {
        let input =
            "<svg><rect/>\n<!-- svg-format-ignore -->\n<circle r=\"3\" cx=\"1\" cy=\"2\"/>\n</svg>";
        let opts = FormatOptions {
            blank_lines: BlankLines::Insert,
            ..Default::default()
        };
        let result = format_with_options(input, opts);
        // Blank line should be before the ignore comment (between rect and comment),
        // not between the comment and the ignored circle.
        assert!(
            result.contains("<rect />\n\n\t<!-- svg-format-ignore -->"),
            "no blank line before ignore comment:\n{result}"
        );
        assert!(
            result.contains("<!-- svg-format-ignore -->\n<circle"),
            "blank line inserted between comment and ignored element:\n{result}"
        );
    }

    #[test]
    fn ignore_file_inside_nested_element_still_skips_file() {
        let input =
            "<svg>\n<g>\n<!-- svg-format-ignore-file -->\n<rect y=\"2\" x=\"1\"/>\n</g>\n</svg>";
        assert_eq!(
            format(input),
            input,
            "ignore-file inside nested element did not skip formatting"
        );
    }

    #[test]
    fn ignore_range_as_first_child_after_start_tag() {
        // ignore-start immediately after <svg> — prev_end is None when
        // handle_ignore first sees the start directive.
        let input = "<svg>\n<!-- svg-format-ignore-start -->\n<rect y=\"2\" x=\"1\"/>\n<!-- svg-format-ignore-end -->\n</svg>";
        let result = format(input);
        assert!(
            result.contains("y=\"2\" x=\"1\""),
            "ignore range lost content when prev_end was None:\n{result}"
        );
    }

    #[test]
    fn ignore_range_first_content_child_not_lost() {
        // The first element inside the range must not be dropped due to
        // write_source_span receiving None as `from`.
        let input = "<svg>\n<!-- svg-format-ignore-start -->\n<rect y=\"2\" x=\"1\"/>\n<circle r=\"3\"/>\n<!-- svg-format-ignore-end -->\n</svg>";
        let result = format(input);
        assert!(
            result.contains("<rect y=\"2\" x=\"1\"/>"),
            "first element inside range was lost:\n{result}"
        );
        assert!(
            result.contains("<circle r=\"3\"/>"),
            "second element inside range was lost:\n{result}"
        );
    }

    // ── Text-content element whitespace sensitivity ──────────────

    #[test]
    fn text_element_entity_refs_stay_inline() {
        let input =
            r#"<svg><text class="label" x="36" y="58">Embedded &lt;style&gt; colors</text></svg>"#;
        let expected = "<svg>\n\t<text class=\"label\" x=\"36\" y=\"58\">Embedded &lt;style&gt; colors</text>\n</svg>";
        assert_eq!(format(input), expected);
    }

    #[test]
    fn text_element_entity_refs_idempotent() {
        let input =
            r#"<svg><text class="label" x="36" y="58">Embedded &lt;style&gt; colors</text></svg>"#;
        let once = format(input);
        let twice = format(&once);
        assert_eq!(once, twice, "not idempotent:\n{once}");
    }

    #[test]
    fn text_element_broken_entity_refs_repaired() {
        // Source previously mangled by a formatter that split entity refs
        // onto separate lines — must collapse back to a single line.
        let input = "<svg>\n<text class=\"label\" x=\"36\" y=\"58\">\n\tEmbedded\n\t&lt;\n\tstyle\n\t&gt;\n\tcolors\n</text>\n</svg>";
        let expected = "<svg>\n\t<text class=\"label\" x=\"36\" y=\"58\">Embedded &lt;style&gt; colors</text>\n</svg>";
        assert_eq!(format(input), expected);
    }

    #[test]
    fn text_element_comparison_entity_refs_keep_spaces() {
        let input = "<svg><text>a &lt; b &gt; c</text></svg>";
        let expected = "<svg>\n\t<text>a &lt; b &gt; c</text>\n</svg>";
        assert_eq!(format(input), expected);
    }

    #[test]
    fn desc_element_entity_ref_stays_inline() {
        let input = "<svg><desc>A &amp; B</desc></svg>";
        let expected = "<svg>\n\t<desc>A &amp; B</desc>\n</svg>";
        assert_eq!(format(input), expected);
    }

    #[test]
    fn text_element_long_entity_ref_content_wraps_to_own_line() {
        // Content with entity refs that exceeds max_inline_tag_width.
        let input = "<svg><text class=\"subtle\" x=\"36\" y=\"84\">hex, rgb(a), hsl(a), hwb, lab, lch, oklab, oklch, transparent, stop-color, CSS vars, and color-mix() &amp; more</text></svg>";
        let result = format(input);
        // Content too wide for inline — should be on its own line.
        assert!(
            result.contains(">\n\t\thex, rgb(a)"),
            "long content not on own line:\n{result}"
        );
        // Must stay as a single line, not split across multiple.
        let content_lines: Vec<&str> = result
            .lines()
            .filter(|l| l.contains("hex, rgb(a)"))
            .collect();
        assert_eq!(
            content_lines.len(),
            1,
            "content split across multiple lines:\n{result}"
        );
    }

    #[test]
    fn tspan_entity_refs_stay_inline() {
        let input = "<svg><text><tspan>a &amp; b</tspan></text></svg>";
        let result = format(input);
        assert!(
            result.contains("<tspan>a &amp; b</tspan>"),
            "tspan content was split:\n{result}"
        );
    }
}
