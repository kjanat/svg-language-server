use std::collections::HashMap;
use std::sync::Arc;

use svg_data::{AttributeValues, BaselineStatus, ContentModel};
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    Color, ColorInformation, ColorPresentation, ColorPresentationParams, ColorProviderCapability,
    CompletionItem, CompletionItemKind, CompletionItemTag, CompletionOptions, CompletionParams,
    CompletionResponse, Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentColorParams, Hover,
    HoverContents, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult,
    InsertTextFormat, MarkupContent, MarkupKind, NumberOrString, Position, Range,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Uri,
};
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

/// Parsed document state: source text + tree-sitter tree.
struct DocumentState {
    source: String,
    tree: tree_sitter::Tree,
}

/// Cache mapping (URI, start_line, start_character) to the original color kind.
type ColorKindCache = Arc<RwLock<HashMap<(Uri, u32, u32), svg_color::ColorKind>>>;

struct SvgLanguageServer {
    client: Client,
    documents: Arc<RwLock<HashMap<Uri, DocumentState>>>,
    parser: Arc<RwLock<tree_sitter::Parser>>,
    color_kinds: ColorKindCache,
}

impl SvgLanguageServer {
    fn new(client: Client) -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_svg::LANGUAGE.into())
            .expect("SVG grammar");
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            parser: Arc::new(RwLock::new(parser)),
            color_kinds: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Parse source, run linter, publish diagnostics, store document state.
    async fn update_document(&self, uri: Uri, source: String) {
        let tree = {
            let mut parser = self.parser.write().await;
            parser.parse(source.as_bytes(), None)
        };

        let Some(tree) = tree else {
            return;
        };

        let source_bytes = source.as_bytes();
        let lint_diags = svg_lint::lint_tree(source_bytes, &tree);

        let lsp_diags: Vec<Diagnostic> = lint_diags
            .into_iter()
            .map(|d| {
                let start_char = byte_col_to_utf16(source_bytes, d.start_row, d.start_col);
                let end_char = byte_col_to_utf16(source_bytes, d.end_row, d.end_col);
                let severity = match d.severity {
                    svg_lint::Severity::Error => DiagnosticSeverity::ERROR,
                    svg_lint::Severity::Warning => DiagnosticSeverity::WARNING,
                    svg_lint::Severity::Information => DiagnosticSeverity::INFORMATION,
                    svg_lint::Severity::Hint => DiagnosticSeverity::HINT,
                };
                Diagnostic::new(
                    Range::new(
                        Position::new(d.start_row as u32, start_char),
                        Position::new(d.end_row as u32, end_char),
                    ),
                    Some(severity),
                    Some(NumberOrString::String(format!("{:?}", d.code))),
                    Some("svg-lint".to_owned()),
                    d.message,
                    None,
                    None,
                )
            })
            .collect();

        self.client
            .publish_diagnostics(uri.clone(), lsp_diags, None)
            .await;

        self.documents
            .write()
            .await
            .insert(uri, DocumentState { source, tree });
    }
}

/// Convert a byte-offset column to UTF-16 code unit count within a given row.
///
/// LSP positions use UTF-16 code units by default. Tree-sitter reports byte offsets,
/// so we must re-encode the line prefix to count UTF-16 units.
fn byte_col_to_utf16(source: &[u8], row: usize, byte_col: usize) -> u32 {
    let line_start: usize = source
        .split(|&b| b == b'\n')
        .take(row)
        .map(|line| line.len() + 1) // +1 for the newline byte
        .sum();

    let end = (line_start + byte_col).min(source.len());
    let line_bytes = &source[line_start..end];
    String::from_utf8_lossy(line_bytes).encode_utf16().count() as u32
}

/// Convert a UTF-16 column offset to a byte offset within a given row.
///
/// Inverse of `byte_col_to_utf16`: LSP sends UTF-16 positions, but tree-sitter
/// uses byte offsets.
fn utf16_to_byte_col(source: &[u8], row: usize, utf16_col: u32) -> usize {
    let line_start: usize = source
        .split(|&b| b == b'\n')
        .take(row)
        .map(|line| line.len() + 1)
        .sum();
    let line_end = source[line_start..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(source.len(), |p| line_start + p);
    let line_str = String::from_utf8_lossy(&source[line_start..line_end]);
    let mut utf16_count = 0u32;
    let mut byte_offset = 0usize;
    for ch in line_str.chars() {
        if utf16_count >= utf16_col {
            break;
        }
        utf16_count += ch.len_utf16() as u32;
        byte_offset += ch.len_utf8();
    }
    byte_offset
}

/// Format element hover documentation as Markdown.
fn format_element_hover(el: &svg_data::ElementDef) -> String {
    let mut parts = Vec::new();

    if el.deprecated {
        parts.push(format!("~~{}~~", el.description));
        parts.push(String::new());
        parts.push("**Deprecated**".to_owned());
    } else {
        parts.push(el.description.to_owned());
    }

    if let Some(baseline) = &el.baseline {
        parts.push(String::new());
        parts.push(format_baseline(baseline));
    }

    parts.push(String::new());
    parts.push(format!("[MDN Reference]({})", el.mdn_url));

    parts.join("\n")
}

/// Format attribute hover documentation as Markdown.
fn format_attribute_hover(attr: &svg_data::AttributeDef) -> String {
    let mut parts = Vec::new();

    if attr.deprecated {
        parts.push(format!("~~{}~~", attr.description));
        parts.push(String::new());
        parts.push("**Deprecated**".to_owned());
    } else {
        parts.push(attr.description.to_owned());
    }

    if let Some(baseline) = &attr.baseline {
        parts.push(String::new());
        parts.push(format_baseline(baseline));
    }

    parts.push(String::new());
    parts.push(format!("[MDN Reference]({})", attr.mdn_url));

    parts.join("\n")
}

// Baseline status SVG icons as data URIs, matching vscode-html-languageservice.
const BASELINE_HIGH: &str = "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMTgiIGhlaWdodD0iMTAiIHZpZXdCb3g9IjAgMCA1NDAgMzAwIiBmaWxsPSJub25lIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPgogIDxzdHlsZT4KICAgIC5ncmVlbi1zaGFwZSB7CiAgICAgIGZpbGw6ICNDNEVFRDA7IC8qIExpZ2h0IG1vZGUgKi8KICAgIH0KCiAgICBAbWVkaWEgKHByZWZlcnMtY29sb3Itc2NoZW1lOiBkYXJrKSB7CiAgICAgIC5ncmVlbi1zaGFwZSB7CiAgICAgICAgZmlsbDogIzEyNTIyNTsgLyogRGFyayBtb2RlICovCiAgICAgIH0KICAgIH0KICA8L3N0eWxlPgogIDxwYXRoIGQ9Ik00MjAgMzBMMzkwIDYwTDQ4MCAxNTBMMzkwIDI0MEwzMzAgMTgwTDMwMCAyMTBMMzkwIDMwMEw1NDAgMTUwTDQyMCAzMFoiIGNsYXNzPSJncmVlbi1zaGFwZSIvPgogIDxwYXRoIGQ9Ik0xNTAgMEwzMCAxMjBMNjAgMTUwTDE1MCA2MEwyMTAgMTIwTDI0MCA5MEwxNTAgMFoiIGNsYXNzPSJncmVlbi1zaGFwZSIvPgogIDxwYXRoIGQ9Ik0zOTAgMEw0MjAgMzBMMTUwIDMwMEwwIDE1MEwzMCAxMjBMMTUwIDI0MEwzOTAgMFoiIGZpbGw9IiMxRUE0NDYiLz4KPC9zdmc+";
const BASELINE_LOW: &str = "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMTgiIGhlaWdodD0iMTAiIHZpZXdCb3g9IjAgMCA1NDAgMzAwIiBmaWxsPSJub25lIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPgogIDxzdHlsZT4KICAgIC5ibHVlLXNoYXBlIHsKICAgICAgZmlsbDogI0E4QzdGQTsgLyogTGlnaHQgbW9kZSAqLwogICAgfQoKICAgIEBtZWRpYSAocHJlZmVycy1jb2xvci1zY2hlbWU6IGRhcmspIHsKICAgICAgLmJsdWUtc2hhcGUgewogICAgICAgIGZpbGw6ICMyRDUwOUU7IC8qIERhcmsgbW9kZSAqLwogICAgICB9CiAgICB9CgogICAgLmRhcmtlci1ibHVlLXNoYXBlIHsKICAgICAgICBmaWxsOiAjMUI2RUYzOwogICAgfQoKICAgIEBtZWRpYSAocHJlZmVycy1jb2xvci1zY2hlbWU6IGRhcmspIHsKICAgICAgICAuZGFya2VyLWJsdWUtc2hhcGUgewogICAgICAgICAgICBmaWxsOiAjNDE4NUZGOwogICAgICAgIH0KICAgIH0KCiAgPC9zdHlsZT4KICA8cGF0aCBkPSJNMTUwIDBMMTgwIDMwTDE1MCA2MEwxMjAgMzBMMTUwIDBaIiBjbGFzcz0iYmx1ZS1zaGFwZSIvPgogIDxwYXRoIGQ9Ik0yMTAgNjBMMjQwIDkwTDIxMCAxMjBMMTgwIDkwTDIxMCA2MFoiIGNsYXNzPSJibHVlLXNoYXBlIi8+CiAgPHBhdGggZD0iTTQ1MCA2MEw0ODAgOTBMNDUwIDEyMEw0MjAgOTBMNDUwIDYwWiIgY2xhc3M9ImJsdWUtc2hhcGUiLz4KICA8cGF0aCBkPSJNNTEwIDEyMEw1NDAgMTUwTDUxMCAxODBMNDgwIDE1MEw1MTAgMTIwWiIgY2xhc3M9ImJsdWUtc2hhcGUiLz4KICA8cGF0aCBkPSJNNDUwIDE4MEw0ODAgMjEwTDQ1MCAyNDBMNDIwIDIxMEw0NTAgMTgwWiIgY2xhc3M9ImJsdWUtc2hhcGUiLz4KICA8cGF0aCBkPSJNMzkwIDI0MEw0MjAgMjcwTDM5MCAzMDBMMzYwIDI3MEwzOTAgMjQwWiIgY2xhc3M9ImJsdWUtc2hhcGUiLz4KICA8cGF0aCBkPSJNMzMwIDE4MEwzNjAgMjEwTDMzMCAyNDBMMzAwIDIxMEwzMzAgMTgwWiIgY2xhc3M9ImJsdWUtc2hhcGUiLz4KICA8cGF0aCBkPSJNOTAgNjBMMTIwIDkwTDkwIDEyMEw2MCA5MEw5MCA2MFoiIGNsYXNzPSJibHVlLXNoYXBlIi8+CiAgPHBhdGggZD0iTTM5MCAwTDQyMCAzMEwxNTAgMzAwTDAgMTUwTDMwIDEyMEwxNTAgMjQwTDM5MCAwWiIgY2xhc3M9ImRhcmtlci1ibHVlLXNoYXBlIi8+Cjwvc3ZnPg==";
const BASELINE_LIMITED: &str = "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMTgiIGhlaWdodD0iMTAiIHZpZXdCb3g9IjAgMCA1NDAgMzAwIiBmaWxsPSJub25lIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPgogIDxzdHlsZT4KICAgIC5ncmF5LXNoYXBlIHsKICAgICAgZmlsbDogI0M2QzZDNjsgLyogTGlnaHQgbW9kZSAqLwogICAgfQoKICAgIEBtZWRpYSAocHJlZmVycy1jb2xvci1zY2hlbWU6IGRhcmspIHsKICAgICAgLmdyYXktc2hhcGUgewogICAgICAgIGZpbGw6ICM1NjU2NTY7IC8qIERhcmsgbW9kZSAqLwogICAgICB9CiAgICB9CiAgPC9zdHlsZT4KICA8cGF0aCBkPSJNMTUwIDBMMjQwIDkwTDIxMCAxMjBMMTIwIDMwTDE1MCAwWiIgZmlsbD0iI0YwOTQwOSIvPgogIDxwYXRoIGQ9Ik00MjAgMzBMNTQwIDE1MEw0MjAgMjcwTDM5MCAyNDBMNDgwIDE1MEwzOTAgNjBMNDIwIDMwWiIgY2xhc3M9ImdyYXktc2hhcGUiLz4KICA8cGF0aCBkPSJNMzMwIDE4MEwzMDAgMjEwTDM5MCAzMDBMNDIwIDI3MEwzMzAgMTgwWiIgZmlsbD0iI0YwOTQwOSIvPgogIDxwYXRoIGQ9Ik0xMjAgMzBMMTUwIDYwTDYwIDE1MEwxNTAgMjQwTDEyMCAyNzBMMCAxNTBMMTIwIDMwWiIgY2xhc3M9ImdyYXktc2hhcGUiLz4KICA8cGF0aCBkPSJNMzkwIDBMNDIwIDMwTDE1MCAzMDBMMTIwIDI3MEwzOTAgMFoiIGZpbGw9IiNGMDk0MDkiLz4KPC9zdmc+";

/// Format a baseline status line with inline icon.
fn format_baseline(baseline: &BaselineStatus) -> String {
    match baseline {
        BaselineStatus::Widely { since } => {
            format!(
                "![Baseline icon]({BASELINE_HIGH}) _Widely available across major browsers (Baseline since {since})_"
            )
        }
        BaselineStatus::Newly { since } => {
            format!(
                "![Baseline icon]({BASELINE_LOW}) _Newly available across major browsers (Baseline since {since})_"
            )
        }
        BaselineStatus::Limited => {
            format!(
                "![Baseline icon]({BASELINE_LIMITED}) _Limited availability across major browsers_"
            )
        }
    }
}

/// Attribute name node kinds recognized by the tree-sitter-svg grammar.
const ATTRIBUTE_NAME_KINDS: &[&str] = &[
    "attribute_name",
    "paint_attribute_name",
    "length_attribute_name",
    "transform_attribute_name",
    "viewbox_attribute_name",
    "id_attribute_name",
];

/// Find the tree-sitter node at a given byte offset, preferring the deepest (leaf) node.
fn deepest_node_at(tree: &tree_sitter::Tree, byte_offset: usize) -> tree_sitter::Node<'_> {
    tree.root_node().descendant_for_byte_range(byte_offset, byte_offset)
        .unwrap_or_else(|| tree.root_node())
}

/// Walk ancestors to find a node matching any of the given kinds.
fn find_ancestor_any<'a>(
    node: tree_sitter::Node<'a>,
    kinds: &[&str],
) -> Option<tree_sitter::Node<'a>> {
    let mut current = node;
    loop {
        if kinds.contains(&current.kind()) {
            return Some(current);
        }
        current = current.parent()?;
    }
}

/// Extract element name from a start_tag, self_closing_tag, or end_tag node.
fn tag_element_name<'a>(tag_node: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let name_node = tag_node.child_by_field_name("name")?;
    name_node.utf8_text(source).ok()
}

/// Extract element name from the enclosing element/svg_root_element.
fn enclosing_element_name<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> Option<&'a str> {
    let elem = find_ancestor_any(node, &["element", "svg_root_element"])?;
    // The element's first child is typically the start_tag
    for i in 0..elem.child_count() {
        let child = elem.child(i as u32)?;
        let kind = child.kind();
        if kind == "start_tag" || kind == "self_closing_tag" {
            return tag_element_name(child, source);
        }
    }
    None
}

/// Build completion items for attribute values based on the attribute's value type.
fn value_completions(attr_name: &str) -> Vec<CompletionItem> {
    let Some(attr_def) = svg_data::attribute(attr_name) else {
        return Vec::new();
    };
    match &attr_def.values {
        AttributeValues::Enum(values) => values
            .iter()
            .map(|v| CompletionItem {
                label: v.to_string(),
                kind: Some(CompletionItemKind::VALUE),
                ..Default::default()
            })
            .collect(),
        AttributeValues::Transform(funcs) => funcs
            .iter()
            .map(|f| CompletionItem {
                label: f.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                insert_text: Some(format!("{f}($0)")),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            })
            .collect(),
        AttributeValues::PreserveAspectRatio {
            alignments,
            meet_or_slice,
        } => {
            let mut items: Vec<CompletionItem> = alignments
                .iter()
                .map(|a| CompletionItem {
                    label: a.to_string(),
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    ..Default::default()
                })
                .collect();
            items.extend(meet_or_slice.iter().map(|m| CompletionItem {
                label: m.to_string(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                ..Default::default()
            }));
            items
        }
        _ => Vec::new(),
    }
}

impl LanguageServer for SvgLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                color_provider: Some(ColorProviderCapability::Simple(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "<".to_string(),
                        " ".to_string(),
                        "\"".to_string(),
                        "'".to_string(),
                    ]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.update_document(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.update_document(params.text_document.uri, change.text)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn document_color(&self, params: DocumentColorParams) -> Result<Vec<ColorInformation>> {
        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&params.text_document.uri) else {
            return Ok(Vec::new());
        };
        let source_bytes = doc.source.as_bytes();
        let colors = svg_color::extract_colors_from_tree(source_bytes, &doc.tree);

        let mut kinds = self.color_kinds.write().await;
        // Clear stale entries for this URI
        kinds.retain(|(uri, _, _), _| *uri != params.text_document.uri);

        let result = colors
            .into_iter()
            .map(|c| {
                let start_char = byte_col_to_utf16(source_bytes, c.start_row, c.start_col);
                let end_char = byte_col_to_utf16(source_bytes, c.end_row, c.end_col);

                kinds.insert(
                    (
                        params.text_document.uri.clone(),
                        c.start_row as u32,
                        start_char,
                    ),
                    c.kind,
                );

                ColorInformation {
                    range: Range::new(
                        Position::new(c.start_row as u32, start_char),
                        Position::new(c.end_row as u32, end_char),
                    ),
                    color: Color {
                        red: c.r,
                        green: c.g,
                        blue: c.b,
                        alpha: c.a,
                    },
                }
            })
            .collect();

        Ok(result)
    }

    async fn color_presentation(
        &self,
        params: ColorPresentationParams,
    ) -> Result<Vec<ColorPresentation>> {
        let key = (
            params.text_document.uri,
            params.range.start.line,
            params.range.start.character,
        );
        let kind = self
            .color_kinds
            .read()
            .await
            .get(&key)
            .copied()
            .unwrap_or(svg_color::ColorKind::Hex);

        let labels = svg_color::color_presentations(
            params.color.red,
            params.color.green,
            params.color.blue,
            params.color.alpha,
            kind,
        );

        let result = labels
            .into_iter()
            .map(|label| ColorPresentation {
                text_edit: Some(TextEdit::new(params.range, label.clone())),
                label,
                additional_text_edits: None,
            })
            .collect();

        Ok(result)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else {
            return Ok(None);
        };

        let source = doc.source.as_bytes();
        let byte_col = utf16_to_byte_col(source, pos.line as usize, pos.character);
        let line_start: usize = source
            .split(|&b| b == b'\n')
            .take(pos.line as usize)
            .map(|line| line.len() + 1)
            .sum();
        let byte_offset = line_start + byte_col;

        let node = deepest_node_at(&doc.tree, byte_offset);
        let kind = node.kind();

        // Element name hover
        if kind == "name"
            && let Some(parent) = node.parent()
        {
            let parent_kind = parent.kind();
            if parent_kind == "start_tag"
                || parent_kind == "self_closing_tag"
                || parent_kind == "end_tag"
            {
                let name_text = node.utf8_text(source).unwrap_or("");
                if let Some(el) = svg_data::element(name_text) {
                    let markdown = format_element_hover(el);
                    return Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: markdown,
                        }),
                        range: None,
                    }));
                }
            }
        }

        // Attribute name hover
        if ATTRIBUTE_NAME_KINDS.contains(&kind) {
            let name_text = node.utf8_text(source).unwrap_or("");
            if let Some(attr) = svg_data::attribute(name_text) {
                let markdown = format_attribute_hover(attr);
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: markdown,
                    }),
                    range: None,
                }));
            }
        }

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else {
            return Ok(None);
        };

        let source = doc.source.as_bytes();
        let byte_col = utf16_to_byte_col(source, pos.line as usize, pos.character);
        let line_start: usize = source
            .split(|&b| b == b'\n')
            .take(pos.line as usize)
            .map(|line| line.len() + 1)
            .sum();
        let byte_offset = line_start + byte_col;

        let node = deepest_node_at(&doc.tree, byte_offset);

        // Detect completion context by walking ancestors
        let mut cursor = node;
        loop {
            let kind = cursor.kind();

            // Inside attribute value → value completions
            if kind.ends_with("_attribute_value") || kind == "quoted_attribute_value" {
                // Walk up to find the attribute name
                if let Some(attr_wrapper) = find_ancestor_any(
                    cursor,
                    &["generic_attribute", "attribute"],
                ) {
                    // First child or child named with attribute name
                    for i in 0..attr_wrapper.child_count() {
                        if let Some(child) = attr_wrapper.child(i as u32)
                            && (ATTRIBUTE_NAME_KINDS.contains(&child.kind())
                                || child.kind() == "attribute_name")
                        {
                            let attr_name = child.utf8_text(source).unwrap_or("");
                            let items = value_completions(attr_name);
                            if !items.is_empty() {
                                return Ok(Some(CompletionResponse::Array(items)));
                            }
                            break;
                        }
                    }
                }
                return Ok(None);
            }

            // Inside a tag → attribute name completions
            if kind == "start_tag" || kind == "self_closing_tag" {
                let elem_name = tag_element_name(cursor, source).unwrap_or("");
                let attrs = svg_data::attributes_for(elem_name);
                let items: Vec<CompletionItem> = attrs
                    .into_iter()
                    .map(|attr| CompletionItem {
                        label: attr.name.to_string(),
                        kind: Some(CompletionItemKind::PROPERTY),
                        detail: Some(attr.description.to_string()),
                        deprecated: if attr.deprecated { Some(true) } else { None },
                        tags: if attr.deprecated {
                            Some(vec![CompletionItemTag::DEPRECATED])
                        } else {
                            None
                        },
                        insert_text: Some(format!("{}=\"$0\"", attr.name)),
                        insert_text_format: Some(InsertTextFormat::SNIPPET),
                        ..Default::default()
                    })
                    .collect();
                return Ok(Some(CompletionResponse::Array(items)));
            }

            // Inside an element → child element completions
            if kind == "element" || kind == "svg_root_element" {
                let elem_name = enclosing_element_name(cursor, source).unwrap_or("");
                let children = svg_data::allowed_children(elem_name);
                let items: Vec<CompletionItem> = if children.is_empty() {
                    // Fallback: suggest all elements
                    svg_data::elements()
                        .iter()
                        .map(element_completion_item)
                        .collect()
                } else {
                    children
                        .into_iter()
                        .filter_map(|name| svg_data::element(name))
                        .map(element_completion_item)
                        .collect()
                };
                return Ok(Some(CompletionResponse::Array(items)));
            }

            // Reached root document without matching → suggest all elements
            if kind == "document" {
                let items: Vec<CompletionItem> = svg_data::elements()
                    .iter()
                    .map(element_completion_item)
                    .collect();
                return Ok(Some(CompletionResponse::Array(items)));
            }

            match cursor.parent() {
                Some(parent) => cursor = parent,
                None => break,
            }
        }

        Ok(None)
    }
}

/// Build a CompletionItem for an element.
fn element_completion_item(el: &svg_data::ElementDef) -> CompletionItem {
    let insert_text = match el.content_model {
        ContentModel::Void => format!("{} />", el.name),
        _ => format!("{}>$0</{}>", el.name, el.name),
    };
    CompletionItem {
        label: el.name.to_string(),
        kind: Some(CompletionItemKind::PROPERTY),
        detail: Some(el.description.to_string()),
        deprecated: if el.deprecated { Some(true) } else { None },
        tags: if el.deprecated {
            Some(vec![CompletionItemTag::DEPRECATED])
        } else {
            None
        },
        insert_text: Some(insert_text),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(SvgLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
