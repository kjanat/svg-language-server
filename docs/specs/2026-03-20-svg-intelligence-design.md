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

## Crate: `svg-data`

### Data Model

```rust
pub struct ElementDef {
    pub name: &'static str,
    pub description: &'static str,
    pub mdn_url: &'static str,
    pub deprecated: bool,
    pub baseline: Option<BaselineStatus>,
    pub allowed_children: &'static [ElementCategory],
    pub required_attrs: &'static [&'static str],
    pub attrs: &'static [&'static str],
    pub global_attrs: bool,
}

pub struct AttributeDef {
    pub name: &'static str,
    pub description: &'static str,
    pub mdn_url: &'static str,
    pub deprecated: bool,
    pub values: AttributeValues,
    pub elements: &'static [&'static str],
}

pub enum AttributeValues {
    Enum(&'static [&'static str]),
    FreeText,
    Color,
    Length,
    Url,
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
   package JSON at build time, generating `svg_elements.rs` and
   `svg_attributes.rs` with `deprecated` and `baseline` fields.
2. **Curated catalog** — element/attribute descriptions and content models
   maintained in a TOML/JSON file in the repo. Content models derived from
   the SVG 1.1 DTD and SVG 2 spec prose.
3. **Runtime refresh** — on `initialize`, optionally fetch latest compat
   data from `https://unpkg.com/@mdn/browser-compat-data/data.json`.
   Overlay onto baked-in data. 5s timeout, non-blocking. Cache locally
   for 24h. Silent fallback to baked-in on failure.

### Public API

```rust
pub fn element(name: &str) -> Option<&'static ElementDef>;
pub fn attribute(name: &str) -> Option<&'static AttributeDef>;
pub fn elements() -> &'static [ElementDef];
pub fn allowed_children(parent: &str) -> &'static [&'static str];
pub fn attributes_for(element: &str) -> Vec<&'static AttributeDef>;
```

## Crate: `svg-lint`

### Diagnostic Types

```rust
pub struct SvgDiagnostic {
    pub range: std::ops::Range<usize>,
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
}

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
pub fn lint(source: &[u8]) -> Vec<SvgDiagnostic>;
pub fn lint_tree(source: &[u8], tree: &tree_sitter::Tree) -> Vec<SvgDiagnostic>;
```

Walk every `element` and `svg_root_element` node:

1. Extract element name from `start_tag > name`
2. Look up `ElementDef` — if missing: `UnknownElement`
3. Check `deprecated` — if true: `DeprecatedElement` (Warning)
4. Check each attribute against `attributes_for(element)` — if unknown:
   `UnknownAttribute` (Hint)
5. Check `required_attrs` — if missing: `MissingRequiredAttr` (Error)
6. For each child element, check against `allowed_children(parent)` —
   if invalid: `InvalidChild` (Error)
7. Track `id` values across document — if duplicate: `DuplicateId` (Warning)

## LSP: Completions

Implements `textDocument/completion`.

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
- `insert_text`: elements get closing tag snippet where appropriate

### Context Detection

Use tree-sitter tree at cursor position:

1. Find node at position
2. Walk up to determine context (inside start tag, attribute value, between
   tags)
3. Select completion strategy

Trigger characters: `<`, ``, `"`, `'`.

## LSP: Hover

Implements `textDocument/hover`.

### Targets

| Node            | Content                                 |
| --------------- | --------------------------------------- |
| Element name    | Description + baseline + MDN link       |
| Attribute name  | Description + allowed values + MDN link |
| Attribute value | Value description (if enumerated)       |

### Format

Element hover (markdown):

```
The `<text>` element defines a graphics element consisting of text.

◇◇ Widely available across major browsers (Baseline since 2015)

[MDN Reference](https://developer.mozilla.org/en-US/docs/Web/SVG/Element/text)
```

Deprecated element hover:

```
~~The `<cursor>` element defines a cursor.~~

⚠️ **Deprecated** — removed in SVG 2.

[MDN Reference](https://developer.mozilla.org/en-US/docs/Web/SVG/Element/cursor)
```

## LSP: Diagnostics

Push-based via `client.publish_diagnostics()`. Recalculated on every
`did_open` and `did_change`. Maps `SvgDiagnostic` to `ls_types::Diagnostic`.

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

| Crate                 | Tests                                                                                                                      |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `svg-data`            | `element("rect")` returns correct def; `allowed_children("text")` includes `tspan`; `attribute("fill")` returns Color type |
| `svg-lint`            | Small SVG snippets → expected diagnostics (TDD per `DiagnosticCode`); valid SVGs produce zero diagnostics                  |
| `svg-language-server` | JSON-RPC integration: completion, hover, diagnostic responses                                                              |

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
