# svg-language-server

LSP server for SVG files — hover docs, completions, diagnostics, and color swatches.

## Features

- **Hover** — element and attribute documentation with MDN links and baseline status
- **Completions** — context-aware suggestions for elements, attributes, and values
- **Diagnostics** — structural validation (invalid nesting, unknown elements, duplicate IDs)
- **Colors** — color swatches and conversions between hex, `rgb()`, `hsl()`, and named colors

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

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg-language-server
