# svg-color

Color extraction and format conversion for SVG documents.

## Features

- Extract color values from parsed SVG trees (hex, modern and legacy `rgb()`/`hsl()`, `hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()`, named colors)
- Extract colors from SVG presentation attributes and embedded `<style>` blocks
- Resolves CSS custom properties and `color-mix(...)` inside embedded `<style>` blocks
- Convert between color formats for LSP color presentations
- Recognizes all 148 CSS named colors

## API

```rust
use svg_color::{extract_colors, extract_colors_from_tree, color_presentations};

// Extract colors from source bytes
let colors = extract_colors(source);

// Or from an already-parsed tree-sitter tree
let colors = extract_colors_from_tree(source, &tree);

// Generate format conversions for a color picker
let labels = color_presentations(r, g, b, a, kind);
```

## Supported Formats

| Format   | Example                                        |
| -------- | ---------------------------------------------- |
| Hex      | `#ff0000`, `#f00`, `#ff000080`                 |
| RGB/RGBA | `rgb(255 0 0 / 0.5)`, `rgb(255, 0, 0)`         |
| HSL/HSLA | `hsl(120 100% 50% / 0.5)`, `hsl(0, 100%, 50%)` |
| HWB      | `hwb(120 15% 10%)`                             |
| Lab      | `lab(60% -5.3654 58.956)`                      |
| LCH      | `lch(62.2345% 59.2 126.2)`                     |
| OKLab    | `oklab(62.8% 0.22488 0.125859)`                |
| OKLCH    | `oklch(62.8% 0.2577 29.23)`                    |
| Named    | `red`, `transparent`, `cornflowerblue`         |

Embedded CSS can also resolve derived colors such as `fill: var(--toolbar-bg)` and `color-mix(in oklch, var(--base), white 8%)`.

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg-language-server
