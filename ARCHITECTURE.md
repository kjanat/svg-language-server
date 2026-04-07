# Architecture

This document describes the data flow and design decisions in the
svg-language-server workspace.

## Crate dependency graph

```
svg-tree            (tree-sitter traversal primitives)
   |
   +-- svg-references  (id, class, custom property extraction)
   +-- svg-color       (color parsing, extraction, presentation)
   +-- svg-lint        (structural diagnostics + suppression)
   |      |
   |      +-- svg-data (generated SVG catalog + BCD compat)
   |
   +-- svg-format      (deterministic structural formatter)
   |
   +-- svg-language-server  (LSP orchestration binary)
          depends on all of the above
```

`svg-tree` is the only crate with zero domain dependencies. Domain crates
stay free of LSP transport types — they return plain Rust structs (`Vec<ColorInfo>`,
`Vec<SvgDiagnostic>`, `String`). The LSP crate converts these to LSP protocol
types (`Diagnostic`, `ColorInformation`, `TextEdit`).

## Request lifecycle

### Document open / change

```
did_open / did_change
  -> update_document(uri, source)
       -> parser.parse(source) -> Tree
       -> read runtime_compat -> to_lint_overrides()
       -> svg_lint::lint_tree(source, tree, overrides)
       -> publish_lint_diagnostics(client, uri, diags)
       -> store DocumentState { source, tree }
```

### Hover

```
textDocument/hover
  -> document_state(uri) -> DocumentState
  -> deepest_node_at(tree, offset) -> Node
  -> build_hover_context(uri, pos, doc, runtime_compat)
       -> match node kind:
            element name   -> svg_data::element() + format_element_hover()
            attribute name -> svg_data::attribute() + format_attribute_hover()
            class          -> collect class definitions from inline + external stylesheets
            custom property -> collect custom property definitions
  -> markdown_hover(content)
```

### Completion

```
textDocument/completion
  -> document_state(uri) -> DocumentState
  -> read runtime_compat (clone + drop lock)
  -> detect context by walking ancestors:
       attribute value -> value_completions(attr_name)
       start_tag       -> attribute_completion_items(elem, existing, compat)
       element         -> child_element_completion_items(elem, compat)
       document root   -> root_element_completion_items()
```

### Definition

```
textDocument/definition
  -> document_state(uri) -> DocumentState
  -> svg_references::definition_target_at(source, tree, offset)
       -> DefinitionTarget::Id | Class | CustomProperty
  -> resolve inline definitions (same document)
  -> resolve external stylesheet definitions (spawn_blocking)
  -> collect locations
```

### Document color

```
textDocument/documentColor
  -> document_state(uri) -> DocumentState
  -> svg_color::extract_colors_from_tree(source, tree)
       -> walks SVG tree for paint attributes
       -> parses embedded <style> blocks with tree-sitter-css
       -> resolves var() + color-mix() via custom property map
  -> cache ColorPositionKey -> ColorKind for color_presentation
```

### Formatting

```
textDocument/formatting
  -> document_state(uri) -> DocumentState
  -> svg_format::format_with_options(source, options)
       -> tree-sitter parse + structural rebuild
       -> canonical attribute ordering
       -> tag layout (inline vs wrapped)
       -> embedded content preserved via host formatter callback
```

## Data sources

### Compile-time catalog (`svg-data`)

The build script (`build.rs` + `build/bcd.rs`, `build/codegen.rs`, `build/spec.rs`)
fetches data from three sources:

1. **Curated JSON** — hand-maintained element/attribute definitions in the crate
2. **BCD (Browser Compat Data)** — fetched from unpkg.com at build time, merged
   into the catalog for deprecation/experimental/baseline/browser-support fields
3. **W3C svgwg spec HTML** — scraped for element descriptions

The build script generates `catalog.rs` (included via `include!()`) containing
static arrays of `ElementDef` and `AttributeDef`.

Set `SVG_DATA_OFFLINE=1` to skip network fetches and use cached data.

### Runtime compat overlay (`RuntimeCompat`)

At LSP startup, `fetch_runtime_compat()` runs on `spawn_blocking` and fetches
fresh BCD + web-features data. This produces a `RuntimeCompat` with per-element
and per-attribute `CompatOverride` maps (deprecated, experimental, baseline,
browser support).

The runtime data enriches three features:

- **Hover** — shows live baseline status and browser versions
- **Diagnostics** — overrides deprecated/experimental flags via `LintOverrides`
- **Completions** — filters deprecated elements/attributes using runtime flags

When compat data arrives, all open documents are re-linted and diagnostics
republished.

## Key design decisions

### Why tree-sitter?

Tree-sitter provides incremental parsing, error recovery, and a grammar-driven
AST. This means the LSP works on incomplete/malformed SVG (common during editing)
without custom error recovery logic. Both the SVG grammar (`tree-sitter-svg`)
and CSS grammar (`tree-sitter-css`) are used.

### Why separate crates?

Each crate compiles independently and has a focused public API. This enables:

- Incremental compilation (changing lint rules doesn't rebuild the formatter)
- Independent consumption (`svg-format` works as a standalone CLI)
- Clear ownership boundaries (types don't leak across domains)

### Why baked catalog + runtime overlay?

The catalog is static spec data that changes infrequently. Baking it at compile
time gives O(1) lookups via `LazyLock<HashMap>` with zero runtime parsing cost.
The runtime overlay adds live browser compat data that changes more frequently,
without requiring a new release for every BCD update.

### Why `#[expect]` on float casts?

Five `#[expect(clippy::cast_possible_truncation)]` annotations exist on
`f64 as f32`, `f32 as u8`, and `f32 as u16` casts in `svg-color`. Rust's
standard library has no `TryFrom<f32> for u8` or `From<f64> for f32`. The
values are pre-clamped to safe ranges, making truncation impossible. Each
annotation carries a `reason` string explaining why no alternative exists.
