use tree_sitter::{Node, Parser};

/// Formatter configuration for SVG pretty-printing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatOptions {
    /// Number of spaces per indentation level when `insert_spaces` is true.
    pub indent_width: usize,
    /// Whether indentation should use spaces (true) or tabs (false).
    pub insert_spaces: bool,
    /// Maximum inline tag width before switching to multi-line attributes.
    pub max_inline_tag_width: usize,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            indent_width: 2,
            insert_spaces: false,
            max_inline_tag_width: 100,
        }
    }
}

/// Format an SVG source string with default options.
pub fn format(source: &str) -> String {
    format_with_options(source, FormatOptions::default())
}

/// Format an SVG source string with explicit options.
pub fn format_with_options(source: &str, options: FormatOptions) -> String {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_svg::LANGUAGE.into())
        .expect("SVG grammar");

    let Some(tree) = parser.parse(source.as_bytes(), None) else {
        return source.to_owned();
    };

    let mut formatter = Formatter::new(source.as_bytes(), options);
    formatter.format_node(tree.root_node(), 0);
    formatter.finish(source)
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

    fn format_node(&mut self, node: Node<'_>, depth: usize) {
        match node.kind() {
            "source_file" => self.format_children(node, depth),
            "svg_root_element" | "element" => self.format_element_like(node, depth),
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
            _ => self.format_children(node, depth),
        }
    }

    fn format_children(&mut self, node: Node<'_>, depth: usize) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.format_node(child, depth);
        }
    }

    fn format_element_like(&mut self, node: Node<'_>, depth: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
        if children.is_empty() {
            return;
        }

        // Self-closing form: <rect .../>
        if children.len() == 1 && children[0].kind() == "self_closing_tag" {
            self.format_node(children[0], depth);
            return;
        }

        for child in children {
            match child.kind() {
                "start_tag" | "end_tag" => self.format_node(child, depth),
                "style_text_double" | "style_text_single" | "script_text_double"
                | "script_text_single" => {
                    if !self.node_text(child).trim().is_empty() {
                        self.write_preserved_block_text(child, depth + 1);
                    }
                }
                "text" | "raw_text" => {
                    if !self.node_text(child).trim().is_empty() {
                        self.write_text_node(child, depth + 1);
                    }
                }
                _ => self.format_node(child, depth + 1),
            }
        }
    }

    fn write_tag_node(&mut self, node: Node<'_>, depth: usize, self_closing: bool) {
        let raw = self.node_text(node).trim().to_string();
        let Some(mut tag) = parse_tag(&raw, self_closing) else {
            self.write_line(depth, &raw);
            return;
        };
        canonicalize_attributes(&mut tag.attributes);

        let mut inline = String::new();
        inline.push('<');
        inline.push_str(&tag.name);
        if !tag.attributes.is_empty() {
            inline.push(' ');
            inline.push_str(&tag.attributes.join(" "));
        }
        if self_closing {
            if tag.attributes.is_empty() {
                inline.push_str("/>");
            } else {
                inline.push_str(" />");
            }
        } else {
            inline.push('>');
        }

        let multiline = raw.contains('\n') || inline.len() > self.options.max_inline_tag_width;
        if !multiline {
            self.write_line(depth, &inline);
            return;
        }

        self.write_line(depth, &format!("<{}", tag.name));
        if tag.attributes.is_empty() {
            self.write_line(depth, if self_closing { "/>" } else { ">" });
            return;
        }

        for attr in &tag.attributes[..tag.attributes.len() - 1] {
            self.write_line(depth + 1, attr);
        }
        let tail = if self_closing { " />" } else { ">" };
        let last = format!("{}{}", tag.attributes[tag.attributes.len() - 1], tail);
        self.write_line(depth + 1, &last);
    }

    fn write_text_node(&mut self, node: Node<'_>, depth: usize) {
        let text = self.node_text(node).to_string();
        if text.trim().is_empty() {
            return;
        }

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            self.write_line(depth, trimmed);
        }
    }

    fn write_preserved_block_text(&mut self, node: Node<'_>, depth: usize) {
        let text = self.node_text(node).to_string();
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
    attributes: Vec<String>,
}

fn canonicalize_attributes(attributes: &mut [String]) {
    attributes.sort_by_key(|left| attribute_sort_key(left));
}

fn attribute_sort_key(attribute: &str) -> (u8, u16, String) {
    let name = attribute_name(attribute);
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

fn attribute_name(attribute: &str) -> &str {
    if let Some((name, _)) = attribute.split_once('=') {
        name.trim()
    } else {
        attribute.trim()
    }
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

        let attr = inner[start..j].trim();
        if !attr.is_empty() {
            attrs.push(attr.to_string());
        }
    }

    Some(ParsedTag {
        name,
        attributes: attrs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_nested_elements() {
        let input = r#"<svg><g><rect/></g></svg>"#;
        let expected = "<svg>\n\t<g>\n\t\t<rect/>\n\t</g>\n</svg>";
        assert_eq!(format(input), expected);
    }

    #[test]
    fn formats_multiline_attributes_consistently() {
        let input = r#"<svg><linearGradient id="sky" x1="0%" y1="0%" x2="0%" y2="100%"></linearGradient></svg>"#;
        let expected = "<svg>\n\t<linearGradient\n\t\tid=\"sky\"\n\t\tx1=\"0%\"\n\t\ty1=\"0%\"\n\t\tx2=\"0%\"\n\t\ty2=\"100%\">\n\t</linearGradient>\n</svg>";
        assert_eq!(format(input), expected);
    }

    #[test]
    fn canonical_attribute_ordering() {
        let input = r#"<svg><rect y="2" width="4" class="hero" id="x" x="1" height="5"/></svg>"#;
        let expected = "<svg>\n\t<rect id=\"x\" class=\"hero\" x=\"1\" y=\"2\" width=\"4\" height=\"5\" />\n</svg>";
        assert_eq!(format(input), expected);
    }

    #[test]
    fn preserves_style_block_content_shape() {
        let input = r#"<svg><style>
  .a { fill: red; }
    .b { stroke: blue; }
</style></svg>"#;
        let expected = "<svg>\n\t<style>\n\t\t.a { fill: red; }\n\t\t  .b { stroke: blue; }\n\t</style>\n</svg>";
        assert_eq!(format(input), expected);
    }
}
