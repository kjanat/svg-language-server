# svg-references

Symbol extraction for SVG ids, CSS classes, and custom properties — resolves reference targets to their definition locations.

## Features

- **Definition targets** — resolve `url(#id)`, `href="#id"`, class names in `class="..."`, and `var(--prop)` to their target kind
- **Id definitions** — collect all `id="..."` attribute values with source spans
- **CSS class definitions** — extract `.class` selectors from inline `<style>` blocks
- **Custom property definitions** — extract `--custom-property` declarations from inline styles
- **Inline stylesheets** — parse `<style>` element content with absolute source positions
- **XML stylesheet hrefs** — collect `<?xml-stylesheet?>` processing instruction hrefs

## API

```rust
use svg_references::{definition_target_at, collect_id_definitions, collect_inline_stylesheets};

// What symbol is the cursor on?
let target = definition_target_at(source, &tree, byte_offset);

// Collect all id definitions
let ids = collect_id_definitions(source, &tree);

// Extract inline <style> blocks
let stylesheets = collect_inline_stylesheets(source, &tree);
```

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg-language-server
