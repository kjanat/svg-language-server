# svg-language-server

LSP server for SVG files — color swatches, hover docs, completions, and diagnostics.

## Features

- `textDocument/documentColor` — color swatches for paint attributes
- `textDocument/colorPresentation` — convert between hex, rgb(), hsl(), named colors
- `textDocument/hover` — element/attribute documentation with MDN links and baseline status
- `textDocument/completion` — context-aware completions for elements, attributes, and values
- `textDocument/publishDiagnostics` — structural validation (invalid nesting, unknown elements, deprecated usage, duplicate IDs)
- `textDocument/formatting` — structural SVG formatting with tab indentation and canonical attribute ordering

## Supported Color Formats

| Format   | Example                                        |
| -------- | ---------------------------------------------- |
| Hex      | `#ff0000`, `#f00`, `#ff000080`                 |
| RGB/RGBA | `rgb(255, 0, 0)`, `rgba(255, 0, 0, 0.5)`       |
| HSL/HSLA | `hsl(0, 100%, 50%)`, `hsla(0, 100%, 50%, 0.5)` |
| Named    | `red`, `cornflowerblue` (148 CSS named colors) |

## Install

```sh
cargo install --git https://github.com/kjanat/svg-language-server svg-language-server
```

## Editor Setup

### Zed

Add to your Zed SVG extension's `extension.toml`:

```toml
[language_servers.svg-language-server]
languages = ["SVG"]
```
