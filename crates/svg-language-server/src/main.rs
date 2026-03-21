use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use svg_data::{AttributeValues, BaselineStatus, ContentModel};
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    Color, ColorInformation, ColorPresentation, ColorPresentationParams, ColorProviderCapability,
    CompletionItem, CompletionItemKind, CompletionItemTag, CompletionOptions, CompletionParams,
    CompletionResponse, Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentColorParams,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverParams,
    HoverProviderCapability, InitializeParams, InitializeResult, InsertTextFormat, Location,
    MarkupContent, MarkupKind, NumberOrString, OneOf, Position, Range, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Uri,
};
use tower_lsp_server::{Client, LanguageServer, LspService, Server};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{Layer, Registry};

/// Parsed document state: source text + tree-sitter tree.
struct DocumentState {
    source: String,
    tree: tree_sitter::Tree,
}

/// Cache mapping (URI, start_line, start_character) to the original color kind.
type ColorKindCache = Arc<RwLock<HashMap<(Uri, u32, u32), svg_color::ColorKind>>>;

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
    let line_start: usize = source
        .split(|&b| b == b'\n')
        .take(position.line as usize)
        .map(|line| line.len() + 1)
        .sum();
    line_start + byte_col
}

fn node_range_utf16(source: &[u8], node: tree_sitter::Node<'_>) -> Range {
    Range::new(
        Position::new(
            node.start_position().row as u32,
            byte_col_to_utf16(
                source,
                node.start_position().row,
                node.start_position().column,
            ),
        ),
        Position::new(
            node.end_position().row as u32,
            byte_col_to_utf16(source, node.end_position().row, node.end_position().column),
        ),
    )
}

fn find_descendant_any<'a>(
    node: tree_sitter::Node<'a>,
    kinds: &[&str],
) -> Option<tree_sitter::Node<'a>> {
    if kinds.contains(&node.kind()) {
        return Some(node);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if let Some(found) = find_descendant_any(cursor.node(), kinds) {
                return Some(found);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    None
}

fn definition_target_id(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    if let Some(iri) = find_ancestor_any(node, &["iri_reference"]) {
        let text = iri.utf8_text(source).ok()?;
        return text.strip_prefix('#').map(ToOwned::to_owned);
    }

    for kind in [
        "paint_server",
        "href_reference",
        "functional_iri_attribute_value",
        "href_attribute_value",
    ] {
        if let Some(container) = find_ancestor_any(node, &[kind])
            && let Some(iri) = find_descendant_any(container, &["iri_reference"])
        {
            let text = iri.utf8_text(source).ok()?;
            return text.strip_prefix('#').map(ToOwned::to_owned);
        }
    }

    if let Some(id_token) = find_ancestor_any(node, &["id_token"]) {
        return Some(id_token.utf8_text(source).ok()?.to_owned());
    }

    for kind in ["quoted_attribute_value", "functional_iri_attribute_value"] {
        if let Some(value_node) = find_ancestor_any(node, &[kind]) {
            let text = value_node.utf8_text(source).ok()?;
            return text.strip_prefix('#').map(ToOwned::to_owned);
        }
    }

    None
}

fn find_id_definition_node<'a>(
    tree: &'a tree_sitter::Tree,
    source: &[u8],
    target_id: &str,
) -> Option<tree_sitter::Node<'a>> {
    let root = tree.root_node();
    let mut cursor = root.walk();

    if !cursor.goto_first_child() {
        return None;
    }

    loop {
        let node = cursor.node();
        if node.kind() == "id_token" && node.utf8_text(source).ok()? == target_id {
            return Some(node);
        }

        if cursor.goto_first_child() {
            continue;
        }

        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return None;
            }
        }
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
                        let lsp_diags: Vec<Diagnostic> = lint_diags
                            .into_iter()
                            .map(|d| {
                                let start_char =
                                    byte_col_to_utf16(source_bytes, d.start_row, d.start_col);
                                let end_char =
                                    byte_col_to_utf16(source_bytes, d.end_row, d.end_col);
                                let severity = match d.severity {
                                    svg_lint::Severity::Error => DiagnosticSeverity::ERROR,
                                    svg_lint::Severity::Warning => DiagnosticSeverity::WARNING,
                                    svg_lint::Severity::Information => {
                                        DiagnosticSeverity::INFORMATION
                                    }
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
                        client
                            .publish_diagnostics(uri.clone(), lsp_diags, None)
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

        let raw_node = deepest_node_at(&doc.tree, byte_offset);
        // Anonymous nodes (string literals like "font-size") need to be resolved
        // to their named parent (e.g. length_attribute_name).
        let node = if !raw_node.is_named() {
            raw_node.parent().unwrap_or(raw_node)
        } else {
            raw_node
        };
        let kind = node.kind();

        let rt = self.runtime_compat.read().await;

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
                    let rt_override = rt.as_ref().and_then(|r| r.elements.get(name_text));
                    let markdown = format_element_hover(el, rt_override);
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

        // Attribute name hover (typed + generic attribute names)
        if ATTRIBUTE_NAME_KINDS.contains(&kind) || kind == "attribute_name" {
            let name_text = node.utf8_text(source).unwrap_or("");
            if let Some(attr) = svg_data::attribute(name_text) {
                let rt_override = rt.as_ref().and_then(|r| r.attributes.get(name_text));
                let markdown = format_attribute_hover(attr, rt_override);
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: markdown,
                    }),
                    range: None,
                }));
            }
            if let Some(markdown) = external_attribute_hover(kind, name_text) {
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

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

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

        let Some(target_id) = definition_target_id(node, source) else {
            return Ok(None);
        };

        let Some(definition) = find_id_definition_node(&doc.tree, source, &target_id) else {
            return Ok(None);
        };

        let location = Location::new(uri.clone(), node_range_utf16(source, definition));
        Ok(Some(GotoDefinitionResponse::Scalar(location)))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_tree(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_svg::LANGUAGE.into())
            .expect("SVG grammar");
        parser.parse(source, None).expect("tree")
    }

    fn offset_of(source: &str, needle: &str) -> usize {
        source.find(needle).expect("needle present")
    }

    #[test]
    fn definition_target_id_resolves_paint_server_reference() {
        let source = r#"<svg><rect fill="url(#style-gradient)" /><linearGradient id="style-gradient" /></svg>"#;
        let tree = parse_tree(source);
        let offset = offset_of(source, "style-gradient)") + 2;
        let node = deepest_node_at(&tree, offset);

        assert_eq!(
            definition_target_id(node, source.as_bytes()).as_deref(),
            Some("style-gradient")
        );
    }

    #[test]
    fn find_id_definition_node_matches_id_token() {
        let source = r#"<svg><rect fill="url(#style-gradient)" /><linearGradient id="style-gradient" /></svg>"#;
        let tree = parse_tree(source);
        let definition =
            find_id_definition_node(&tree, source.as_bytes(), "style-gradient").expect("id token");

        assert_eq!(definition.kind(), "id_token");
        assert_eq!(
            definition.utf8_text(source.as_bytes()).expect("text"),
            "style-gradient"
        );
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
