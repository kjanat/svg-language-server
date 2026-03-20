use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    Color, ColorInformation, ColorPresentation, ColorPresentationParams, ColorProviderCapability,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentColorParams, InitializeParams, InitializeResult, Position, Range, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Uri,
};
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

/// Cache mapping (URI, start_line, start_character) to the original color kind.
type ColorKindCache = Arc<RwLock<HashMap<(Uri, u32, u32), svg_color::ColorKind>>>;

struct SvgLanguageServer {
    _client: Client,
    documents: Arc<RwLock<HashMap<Uri, String>>>,
    /// Cache of (uri, start_line, start_character) → ColorKind from last document_color call.
    color_kinds: ColorKindCache,
}

impl SvgLanguageServer {
    fn new(client: Client) -> Self {
        Self {
            _client: client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            color_kinds: Arc::new(RwLock::new(HashMap::new())),
        }
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

impl LanguageServer for SvgLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                color_provider: Some(ColorProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.documents
            .write()
            .await
            .insert(params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.documents
                .write()
                .await
                .insert(params.text_document.uri, change.text);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
    }

    async fn document_color(&self, params: DocumentColorParams) -> Result<Vec<ColorInformation>> {
        let docs = self.documents.read().await;
        let Some(source) = docs.get(&params.text_document.uri) else {
            return Ok(Vec::new());
        };
        let source_bytes = source.as_bytes();
        let colors = svg_color::extract_colors(source_bytes);

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
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(SvgLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
