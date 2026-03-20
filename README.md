# svg-language-server

Minimal LSP server providing inline color swatches and color picker for SVG paint attributes.

## Features

- `textDocument/documentColor` — color swatches for `fill`, `stroke`, `color`, `stop-color`, `flood-color`, `lighting-color`
- `textDocument/colorPresentation` — convert between hex, rgb(), hsl(), and CSS named colors

## Supported Color Formats

| Format | Example |
|--------|---------|
| Hex | `#ff0000`, `#f00`, `#ff000080` |
| RGB/RGBA | `rgb(255, 0, 0)`, `rgba(255, 0, 0, 0.5)` |
| HSL/HSLA | `hsl(0, 100%, 50%)`, `hsla(0, 100%, 50%, 0.5)` |
| Named | `red`, `cornflowerblue` (148 CSS named colors) |

## Install

```sh
cargo install svg-language-server
```

## Editor Setup

### Zed

Add to your Zed SVG extension's `extension.toml`:

```toml
[language_servers.svg-language-server]
languages = ["SVG"]
```
