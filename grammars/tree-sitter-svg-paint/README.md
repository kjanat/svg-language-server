# tree-sitter-svg-paint

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)][LICENSE-MIT]

A [Tree-sitter] grammar for **SVG paint and color** values — `fill`, `stroke`,
`stop-color`, `color`, and related properties — built against [SVG2 painting]
and [CSS Color].

> [!IMPORTANT]
> NOT PUBLISHED, NO PUBLICATION TIMELINE. IN DEVELOPMENT, SUBJECT TO BREAKING
> CHANGES.

## What This Parses

Only paint/color value strings. It is **injected** into [`tree-sitter-svg`] over
the paint attribute values (see that grammar's [`queries/injections.scm`]); the
host keeps the value as one opaque payload token and this grammar resolves it
into typed color nodes:

```text
fill="url(#grad) rgb(255 0 0)"

paint_server   url(#grad) fallback rgb_color
```

Covers paint servers (`url(...)` with keyword/color fallback), named colors,
`#hex`, and the CSS color functions `rgb`/`rgba`, `hsl`/`hsla`, `hwb`, `lab`,
`lch`, `oklab`, `oklch`, `color()`, and `color-mix()`. `color_value` is a
supertype, so a single `(color_value)` capture matches every color kind while
the concrete node still appears directly in the tree.

The standalone grammar declares a `conflicts` pair on `paint_server` so a
trailing separator parses without the host's closing quote.

## Development

```sh
tree-sitter generate      # regenerate src/parser.c from grammar.js
tree-sitter test          # corpus + tag/highlight tests
bun run test              # node binding + drift-guard tests
```

`constants.js` exports the color-space/hue-interpolation token sets;
`grammar.test.ts` asserts them against the shared catalog to prevent drift.

## License

[MIT][LICENSE-MIT] or [Apache-2.0][LICENSE-APACHE] © Kaj Kowalski

[`tree-sitter-svg`]: ../tree-sitter-svg/
[`queries/injections.scm`]: ../tree-sitter-svg/queries/injections.scm
[LICENSE-APACHE]: ./LICENSE-APACHE
[LICENSE-MIT]: ./LICENSE-MIT
[SVG2 painting]: https://www.w3.org/TR/SVG2/painting.html
[CSS Color]: https://www.w3.org/TR/css-color-4/
[Tree-sitter]: https://tree-sitter.github.io/tree-sitter/
