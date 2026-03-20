# SVG Color Language Server — Design Spec

## Context

Zed (and other editors) show inline color swatches next to color values via LSP
`textDocument/documentColor`. No SVG LSP exists that implements this. The
tree-sitter-svg grammar already parses paint attributes into structured nodes
(`hex_color`, `functional_color`, `named_color`), so the LSP can leverage the
parse tree instead of regex scanning.

## Goal

A minimal, focused LSP server that provides:

1. **`textDocument/documentColor`** — return color swatches for SVG paint attributes
2. **`textDocument/colorPresentation`** — convert colors between hex/rgb/hsl formats (color picker)

Nothing else. No completion, hover, diagnostics, or formatting.

## Architecture

### Crate Structure

```
crates/
  svg-color/              # Pure color extraction library
  svg-language-server/    # LSP binary (thin glue)
```

**`svg-color`** (library crate):
- Input: source text (as `&[u8]`)
- Uses `tree-sitter` + `tree-sitter-svg` to parse
- Walks CST for color-bearing nodes
- Returns `Vec<ColorInfo>` with RGBA values + byte ranges + original format
- No LSP types — pure data, independently testable
- Handles incremental re-parse via tree-sitter's edit API

**`svg-language-server`** (binary crate):
- `tower-lsp-server` for LSP scaffolding
- Document lifecycle: `didOpen`, `didChange`, `didClose`
- Translates `svg-color::ColorInfo` → LSP `ColorInformation`
- Translates LSP `ColorPresentationParams` → format conversions
- Byte offset → LSP position (UTF-16 code unit) conversion

### Color Extraction Pipeline

```
source text
  → tree-sitter parse (full or incremental)
  → tree query: (color_value) @color
  → for each match, check child node kind:
      hex_color        → parse #RGB / #RGBA / #RRGGBB / #RRGGBBAA
      functional_color → secondary parse of rgb()/rgba()/hsl()/hsla()
      named_color      → validate against 148 CSS named color table
  → Vec<ColorInfo { r, g, b, a, byte_range, kind }>
```

### Tree-sitter Query

```scheme
(color_value) @color
```

This single query captures all color nodes regardless of nesting context:
- Direct paint values: `fill="#ff0000"`
- Paint server fallbacks: `fill="url(#grad) #ff0000"`

The grammar ensures `color_value` only appears inside `paint_attribute` or
`paint_server` nodes, so no false positives from comments, CDATA, or scripts.

### Grammar Constraints

**`named_color` is a catch-all**: The grammar rule `named_color: _ =>
token(/[A-Za-z][A-Za-z-]*/)` matches any alphabetic string, not just CSS color
names. `fill="banana"` produces a `(named_color)` node. The `svg-color` crate
MUST validate against the 148 CSS named color table and silently skip
non-matching values.

**`functional_color` is opaque**: The grammar captures `rgb(255, 0, 0)` as a
single flat token with no sub-structure. The `svg-color` crate must parse the
token text with a secondary parser to extract numeric RGB/HSL components.

### Data Types

```rust
struct ColorInfo {
    r: f32,      // 0.0–1.0
    g: f32,
    b: f32,
    a: f32,
    byte_range: Range<usize>,
    row: usize,
    col: usize,  // byte offset within line (simplifies UTF-16 conversion)
    kind: ColorKind,
}

enum ColorKind {
    Hex,
    Functional,
    Named,
}
```

Tracking `kind` lets `colorPresentation` default to the user's original format.
Storing `row`/`col` from tree-sitter's `start_position()` avoids scanning from
document start for LSP position conversion.

### SVG Attributes Covered

| Attribute | Grammar Node | Colors? |
|-----------|-------------|---------|
| `fill` | `paint_attribute` | Yes |
| `stroke` | `paint_attribute` | Yes |
| `color` | `paint_attribute` | Yes (inherited, affects `currentColor`) |
| `stop-color` | `paint_attribute` | Yes |
| `flood-color` | `paint_attribute` | Yes |
| `lighting-color` | `paint_attribute` | Yes |
| `style="..."` | `style_attribute` | No (CSS LSP handles) |

### Color Presentation Formats

When the user clicks a color swatch, offer these alternatives:

- `#RRGGBB` / `#RRGGBBAA`
- `rgb(R, G, B)` / `rgba(R, G, B, A)`
- `hsl(H, S%, L%)` / `hsla(H, S%, L%, A)`
- Named color (only if exact match exists in CSS named color table)

Default to the user's original format first in the list.

## Dependencies

### `svg-color`

| Crate | Purpose |
|-------|---------|
| `tree-sitter` | Parser runtime |
| `tree-sitter-svg` | SVG grammar |

### `svg-language-server`

| Crate | Purpose |
|-------|---------|
| `svg-color` | Color extraction (workspace) |
| `tower-lsp-server` | LSP framework (community fork) |
| `tokio` | Async runtime |
| `serde_json` | LSP message serialization |

## Distribution

### Phase 1: Cargo

- `cargo install svg-language-server`
- Editors locate via PATH

### Phase 2: npm + GitHub Releases (deferred)

- Pre-built binaries per platform in GitHub releases (CI matrix)
- npm package wraps binary download via `postinstall`
- Zed extension uses `npm_install_package()` for auto-install

### Zed Integration

In `zed-svg/extension.toml`:
```toml
[language_servers.svg-language-server]
languages = ["SVG"]

[language_servers.svg-language-server.language_ids]
SVG = "svg"
```

Extension Rust code implements `language_server_command()` — Phase 1 uses
`worktree.which("svg-language-server")` PATH lookup, Phase 2 adds npm fallback.

## Document State Management

- On `didOpen`: full parse, extract colors, cache
- On `didChange`: incremental tree-sitter edit, re-query full tree for colors
  (SVGs are small; full-tree query is negligible for documents under 1MB)
- On `didClose`: drop cached state

## Testing

### `svg-color` unit tests

- Hex colors: `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`
- Functional: `rgb()`, `rgba()`, `hsl()`, `hsla()` with various spacing
- Named: subset of 148 CSS named colors (representative sample)
- Invalid named colors skipped: `fill="banana"` → no ColorInfo
- Keywords skipped: `none`, `currentColor`, `inherit`, `context-fill`, `context-stroke`
- Paint server fallbacks: `url(#id) red`, `url(#id) #ff0000`
- No false positives: colors in comments, CDATA, script, attribute names
- Empty/whitespace paint values
- Multiple paint attributes on one element

### `svg-language-server` integration tests

- Document open → color response
- Document edit → updated colors
- Color presentation → format conversion round-trips
- UTF-16 position accuracy with multi-byte characters

## Non-Goals

- CSS color extraction from `<style>` elements or `style=""` attributes (CSS LSP handles)
- SVG validation / diagnostics
- Attribute completion
- Hover documentation
- Go-to-definition
- Formatting
- `opacity` attribute values (numeric, not colors)
- Colors in generic attributes (defeats purpose of using parse tree)

## Resolved Questions

- **Opacity as colors?** No — numeric values, not colors
- **Colors in generic attributes?** No — grammar distinguishes `paint_attribute`
  from `generic_attribute`; parsing generic attrs would require regex and defeat
  the tree-sitter approach
