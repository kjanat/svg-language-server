//! Language-server implementation for SVG editing features.
//!
//! This crate wires together the shared workspace crates for catalog lookup,
//! linting, formatting, color handling, and reference resolution behind the
//! Language Server Protocol.

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock as StdRwLock},
};

use serde_json::Value;
use tokio::sync::RwLock;
use tower_lsp_server::{
    Client, LanguageServer, LspService, Server,
    jsonrpc::Result,
    ls_types::{
        CodeActionParams, CodeActionProviderCapability, CodeActionResponse, Color,
        ColorInformation, ColorPresentation, ColorPresentationParams, ColorProviderCapability,
        CompletionItem, CompletionOptions, CompletionParams, CompletionResponse,
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DocumentColorParams, DocumentFormattingParams, ExecuteCommandOptions, ExecuteCommandParams,
        GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverParams,
        HoverProviderCapability, InitializeParams, InitializeResult, MarkupContent, MarkupKind,
        MessageType, OneOf, Position, Range, ServerCapabilities, TextDocumentSyncCapability,
        TextDocumentSyncKind, TextEdit, Uri,
    },
};
use url::Url;

mod clipboard;
mod code_actions;
mod compat;
mod completion;
mod definition;
mod diagnostics;
mod hover;
mod logging;
mod positions;
mod stylesheets;

use clipboard::{copy_text_to_system_clipboard, svg_data_uri};
use code_actions::{
    copy_data_uri_code_action, effective_suppression_row, suppression_code,
    suppression_code_actions_for_diagnostic,
};
use compat::{RuntimeCompat, fetch_runtime_compat};
use completion::{
    attribute_completion_items, child_element_completion_items, completion_trigger_characters,
    enclosing_element_name, existing_attribute_names, first_attribute_name_text,
    is_comment_like_context, is_embedded_non_svg_context, root_element_completion_items,
    style_completion_items, tag_element_name, value_completions,
};
use definition::{DefinitionContext, build_definition_context, stylesheet_definition_locations};
use diagnostics::publish_lint_diagnostics;
use hover::{
    external_attribute_hover, format_attribute_hover, format_class_hover,
    format_custom_property_hover, format_element_hover,
};
use logging::init_logging;
use positions::{byte_col_to_utf16, byte_offset_for_position, end_position_utf16, u32_from_usize};
use stylesheets::{
    CachedStylesheet, ClassDefinitionHover, CustomPropertyDefinitionHover,
    class_definition_hovers_from_stylesheet, custom_property_definition_hovers_from_stylesheet,
    definition_response_from_locations, resolve_external_stylesheet,
};
use svg_tree::{deepest_node_at, find_ancestor_any, is_attribute_name_kind};

/// Parsed document state: source text + tree-sitter tree.
///
/// Invariant: `tree` is always the parse result of `source`. Construct only
/// via [`SvgLanguageServer::update_document`].
#[derive(Clone)]
pub(crate) struct DocumentState {
    pub(crate) version: i32,
    pub(crate) source: String,
    pub(crate) tree: tree_sitter::Tree,
}

/// Position key for color kind cache lookups.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
struct ColorPositionKey {
    uri: Uri,
    line: u32,
    character_utf16: u32,
}

type ColorKindCache = Arc<RwLock<HashMap<ColorPositionKey, svg_color::ColorKind>>>;
pub(crate) type StylesheetCache =
    Arc<StdRwLock<HashMap<String, Arc<OnceLock<Option<CachedStylesheet>>>>>>;
const COPY_DATA_URI_COMMAND: &str = "svg.copyDataUri";
const COPY_DATA_URI_ACTION_TITLE: &str = "Copy SVG as data URI";

fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        color_provider: Some(ColorProviderCapability::Simple(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: vec![COPY_DATA_URI_COMMAND.to_owned()],
            ..Default::default()
        }),
        document_formatting_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(completion_trigger_characters()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

const fn markdown_hover(value: String) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    }
}

async fn resolve_external_stylesheet_off_thread(
    stylesheet_cache: &StylesheetCache,
    base_uri: &Uri,
    href: String,
) -> Option<(CachedStylesheet, bool)> {
    let stylesheet_cache = stylesheet_cache.clone();
    let base_uri = base_uri.clone();
    tokio::task::spawn_blocking(move || {
        resolve_external_stylesheet(&stylesheet_cache, &base_uri, &href)
    })
    .await
    .unwrap_or_else(|err| {
        tracing::warn!(error = %err, "stylesheet resolution task failed");
        None
    })
}

fn completion_response(items: Vec<CompletionItem>) -> Option<CompletionResponse> {
    (!items.is_empty()).then_some(CompletionResponse::Array(items))
}

struct ClassHoverContext {
    target: String,
    definitions: Vec<ClassDefinitionHover>,
    stylesheet_hrefs: Vec<String>,
}

struct PropertyHoverContext {
    target: String,
    definitions: Vec<CustomPropertyDefinitionHover>,
    stylesheet_hrefs: Vec<String>,
}

struct HoverContext {
    element_markdown: Option<String>,
    attribute_markdown: Option<String>,
    class_hover: ClassHoverContext,
    property_hover: PropertyHoverContext,
}

const fn empty_class_hover_context() -> ClassHoverContext {
    ClassHoverContext {
        target: String::new(),
        definitions: Vec::new(),
        stylesheet_hrefs: Vec::new(),
    }
}

const fn empty_property_hover_context() -> PropertyHoverContext {
    PropertyHoverContext {
        target: String::new(),
        definitions: Vec::new(),
        stylesheet_hrefs: Vec::new(),
    }
}

fn build_hover_context(
    uri: &Uri,
    pos: Position,
    doc: &DocumentState,
    runtime_compat: Option<&RuntimeCompat>,
) -> HoverContext {
    let source = doc.source.as_bytes();
    let byte_offset = byte_offset_for_position(source, pos);
    let raw_node = deepest_node_at(&doc.tree, byte_offset);
    let node = if raw_node.is_named() {
        raw_node
    } else {
        raw_node.parent().unwrap_or(raw_node)
    };
    let kind = node.kind().to_owned();
    let node_text = node.utf8_text(source).unwrap_or("").to_owned();

    let element_markdown = if kind == "name" {
        node.parent().and_then(|parent| {
            let parent_kind = parent.kind();
            if parent_kind == "start_tag"
                || parent_kind == "self_closing_tag"
                || parent_kind == "end_tag"
            {
                svg_data::element(&node_text).map(|element| {
                    let runtime_override =
                        runtime_compat.and_then(|runtime| runtime.elements.get(&node_text));
                    format_element_hover(element, runtime_override)
                })
            } else {
                None
            }
        })
    } else {
        None
    };

    let attribute_markdown = if is_attribute_name_kind(&kind) {
        svg_data::attribute(&node_text).map_or_else(
            || external_attribute_hover(&kind, &node_text),
            |attribute| {
                let runtime_override =
                    runtime_compat.and_then(|runtime| runtime.attributes.get(&node_text));
                Some(format_attribute_hover(attribute, runtime_override))
            },
        )
    } else {
        None
    };

    let definition_target = svg_references::definition_target_at(source, &doc.tree, byte_offset);
    let stylesheet_hrefs = svg_references::extract_xml_stylesheet_hrefs(source);
    let inline_stylesheets = svg_references::collect_inline_stylesheets(source, &doc.tree);

    let class_hover = match &definition_target {
        Some(svg_references::DefinitionTarget::Class(target_class)) => ClassHoverContext {
            target: target_class.clone(),
            definitions: inline_stylesheets
                .iter()
                .flat_map(|stylesheet| {
                    svg_references::collect_class_definitions_from_stylesheet(
                        &stylesheet.css,
                        stylesheet.start_row,
                        stylesheet.start_col,
                    )
                })
                .filter(|definition| definition.name == *target_class)
                .map(|definition| {
                    ClassDefinitionHover::new(uri.clone(), doc.source.clone(), definition)
                })
                .collect(),
            stylesheet_hrefs: stylesheet_hrefs.clone(),
        },
        _ => empty_class_hover_context(),
    };

    let property_hover = match &definition_target {
        Some(svg_references::DefinitionTarget::CustomProperty(target_property)) => {
            PropertyHoverContext {
                target: target_property.clone(),
                definitions: inline_stylesheets
                    .iter()
                    .flat_map(|stylesheet| {
                        svg_references::collect_custom_property_definitions_from_stylesheet(
                            &stylesheet.css,
                            stylesheet.start_row,
                            stylesheet.start_col,
                        )
                    })
                    .filter(|definition| definition.name == *target_property)
                    .map(|definition| {
                        CustomPropertyDefinitionHover::new(
                            uri.clone(),
                            doc.source.clone(),
                            definition,
                        )
                    })
                    .collect(),
                stylesheet_hrefs,
            }
        }
        _ => empty_property_hover_context(),
    };

    HoverContext {
        element_markdown,
        attribute_markdown,
        class_hover,
        property_hover,
    }
}

struct SvgLanguageServer {
    client: Client,
    documents: Arc<RwLock<HashMap<Uri, DocumentState>>>,
    parser: Arc<RwLock<tree_sitter::Parser>>,
    color_kinds: ColorKindCache,
    stylesheet_cache: StylesheetCache,
    runtime_compat: Arc<RwLock<Option<RuntimeCompat>>>,
}

impl SvgLanguageServer {
    fn new(client: Client) -> Self {
        let mut parser = tree_sitter::Parser::new();
        if parser
            .set_language(&tree_sitter_svg::LANGUAGE.into())
            .is_err()
        {
            panic!("failed to load tree-sitter SVG grammar");
        }
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            parser: Arc::new(RwLock::new(parser)),
            color_kinds: Arc::new(RwLock::new(HashMap::new())),
            stylesheet_cache: Arc::new(StdRwLock::new(HashMap::new())),
            runtime_compat: Arc::new(RwLock::new(None)),
        }
    }

    async fn document_state(&self, uri: &Uri) -> Option<DocumentState> {
        let docs = self.documents.read().await;
        docs.get(uri).cloned()
    }

    /// Parse source, run linter, publish diagnostics, store document state.
    async fn update_document(&self, uri: Uri, source: String, version: i32) {
        let tree = {
            let mut parser = self.parser.write().await;
            parser.parse(source.as_bytes(), None)
        };

        let Some(tree) = tree else {
            tracing::warn!(uri = ?uri, "tree-sitter parse returned None; document state not updated");
            return;
        };

        let state = DocumentState {
            version,
            source,
            tree,
        };
        let source_bytes = state.source.as_bytes();
        let overrides = {
            let compat = self.runtime_compat.read().await;
            compat.as_ref().map(RuntimeCompat::to_lint_overrides)
        };
        let lint_diags = svg_lint::lint_tree(source_bytes, &state.tree, overrides.as_ref());
        self.documents
            .write()
            .await
            .insert(uri.clone(), state.clone());

        publish_lint_diagnostics(&self.client, uri, source_bytes, lint_diags, Some(version)).await;
    }

    async fn copy_svg_as_data_uri(&self, uri: &Uri) -> std::result::Result<(), String> {
        let source = {
            let docs = self.documents.read().await;
            if let Some(doc) = docs.get(uri) {
                doc.source.clone()
            } else {
                let url = Url::parse(uri.as_str())
                    .map_err(|err| format!("Invalid URI {}: {err}", uri.as_str()))?;
                let path = url
                    .to_file_path()
                    .map_err(|()| format!("Cannot resolve file path for {}", uri.as_str()))?;
                tokio::fs::read_to_string(&path)
                    .await
                    .map_err(|err| format!("Failed to read {}: {err}", path.display()))?
            }
        };

        let data_uri = svg_data_uri(&source);
        tokio::task::spawn_blocking(move || copy_text_to_system_clipboard(&data_uri))
            .await
            .map_err(|err| format!("Clipboard task failed: {err}"))?
    }
}

impl LanguageServer for SvgLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("initialize");

        // Spawn background compat data refresh
        let compat = self.runtime_compat.clone();
        let client = self.client.clone();
        let documents = self.documents.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(fetch_runtime_compat).await;
            match result {
                Ok(Some(data)) => {
                    let el_count = data.elements.len();
                    let attr_count = data.attributes.len();
                    *compat.write().await = Some(data);
                    tracing::info!(
                        elements = el_count,
                        attributes = attr_count,
                        "runtime compat data loaded"
                    );
                    // Re-lint open documents with fresh compat overrides.
                    let overrides = {
                        let c = compat.read().await;
                        c.as_ref().map(RuntimeCompat::to_lint_overrides)
                    };
                    let snapshot: Vec<_> = {
                        let docs = documents.read().await;
                        docs.iter()
                            .map(|(uri, doc)| (uri.clone(), doc.clone()))
                            .collect()
                    };
                    for (uri, doc) in snapshot {
                        let source_bytes = doc.source.as_bytes();
                        let lint_diags =
                            svg_lint::lint_tree(source_bytes, &doc.tree, overrides.as_ref());
                        let is_current = {
                            let docs = documents.read().await;
                            docs.get(&uri).is_some_and(|current| {
                                current.version == doc.version && current.source == doc.source
                            })
                        };
                        if is_current {
                            publish_lint_diagnostics(
                                &client,
                                uri,
                                source_bytes,
                                lint_diags,
                                Some(doc.version),
                            )
                            .await;
                        }
                    }
                }
                Ok(None) => {
                    tracing::info!("runtime compat fetch returned no data (offline?)");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "runtime compat fetch failed");
                }
            }
        });

        Ok(InitializeResult {
            capabilities: server_capabilities(),
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("shutdown requested");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        tracing::debug!(uri = ?params.text_document.uri, "did_open");
        self.update_document(
            params.text_document.uri,
            params.text_document.text,
            params.text_document.version,
        )
        .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            tracing::debug!(uri = ?params.text_document.uri, "did_change");
            self.update_document(
                params.text_document.uri,
                change.text,
                params.text_document.version,
            )
            .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        tracing::debug!(uri = ?params.text_document.uri, "did_close");
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
        self.color_kinds
            .write()
            .await
            .retain(|key, _| key.uri != params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let Some(doc) = self.document_state(&params.text_document.uri).await else {
            return Ok(None);
        };

        let options = svg_format::FormatOptions {
            indent_width: params.options.tab_size as usize,
            insert_spaces: params.options.insert_spaces,
            ..Default::default()
        };
        let formatted = svg_format::format_with_options(&doc.source, options);
        if formatted == doc.source {
            return Ok(Some(Vec::new()));
        }

        let edit = TextEdit::new(
            Range::new(Position::new(0, 0), end_position_utf16(&doc.source)),
            formatted,
        );
        Ok(Some(vec![edit]))
    }

    async fn document_color(&self, params: DocumentColorParams) -> Result<Vec<ColorInformation>> {
        let Some(doc) = self.document_state(&params.text_document.uri).await else {
            return Ok(Vec::new());
        };
        let source_bytes = doc.source.as_bytes();
        let entries = svg_color::extract_colors_from_tree(source_bytes, &doc.tree)
            .into_iter()
            .map(|color| {
                let start_char = byte_col_to_utf16(source_bytes, color.start_row, color.start_col);
                let end_char = byte_col_to_utf16(source_bytes, color.end_row, color.end_col);
                let key = ColorPositionKey {
                    uri: params.text_document.uri.clone(),
                    line: u32_from_usize(color.start_row),
                    character_utf16: start_char,
                };
                let info = ColorInformation {
                    range: Range::new(
                        Position::new(u32_from_usize(color.start_row), start_char),
                        Position::new(u32_from_usize(color.end_row), end_char),
                    ),
                    color: Color {
                        red: color.r,
                        green: color.g,
                        blue: color.b,
                        alpha: color.a,
                    },
                };
                (info, key, color.kind)
            })
            .collect::<Vec<_>>();
        let uri = params.text_document.uri.clone();

        let result = entries.iter().map(|(info, _, _)| info.clone()).collect();

        {
            let mut kinds = self.color_kinds.write().await;
            // Clear stale entries for this URI
            kinds.retain(|key, _| key.uri != uri);
            for (_, key, kind) in entries {
                kinds.insert(key, kind);
            }
        }

        Ok(result)
    }

    async fn color_presentation(
        &self,
        params: ColorPresentationParams,
    ) -> Result<Vec<ColorPresentation>> {
        let key = ColorPositionKey {
            uri: params.text_document.uri,
            line: params.range.start.line,
            character_utf16: params.range.start.character,
        };
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

        let Some(doc) = self.document_state(uri).await else {
            return Ok(None);
        };
        let HoverContext {
            element_markdown,
            attribute_markdown,
            class_hover,
            property_hover,
        } = {
            let runtime_compat = self.runtime_compat.read().await;
            build_hover_context(uri, pos, &doc, runtime_compat.as_ref())
        };

        if let Some(markdown) = element_markdown {
            return Ok(Some(markdown_hover(markdown)));
        }

        if let Some(markdown) = attribute_markdown {
            return Ok(Some(markdown_hover(markdown)));
        }

        let ClassHoverContext {
            target: target_class,
            mut definitions,
            stylesheet_hrefs,
        } = class_hover;
        if !target_class.is_empty() {
            let mut local_definitions = Vec::new();
            let mut remote_definitions = Vec::new();

            for href in stylesheet_hrefs {
                let Some((stylesheet, is_remote)) =
                    resolve_external_stylesheet_off_thread(&self.stylesheet_cache, uri, href).await
                else {
                    continue;
                };

                let defs = class_definition_hovers_from_stylesheet(
                    &stylesheet.uri,
                    &stylesheet.source,
                    &target_class,
                );

                if is_remote {
                    remote_definitions.extend(defs);
                } else {
                    local_definitions.extend(defs);
                }
            }

            definitions.extend(local_definitions);
            definitions.extend(remote_definitions);

            if !definitions.is_empty() {
                return Ok(Some(markdown_hover(format_class_hover(
                    &target_class,
                    &definitions,
                ))));
            }
        }

        let PropertyHoverContext {
            target: target_property,
            mut definitions,
            stylesheet_hrefs,
        } = property_hover;
        if !target_property.is_empty() {
            let mut local_definitions = Vec::new();
            let mut remote_definitions = Vec::new();

            for href in stylesheet_hrefs {
                let Some((stylesheet, is_remote)) =
                    resolve_external_stylesheet_off_thread(&self.stylesheet_cache, uri, href).await
                else {
                    continue;
                };

                let defs = custom_property_definition_hovers_from_stylesheet(
                    &stylesheet.uri,
                    &stylesheet.source,
                    &target_property,
                );

                if is_remote {
                    remote_definitions.extend(defs);
                } else {
                    local_definitions.extend(defs);
                }
            }

            definitions.extend(local_definitions);
            definitions.extend(remote_definitions);

            if !definitions.is_empty() {
                return Ok(Some(markdown_hover(format_custom_property_hover(
                    &target_property,
                    &definitions,
                ))));
            }
        }

        Ok(None)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let Some(doc) = self.document_state(uri).await else {
            return Ok(None);
        };

        let mut seen = std::collections::HashSet::new();
        let mut actions = vec![copy_data_uri_code_action(uri)];

        for diagnostic in &params.context.diagnostics {
            let Some(code) = suppression_code(diagnostic) else {
                continue;
            };
            let effective_row =
                effective_suppression_row(doc.source.as_bytes(), &doc.tree, diagnostic);
            let key = (code.to_owned(), effective_row);
            if !seen.insert(key) {
                continue;
            }
            actions.extend(suppression_code_actions_for_diagnostic(
                uri,
                &doc.source,
                diagnostic,
                effective_row,
            ));
        }

        Ok(Some(actions))
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        match params.command.as_str() {
            COPY_DATA_URI_COMMAND => {
                let uri = params
                    .arguments
                    .first()
                    .and_then(Value::as_str)
                    .and_then(|value| value.parse::<Uri>().ok());

                let Some(uri) = uri else {
                    self.client
                        .show_message(
                            MessageType::ERROR,
                            "Copy SVG as data URI requires a document URI.",
                        )
                        .await;
                    return Ok(None);
                };

                match self.copy_svg_as_data_uri(&uri).await {
                    Ok(()) => {
                        self.client
                            .show_message(MessageType::INFO, "Copied SVG as data URI.")
                            .await;
                    }
                    Err(message) => {
                        self.client.show_message(MessageType::ERROR, message).await;
                    }
                }

                Ok(None)
            }
            _ => Ok(None),
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let Some(doc) = self.document_state(uri).await else {
            return Ok(None);
        };
        let Some(DefinitionContext {
            target,
            inline_locations,
            stylesheet_hrefs,
        }) = build_definition_context(uri, pos, &doc)
        else {
            return Ok(None);
        };

        if matches!(target, svg_references::DefinitionTarget::Id(_)) {
            return Ok(definition_response_from_locations(inline_locations));
        }

        let mut locations = inline_locations;
        let mut local_locations = Vec::new();
        let mut remote_locations = Vec::new();

        for href in stylesheet_hrefs {
            let Some((stylesheet, is_remote)) =
                resolve_external_stylesheet_off_thread(&self.stylesheet_cache, uri, href).await
            else {
                continue;
            };

            let defs = stylesheet_definition_locations(&stylesheet, &target);

            if is_remote {
                remote_locations.extend(defs);
            } else {
                local_locations.extend(defs);
            }
        }

        locations.extend(local_locations);
        locations.extend(remote_locations);

        Ok(definition_response_from_locations(locations))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let runtime_compat = {
            let guard = self.runtime_compat.read().await;
            guard.clone()
        };

        let Some(doc) = self.document_state(uri).await else {
            return Ok(None);
        };

        let source = doc.source.as_bytes();
        let byte_offset = byte_offset_for_position(source, pos);
        let node = deepest_node_at(&doc.tree, byte_offset);

        if is_comment_like_context(node) {
            return Ok(None);
        }

        if let Some(items) = style_completion_items(source, &doc.tree, byte_offset)
            && let Some(response) = completion_response(items)
        {
            return Ok(Some(response));
        }

        if is_embedded_non_svg_context(node, source) {
            return Ok(None);
        }

        // Detect completion context by walking ancestors
        let mut cursor = node;
        loop {
            let kind = cursor.kind();

            // Inside attribute value → value completions
            if kind.ends_with("_attribute_value") || kind == "quoted_attribute_value" {
                // Walk up to find the attribute name
                if let Some(attr_wrapper) =
                    find_ancestor_any(cursor, &["generic_attribute", "attribute"])
                    && let Some(attr_name) = first_attribute_name_text(attr_wrapper, source)
                {
                    let items = value_completions(&attr_name, source, &doc.tree, cursor);
                    if let Some(response) = completion_response(items) {
                        return Ok(Some(response));
                    }
                }
                return Ok(None);
            }

            // Inside a tag → attribute name completions
            if kind == "start_tag" || kind == "self_closing_tag" {
                let elem_name = tag_element_name(cursor, source).unwrap_or("");
                let existing = existing_attribute_names(cursor, source);
                return Ok(completion_response(attribute_completion_items(
                    elem_name,
                    &existing,
                    runtime_compat.as_ref(),
                )));
            }

            // Inside an element → child element completions
            if kind == "element" || kind == "svg_root_element" {
                let elem_name = enclosing_element_name(cursor, source).unwrap_or("");
                let Some(_) = svg_data::element(elem_name) else {
                    return Ok(None);
                };

                return Ok(completion_response(child_element_completion_items(
                    elem_name,
                    runtime_compat.as_ref(),
                )));
            }

            // Reached root document without matching → suggest root svg element
            if kind == "document" {
                return Ok(completion_response(root_element_completion_items()));
            }

            match cursor.parent() {
                Some(parent) => cursor = parent,
                None => break,
            }
        }

        Ok(None)
    }
}

/// Run the SVG language server over stdio using the LSP transport.
pub async fn run_stdio_server() {
    let _logging = init_logging();
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(SvgLanguageServer::new);
    tracing::info!("starting LSP server");
    Server::new(stdin, stdout, socket).serve(service).await;
    tracing::info!("LSP server exited");
}

#[cfg(test)]
mod tests {
    use tower_lsp_server::ls_types::CodeActionOrCommand;

    use super::*;
    use crate::positions::position_for_byte_offset;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    fn offset_of(source: &str, needle: &str) -> std::result::Result<usize, &'static str> {
        source.find(needle).ok_or("needle not present")
    }

    #[test]
    fn ascii_fixture_lengths_match_inline_probes() {
        assert_eq!(r#"<svg><rect height="32" /></svg>"#.len(), 31);
        assert_eq!(r"<svg><script>con</script></svg>".len(), 31);
        assert_eq!(r"<svg><rect></rect></svg>".len(), 24);
        assert_eq!(r#"<svg><use height="32" /></svg>"#.len(), 30);
        assert_eq!(
            r#"<svg><defs><linearGradient id="g1" /></defs><use href="" /></svg>"#.len(),
            65
        );
    }

    #[test]
    fn byte_offsets_match_inline_completion_probes() {
        let cases = [
            (r#"<svg><rect height="32" /></svg>"#, 22u32, 22usize),
            (r#"<svg><use height="32" /></svg>"#, 22u32, 22usize),
            (r"<svg><script>con</script></svg>", 15u32, 15usize),
            (
                r#"<svg><defs><linearGradient id="g1" /></defs><use href="" /></svg>"#,
                55u32,
                55usize,
            ),
        ];

        for (source, character, expected_offset) in cases {
            let position = Position::new(0, character);
            let actual_offset = byte_offset_for_position(source.as_bytes(), position);
            assert_eq!(
                actual_offset, expected_offset,
                "unexpected byte offset for {source:?} at UTF-16 column {character}"
            );
            assert_eq!(
                position_for_byte_offset(source.as_bytes(), expected_offset),
                position,
                "offset round-trip failed for {source:?} at byte {expected_offset}"
            );
        }
    }

    #[test]
    fn multiline_completion_probe_positions_match_inline_checks() -> TestResult {
        let source = r#"<svg>
    <filter id="f1">
        <!-- Place cursor after < here -->
    </filter>
</svg>"#;
        let position = Position::new(2, 33);
        let expected_offset = offset_of(source, "< here")? + 1;

        assert_eq!(
            byte_offset_for_position(source.as_bytes(), position),
            expected_offset,
            "unexpected byte offset for multiline comment completion probe"
        );
        assert_eq!(
            position_for_byte_offset(source.as_bytes(), expected_offset),
            position,
            "multiline comment completion probe should round-trip"
        );
        Ok(())
    }

    #[test]
    fn copy_data_uri_code_action_uses_document_uri() -> TestResult {
        let action = copy_data_uri_code_action(&"file:///test.svg".parse::<Uri>()?);
        let CodeActionOrCommand::CodeAction(action) = action else {
            panic!("expected code action");
        };
        let command = action.command.ok_or("copy action should have a command")?;
        let uri = command
            .arguments
            .ok_or("copy action should have a uri")?
            .into_iter()
            .next()
            .ok_or("copy action should include exactly one uri")?;

        assert_eq!(command.command, COPY_DATA_URI_COMMAND);
        assert_eq!(uri.as_str(), Some("file:///test.svg"));
        Ok(())
    }

    #[test]
    fn goto_definition_target_resolves_paint_server_reference() -> TestResult {
        let source = r#"<svg><rect fill="url(#style-gradient)" /><linearGradient id="style-gradient" /></svg>"#;
        let offset = offset_of(source, "style-gradient)")? + 2;

        assert_eq!(
            svg_references::definition_target_at(
                source.as_bytes(),
                &svg_references_test_tree(source)?,
                offset,
            ),
            Some(svg_references::DefinitionTarget::Id(
                "style-gradient".into()
            ))
        );
        Ok(())
    }

    #[test]
    fn goto_definition_target_does_not_resolve_url_wrapper() -> TestResult {
        let source = r#"<svg><rect fill="url(#style-gradient)" /><linearGradient id="style-gradient" /></svg>"#;
        let offset = offset_of(source, "url(")? + 1;

        assert_eq!(
            svg_references::definition_target_at(
                source.as_bytes(),
                &svg_references_test_tree(source)?,
                offset,
            ),
            None
        );
        Ok(())
    }

    #[test]
    fn collect_id_definitions_matches_id_token() -> TestResult {
        let source = r#"<svg><rect fill="url(#style-gradient)" /><linearGradient id="style-gradient" /></svg>"#;
        let definitions = svg_references::collect_id_definitions(
            source.as_bytes(),
            &svg_references_test_tree(source)?,
        );
        assert!(
            definitions
                .iter()
                .any(|definition| definition.name == "style-gradient")
        );
        Ok(())
    }

    fn svg_references_test_tree(
        source: &str,
    ) -> std::result::Result<tree_sitter::Tree, &'static str> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_svg::LANGUAGE.into())
            .map_err(|_| "SVG grammar")?;
        parser.parse(source, None).ok_or("tree")
    }
}
