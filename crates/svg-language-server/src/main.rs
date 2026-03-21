use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, OnceLock, RwLock as StdRwLock};

use arboard::Clipboard;
use serde_json::Value;
use svg_data::{AttributeValues, BaselineStatus, ContentModel};
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, Color, ColorInformation, ColorPresentation,
    ColorPresentationParams, ColorProviderCapability, Command, CompletionItem, CompletionItemKind,
    CompletionItemTag, CompletionOptions, CompletionParams, CompletionResponse, Diagnostic,
    DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentColorParams, ExecuteCommandOptions, ExecuteCommandParams,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverParams,
    HoverProviderCapability, InitializeParams, InitializeResult, InsertTextFormat, Location,
    MarkupContent, MarkupKind, MessageType, NumberOrString, OneOf, Position, Range,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Uri,
    WorkspaceEdit,
};
use tower_lsp_server::{Client, LanguageServer, LspService, Server};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{Layer, Registry};
use url::Url;

/// Parsed document state: source text + tree-sitter tree.
struct DocumentState {
    source: String,
    tree: tree_sitter::Tree,
}

/// Cache mapping (URI, start_line, start_character) to the original color kind.
type ColorKindCache = Arc<RwLock<HashMap<(Uri, u32, u32), svg_color::ColorKind>>>;
type StylesheetCache = Arc<StdRwLock<HashMap<String, Arc<OnceLock<Option<CachedStylesheet>>>>>>;
const COPY_DATA_URI_COMMAND: &str = "svg.copyDataUri";

#[derive(Clone)]
struct CachedStylesheet {
    uri: Uri,
    source: String,
    class_definitions: Vec<svg_references::NamedSpan>,
    custom_property_definitions: Vec<svg_references::NamedSpan>,
}

#[derive(Clone)]
struct ClassDefinitionHover {
    uri: Uri,
    source: String,
    definition: svg_references::NamedSpan,
}

#[derive(Clone)]
struct CustomPropertyDefinitionHover {
    uri: Uri,
    source: String,
    definition: svg_references::NamedSpan,
}

struct HoverSourceLink {
    label: String,
    target: String,
}

/// Runtime compat override for a single element or attribute.
struct CompatOverride {
    deprecated: bool,
    baseline: Option<BaselineStatus>,
}

/// Runtime-fetched compat data, overlays the baked-in catalog.
struct RuntimeCompat {
    elements: HashMap<String, CompatOverride>,
    attributes: HashMap<String, CompatOverride>,
}

struct SvgLanguageServer {
    client: Client,
    documents: Arc<RwLock<HashMap<Uri, DocumentState>>>,
    parser: Arc<RwLock<tree_sitter::Parser>>,
    color_kinds: ColorKindCache,
    stylesheet_cache: StylesheetCache,
    runtime_compat: Arc<RwLock<Option<RuntimeCompat>>>,
}

struct LoggingGuards {
    _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
    _stderr_guard: tracing_appender::non_blocking::WorkerGuard,
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
            stylesheet_cache: Arc::new(StdRwLock::new(HashMap::new())),
            runtime_compat: Arc::new(RwLock::new(None)),
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
        publish_lint_diagnostics(&self.client, uri.clone(), source_bytes, lint_diags).await;

        self.documents
            .write()
            .await
            .insert(uri, DocumentState { source, tree });
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
                    .map_err(|_| format!("Cannot resolve file path for {}", uri.as_str()))?;
                fs::read_to_string(&path)
                    .map_err(|err| format!("Failed to read {}: {err}", path.display()))?
            }
        };

        let data_uri = svg_data_uri(&source);
        let mut clipboard =
            Clipboard::new().map_err(|err| format!("Clipboard unavailable: {err}"))?;
        clipboard
            .set_text(data_uri)
            .map_err(|err| format!("Failed to copy data URI to clipboard: {err}"))?;
        Ok(())
    }
}

fn lint_diagnostic_to_lsp(source: &[u8], diagnostic: svg_lint::SvgDiagnostic) -> Diagnostic {
    let start_char = byte_col_to_utf16(source, diagnostic.start_row, diagnostic.start_col);
    let end_char = byte_col_to_utf16(source, diagnostic.end_row, diagnostic.end_col);
    let severity = match diagnostic.severity {
        svg_lint::Severity::Error => DiagnosticSeverity::ERROR,
        svg_lint::Severity::Warning => DiagnosticSeverity::WARNING,
        svg_lint::Severity::Information => DiagnosticSeverity::INFORMATION,
        svg_lint::Severity::Hint => DiagnosticSeverity::HINT,
    };

    Diagnostic::new(
        Range::new(
            Position::new(diagnostic.start_row as u32, start_char),
            Position::new(diagnostic.end_row as u32, end_char),
        ),
        Some(severity),
        Some(NumberOrString::String(diagnostic.code.as_str().to_owned())),
        Some("svg-lint".to_owned()),
        diagnostic.message,
        None,
        None,
    )
}

async fn publish_lint_diagnostics(
    client: &Client,
    uri: Uri,
    source: &[u8],
    diagnostics: Vec<svg_lint::SvgDiagnostic>,
) {
    let diagnostics = diagnostics
        .into_iter()
        .map(|diagnostic| lint_diagnostic_to_lsp(source, diagnostic))
        .collect();
    client.publish_diagnostics(uri, diagnostics, None).await;
}

fn default_log_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("SVG_LS_LOG_DIR") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(path).join("svg-language-server");
    }
    if let Some(path) = std::env::var_os("HOME") {
        return PathBuf::from(path)
            .join(".cache")
            .join("svg-language-server");
    }
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(path).join("svg-language-server");
    }
    std::env::temp_dir().join("svg-language-server")
}

fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| {
                panic_info
                    .payload()
                    .downcast_ref::<String>()
                    .map(String::as_str)
            })
            .unwrap_or("non-string panic payload");

        tracing::error!(target: "svg_language_server::panic", %location, %payload, "panic");
        eprintln!("svg-language-server panic at {location}: {payload}");

        previous_hook(panic_info);
    }));
}

fn init_logging() -> LoggingGuards {
    let log_dir = default_log_dir();
    let stderr_appender = tracing_appender::non_blocking(std::io::stderr());
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(stderr_appender.0.clone())
        .with_target(true)
        .with_filter(LevelFilter::INFO)
        .boxed();

    let mut file_guard = None;
    let mut file_log_path = None;
    let mut file_layer = None;

    if fs::create_dir_all(&log_dir).is_ok() {
        let path = log_dir.join("server.log");
        if let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) {
            let non_blocking = tracing_appender::non_blocking(file);
            file_log_path = Some(path);
            file_guard = Some(non_blocking.1);
            file_layer = Some(
                tracing_subscriber::fmt::layer()
                    .with_writer(non_blocking.0)
                    .with_ansi(false)
                    .with_target(true)
                    .with_filter(LevelFilter::DEBUG)
                    .boxed(),
            );
        }
    }

    let subscriber = Registry::default().with(stderr_layer).with(file_layer);

    if let Err(err) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("svg-language-server failed to initialize tracing subscriber: {err}");
    }

    install_panic_hook();

    if let Some(path) = &file_log_path {
        tracing::info!(log_file = %path.display(), "logging initialized");
    } else {
        tracing::warn!("logging initialized without file sink");
    }

    LoggingGuards {
        _file_guard: file_guard,
        _stderr_guard: stderr_appender.1,
    }
}

// ---- Runtime compat data refresh ----

const BCD_URL: &str = "https://unpkg.com/@mdn/browser-compat-data@latest/data.json";
const WEB_FEATURES_URL: &str = "https://unpkg.com/web-features@latest/data.json";

/// Fetch BCD + web-features from unpkg, parse into a `RuntimeCompat` overlay.
/// Runs synchronously (intended for `spawn_blocking`).
fn fetch_runtime_compat() -> Option<RuntimeCompat> {
    let bcd_text = ureq::get(BCD_URL)
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    let bcd_json: serde_json::Value = serde_json::from_str(&bcd_text).ok()?;

    let wf_json: serde_json::Value = ureq::get(WEB_FEATURES_URL)
        .call()
        .ok()
        .and_then(|mut r| r.body_mut().read_to_string().ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);

    let wf_features = wf_json.get("features");

    let svg_elements = bcd_json.pointer("/svg/elements")?.as_object()?;

    let mut elements = HashMap::new();
    let mut attributes = HashMap::new();

    for (el_name, el_data) in svg_elements {
        // Element-level compat
        if let Some(compat) = el_data.pointer("/__compat") {
            let deprecated = compat
                .pointer("/status/deprecated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let baseline = resolve_baseline(compat, wf_features);
            elements.insert(
                el_name.clone(),
                CompatOverride {
                    deprecated,
                    baseline,
                },
            );
        }

        // Element-specific attributes (e.g. svg.elements.svg.baseProfile)
        if let Some(obj) = el_data.as_object() {
            for (key, val) in obj {
                if key == "__compat" {
                    continue;
                }
                if let Some(compat) = val.pointer("/__compat") {
                    let deprecated = compat
                        .pointer("/status/deprecated")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let baseline = resolve_baseline(compat, wf_features);
                    attributes
                        .entry(key.clone())
                        .and_modify(|existing: &mut CompatOverride| {
                            if deprecated {
                                existing.deprecated = true;
                            }
                            if existing.baseline.is_none() {
                                existing.baseline = baseline;
                            }
                        })
                        .or_insert(CompatOverride {
                            deprecated,
                            baseline,
                        });
                }
            }
        }
    }

    // Global attributes
    if let Some(global_attrs) = bcd_json.pointer("/svg/global_attributes")
        && let Some(obj) = global_attrs.as_object()
    {
        for (attr_name, attr_data) in obj {
            if let Some(compat) = attr_data.pointer("/__compat") {
                let deprecated = compat
                    .pointer("/status/deprecated")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let baseline = resolve_baseline(compat, wf_features);
                attributes
                    .entry(attr_name.clone())
                    .or_insert(CompatOverride {
                        deprecated,
                        baseline,
                    });
            }
        }
    }

    Some(RuntimeCompat {
        elements,
        attributes,
    })
}

/// Resolve baseline status from a BCD __compat object + web-features data.
fn resolve_baseline(
    compat: &serde_json::Value,
    wf_features: Option<&serde_json::Value>,
) -> Option<BaselineStatus> {
    let wf = wf_features?;
    let tags = compat.get("tags")?.as_array()?;
    let feature_id = tags
        .iter()
        .find_map(|t| t.as_str()?.strip_prefix("web-features:"))?;
    let status = wf.get(feature_id)?.get("status")?;

    match status.get("baseline")? {
        serde_json::Value::Bool(false) => Some(BaselineStatus::Limited),
        serde_json::Value::String(s) if s == "high" => {
            let since = parse_year(status, "baseline_high_date")?;
            Some(BaselineStatus::Widely { since })
        }
        serde_json::Value::String(s) if s == "low" => {
            let since = parse_year(status, "baseline_low_date")?;
            Some(BaselineStatus::Newly { since })
        }
        _ => None,
    }
}

fn parse_year(status: &serde_json::Value, key: &str) -> Option<u16> {
    status.get(key)?.as_str()?.split('-').next()?.parse().ok()
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

fn byte_offset_for_position(source: &[u8], position: Position) -> usize {
    let byte_col = utf16_to_byte_col(source, position.line as usize, position.character);
    byte_offset_for_row_col(source, position.line as usize, byte_col)
}

fn byte_offset_for_row_col(source: &[u8], row: usize, byte_col: usize) -> usize {
    let line_start: usize = source
        .split(|&b| b == b'\n')
        .take(row)
        .map(|line| line.len() + 1)
        .sum();
    line_start + byte_col
}

fn span_range_utf16(source: &[u8], span: &svg_references::Span) -> Range {
    Range::new(
        Position::new(
            span.start_row as u32,
            byte_col_to_utf16(source, span.start_row, span.start_col),
        ),
        Position::new(
            span.end_row as u32,
            byte_col_to_utf16(source, span.end_row, span.end_col),
        ),
    )
}

fn named_span_location(uri: Uri, source: &[u8], named: &svg_references::NamedSpan) -> Location {
    Location::new(uri, span_range_utf16(source, &named.span))
}

fn class_definition_hovers_from_stylesheet(
    uri: Uri,
    source: &str,
    target_class: &str,
) -> Vec<ClassDefinitionHover> {
    svg_references::collect_class_definitions_from_stylesheet(source, 0, 0)
        .into_iter()
        .filter(|definition| definition.name == target_class)
        .map(|definition| ClassDefinitionHover {
            uri: uri.clone(),
            source: source.to_owned(),
            definition,
        })
        .collect()
}

fn custom_property_definition_hovers_from_stylesheet(
    uri: Uri,
    source: &str,
    target_property: &str,
) -> Vec<CustomPropertyDefinitionHover> {
    svg_references::collect_custom_property_definitions_from_stylesheet(source, 0, 0)
        .into_iter()
        .filter(|definition| definition.name == target_property)
        .map(|definition| CustomPropertyDefinitionHover {
            uri: uri.clone(),
            source: source.to_owned(),
            definition,
        })
        .collect()
}

fn definition_response_from_locations(
    mut locations: Vec<Location>,
) -> Option<GotoDefinitionResponse> {
    if locations.is_empty() {
        return None;
    }
    if locations.len() == 1 {
        return Some(GotoDefinitionResponse::Scalar(locations.remove(0)));
    }
    Some(GotoDefinitionResponse::Array(locations))
}

fn parse_stylesheet(uri: Uri, source: String) -> CachedStylesheet {
    let class_definitions =
        svg_references::collect_class_definitions_from_stylesheet(&source, 0, 0);
    let custom_property_definitions =
        svg_references::collect_custom_property_definitions_from_stylesheet(&source, 0, 0);
    CachedStylesheet {
        uri,
        source,
        class_definitions,
        custom_property_definitions,
    }
}

fn format_class_hover(class_name: &str, definitions: &[ClassDefinitionHover]) -> String {
    format_definition_hover(
        definitions.iter().map(|definition| {
            (
                css_rule_snippet(&definition.source, &definition.definition.span),
                hover_source_link(&definition.uri, definition.definition.span.start_row),
            )
        }),
        &format!(".{class_name}"),
    )
}

fn format_custom_property_hover(
    property_name: &str,
    definitions: &[CustomPropertyDefinitionHover],
) -> String {
    format_definition_hover(
        definitions.iter().map(|definition| {
            (
                css_declaration_snippet(&definition.source, &definition.definition.span),
                hover_source_link(&definition.uri, definition.definition.span.start_row),
            )
        }),
        property_name,
    )
}

fn format_definition_hover(
    definitions: impl Iterator<Item = (String, HoverSourceLink)>,
    fallback_label: &str,
) -> String {
    let sections: Vec<String> = definitions
        .map(|(snippet, source)| {
            let trimmed = snippet.trim();
            let mut section = String::new();
            if trimmed.is_empty() {
                section.push_str(&format!("`{fallback_label}`"));
            } else {
                section.push_str("```css\n");
                section.push_str(trimmed);
                section.push_str("\n```");
            }
            section.push_str("\nDefined in [");
            section.push_str(&source.label);
            section.push_str("](");
            section.push_str(&source.target);
            section.push(')');
            section
        })
        .collect();

    sections.join("\n\n---\n\n")
}

fn hover_source_link(uri: &Uri, start_row: usize) -> HoverSourceLink {
    let line = start_row + 1;
    let Ok(url) = Url::parse(uri.as_str()) else {
        return HoverSourceLink {
            label: format!("{}:{line}", uri.as_str()),
            target: uri.as_str().to_owned(),
        };
    };

    match url.scheme() {
        "file" => {
            let Ok(path) = url.to_file_path() else {
                return HoverSourceLink {
                    label: format!("{}:{line}", uri.as_str()),
                    target: uri.as_str().to_owned(),
                };
            };

            let target = format!("{}#L{line}", url);
            if let Ok(cwd) = std::env::current_dir()
                && let Ok(relative) = path.strip_prefix(&cwd)
            {
                return HoverSourceLink {
                    label: format!("{}:{line}", relative.display()),
                    target,
                };
            }

            if let Some(file_name) = path.file_name() {
                return HoverSourceLink {
                    label: format!("{}:{line}", file_name.to_string_lossy()),
                    target,
                };
            }

            HoverSourceLink {
                label: format!("{}:{line}", path.display()),
                target,
            }
        }
        "http" | "https" => {
            let host = url.host_str().unwrap_or_default();
            HoverSourceLink {
                label: format!("{host}{}:{line}", url.path()),
                target: format!("{url}#L{line}"),
            }
        }
        _ => HoverSourceLink {
            label: format!("{}:{line}", uri.as_str()),
            target: uri.as_str().to_owned(),
        },
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CssCompletionContext {
    Selector,
    Property,
    Value,
}

const CSS_PROPERTY_NAMES: &[&str] = &[
    "alignment-baseline",
    "clip-path",
    "clip-rule",
    "color",
    "color-interpolation",
    "color-rendering",
    "cursor",
    "display",
    "dominant-baseline",
    "fill",
    "fill-opacity",
    "fill-rule",
    "filter",
    "flood-color",
    "flood-opacity",
    "font-family",
    "font-size",
    "font-style",
    "font-weight",
    "image-rendering",
    "lighting-color",
    "marker-end",
    "marker-mid",
    "marker-start",
    "mask",
    "mix-blend-mode",
    "opacity",
    "overflow",
    "paint-order",
    "pointer-events",
    "shape-rendering",
    "stop-color",
    "stop-opacity",
    "stroke",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-opacity",
    "stroke-width",
    "text-anchor",
    "text-decoration-color",
    "transform",
    "transform-box",
    "transform-origin",
    "vector-effect",
    "visibility",
];

fn style_completion_items(
    source: &[u8],
    tree: &tree_sitter::Tree,
    byte_offset: usize,
) -> Option<Vec<CompletionItem>> {
    let stylesheet = svg_references::collect_inline_stylesheets(source, tree)
        .into_iter()
        .find(|stylesheet| {
            let end = stylesheet.start_byte + stylesheet.css.len();
            (stylesheet.start_byte..=end).contains(&byte_offset)
        })?;

    let css_offset = byte_offset.saturating_sub(stylesheet.start_byte);
    Some(css_completion_items(&stylesheet.css, css_offset))
}

fn css_completion_items(css: &str, byte_offset: usize) -> Vec<CompletionItem> {
    let context = css_completion_context(css, byte_offset);
    let custom_properties =
        svg_references::collect_custom_property_definitions_from_stylesheet(css, 0, 0);

    match context {
        CssCompletionContext::Selector => css_selector_completions(),
        CssCompletionContext::Property => css_property_completions(),
        CssCompletionContext::Value => css_value_completions(&custom_properties),
    }
}

fn css_completion_context(css: &str, byte_offset: usize) -> CssCompletionContext {
    let offset = byte_offset.min(css.len());
    let before = &css[..offset];

    let last_open = before.rfind('{');
    let last_close = before.rfind('}');
    let in_block = match (last_open, last_close) {
        (Some(open), Some(close)) => open > close,
        (Some(_), None) => true,
        _ => false,
    };

    if !in_block {
        return CssCompletionContext::Selector;
    }

    let block_start = last_open.map_or(0, |idx| idx + 1);
    let block_prefix = &before[block_start..];
    let declaration_start = block_prefix
        .rfind(';')
        .map_or(block_start, |idx| block_start + idx + 1);
    let declaration_prefix = &before[declaration_start..];

    if declaration_prefix.contains(':') {
        CssCompletionContext::Value
    } else {
        CssCompletionContext::Property
    }
}

fn css_selector_completions() -> Vec<CompletionItem> {
    let mut items = vec![
        CompletionItem {
            label: ":root".to_owned(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("CSS root selector".to_owned()),
            ..Default::default()
        },
        CompletionItem {
            label: ".".to_owned(),
            kind: Some(CompletionItemKind::REFERENCE),
            insert_text: Some(".$0".to_owned()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Class selector".to_owned()),
            ..Default::default()
        },
        CompletionItem {
            label: "#".to_owned(),
            kind: Some(CompletionItemKind::REFERENCE),
            insert_text: Some("#$0".to_owned()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("ID selector".to_owned()),
            ..Default::default()
        },
    ];

    items.extend(svg_data::elements().iter().map(|element| CompletionItem {
        label: element.name.to_owned(),
        kind: Some(CompletionItemKind::CLASS),
        detail: Some("SVG element selector".to_owned()),
        ..Default::default()
    }));

    items
}

fn css_property_completions() -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = CSS_PROPERTY_NAMES
        .iter()
        .map(|property| CompletionItem {
            label: (*property).to_owned(),
            kind: Some(CompletionItemKind::PROPERTY),
            insert_text: Some(format!("{property}: $0;")),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        })
        .collect();

    items.push(CompletionItem {
        label: "--custom-property".to_owned(),
        kind: Some(CompletionItemKind::VARIABLE),
        insert_text: Some("--$1: $0;".to_owned()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        detail: Some("CSS custom property".to_owned()),
        ..Default::default()
    });

    items
}

fn css_value_completions(custom_properties: &[svg_references::NamedSpan]) -> Vec<CompletionItem> {
    let mut items = vec![
        css_value_keyword("none"),
        css_value_keyword("currentColor"),
        css_value_keyword("transparent"),
        css_value_keyword("inherit"),
        css_value_function("var()", "var(--$0)", "CSS custom property reference"),
        css_value_function("url()", "url(#$0)", "SVG fragment reference"),
        css_value_function("rgb()", "rgb($0)", "RGB color"),
        css_value_function("hsl()", "hsl($0)", "HSL color"),
        css_value_function("hwb()", "hwb($0)", "HWB color"),
        css_value_function("lab()", "lab($0)", "Lab color"),
        css_value_function("lch()", "lch($0)", "LCH color"),
        css_value_function("oklab()", "oklab($0)", "Oklab color"),
        css_value_function("oklch()", "oklch($0)", "Oklch color"),
        css_value_function(
            "color-mix()",
            "color-mix(in oklch, $1, $2)",
            "Mixed color expression",
        ),
    ];

    let mut seen = std::collections::HashSet::new();
    for property in custom_properties {
        if !seen.insert(property.name.clone()) {
            continue;
        }
        items.push(CompletionItem {
            label: format!("var({})", property.name),
            kind: Some(CompletionItemKind::VARIABLE),
            insert_text: Some(format!("var({})", property.name)),
            detail: Some("CSS custom property".to_owned()),
            ..Default::default()
        });
    }

    items
}

fn css_value_keyword(keyword: &str) -> CompletionItem {
    CompletionItem {
        label: keyword.to_owned(),
        kind: Some(CompletionItemKind::VALUE),
        ..Default::default()
    }
}

fn css_value_function(label: &str, snippet: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(CompletionItemKind::FUNCTION),
        insert_text: Some(snippet.to_owned()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        detail: Some(detail.to_owned()),
        ..Default::default()
    }
}

fn suppression_code(diagnostic: &Diagnostic) -> Option<&str> {
    if diagnostic.source.as_deref() != Some("svg-lint") {
        return None;
    }

    match diagnostic.code.as_ref()? {
        NumberOrString::String(code) => code
            .parse::<svg_lint::DiagnosticCode>()
            .ok()
            .map(|_| code.as_str()),
        NumberOrString::Number(_) => None,
    }
}

fn line_indentation(source: &str, row: usize) -> String {
    source
        .lines()
        .nth(row)
        .unwrap_or_default()
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .collect()
}

fn line_start_range(row: u32) -> Range {
    Range::new(Position::new(row, 0), Position::new(row, 0))
}

fn file_suppression_insert_position(source: &str) -> Position {
    if source.starts_with("<?xml")
        && let Some(decl_end) = source.find("?>")
    {
        let mut offset = decl_end + 2;
        if source[offset..].starts_with("\r\n") {
            offset += 2;
        } else if source[offset..].starts_with('\n') {
            offset += 1;
        }
        return position_for_byte_offset(source.as_bytes(), offset);
    }

    Position::new(0, 0)
}

fn position_for_byte_offset(source: &[u8], byte_offset: usize) -> Position {
    let clamped = byte_offset.min(source.len());
    let row = source[..clamped]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count();
    let line_start = source[..clamped]
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map_or(0, |idx| idx + 1);
    let col = byte_col_to_utf16(source, row, clamped.saturating_sub(line_start));
    Position::new(row as u32, col)
}

fn suppression_comment_text(code: &str, next_line: bool, indentation: &str) -> String {
    let directive = if next_line {
        "svg-lint-disable-next-line"
    } else {
        "svg-lint-disable"
    };
    format!("{indentation}<!-- {directive} {code} -->\n")
}

fn suppression_workspace_edit(uri: &Uri, range: Range, new_text: String) -> WorkspaceEdit {
    WorkspaceEdit {
        changes: Some(HashMap::from([(
            uri.clone(),
            vec![TextEdit { range, new_text }],
        )])),
        ..Default::default()
    }
}

fn suppression_code_actions_for_diagnostic(
    uri: &Uri,
    source: &str,
    diagnostic: &Diagnostic,
) -> Vec<CodeActionOrCommand> {
    let Some(code) = suppression_code(diagnostic) else {
        return Vec::new();
    };

    let line = diagnostic.range.start.line as usize;
    let indentation = line_indentation(source, line);
    let line_comment = suppression_comment_text(code, true, &indentation);
    let file_comment = suppression_comment_text(code, false, "");
    let file_position = file_suppression_insert_position(source);

    vec![
        CodeActionOrCommand::CodeAction(CodeAction {
            title: format!("Suppress {code} on this line"),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            edit: Some(suppression_workspace_edit(
                uri,
                line_start_range(diagnostic.range.start.line),
                line_comment,
            )),
            is_preferred: Some(false),
            ..Default::default()
        }),
        CodeActionOrCommand::CodeAction(CodeAction {
            title: format!("Suppress {code} in this file"),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            edit: Some(suppression_workspace_edit(
                uri,
                Range::new(file_position, file_position),
                file_comment,
            )),
            is_preferred: Some(false),
            ..Default::default()
        }),
    ]
}

fn copy_data_uri_code_action(uri: &Uri) -> CodeActionOrCommand {
    CodeActionOrCommand::CodeAction(CodeAction {
        title: "Copy SVG as data URI".to_owned(),
        kind: Some(CodeActionKind::SOURCE),
        command: Some(Command {
            title: "Copy SVG as data URI".to_owned(),
            command: COPY_DATA_URI_COMMAND.to_owned(),
            arguments: Some(vec![Value::String(uri.as_str().to_owned())]),
        }),
        ..Default::default()
    })
}

fn css_rule_snippet(source: &str, span: &svg_references::Span) -> String {
    let source_bytes = source.as_bytes();
    let start = byte_offset_for_row_col(source_bytes, span.start_row, span.start_col);
    if start >= source_bytes.len() {
        return String::new();
    }

    if let Some(block_open) = source_bytes[start..]
        .iter()
        .position(|&byte| byte == b'{')
        .map(|offset| start + offset)
    {
        let selector_start = source_bytes[..start]
            .iter()
            .rposition(|&byte| byte == b'}')
            .map_or(0, |idx| idx + 1);

        if let Some(block_end) = matching_brace_end(source_bytes, block_open) {
            return source[selector_start..block_end].trim().to_owned();
        }
    }

    line_text_at(source, span.start_row)
}

fn css_declaration_snippet(source: &str, span: &svg_references::Span) -> String {
    let source_bytes = source.as_bytes();
    let start = byte_offset_for_row_col(source_bytes, span.start_row, span.start_col);
    if start >= source_bytes.len() {
        return String::new();
    }

    let declaration_start = source_bytes[..start]
        .iter()
        .rposition(|&byte| matches!(byte, b';' | b'{'))
        .map_or(0, |idx| idx + 1);
    let declaration_end = source_bytes[start..]
        .iter()
        .position(|&byte| matches!(byte, b';' | b'}'))
        .map_or(source_bytes.len(), |idx| start + idx);

    source[declaration_start..declaration_end].trim().to_owned()
}

fn matching_brace_end(source: &[u8], open_index: usize) -> Option<usize> {
    let mut depth = 0usize;

    for (idx, byte) in source.iter().enumerate().skip(open_index) {
        match *byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(idx + 1);
                }
            }
            _ => {}
        }
    }

    None
}

fn line_text_at(source: &str, row: usize) -> String {
    source
        .lines()
        .nth(row)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn resolve_stylesheet_url(base_uri: &Uri, href: &str) -> Option<Url> {
    if let Ok(url) = Url::parse(href) {
        return Some(url);
    }

    let base = Url::parse(base_uri.as_str()).ok()?;
    base.join(href).ok()
}

fn resolve_file_stylesheet(url: &Url) -> Option<CachedStylesheet> {
    let path = url.to_file_path().ok()?;
    let source = fs::read_to_string(path).ok()?;
    let uri = url.as_str().parse().ok()?;
    Some(parse_stylesheet(uri, source))
}

fn resolve_remote_stylesheet(cache: &StylesheetCache, url: &Url) -> Option<CachedStylesheet> {
    let key = url.as_str().to_owned();
    let cell = if let Ok(guard) = cache.read() {
        guard.get(&key).cloned()
    } else {
        None
    }
    .or_else(|| {
        let mut guard = cache.write().ok()?;
        Some(
            guard
                .entry(key)
                .or_insert_with(|| Arc::new(OnceLock::new()))
                .clone(),
        )
    })?;

    cell.get_or_init(|| {
        let source = ureq::get(url.as_str())
            .call()
            .ok()?
            .body_mut()
            .read_to_string()
            .ok()?;
        let uri = url.as_str().parse().ok()?;
        Some(parse_stylesheet(uri, source))
    })
    .clone()
}

fn resolve_external_stylesheet(
    cache: &StylesheetCache,
    base_uri: &Uri,
    href: &str,
) -> Option<(CachedStylesheet, bool)> {
    let url = resolve_stylesheet_url(base_uri, href)?;
    match url.scheme() {
        "file" => resolve_file_stylesheet(&url).map(|sheet| (sheet, false)),
        "http" | "https" => resolve_remote_stylesheet(cache, &url).map(|sheet| (sheet, true)),
        _ => None,
    }
}

/// Format element hover documentation as Markdown.
/// If `rt` is Some, its deprecated/baseline override the baked-in values.
fn format_element_hover(el: &svg_data::ElementDef, rt: Option<&CompatOverride>) -> String {
    let deprecated = rt.map_or(el.deprecated, |r| r.deprecated);
    let baseline = rt
        .and_then(|r| r.baseline.as_ref())
        .or(el.baseline.as_ref());

    let mut parts = Vec::new();

    if deprecated {
        parts.push(format!("~~{}~~", el.description));
        parts.push(String::new());
        parts.push("**Deprecated**".to_owned());
    } else {
        parts.push(el.description.to_owned());
    }

    if let Some(baseline) = baseline {
        parts.push(String::new());
        parts.push(format_baseline(baseline));
    }

    parts.push(String::new());
    parts.push(format!("[MDN Reference]({})", el.mdn_url));

    parts.join("\n")
}

/// Format attribute hover documentation as Markdown.
/// If `rt` is Some, its deprecated/baseline override the baked-in values.
fn format_attribute_hover(attr: &svg_data::AttributeDef, rt: Option<&CompatOverride>) -> String {
    let deprecated = rt.map_or(attr.deprecated, |r| r.deprecated);
    let baseline = rt
        .and_then(|r| r.baseline.as_ref())
        .or(attr.baseline.as_ref());

    let mut parts = Vec::new();

    if deprecated {
        parts.push(format!("~~{}~~", attr.description));
        parts.push(String::new());
        parts.push("**Deprecated**".to_owned());
    } else {
        parts.push(attr.description.to_owned());
    }

    // Show allowed values for enumerated attributes
    match &attr.values {
        svg_data::AttributeValues::Enum(vals) => {
            parts.push(String::new());
            parts.push(format!("Values: `{}`", vals.join("` | `")));
        }
        svg_data::AttributeValues::Transform(funcs) => {
            parts.push(String::new());
            parts.push(format!("Functions: `{}`", funcs.join("` | `")));
        }
        svg_data::AttributeValues::PreserveAspectRatio {
            alignments,
            meet_or_slice,
        } => {
            parts.push(String::new());
            parts.push(format!("Alignments: `{}`", alignments.join("` | `")));
            parts.push(format!("Scaling: `{}`", meet_or_slice.join("` | `")));
        }
        _ => {}
    }

    if let Some(baseline) = baseline {
        parts.push(String::new());
        parts.push(format_baseline(baseline));
    }

    parts.push(String::new());
    parts.push(format!("[MDN Reference]({})", attr.mdn_url));

    parts.join("\n")
}

/// Format externally sourced attribute documentation as Markdown.
fn format_external_attribute_hover(
    description: impl AsRef<str>,
    reference_label: &str,
    reference_url: &str,
) -> String {
    format!(
        "{}\n\n[{}]({})",
        description.as_ref(),
        reference_label,
        reference_url
    )
}

fn external_attribute_hover(kind: &str, attr_name: &str) -> Option<String> {
    const XML_NAMES_URL: &str = "https://www.w3.org/TR/REC-xml-names/";
    const XML_DECL_URL: &str = "https://www.w3.org/TR/xml/";

    match kind {
        "xml_version_attribute_name" => {
            return Some(format_external_attribute_hover(
                "Specifies the XML version used by the document declaration.",
                "W3C XML Reference",
                XML_DECL_URL,
            ));
        }
        "xml_encoding_attribute_name" => {
            return Some(format_external_attribute_hover(
                "Specifies the character encoding declared for the XML document.",
                "W3C XML Reference",
                XML_DECL_URL,
            ));
        }
        "xml_standalone_attribute_name" => {
            return Some(format_external_attribute_hover(
                "Declares whether the XML document relies on external markup declarations.",
                "W3C XML Reference",
                XML_DECL_URL,
            ));
        }
        _ => {}
    }

    if attr_name == "xmlns" {
        return Some(format_external_attribute_hover(
            "Declares the default XML namespace for this element and its descendants.",
            "W3C Namespaces in XML",
            XML_NAMES_URL,
        ));
    }

    if let Some(prefix) = attr_name.strip_prefix("xmlns:") {
        return Some(format_external_attribute_hover(
            format!(
                "Declares the `{prefix}` XML namespace prefix for this element and its descendants."
            ),
            "W3C Namespaces in XML",
            XML_NAMES_URL,
        ));
    }

    let mdn_reference_url = |name: &str| {
        format!("https://developer.mozilla.org/en-US/docs/Web/SVG/Reference/Attribute/{name}")
    };

    match attr_name {
        "xml:lang" => Some(format_external_attribute_hover(
            "Specifies the natural language used by the element's text content and attribute values.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xml:space" => Some(format_external_attribute_hover(
            "Controls how XML whitespace is handled for the element's character data.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xml:base" => Some(format_external_attribute_hover(
            "Specifies the base URI used to resolve relative URLs within the element.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:href" => Some(format_external_attribute_hover(
            "Legacy XLink form of `href` used to point at linked resources in SVG.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:arcrole" => Some(format_external_attribute_hover(
            "Legacy XLink attribute that identifies the semantic role of the link arc.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:role" => Some(format_external_attribute_hover(
            "Legacy XLink attribute that identifies the semantic role of the linked resource.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:show" => Some(format_external_attribute_hover(
            "Legacy XLink attribute that hints how the linked resource should be presented.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:title" => Some(format_external_attribute_hover(
            "Legacy XLink attribute that provides a human-readable title for the link.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:type" => Some(format_external_attribute_hover(
            "Legacy XLink attribute that declares the XLink link type.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:actuate" => Some(format_external_attribute_hover(
            "Legacy XLink attribute that hints when the linked resource should be traversed.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        _ => None,
    }
}

// Baseline status SVG icons embedded from assets/, base64-encoded into data URIs.
static BASELINE_HIGH: LazyLock<String> =
    LazyLock::new(|| svg_data_uri(include_str!("../assets/baseline-high.svg")));
static BASELINE_LOW: LazyLock<String> =
    LazyLock::new(|| svg_data_uri(include_str!("../assets/baseline-low.svg")));
static BASELINE_LIMITED: LazyLock<String> =
    LazyLock::new(|| svg_data_uri(include_str!("../assets/baseline-limited.svg")));

fn svg_data_uri(svg: &str) -> String {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(svg);
    format!("data:image/svg+xml;base64,{encoded}")
}

/// Format a baseline status line with inline icon.
fn format_baseline(baseline: &BaselineStatus) -> String {
    match baseline {
        BaselineStatus::Widely { since } => {
            let icon = &*BASELINE_HIGH;
            format!(
                "![Baseline icon]({icon}) _Widely available across major browsers (Baseline since {since})_"
            )
        }
        BaselineStatus::Newly { since } => {
            let icon = &*BASELINE_LOW;
            format!(
                "![Baseline icon]({icon}) _Newly available across major browsers (Baseline since {since})_"
            )
        }
        BaselineStatus::Limited => {
            let icon = &*BASELINE_LIMITED;
            format!("![Baseline icon]({icon}) _Limited availability across major browsers_")
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
    "preserve_aspect_ratio_attribute_name",
    "points_attribute_name",
    "d_attribute_name",
    "href_attribute_name",
    "style_attribute_name",
    "functional_iri_attribute_name",
    "opacity_attribute_name",
    "class_attribute_name",
    "event_attribute_name",
    "id_attribute_name",
    "xml_version_attribute_name",
    "xml_encoding_attribute_name",
    "xml_standalone_attribute_name",
];

/// Find the tree-sitter node at a given byte offset, preferring the deepest (leaf) node.
fn deepest_node_at(tree: &tree_sitter::Tree, byte_offset: usize) -> tree_sitter::Node<'_> {
    tree.root_node()
        .descendant_for_byte_range(byte_offset, byte_offset)
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
fn enclosing_element_name<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
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
                    // Re-lint all open documents with fresh data
                    let docs = documents.read().await;
                    for (uri, doc) in docs.iter() {
                        let source_bytes = doc.source.as_bytes();
                        let lint_diags = svg_lint::lint_tree(source_bytes, &doc.tree);
                        publish_lint_diagnostics(&client, uri.clone(), source_bytes, lint_diags)
                            .await;
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
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                color_provider: Some(ColorProviderCapability::Simple(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![COPY_DATA_URI_COMMAND.to_owned()],
                    ..Default::default()
                }),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "<".to_string(),
                        " ".to_string(),
                        "\"".to_string(),
                        "'".to_string(),
                        ":".to_string(),
                        "-".to_string(),
                    ]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("shutdown requested");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        tracing::debug!(uri = ?params.text_document.uri, "did_open");
        self.update_document(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            tracing::debug!(uri = ?params.text_document.uri, "did_change");
            self.update_document(params.text_document.uri, change.text)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        tracing::debug!(uri = ?params.text_document.uri, "did_close");
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

        let (element_markdown, attribute_markdown, class_hover, property_hover) = {
            let docs = self.documents.read().await;
            let Some(doc) = docs.get(uri) else {
                return Ok(None);
            };

            let source = doc.source.as_bytes();
            let byte_offset = byte_offset_for_position(source, pos);

            let raw_node = deepest_node_at(&doc.tree, byte_offset);
            let node = if !raw_node.is_named() {
                raw_node.parent().unwrap_or(raw_node)
            } else {
                raw_node
            };
            let kind = node.kind().to_owned();
            let node_text = node.utf8_text(source).unwrap_or("").to_owned();

            let rt = self.runtime_compat.read().await;
            let element_markdown = if kind == "name" {
                node.parent().and_then(|parent| {
                    let parent_kind = parent.kind();
                    if parent_kind == "start_tag"
                        || parent_kind == "self_closing_tag"
                        || parent_kind == "end_tag"
                    {
                        svg_data::element(&node_text).map(|el| {
                            let rt_override = rt.as_ref().and_then(|r| r.elements.get(&node_text));
                            format_element_hover(el, rt_override)
                        })
                    } else {
                        None
                    }
                })
            } else {
                None
            };

            let attribute_markdown =
                if ATTRIBUTE_NAME_KINDS.contains(&kind.as_str()) || kind == "attribute_name" {
                    if let Some(attr) = svg_data::attribute(&node_text) {
                        let rt_override = rt.as_ref().and_then(|r| r.attributes.get(&node_text));
                        Some(format_attribute_hover(attr, rt_override))
                    } else {
                        external_attribute_hover(&kind, &node_text)
                    }
                } else {
                    None
                };

            let definition_target =
                svg_references::definition_target_at(source, &doc.tree, byte_offset);
            let stylesheet_hrefs = svg_references::extract_xml_stylesheet_hrefs(source);
            let inline_stylesheets = svg_references::collect_inline_stylesheets(source, &doc.tree);

            let class_hover = if let Some(svg_references::DefinitionTarget::Class(target_class)) =
                &definition_target
            {
                let definitions = inline_stylesheets
                    .iter()
                    .flat_map(|stylesheet| {
                        svg_references::collect_class_definitions_from_stylesheet(
                            &stylesheet.css,
                            stylesheet.start_row,
                            stylesheet.start_col,
                        )
                    })
                    .filter(|definition| definition.name == *target_class)
                    .map(|definition| ClassDefinitionHover {
                        uri: uri.clone(),
                        source: doc.source.clone(),
                        definition,
                    })
                    .collect::<Vec<_>>();

                (target_class.clone(), definitions, stylesheet_hrefs.clone())
            } else {
                (String::new(), Vec::new(), Vec::new())
            };

            let property_hover =
                if let Some(svg_references::DefinitionTarget::CustomProperty(target_property)) =
                    &definition_target
                {
                    let definitions = inline_stylesheets
                        .iter()
                        .flat_map(|stylesheet| {
                            svg_references::collect_custom_property_definitions_from_stylesheet(
                                &stylesheet.css,
                                stylesheet.start_row,
                                stylesheet.start_col,
                            )
                        })
                        .filter(|definition| definition.name == *target_property)
                        .map(|definition| CustomPropertyDefinitionHover {
                            uri: uri.clone(),
                            source: doc.source.clone(),
                            definition,
                        })
                        .collect::<Vec<_>>();

                    (target_property.clone(), definitions, stylesheet_hrefs)
                } else {
                    (String::new(), Vec::new(), Vec::new())
                };

            (
                element_markdown,
                attribute_markdown,
                class_hover,
                property_hover,
            )
        };

        if let Some(markdown) = element_markdown {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: markdown,
                }),
                range: None,
            }));
        }

        if let Some(markdown) = attribute_markdown {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: markdown,
                }),
                range: None,
            }));
        }

        let (target_class, mut class_definitions, stylesheet_hrefs) = class_hover;
        if !target_class.is_empty() {
            let mut local_definitions = Vec::new();
            let mut remote_definitions = Vec::new();

            for href in stylesheet_hrefs {
                let Some((stylesheet, is_remote)) =
                    resolve_external_stylesheet(&self.stylesheet_cache, uri, &href)
                else {
                    continue;
                };

                let defs = class_definition_hovers_from_stylesheet(
                    stylesheet.uri.clone(),
                    &stylesheet.source,
                    &target_class,
                );

                if is_remote {
                    remote_definitions.extend(defs);
                } else {
                    local_definitions.extend(defs);
                }
            }

            class_definitions.extend(local_definitions);
            class_definitions.extend(remote_definitions);

            if !class_definitions.is_empty() {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format_class_hover(&target_class, &class_definitions),
                    }),
                    range: None,
                }));
            }
        }

        let (target_property, mut property_definitions, stylesheet_hrefs) = property_hover;
        if !target_property.is_empty() {
            let mut local_definitions = Vec::new();
            let mut remote_definitions = Vec::new();

            for href in stylesheet_hrefs {
                let Some((stylesheet, is_remote)) =
                    resolve_external_stylesheet(&self.stylesheet_cache, uri, &href)
                else {
                    continue;
                };

                let defs = custom_property_definition_hovers_from_stylesheet(
                    stylesheet.uri.clone(),
                    &stylesheet.source,
                    &target_property,
                );

                if is_remote {
                    remote_definitions.extend(defs);
                } else {
                    local_definitions.extend(defs);
                }
            }

            property_definitions.extend(local_definitions);
            property_definitions.extend(remote_definitions);

            if !property_definitions.is_empty() {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format_custom_property_hover(
                            &target_property,
                            &property_definitions,
                        ),
                    }),
                    range: None,
                }));
            }
        }

        Ok(None)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let source = {
            let docs = self.documents.read().await;
            let Some(doc) = docs.get(uri) else {
                return Ok(None);
            };
            doc.source.clone()
        };

        let mut seen = std::collections::HashSet::new();
        let mut actions = vec![copy_data_uri_code_action(uri)];

        for diagnostic in &params.context.diagnostics {
            let Some(code) = suppression_code(diagnostic) else {
                continue;
            };
            let key = (code.to_owned(), diagnostic.range.start.line);
            if !seen.insert(key) {
                continue;
            }
            actions.extend(suppression_code_actions_for_diagnostic(
                uri, &source, diagnostic,
            ));
        }

        if actions.is_empty() {
            return Ok(None);
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

        let (target, inline_locations, stylesheet_hrefs) = {
            let docs = self.documents.read().await;
            let Some(doc) = docs.get(uri) else {
                return Ok(None);
            };

            let source = doc.source.as_bytes();
            let byte_offset = byte_offset_for_position(source, pos);
            let Some(target) = svg_references::definition_target_at(source, &doc.tree, byte_offset)
            else {
                return Ok(None);
            };

            match &target {
                svg_references::DefinitionTarget::Id(target_id) => {
                    let locations = svg_references::collect_id_definitions(source, &doc.tree)
                        .into_iter()
                        .filter(|definition| definition.name == *target_id)
                        .map(|definition| named_span_location(uri.clone(), source, &definition))
                        .collect();
                    (target, locations, Vec::new())
                }
                svg_references::DefinitionTarget::Class(target_class) => {
                    let inline_locations =
                        svg_references::collect_inline_stylesheets(source, &doc.tree)
                            .into_iter()
                            .flat_map(|stylesheet| {
                                svg_references::collect_class_definitions_from_stylesheet(
                                    &stylesheet.css,
                                    stylesheet.start_row,
                                    stylesheet.start_col,
                                )
                            })
                            .filter(|definition| definition.name == *target_class)
                            .map(|definition| named_span_location(uri.clone(), source, &definition))
                            .collect();
                    let hrefs = svg_references::extract_xml_stylesheet_hrefs(source);
                    (target, inline_locations, hrefs)
                }
                svg_references::DefinitionTarget::CustomProperty(target_property) => {
                    let inline_locations =
                        svg_references::collect_inline_stylesheets(source, &doc.tree)
                            .into_iter()
                            .flat_map(|stylesheet| {
                                svg_references::collect_custom_property_definitions_from_stylesheet(
                                    &stylesheet.css,
                                    stylesheet.start_row,
                                    stylesheet.start_col,
                                )
                            })
                            .filter(|definition| definition.name == *target_property)
                            .map(|definition| named_span_location(uri.clone(), source, &definition))
                            .collect();
                    let hrefs = svg_references::extract_xml_stylesheet_hrefs(source);
                    (target, inline_locations, hrefs)
                }
            }
        };

        if matches!(target, svg_references::DefinitionTarget::Id(_)) {
            return Ok(definition_response_from_locations(inline_locations));
        }

        let mut locations = inline_locations;
        let mut local_locations = Vec::new();
        let mut remote_locations = Vec::new();

        for href in stylesheet_hrefs {
            let Some((stylesheet, is_remote)) =
                resolve_external_stylesheet(&self.stylesheet_cache, uri, &href)
            else {
                continue;
            };

            let defs = match &target {
                svg_references::DefinitionTarget::Class(target_class) => stylesheet
                    .class_definitions
                    .iter()
                    .filter(|definition| definition.name == *target_class)
                    .map(|definition| {
                        named_span_location(
                            stylesheet.uri.clone(),
                            stylesheet.source.as_bytes(),
                            definition,
                        )
                    })
                    .collect::<Vec<_>>(),
                svg_references::DefinitionTarget::CustomProperty(target_property) => stylesheet
                    .custom_property_definitions
                    .iter()
                    .filter(|definition| definition.name == *target_property)
                    .map(|definition| {
                        named_span_location(
                            stylesheet.uri.clone(),
                            stylesheet.source.as_bytes(),
                            definition,
                        )
                    })
                    .collect::<Vec<_>>(),
                svg_references::DefinitionTarget::Id(_) => Vec::new(),
            };

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

        if let Some(items) = style_completion_items(source, &doc.tree, byte_offset)
            && !items.is_empty()
        {
            return Ok(Some(CompletionResponse::Array(items)));
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
                {
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
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn offset_of(source: &str, needle: &str) -> usize {
        source.find(needle).expect("needle present")
    }

    #[test]
    fn goto_definition_target_resolves_paint_server_reference() {
        let source = r#"<svg><rect fill="url(#style-gradient)" /><linearGradient id="style-gradient" /></svg>"#;
        let offset = offset_of(source, "style-gradient)") + 2;

        assert_eq!(
            svg_references::definition_target_at(
                source.as_bytes(),
                &svg_references_test_tree(source),
                offset,
            ),
            Some(svg_references::DefinitionTarget::Id(
                "style-gradient".into()
            ))
        );
    }

    #[test]
    fn goto_definition_target_does_not_resolve_url_wrapper() {
        let source = r#"<svg><rect fill="url(#style-gradient)" /><linearGradient id="style-gradient" /></svg>"#;
        let offset = offset_of(source, "url(") + 1;

        assert_eq!(
            svg_references::definition_target_at(
                source.as_bytes(),
                &svg_references_test_tree(source),
                offset,
            ),
            None
        );
    }

    #[test]
    fn collect_id_definitions_matches_id_token() {
        let source = r#"<svg><rect fill="url(#style-gradient)" /><linearGradient id="style-gradient" /></svg>"#;
        let definitions = svg_references::collect_id_definitions(
            source.as_bytes(),
            &svg_references_test_tree(source),
        );
        assert!(
            definitions
                .iter()
                .any(|definition| definition.name == "style-gradient")
        );
    }

    #[test]
    fn resolve_stylesheet_url_handles_relative_file_href() {
        let base: Uri = "file:///tmp/example.svg".parse().expect("uri");
        let resolved = resolve_stylesheet_url(&base, "styles/site.css").expect("resolved");

        assert_eq!(resolved.as_str(), "file:///tmp/styles/site.css");
    }

    #[test]
    fn resolve_file_stylesheet_collects_class_definitions() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("svg-ls-style-{unique}"));
        fs::create_dir_all(&temp_dir).expect("temp dir");
        let css_path = temp_dir.join("style.css");
        fs::write(&css_path, ".uses-color { fill: red; }").expect("css written");

        let url = Url::from_file_path(&css_path).expect("file url");
        let stylesheet = resolve_file_stylesheet(&url).expect("stylesheet");

        assert_eq!(
            stylesheet
                .class_definitions
                .iter()
                .map(|definition| definition.name.as_str())
                .collect::<Vec<_>>(),
            vec!["uses-color"]
        );

        fs::remove_file(&css_path).expect("cleanup css");
        fs::remove_dir(&temp_dir).expect("cleanup dir");
    }

    fn svg_references_test_tree(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_svg::LANGUAGE.into())
            .expect("SVG grammar");
        parser.parse(source, None).expect("tree")
    }
}
