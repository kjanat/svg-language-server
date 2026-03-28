# SVG Intelligence — Design Spec

> Completions, hover docs, and diagnostics for SVG files via LSP.

## Goal

Turn `svg-language-server` from a color-only LSP into a full SVG language
intelligence server: context-aware completions, element/attribute hover
documentation with MDN links and baseline status, and structural diagnostics.

## Reference UX

Match `vscode-html-language-server`'s hover format:

```
The <text> element defines a graphics element consisting of text.

◇◇ Widely available across major browsers (Baseline since 2015)

MDN Reference
```

## Architecture

Two new crates in the workspace, alongside existing `svg-color`:

```
svg-language-server/crates/
├── svg-color/           # existing — color extraction
├── svg-data/            # NEW — SVG element/attribute catalog
├── svg-lint/            # NEW — structural diagnostics engine
└── svg-language-server/ # LSP binary — protocol glue
```

Rationale: separate crates for incremental build times. `svg-data` rarely
changes (generated catalog). `svg-lint` changes when diagnostic rules evolve.
The LSP crate wires completions, hover, and diagnostics to the protocol.

### Shared Tree Lifecycle

The LSP crate parses each document **once** on `did_open` / `did_change` and
stores the `tree_sitter::Tree` alongside the source text. All consumers
receive a reference to the shared tree:

```rust
struct DocumentState {
    source: String,
    tree: tree_sitter::Tree,
}

// In SvgLanguageServer:
documents: Arc<RwLock<HashMap<Uri, DocumentState>>>,
```

This eliminates duplicate parsing across `svg-color::extract_colors_from_tree`,
`svg-lint::lint_tree`, completion context detection, and hover lookups. The
`Parser` instance is owned by the server and reused across documents.

## Crate: `svg-data`

### Data Model

```rust
pub struct ElementDef {
    pub name: &'static str,
    pub description: &'static str,
    pub mdn_url: &'static str,
    pub deprecated: bool,
    pub baseline: Option<BaselineStatus>,
    pub content_model: ContentModel,
    pub required_attrs: &'static [&'static str],
    pub attrs: &'static [&'static str], // element-specific attr names
    pub global_attrs: bool,             // accepts presentation attrs
}

/// Whether an element is a container, void, or text-content element.
pub enum ContentModel {
    /// Can contain child elements from these categories.
    Children(&'static [ElementCategory]),
    /// Self-closing / void element (e.g. <rect/>, <circle/>).
    Void,
    /// Contains raw text content, no child elements (e.g. <title>, <desc>).
    Text,
}

pub struct AttributeDef {
    pub name: &'static str,
    pub description: &'static str,
    pub mdn_url: &'static str,
    pub deprecated: bool,
    pub baseline: Option<BaselineStatus>,
    pub values: AttributeValues,
    pub elements: &'static [&'static str],
}

pub enum AttributeValues {
    /// Enumerated keywords (e.g. stroke-linecap: butt|round|square).
    Enum(&'static [&'static str]),
    /// Free-form text (e.g. id, class).
    FreeText,
    /// Color value — delegates to svg-color for parsing.
    Color,
    /// Length or percentage (e.g. x, width, r).
    Length,
    /// URL / IRI reference (e.g. href, xlink:href).
    Url,
    /// Number or percentage (e.g. opacity, fill-opacity).
    NumberOrPercentage,
    /// Transform function list (e.g. rotate(45) translate(10,20)).
    Transform(&'static [&'static str]), // valid function names
    /// viewBox: four space-separated numbers.
    ViewBox,
    /// preserveAspectRatio: alignment + meetOrSlice.
    PreserveAspectRatio {
        alignments: &'static [&'static str],
        meet_or_slice: &'static [&'static str],
    },
    /// Coordinate pair list (e.g. points on <polygon>, <polyline>).
    Points,
    /// SVG path data (d attribute).
    PathData,
}

pub enum BaselineStatus {
    Widely { since: u16 },
    Newly { since: u16 },
    Limited,
}

pub enum ElementCategory {
    Container,
    Shape,
    Text,
    Gradient,
    Filter,
    Descriptive,
    Structural,
    Animation,
    PaintServer,
    ClipMask,
    LightSource,
    FilterPrimitive,
}
```

### Data Sources

1. **`@mdn/browser-compat-data`** — `build.rs` fetches and parses the npm
   package JSON at build time, generating Rust code with `deprecated` and
   `baseline` fields for elements and attributes.
2. **Curated catalog** — element/attribute descriptions and content models
   maintained in a TOML/JSON file in the repo. Content models derived from
   the SVG 1.1 DTD and SVG 2 spec prose.
3. **Runtime refresh** — on `initialize`, spawn a background task that
   fetches latest compat data from
   `https://unpkg.com/@mdn/browser-compat-data/data.json`. 5s timeout.
   Cache to disk for 24h. Silent fallback to baked-in on failure, log
   via `window/logMessage`.

   **Lifetime model for runtime data:** The baked-in catalog uses
   `&'static` references. Runtime-fetched compat updates are stored in a
   separate `CompatOverlay` map (`HashMap<&'static str, CompatUpdate>`)
   that only overrides `deprecated` and `baseline` fields. Lookup
   functions check the overlay first, falling back to the static data.
   The overlay is owned by the LSP server and passed to lookup functions
   by reference — no `Box::leak` needed.

### Public API

```rust
/// Lookup an element definition by name.
pub fn element(name: &str) -> Option<&'static ElementDef>;

/// Lookup an attribute definition by name.
pub fn attribute(name: &str) -> Option<&'static AttributeDef>;

/// All known SVG element definitions.
pub fn elements() -> &'static [ElementDef];

/// Map an ElementCategory to concrete element names.
pub fn elements_in_category(cat: ElementCategory) -> &'static [&'static str];

/// Concrete element names allowed as children of the given parent.
/// Resolves ElementCategory values in the parent's content model to names.
/// Returns an empty slice for void or text-content elements.
pub fn allowed_children(parent: &str) -> Vec<&'static str>;

/// All attributes valid for the given element (element-specific + globals).
pub fn attributes_for(element: &str) -> Vec<&'static AttributeDef>;
```

Note: `allowed_children` and `attributes_for` return `Vec` because they
merge multiple sources at runtime (categories → names, element-specific +
global attrs). The allocation is negligible for LSP request handling.

## Crate: `svg-lint`

Dependencies: `svg-data`, `tree-sitter`, `tree-sitter-svg`.

### Diagnostic Types

```rust
pub struct SvgDiagnostic {
    /// Byte offset range in source (for slicing source text).
    pub byte_range: std::ops::Range<usize>,
    /// Start position — row and byte-offset column (from tree-sitter Point).
    pub start_row: usize,
    pub start_col: usize,
    /// End position — row and byte-offset column.
    pub end_row: usize,
    pub end_col: usize,
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
}
```

**Position convention:** `start_col` / `end_col` are byte offsets within the
line, matching tree-sitter's `Point::column`. The LSP crate converts these
to UTF-16 code units using the existing `byte_col_to_utf16` helper.

```rust
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

pub enum DiagnosticCode {
    InvalidChild,
    MissingRequiredAttr,
    DeprecatedElement,
    DeprecatedAttribute,
    UnknownElement,
    UnknownAttribute,
    DuplicateId,
}
```

### Analysis Pipeline

```rust
/// Convenience: parse source and lint.
pub fn lint(source: &[u8]) -> Vec<SvgDiagnostic>;

/// Lint an already-parsed tree (avoids re-parsing in the LSP server).
pub fn lint_tree(source: &[u8], tree: &tree_sitter::Tree) -> Vec<SvgDiagnostic>;
```

Walk every `element` and `svg_root_element` node:

1. Extract element name from `start_tag > name` or `self_closing_tag > name`
2. Look up `ElementDef` — if missing: `UnknownElement`
3. Check `deprecated` — if true: `DeprecatedElement` (Warning)
4. Check each attribute against `attributes_for(element)` — if unknown:
   `UnknownAttribute` (Hint)
5. Check `required_attrs` — if missing: `MissingRequiredAttr` (Error)
6. For each child element, check against `allowed_children(parent)` —
   if invalid: `InvalidChild` (Error)
7. Track `id` values across document — if duplicate: `DuplicateId` (Warning)

## LSP: Completions

Implements `textDocument/completion`. All fields populated eagerly (no
`completionItem/resolve` — the catalog is small enough that full items
are cheap to build).

### Trigger Points

| Cursor position | Completes                                        | Source                               |
| --------------- | ------------------------------------------------ | ------------------------------------ |
| `<│`            | Element names (filtered by parent content model) | `svg_data::allowed_children(parent)` |
| `</│`           | Matching close tag                               | Tree-sitter parent name              |
| `<rect │`       | Attribute names for element                      | `svg_data::attributes_for("rect")`   |
| `fill="│"`      | Attribute values by type                         | `svg_data::attribute("fill").values` |

### Completion Items

- `label`: name
- `kind`: Element / Property / Value
- `detail`: short description
- `documentation`: markdown with baseline + MDN link
- `deprecated`: boolean (strikethrough rendering)
- `insert_text`: void elements get `<name />`, containers get `<name>$0</name>`

### Context Detection

Use tree-sitter tree at cursor position. Relevant node kinds:

| Node kind                                           | Cursor context     | Completion type  |
| --------------------------------------------------- | ------------------ | ---------------- |
| Between `element`/`svg_root_element` children       | Between tags       | Element names    |
| Inside `start_tag` after name and attributes        | Attribute position | Attribute names  |
| Inside `self_closing_tag` after name and attributes | Attribute position | Attribute names  |
| Inside attribute value (quoted string)              | Value position     | Attribute values |

Walk up from the node at cursor position to determine which context applies.

Trigger characters: `<`, ``, `"`, `'`. Space triggers are filtered: only
return completions when space follows a tag name or attribute value (not
between arbitrary tokens).

## LSP: Hover

Implements `textDocument/hover`.

### Targets

| Hovered node kind                                          | Content                                                      |
| ---------------------------------------------------------- | ------------------------------------------------------------ |
| `name` inside `start_tag` / `end_tag` / `self_closing_tag` | Element description + baseline + MDN link                    |
| Attribute name node                                        | Attribute description + baseline + allowed values + MDN link |
| Attribute value (if enumerated)                            | Value description                                            |

### Format

Element hover (markdown):

```
The `<text>` element defines a graphics element consisting of text.

◇◇ Widely available across major browsers (Baseline since 2015)

[MDN Reference](https://developer.mozilla.org/docs/Web/SVG/Element/text)
```

Deprecated element hover:

```
~~The `<cursor>` element defines a cursor.~~

⚠️ **Deprecated** — removed in SVG 2.

[MDN Reference](https://developer.mozilla.org/docs/Web/SVG/Element/cursor)
```

## LSP: Diagnostics

Push-based via `client.publish_diagnostics()`. Recalculated on every
`did_open` and `did_change`. Maps `SvgDiagnostic` to `ls_types::Diagnostic`
using `byte_col_to_utf16` for position conversion.

The `_client` field in `SvgLanguageServer` gets renamed to `client` and
is used for `publish_diagnostics()` and `log_message()` (compat refresh).

## LSP Capabilities

```rust
ServerCapabilities {
    text_document_sync: Full,
    color_provider: true,
    completion_provider: Some(CompletionOptions {
        trigger_characters: Some(vec!["<", " ", "\"", "'"]),
        ..Default::default()
    }),
    hover_provider: Some(true),
}
```

## Testing

| Crate                 | Tests                                                                                                                                                                                                                          |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `svg-data`            | `element("rect")` returns correct def; `allowed_children("text")` includes `tspan` but not `rect`; `attribute("fill").values` is `Color`; `attributes_for("path")` includes `d`; `elements_in_category(Shape)` includes `rect` |
| `svg-lint`            | Small SVG snippets → expected diagnostics (TDD per `DiagnosticCode`); valid SVGs produce zero diagnostics                                                                                                                      |
| `svg-language-server` | JSON-RPC integration: completion, hover, diagnostic responses                                                                                                                                                                  |

## Non-Goals

- CSS extraction from `<style>` or `style=""` (CSS LSP handles this)
- Formatting / pretty-printing
- Go-to-definition
- Rename symbol
- SVG rendering / preview

## Deferred

- Attribute value validation beyond enumerated values (e.g. path `d` syntax)
- Cross-file analysis (`<use href="other.svg#id">`)
- Quick-fix code actions (auto-fix diagnostics)
- `completionItem/resolve` (not needed while catalog is small)
