# svg-language-server

Rust workspace for SVG tooling.

This monorepo contains the `svg-language-server` binary plus the crates that
power formatting, linting, color analysis, spec data lookup, and
definition/reference navigation.

> [!IMPORTANT]
> This project is not published to crates.io yet.
> Expect breaking changes while the workspace is still under active development.

## Workspace Contents

| Crate                 | Purpose                                                                |
| --------------------- | ---------------------------------------------------------------------- |
| `svg-language-server` | LSP binary for SVG files                                               |
| `svg-format`          | Structural SVG formatter library and CLI                               |
| `svg-lint`            | Structural SVG diagnostics                                             |
| `svg-color`           | Color extraction and color presentation helpers                        |
| `svg-data`            | Generated SVG catalog from MDN/browser compatibility data              |
| `svg-references`      | Symbol extraction for `id`, CSS class, and custom property definitions |
| `svg-tree`            | Shared tree-sitter helpers and tree utilities                          |

### Dependency Graph

```mermaid
graph BT
  svg-color --> svg-tree
  svg-references --> svg-tree
  svg-lint --> svg-tree
  svg-lint --> svg-data
  svg-language-server --> svg-color
  svg-language-server --> svg-data
  svg-language-server --> svg-format
  svg-language-server --> svg-lint
  svg-language-server --> svg-references
  svg-language-server --> svg-tree
```

## Language Server Features

- `textDocument/hover` for element and attribute docs, MDN links, and baseline status
- `textDocument/completion` for SVG element, attribute, value, and inline CSS completions
- `textDocument/publishDiagnostics` for structural validation such as unknown elements, invalid nesting, duplicate IDs, deprecated usage, and missing local references
- `textDocument/documentColor` for paint color discovery in SVG attributes and embedded stylesheets
- `textDocument/colorPresentation` for converting colors between multiple CSS/SVG formats
- `textDocument/definition` for local `id` targets plus CSS class and custom property definitions
- `textDocument/formatting` for deterministic structural SVG formatting

## Color Support

`svg-color` recognizes and presents a broad set of CSS/SVG color syntaxes,
including:

- hex
- `rgb()` and `rgba()`
- `hsl()` and `hsla()`
- `hwb()`
- `lab()` and `lch()`
- `oklab()` and `oklch()`
- named colors
- derived values from embedded CSS such as `var(...)` and `color-mix(...)`

## Getting Started

### Prerequisites

- Rust toolchain
- `just`
- `dprint`
- `bun`

### Build And Test

```sh
just build
just ci
```

### Run The Language Server

```sh
just run-lsp
```

### Install Local Binaries

```sh
just install
just install-format
```

### Install Published Binaries With npm

```sh
npm install --global svg-language-server
npm install --global svg-format
```

These are thin installer packages: they fetch only the matching GitHub Release artifact for the current OS/architecture instead of bundling every platform into the npm tarball.

If you want to install directly from GitHub instead of a local checkout:

```sh
cargo install --git https://github.com/kjanat/svg-language-server svg-language-server
```

## Repository Layout

```text
crates/
  svg-language-server/  LSP binary and request handlers
  svg-format/           formatter library and CLI
  svg-lint/             diagnostics engine
  svg-color/            color parsing, extraction, and presentation
  svg-data/             generated SVG catalog
  svg-references/       definition/reference analysis
docs/
  plans/
  specs/
samples/                manual fixtures and examples
```

## Development Commands

```sh
just check
just format
just lint
just typecheck
just test
just ci
```

## Editor Setup

### Zed

Add this to your SVG extension's `extension.toml`:

```toml
[language_servers.svg-language-server]
languages = ["SVG"]
```

## Formatter Plugin

The dprint plugin lives in a separate repository:
https://github.com/kjanat/dprint-plugin-svg

## Release Publishing

- Git tags publish both binaries to one GitHub Release.
- npm packages `svg-language-server` and `svg-format` are published from GitHub Actions.
- Run `just release-prepare <version>` to bump versions, run CI, commit, and create the local `v<version>` tag. This requires `bun` locally. Pushing that tag triggers publication.
- See `docs/releasing.md` for the bootstrap and trusted-publisher details.
