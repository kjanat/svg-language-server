# Discoveries

## tree-sitter-svg Consumer Gotchas

- `descendant_for_byte_range` returns **anonymous leaf nodes** whose `kind()` is the literal text (e.g. `"font-size"`) instead of the named grammar node kind (`"length_attribute_name"`). Must walk to `parent()` with `is_named()` check to get the typed node for attribute dispatch.

- Typed attribute names (`paint_attribute_name`, `length_attribute_name`, `transform_attribute_name`, `viewbox_attribute_name`, `id_attribute_name`) each have their own node kind. Generic/unrecognized attributes use `attribute_name`. Hover/completion handlers must check **all** attribute name kinds, not just the typed ones.

- Element tag names live inside `start_tag`/`self_closing_tag`/`end_tag` as a child `"name"` node. The element node itself is `element`, `svg_element`, `path_element`, `style_element`, or `script_element` — you need to descend into the tag to get the actual element name text.

- `style_element` and `script_element` have `raw_text` content (opaque text blob). Their inner content is **not** parsed as XML — injections handle CSS/JS separately. Don't try to walk child elements inside these.

- Path data sub-grammar nodes (`path_command`, `coordinate_pair`, `arc_argument`, etc.) are children of `path_data` inside a `path_element`'s `d` attribute value. The `d` attribute uses `path_attribute_name` node kind, not `attribute_name`.

## LSP / tower-lsp-server

- `tower-lsp-server` 0.23 uses `ls_types` (not `lsp_types`), `Uri` (not `Url`), native async (no `#[async_trait]`), `Color` fields are `f32`

- Position conversion between tree-sitter (byte offsets, 0-based rows) and LSP (UTF-16 code units, 0-based lines) requires explicit `byte_col_to_utf16()` / `utf16_to_byte_col()` helpers

## svg-data Catalog

- `<style>` and `<script>` are "never-rendered" elements not in any traditional SVG element category (Container, Shape, etc.). Must be explicitly categorized or they'll be rejected as invalid children by content-model lint rules.

- BCD (`@mdn/browser-compat-data`) stores attribute compat under both `svg.elements.{el}.{attr}` (element-specific) and `svg.global_attributes.{attr}` (presentation attributes). Must check both paths for complete coverage.

- BCD `tags` array (e.g. `["web-features:svg"]`) cross-references into `web-features` package's `features[id].status` for baseline data. The tag prefix `web-features:` must be stripped before lookup.

- Curated catalog covers ~56 attributes but SVG has hundreds. BCD-only attributes need auto-generated entries at build time or they get no hover/diagnostics.
