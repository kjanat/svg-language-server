use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    ColorInformation, ColorPresentation, ColorPresentationParams, ColorProviderCapability,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentColorParams, InitializeParams, InitializeResult, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

struct SvgLanguageServer {
    client: Client,
    documents: Arc<RwLock<HashMap<Uri, String>>>,
}

impl SvgLanguageServer {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
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

    async fn document_color(
        &self,
        _params: DocumentColorParams,
    ) -> Result<Vec<ColorInformation>> {
        Ok(Vec::new())
    }

    async fn color_presentation(
        &self,
        _params: ColorPresentationParams,
    ) -> Result<Vec<ColorPresentation>> {
        Ok(Vec::new())
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| SvgLanguageServer::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
