# svg-color

Color extraction and format conversion for SVG documents.

## Features

- Extract color values from parsed SVG trees (hex, `rgb()`, `hsl()`, named colors)
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

| Format   | Example                          |
| -------- | -------------------------------- |
| Hex      | `#ff0000`, `#f00`, `#ff000080`   |
| RGB/RGBA | `rgb(255, 0, 0)`, `rgba(…, 0.5)` |
| HSL/HSLA | `hsl(0, 100%, 50%)`              |
| Named    | `red`, `cornflowerblue`          |

## Part of [svg-language-server](https://github.com/kjanat/svg-language-server)
