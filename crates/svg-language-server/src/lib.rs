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
        DidChangeConfigurationParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
        DidOpenTextDocumentParams, DocumentColorParams, DocumentFormattingParams,
        ExecuteCommandOptions, ExecuteCommandParams, GotoDefinitionParams, GotoDefinitionResponse,
        Hover, HoverContents, HoverParams, HoverProviderCapability, InitializeParams,
        InitializeResult, MarkupContent, MarkupKind, MessageType, OneOf, Position, Range,
        ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Uri,
    },
};
use url::Url;

mod clipboard;
mod code_actions;
mod compat;
mod completion;
mod definition;
mod diagnostics;
mod freshness;
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
    UnsupportedAttributeHoverProfile, external_attribute_hover,
    format_attribute_hover_with_profile_name, format_class_hover, format_custom_property_hover,
    format_element_hover_with_profile, format_unsupported_attribute_hover_with_profile_name,
    profile_lifecycle_hover_line,
};
use logging::init_logging;
use positions::{byte_col_to_utf16, byte_offset_for_position, end_position_utf16, u32_from_usize};
use stylesheets::{
    CachedStylesheet, ClassDefinitionHover, CustomPropertyDefinitionHover,
    class_definition_hovers_from_stylesheet, custom_property_definition_hovers_from_stylesheet,
    definition_response_from_locations, resolve_external_stylesheet,
};
use svg_tree::{
    deepest_node_at, find_ancestor_any, is_attribute_name_kind, is_attribute_node_kind,
};

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

/// The compatibility target the workspace configured, modelled as a real ADT
/// rather than a bare [`svg_data::SpecSnapshotId`].
///
/// This makes the configured target's *kind* explicit so each variant can carry
/// its own resolution rules and (eventually) its own constraint application:
///
/// - [`Snapshot`](ConfiguredTarget::Snapshot) — one of the four curated
///   snapshots, the historical default. Drives the snapshot-typed completion /
///   hover / lint pipeline directly.
/// - [`Edition`](ConfiguredTarget::Edition) — any edition-keyed inventory,
///   including the SVG 1.0 REC, SVG 1.1 PR, and the older SVG 2 CRs that have no
///   [`SpecSnapshotId`]. The existing pipeline is snapshot-typed, so an edition
///   is *also* resolved to a base snapshot (its nearest curated equivalent) for
///   the catalog lookups; the richer [`EditionId`] is retained so edition-keyed
///   inventory wiring can consume it.
/// - [`SvgNative`](ConfiguredTarget::SvgNative) — the SVG Native profile, an
///   SVG 2 subset. Resolves to the SVG 2 editor's draft base snapshot, with
///   profile constraints applied on top (see [`ProfileConfig::is_constrained`]).
#[derive(Clone)]
enum ConfiguredTarget {
    /// One of the four curated catalog snapshots.
    Snapshot(svg_data::SpecSnapshotId),
    /// An edition-keyed inventory addressed by its [`EditionId`].
    Edition(svg_data::inventory::EditionId),
    /// The SVG Native profile (an SVG 2 subset).
    SvgNative,
}

impl ConfiguredTarget {
    /// The base [`SpecSnapshotId`] that drives the snapshot-typed catalog
    /// pipeline (completion / hover / lint) for this target.
    ///
    /// Snapshots are themselves. Editions map to their nearest curated snapshot
    /// (the editor's draft for any SVG 2 edition, SVG 1.1 SE for the 1.x
    /// editions). SVG Native is an SVG 2 subset, so it bases on the SVG 2
    /// editor's draft.
    const fn base_snapshot(&self) -> svg_data::SpecSnapshotId {
        match self {
            Self::Snapshot(snapshot) => *snapshot,
            Self::Edition(edition) => edition_base_snapshot(edition),
            Self::SvgNative => svg_data::SpecSnapshotId::Svg2EditorsDraft,
        }
    }
}

/// A short, stable description of a configured target for tracing/logging.
fn describe_target(target: &ConfiguredTarget) -> String {
    match target {
        ConfiguredTarget::Snapshot(snapshot) => format!("snapshot:{}", snapshot.as_str()),
        ConfiguredTarget::Edition(edition) => match &edition.date {
            svg_data::inventory::EditionDate::Dated { date } => {
                format!("edition:{:?}:{date}", edition.series)
            }
            svg_data::inventory::EditionDate::EditorsDraft => {
                format!("edition:{:?}:editors-draft", edition.series)
            }
        },
        ConfiguredTarget::SvgNative => "profile:svg-native".to_owned(),
    }
}

/// Whether `edition` is one of the four curated [`SpecSnapshotId`] editions
/// (which the snapshot-typed catalog already models faithfully).
fn edition_is_curated_snapshot(edition: &svg_data::inventory::EditionId) -> bool {
    use svg_data::inventory::EditionId;
    [
        svg_data::SpecSnapshotId::Svg11Rec20030114,
        svg_data::SpecSnapshotId::Svg11Rec20110816,
        svg_data::SpecSnapshotId::Svg2Cr20181004,
        svg_data::SpecSnapshotId::Svg2EditorsDraft,
    ]
    .iter()
    .any(|snapshot| &EditionId::for_snapshot(*snapshot) == edition)
}

/// Map an [`EditionId`] onto the nearest curated [`SpecSnapshotId`] for the
/// snapshot-typed pipeline. SVG 1.0/1.1 editions collapse to SVG 1.1 SE; SVG 2
/// editions collapse to the editor's draft.
const fn edition_base_snapshot(
    edition: &svg_data::inventory::EditionId,
) -> svg_data::SpecSnapshotId {
    use svg_data::edition::Series;
    match edition.series {
        Series::Svg10 | Series::Svg11 => svg_data::SpecSnapshotId::Svg11Rec20110816,
        Series::Svg2 => svg_data::SpecSnapshotId::Svg2EditorsDraft,
    }
}

#[derive(Clone)]
struct ProfileConfig {
    /// What the workspace asked for, in its native kind.
    target: ConfiguredTarget,
    /// When true, the resolved base snapshot is used verbatim for every
    /// document; the root `<svg version="X">` attribute is ignored. Lets users
    /// pin a profile for workspaces that mix legacy and modern SVGs.
    force: bool,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            target: ConfiguredTarget::Snapshot(svg_lint::LintOptions::default().profile),
            force: false,
        }
    }
}

impl ProfileConfig {
    /// The base snapshot the workspace resolved to (ignoring per-document root
    /// `version`).
    const fn resolved(&self) -> svg_data::SpecSnapshotId {
        self.target.base_snapshot()
    }

    /// Whether this target additionally constrains the base snapshot (i.e. the
    /// SVG Native profile, which restricts the SVG 2 element/attribute set).
    /// Snapshots and editions impose no extra constraint beyond their base.
    const fn is_constrained(&self) -> bool {
        matches!(self.target, ConfiguredTarget::SvgNative)
    }

    /// The baked, spec-faithful [`Inventory`] for an [`Edition`] target, when
    /// the configured edition is *not* one of the four curated snapshots.
    ///
    /// [`Snapshot`]/[`SvgNative`] targets, and editions that coincide with a
    /// curated snapshot, return `None` — the curated catalog already models
    /// them faithfully, so there is nothing to restrict. A returned inventory is
    /// used as an *additive restriction* over the curated completion lists: only
    /// names the edition's inventory actually declares survive (see
    /// `restrict_attribute_items_to_inventory` /
    /// `restrict_child_items_to_inventory`).
    ///
    /// [`Snapshot`]: ConfiguredTarget::Snapshot
    /// [`Edition`]: ConfiguredTarget::Edition
    /// [`SvgNative`]: ConfiguredTarget::SvgNative
    fn edition_inventory(&self) -> Option<&'static svg_data::inventory::Inventory> {
        let ConfiguredTarget::Edition(edition) = &self.target else {
            return None;
        };
        // Editions that *are* a curated snapshot are already faithfully modelled
        // by the snapshot pipeline; only the additive (non-snapshot) editions
        // need an inventory restriction.
        if edition_is_curated_snapshot(edition) {
            return None;
        }
        svg_data::inventory::for_edition(edition)
    }

    /// The baked SVG Native constraint set to enforce, when this target is the
    /// SVG Native profile. `None` for snapshot/edition targets, which impose no
    /// reductive constraint beyond their base catalog. Used to drop completion
    /// items for constructs SVG Native does not support.
    fn native_constraints(&self) -> Option<&'static svg_data::profile::SvgNative> {
        self.is_constrained().then(svg_data::profile::svg_native)
    }

    /// The edition inventory to restrict against for this document.
    ///
    /// A document that declares a `version` resolving to an edition with no
    /// faithful snapshot (SVG 1.0) is restricted to that edition's inventory on
    /// top of its nearest-snapshot base — so an SVG 1.0 document is linted as
    /// SVG 1.0, not the SVG 1.1 snapshot its version collapses to. Document
    /// detection wins over the configured edition unless `force` is set; when
    /// neither applies, the configured edition target (if any) is used.
    fn active_edition_inventory(
        &self,
        doc: &DocumentState,
    ) -> Option<&'static svg_data::inventory::Inventory> {
        if !self.force
            && let Some(version) =
                svg_lint::extract_declared_version(&doc.tree, doc.source.as_bytes())
            && let Some(edition) = svg_data::edition_for_svg_version_attr(version)
        {
            return svg_data::inventory::for_edition(&edition);
        }
        self.edition_inventory()
    }

    fn lint_options_for(&self, doc: &DocumentState) -> svg_lint::LintOptions {
        svg_lint::LintOptions {
            profile: self.effective_profile_for(doc),
            native: self.native_constraints(),
            edition: self.active_edition_inventory(doc),
        }
    }

    fn effective_profile_for(&self, doc: &DocumentState) -> svg_data::SpecSnapshotId {
        svg_lint::effective_profile(
            &doc.tree,
            doc.source.as_bytes(),
            self.resolved(),
            self.force,
        )
    }
}

fn configured_profile(config: &Value) -> Option<&str> {
    config
        .get("svg")
        .and_then(Value::as_object)
        .and_then(|svg| svg.get("profile"))
        .and_then(Value::as_str)
}

fn configured_force_profile(config: &Value) -> bool {
    config
        .get("svg")
        .and_then(Value::as_object)
        .and_then(|svg| svg.get("force_profile"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Whether the client opted into the runtime spec-freshness check
/// (`svg.spec_freshness_check`). Off by default: it contacts `api.w3.org` and
/// `api.github.com`, which users must consent to.
fn configured_spec_freshness(config: &Value) -> bool {
    config
        .get("svg")
        .and_then(Value::as_object)
        .and_then(|svg| svg.get("spec_freshness_check"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Whether the client allows the runtime browser-compat refresh
/// (`svg.runtime_compat`). On by default: it fetches MDN BCD + web-features from
/// `unpkg.com` to overlay fresher support/baseline data onto the baked catalog.
/// Offline or privacy-sensitive sessions can set this to `false` to keep the
/// server fully local — hover and lint then use the baked compat data only.
fn configured_runtime_compat(config: &Value) -> bool {
    config
        .get("svg")
        .and_then(Value::as_object)
        .and_then(|svg| svg.get("runtime_compat"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// Match a config string against the SVG Native profile aliases.
fn is_svg_native_profile(requested: &str) -> bool {
    let normalized = requested
        .trim()
        .to_ascii_lowercase()
        .replace(['_', ' '], "-");
    matches!(normalized.as_str(), "svg-native" | "svgnative" | "native")
}

/// Resolve an `svg.edition` config value into an [`EditionId`] with a baked
/// inventory.
///
/// Two surfaces are accepted:
/// - a **string** edition key (e.g. `"svg2-2016-09-15"`,
///   `"svg2-editors-draft"`), resolved through
///   [`svg_data::resolve_edition_id`] so the LSP and the rest of the crate share
///   one canonical key vocabulary (punctuation/case-insensitive); or
/// - an **object** `{ "series": "svg2", "date": "2016-09-15" }` for a dated
///   edition / `{ "series": "svg2", "editors_draft": true }` for the rolling
///   editor's draft.
///
/// Either way the result is returned only when
/// [`svg_data::inventory::for_edition`] has a baked inventory for it, so an
/// unknown edition falls back rather than silently selecting an empty catalog.
fn configured_edition(config: &Value) -> Option<svg_data::inventory::EditionId> {
    use svg_data::edition::Series;
    use svg_data::inventory::EditionId;

    let edition_value = config
        .get("svg")
        .and_then(Value::as_object)
        .and_then(|svg| svg.get("edition"))?;

    // String form delegates to svg-data's canonical key resolver. Every edition
    // it returns comes from the baked `EDITION_INVENTORIES`, so a baked
    // inventory is guaranteed (the `for_edition` check below is then a no-op).
    if let Some(key) = edition_value.as_str() {
        return svg_data::resolve_edition_id(key)
            .filter(|id| svg_data::inventory::for_edition(id).is_some());
    }

    let edition = edition_value.as_object()?;

    let series = match edition.get("series").and_then(Value::as_str)?.trim() {
        "svg10" | "svg1.0" | "1.0" => Series::Svg10,
        "svg11" | "svg1.1" | "1.1" => Series::Svg11,
        "svg2" | "svg2.0" | "2" | "2.0" => Series::Svg2,
        _ => return None,
    };

    let id = if edition
        .get("editors_draft")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        EditionId::editors_draft(series)
    } else {
        let date = edition.get("date").and_then(Value::as_str)?.trim();
        EditionId {
            series,
            date: svg_data::inventory::EditionDate::Dated {
                date: std::borrow::Cow::Owned(date.to_owned()),
            },
        }
    };

    // Curated snapshot editions are already represented by the snapshot
    // catalog, so they do not need an additive inventory to be accepted.
    if edition_is_curated_snapshot(&id) {
        return Some(id);
    }

    // Non-curated editions need a baked inventory to act as the authority.
    svg_data::inventory::for_edition(&id)
        .is_some()
        .then_some(id)
}

fn resolve_profile_config(config: &Value) -> (ProfileConfig, Option<String>) {
    let force = configured_force_profile(config);

    // Edition-keyed inventories (`svg.edition`) take precedence: they address
    // the additive universe beyond the four curated snapshots.
    if let Some(edition) = configured_edition(config) {
        return (
            ProfileConfig {
                target: ConfiguredTarget::Edition(edition),
                force,
            },
            None,
        );
    }

    let Some(requested) = configured_profile(config) else {
        return (
            ProfileConfig {
                force,
                ..ProfileConfig::default()
            },
            None,
        );
    };

    if is_svg_native_profile(requested) {
        return (
            ProfileConfig {
                target: ConfiguredTarget::SvgNative,
                force,
            },
            None,
        );
    }

    if let Some(resolved) = svg_data::resolve_profile_id(requested) {
        return (
            ProfileConfig {
                target: ConfiguredTarget::Snapshot(resolved),
                force,
            },
            None,
        );
    }

    (
        ProfileConfig {
            force,
            ..ProfileConfig::default()
        },
        Some(format!(
            "Unknown SVG profile `{requested}`; falling back to {}.",
            svg_lint::LintOptions::default().profile.as_str()
        )),
    )
}

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

fn attribute_wrapper_ancestor(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut cursor = node;
    loop {
        let kind = cursor.kind();
        if is_attribute_node_kind(kind) {
            return Some(cursor);
        }
        cursor = cursor.parent()?;
    }
}

/// Restrict attribute completion `items` to attributes the edition `inventory`
/// attaches to `elem_name` via its `(element, attribute)` edges.
///
/// This is the additive inventory wiring for an edition target: the curated
/// catalog may surface modern HTML/CSS attributes the older edition never
/// defined, so we drop any item the edition's spec-faithful inventory does not
/// declare for this element. When the inventory knows nothing about the element
/// (no edges), the list is left as-is rather than emptied — an unknown element
/// is not evidence that *no* attribute applies.
fn restrict_attribute_items_to_inventory(
    items: &mut Vec<CompletionItem>,
    inventory: &svg_data::inventory::Inventory,
    elem_name: &str,
) {
    let allowed: std::collections::HashSet<&str> = inventory
        .attributes_for_element(elem_name)
        .map(|attribute| attribute.name)
        .collect();
    if allowed.is_empty() {
        return;
    }
    items.retain(|item| allowed.contains(item.label.as_str()));
}

/// Restrict child-element completion `items` to elements the edition `inventory`
/// declares. Mirrors [`restrict_attribute_items_to_inventory`]: an empty
/// inventory element set leaves the list untouched.
fn restrict_child_items_to_inventory(
    items: &mut Vec<CompletionItem>,
    inventory: &svg_data::inventory::Inventory,
) {
    let allowed: std::collections::HashSet<&str> = inventory
        .elements
        .iter()
        .map(|element| element.name)
        .collect();
    if allowed.is_empty() {
        return;
    }
    items.retain(|item| allowed.contains(item.label.as_str()));
}

/// Drop attribute completion `items` that SVG Native does not support on
/// `elem_name`. An attribute is removed when the profile records it as fully
/// unsupported (as an attribute or a presentation property), or when it is
/// `SupportedOnly` on a fixed set of elements that does not include `elem_name`.
fn restrict_attribute_items_to_native(
    items: &mut Vec<CompletionItem>,
    native: &svg_data::profile::SvgNative,
    elem_name: &str,
) {
    use svg_data::profile::{ConstraintKind, ConstraintScope};
    items.retain(|item| {
        let name = item.label.as_str();
        let unsupported = native.is_unsupported(ConstraintKind::Attribute, name)
            || native.is_unsupported(ConstraintKind::Property, name);
        if unsupported {
            return false;
        }
        // `SupportedOnly { Elements }` is an allowlist of bearer elements; drop
        // the item when this element is not among them. Other scope shapes
        // (values/units/image-formats) constrain the *value*, not whether the
        // attribute applies, so they do not gate completion here.
        for kind in [ConstraintKind::Attribute, ConstraintKind::Property] {
            if let Some(ConstraintScope::Elements { names }) = native.supported_only(kind, name)
                && !names.contains(&elem_name)
            {
                return false;
            }
        }
        true
    });
}

/// Drop element completion `items` for elements SVG Native does not support.
fn restrict_element_items_to_native(
    items: &mut Vec<CompletionItem>,
    native: &svg_data::profile::SvgNative,
) {
    use svg_data::profile::ConstraintKind;
    items.retain(|item| !native.is_unsupported(ConstraintKind::Element, item.label.as_str()));
}

fn completion_from_context(
    source: &[u8],
    tree: &tree_sitter::Tree,
    node: tree_sitter::Node<'_>,
    profile: svg_data::SpecSnapshotId,
    // Additive restriction for an edition target outside the four curated
    // snapshots: completion lists are filtered to names the edition's
    // spec-faithful inventory declares. `None` for snapshot/native targets keeps
    // the curated catalog's behavior unchanged.
    inventory: Option<&svg_data::inventory::Inventory>,
    // SVG Native reductive constraints, when the target is the SVG Native
    // profile: completion lists drop constructs the profile does not support.
    // `None` for snapshot/edition targets.
    native: Option<&svg_data::profile::SvgNative>,
) -> Option<CompletionResponse> {
    let mut cursor = node;
    loop {
        let kind = cursor.kind();

        if kind.ends_with("_attribute_value") || kind == "quoted_attribute_value" {
            if let Some(attr_wrapper) = attribute_wrapper_ancestor(cursor)
                && let Some(attr_name) = first_attribute_name_text(attr_wrapper, source)
            {
                let items = value_completions(&attr_name, source, tree, cursor, profile);
                if let Some(response) = completion_response(items) {
                    return Some(response);
                }
            }
            return None;
        }

        if kind == "start_tag" || kind == "self_closing_tag" {
            let elem_name = tag_element_name(cursor, source).unwrap_or("");
            let existing = existing_attribute_names(cursor, source);
            let mut items = attribute_completion_items(elem_name, &existing, profile);
            if let Some(inventory) = inventory {
                restrict_attribute_items_to_inventory(&mut items, inventory, elem_name);
            }
            if let Some(native) = native {
                restrict_attribute_items_to_native(&mut items, native, elem_name);
            }
            return completion_response(items);
        }

        if kind == "element" || kind == "svg_root_element" {
            let elem_name = enclosing_element_name(cursor, source).unwrap_or("");
            svg_data::element(elem_name)?;
            let mut items = child_element_completion_items(elem_name, profile);
            if let Some(inventory) = inventory {
                restrict_child_items_to_inventory(&mut items, inventory);
            }
            if let Some(native) = native {
                restrict_element_items_to_native(&mut items, native);
            }
            return completion_response(items);
        }

        if kind == "document" {
            let mut items = root_element_completion_items(profile);
            if let Some(native) = native {
                restrict_element_items_to_native(&mut items, native);
            }
            return completion_response(items);
        }

        cursor = cursor.parent()?;
    }
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
    profile: svg_data::SpecSnapshotId,
    runtime_compat: Option<&RuntimeCompat>,
    native: Option<&'static svg_data::profile::SvgNative>,
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

    let element_markdown =
        build_element_hover_markdown(node, &node_text, profile, runtime_compat, native);
    let attribute_markdown = build_attribute_hover_markdown(
        node,
        &kind,
        &node_text,
        source,
        profile,
        runtime_compat,
        native,
    );

    let definition_target = svg_references::definition_target_at(source, &doc.tree, byte_offset);
    let stylesheet_hrefs = svg_references::extract_stylesheet_hrefs(source, &doc.tree);
    let inline_stylesheets = svg_references::collect_inline_stylesheets(source, &doc.tree);

    let (class_hover, property_hover) = match &definition_target {
        Some(svg_references::DefinitionTarget::Class(target_class)) => (
            ClassHoverContext {
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
                stylesheet_hrefs,
            },
            empty_property_hover_context(),
        ),
        Some(svg_references::DefinitionTarget::CustomProperty(target_property)) => (
            empty_class_hover_context(),
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
            },
        ),
        _ => (empty_class_hover_context(), empty_property_hover_context()),
    };

    HoverContext {
        element_markdown,
        attribute_markdown,
        class_hover,
        property_hover,
    }
}

fn build_element_hover_markdown(
    node: tree_sitter::Node<'_>,
    node_text: &str,
    profile: svg_data::SpecSnapshotId,
    runtime_compat: Option<&RuntimeCompat>,
    native: Option<&'static svg_data::profile::SvgNative>,
) -> Option<String> {
    if node.kind() != "name" {
        return None;
    }

    node.parent()
        .filter(|parent| matches!(parent.kind(), "start_tag" | "self_closing_tag" | "end_tag"))
        .and_then(|_| {
            let lookup = svg_data::element_for_profile(profile, node_text);
            let profile_lifecycle = profile_lifecycle_hover_line(profile, &lookup);
            let runtime_override =
                runtime_compat.and_then(|runtime| runtime.elements.get(node_text));

            match lookup {
                svg_data::ProfileLookup::Present { value, .. } => {
                    Some(format_element_hover_with_profile(
                        value,
                        profile,
                        profile_lifecycle,
                        runtime_override,
                        native,
                    ))
                }
                svg_data::ProfileLookup::UnsupportedInProfile { .. } => {
                    svg_data::element(node_text).map(|element| {
                        format_element_hover_with_profile(
                            element,
                            profile,
                            profile_lifecycle,
                            runtime_override,
                            native,
                        )
                    })
                }
                svg_data::ProfileLookup::Unknown => None,
            }
        })
}

fn build_attribute_hover_markdown(
    node: tree_sitter::Node<'_>,
    kind: &str,
    node_text: &str,
    source: &[u8],
    profile: svg_data::SpecSnapshotId,
    runtime_compat: Option<&RuntimeCompat>,
    native: Option<&'static svg_data::profile::SvgNative>,
) -> Option<String> {
    if !is_attribute_name_kind(kind) {
        return None;
    }

    let lookup = svg_data::attribute_for_profile(profile, node_text);
    let element_name = attribute_owner_element_name(node, source);
    let profile_lifecycle = profile_lifecycle_hover_line(profile, &lookup);
    let runtime_override = runtime_compat.and_then(|runtime| runtime.attributes.get(node_text));

    match lookup {
        svg_data::ProfileLookup::Present { value, .. } => {
            Some(format_attribute_hover_with_profile_name(
                value,
                node_text,
                element_name.as_deref(),
                profile,
                profile_lifecycle,
                runtime_override,
                native,
            ))
        }
        svg_data::ProfileLookup::UnsupportedInProfile { known_in } => {
            svg_data::attribute(node_text).map(|attribute| {
                format_unsupported_attribute_hover_with_profile_name(
                    attribute,
                    node_text,
                    element_name.as_deref(),
                    UnsupportedAttributeHoverProfile {
                        profile,
                        known_in,
                        profile_lifecycle,
                        rt: runtime_override,
                        native,
                    },
                )
            })
        }
        svg_data::ProfileLookup::Unknown => external_attribute_hover(kind, node_text),
    }
}

fn attribute_owner_element_name(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let tag = find_ancestor_any(node, &["start_tag", "self_closing_tag"])?;
    let mut cursor = tag.walk();
    tag.children(&mut cursor)
        .find(|child| child.kind() == "name")
        .and_then(|name| name.utf8_text(source).ok())
        .map(str::to_owned)
}

#[derive(Clone)]
struct SvgLanguageServer {
    client: Client,
    documents: Arc<RwLock<HashMap<Uri, Arc<DocumentState>>>>,
    parser: Arc<RwLock<tree_sitter::Parser>>,
    color_kinds: ColorKindCache,
    stylesheet_cache: StylesheetCache,
    runtime_compat: Arc<RwLock<Option<RuntimeCompat>>>,
    profile_config: Arc<RwLock<ProfileConfig>>,
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
            profile_config: Arc::new(RwLock::new(ProfileConfig::default())),
        }
    }

    async fn document_state(&self, uri: &Uri) -> Option<Arc<DocumentState>> {
        let docs = self.documents.read().await;
        docs.get(uri).cloned()
    }

    async fn current_lint_inputs_for(
        &self,
        doc: &DocumentState,
    ) -> (
        svg_lint::LintOptions,
        Option<svg_lint::LintOverrides>,
        Option<svg_lint::VerdictOverrides>,
    ) {
        let profile = self.profile_config.read().await;
        let lint_overrides;
        let verdict_overrides;
        {
            let runtime = self.runtime_compat.read().await;
            lint_overrides = runtime.as_ref().map(RuntimeCompat::to_lint_overrides);
            verdict_overrides = runtime.as_ref().map(RuntimeCompat::to_verdict_overrides);
        }
        (
            profile.lint_options_for(doc),
            lint_overrides,
            verdict_overrides,
        )
    }

    async fn effective_profile_for_doc(&self, doc: &DocumentState) -> svg_data::SpecSnapshotId {
        self.profile_config.read().await.effective_profile_for(doc)
    }

    async fn relint_open_documents(&self) {
        let profile_config = self.profile_config.read().await.clone();
        let (overrides, verdict_overrides) = {
            let runtime = self.runtime_compat.read().await;
            (
                runtime.as_ref().map(RuntimeCompat::to_lint_overrides),
                runtime.as_ref().map(RuntimeCompat::to_verdict_overrides),
            )
        };
        let snapshot: Vec<_> = {
            let docs = self.documents.read().await;
            docs.iter()
                .map(|(uri, doc)| (uri.clone(), doc.clone()))
                .collect()
        };

        for (uri, doc) in snapshot {
            let source_bytes = doc.source.as_bytes();
            let lint_options = profile_config.lint_options_for(&doc);
            let lint_diags = svg_lint::lint_tree_with_compat(
                source_bytes,
                &doc.tree,
                lint_options,
                overrides.as_ref(),
                verdict_overrides.as_ref(),
            );
            // A document can change after this check and before publish. That
            // brief stale-diagnostics window is acceptable here; holding the
            // read lock through publish would be worse than eventual consistency.
            let is_current = {
                let docs = self.documents.read().await;
                docs.get(&uri).is_some_and(|current| {
                    current.version == doc.version && current.source == doc.source
                })
            };
            if is_current {
                publish_lint_diagnostics(
                    &self.client,
                    uri,
                    source_bytes,
                    lint_diags,
                    Some(doc.version),
                )
                .await;
            }
        }
    }

    async fn apply_profile_config(&self, config: &Value) {
        let (resolved, warning) = resolve_profile_config(config);
        tracing::debug!(
            target = describe_target(&resolved.target),
            resolved_profile = resolved.resolved().as_str(),
            constrained = resolved.is_constrained(),
            "applied SVG profile config"
        );
        *self.profile_config.write().await = resolved;

        if let Some(message) = warning {
            self.client.log_message(MessageType::WARNING, message).await;
        }
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

        let state = Arc::new(DocumentState {
            version,
            source,
            tree,
        });
        let source_bytes = state.source.as_bytes();
        let (lint_options, overrides, verdict_overrides) =
            self.current_lint_inputs_for(&state).await;
        let lint_diags = svg_lint::lint_tree_with_compat(
            source_bytes,
            &state.tree,
            lint_options,
            overrides.as_ref(),
            verdict_overrides.as_ref(),
        );
        self.documents
            .write()
            .await
            .insert(uri.clone(), state.clone());
        self.color_kinds
            .write()
            .await
            .retain(|key, _| key.uri != uri);

        publish_lint_diagnostics(&self.client, uri, source_bytes, lint_diags, Some(version)).await;
    }

    async fn copy_svg_as_data_uri(&self, uri: &Uri) -> std::result::Result<(), String> {
        let cached = {
            let docs = self.documents.read().await;
            docs.get(uri).map(|doc| doc.source.clone())
        };
        let source = if let Some(s) = cached {
            s
        } else {
            let url = Url::parse(uri.as_str())
                .map_err(|err| format!("Invalid URI {}: {err}", uri.as_str()))?;
            let path = url
                .to_file_path()
                .map_err(|()| format!("Cannot resolve file path for {}", uri.as_str()))?;
            tokio::fs::read_to_string(&path)
                .await
                .map_err(|err| format!("Failed to read {}: {err}", path.display()))?
        };

        let data_uri = svg_data_uri(&source);
        tokio::task::spawn_blocking(move || copy_text_to_system_clipboard(&data_uri))
            .await
            .map_err(|err| format!("Clipboard task failed: {err}"))?
    }
}

impl LanguageServer for SvgLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("initialize");

        let spec_freshness_enabled = params
            .initialization_options
            .as_ref()
            .is_some_and(configured_spec_freshness);

        // Runtime compat refresh is opt-out (default on). Absent options keep the
        // default so a client that sends no config still gets fresh BCD.
        let runtime_compat_enabled = params
            .initialization_options
            .as_ref()
            .is_none_or(configured_runtime_compat);

        if let Some(config) = params.initialization_options.as_ref() {
            self.apply_profile_config(config).await;
        }

        // Opt-in background spec-freshness probe: warn once if the baked spec
        // catalog has drifted from live W3C / svgwg. Best-effort and silent when
        // offline or up to date (see `freshness`).
        if spec_freshness_enabled {
            let server = self.clone();
            tokio::spawn(async move {
                match tokio::task::spawn_blocking(freshness::fetch_spec_freshness).await {
                    Ok(Some(report)) if report.is_stale() => {
                        server
                            .client
                            .show_message(MessageType::WARNING, report.message())
                            .await;
                    }
                    Ok(_) => tracing::info!("spec freshness: up to date or undetermined"),
                    Err(error) => tracing::warn!(%error, "spec freshness task panicked"),
                }
            });
        }

        // Spawn background compat data refresh, unless the client opted out
        // (`svg.runtime_compat: false`) to keep the session offline/private.
        if runtime_compat_enabled {
            let compat = self.runtime_compat.clone();
            let server = self.clone();
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
                        server.relint_open_documents().await;
                    }
                    Ok(None) => {
                        tracing::info!("runtime compat fetch returned no data (offline?)");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "runtime compat fetch failed");
                    }
                }
            });
        } else {
            tracing::info!("runtime compat refresh disabled via svg.runtime_compat=false");
        }

        Ok(InitializeResult {
            capabilities: server_capabilities(),
            ..Default::default()
        })
    }

    #[allow(
        clippy::unused_async,
        reason = "async required by the LanguageServer trait; this impl has nothing to await"
    )]
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

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        tracing::debug!("did_change_configuration");
        self.apply_profile_config(&params.settings).await;
        self.relint_open_documents().await;
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
            let profile = self.effective_profile_for_doc(&doc).await;
            // `native_constraints()` returns a `'static` reference, so it stays
            // valid after the config guard is released.
            let native = self.profile_config.read().await.native_constraints();
            let runtime_compat = self.runtime_compat.read().await;
            build_hover_context(uri, pos, &doc, profile, runtime_compat.as_ref(), native)
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

        let response = {
            let profile_config = self.profile_config.read().await;
            completion_from_context(
                source,
                &doc.tree,
                node,
                profile_config.effective_profile_for(&doc),
                profile_config.active_edition_inventory(&doc),
                profile_config.native_constraints(),
            )
        };
        Ok(response)
    }
}

/// Run the SVG language server over stdio using the LSP transport.
///
/// # Examples
///
/// ```rust,no_run
/// #[tokio::main]
/// async fn main() {
///     svg_language_server::run_stdio_server().await;
/// }
/// ```
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

    fn labeled(label: &str) -> CompletionItem {
        CompletionItem {
            label: label.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn native_constraints_only_for_svg_native_target() {
        let native_cfg = ProfileConfig {
            target: ConfiguredTarget::SvgNative,
            force: false,
        };
        assert!(native_cfg.native_constraints().is_some());
        assert!(ProfileConfig::default().native_constraints().is_none());
    }

    #[test]
    fn svg_native_restricts_completion_to_supported_constructs() {
        let native = svg_data::profile::svg_native();

        // `clipPath` is recorded unsupported by SVG Native; `g` is not.
        let mut elements = vec![labeled("g"), labeled("clipPath")];
        restrict_element_items_to_native(&mut elements, native);
        let labels: Vec<&str> = elements.iter().map(|item| item.label.as_str()).collect();
        assert!(labels.contains(&"g"), "supported element kept: {labels:?}");
        assert!(
            !labels.contains(&"clipPath"),
            "unsupported element dropped: {labels:?}"
        );

        // `clip-path` is an unsupported attribute; `fill` is not.
        let mut attributes = vec![labeled("fill"), labeled("clip-path")];
        restrict_attribute_items_to_native(&mut attributes, native, "rect");
        let labels: Vec<&str> = attributes.iter().map(|item| item.label.as_str()).collect();
        assert!(
            labels.contains(&"fill"),
            "supported attribute kept: {labels:?}"
        );
        assert!(
            !labels.contains(&"clip-path"),
            "unsupported attribute dropped: {labels:?}"
        );
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

    #[test]
    fn config_without_target_defaults_to_snapshot() {
        let (config, warning) = resolve_profile_config(&serde_json::json!({}));
        assert!(matches!(config.target, ConfiguredTarget::Snapshot(_)));
        assert!(!config.is_constrained());
        assert!(config.edition_inventory().is_none());
        assert!(warning.is_none());
    }

    #[test]
    fn config_svg_native_profile_is_constrained_target() {
        let (config, warning) =
            resolve_profile_config(&serde_json::json!({ "svg": { "profile": "svg-native" } }));
        assert!(matches!(config.target, ConfiguredTarget::SvgNative));
        assert!(config.is_constrained());
        // SVG Native bases on the SVG 2 editor's draft.
        assert_eq!(
            config.resolved(),
            svg_data::SpecSnapshotId::Svg2EditorsDraft
        );
        // No edition inventory restriction for a profile target.
        assert!(config.edition_inventory().is_none());
        assert!(warning.is_none());
    }

    #[test]
    fn config_dated_edition_resolves_when_inventory_baked() {
        // The 2016-09-15 SVG 2 CR has a baked inventory but is *not* a curated
        // snapshot, so it must surface an edition inventory restriction.
        let (config, warning) = resolve_profile_config(&serde_json::json!({
            "svg": { "edition": { "series": "svg2", "date": "2016-09-15" } }
        }));
        assert!(matches!(config.target, ConfiguredTarget::Edition(_)));
        assert!(config.edition_inventory().is_some());
        // Any SVG 2 edition bases on the editor's draft for the snapshot pipeline.
        assert_eq!(
            config.resolved(),
            svg_data::SpecSnapshotId::Svg2EditorsDraft
        );
        assert!(warning.is_none());
    }

    #[test]
    fn config_editors_draft_edition_has_no_inventory_restriction() {
        // The editor's-draft edition *is* a curated snapshot, so it imposes no
        // additive inventory restriction even though it parses as an edition.
        let (config, _) = resolve_profile_config(&serde_json::json!({
            "svg": { "edition": { "series": "svg2", "editors_draft": true } }
        }));
        assert!(matches!(config.target, ConfiguredTarget::Edition(_)));
        assert!(config.edition_inventory().is_none());
    }

    #[test]
    fn config_unknown_edition_falls_back_to_profile() {
        // A series/date with no baked inventory is rejected and falls through to
        // the (absent) profile, i.e. the default snapshot.
        let (config, _) = resolve_profile_config(&serde_json::json!({
            "svg": { "edition": { "series": "svg2", "date": "1999-01-01" } }
        }));
        assert!(matches!(config.target, ConfiguredTarget::Snapshot(_)));
    }
}
