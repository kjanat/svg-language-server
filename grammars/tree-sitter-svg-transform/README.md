# tree-sitter-svg-transform

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)][LICENSE-MIT]

A [Tree-sitter] grammar for **SVG transform lists** — the value of `transform`
and its variants (`gradientTransform`, `patternTransform`) — built against the
[SVG2 transform syntax].

> [!IMPORTANT]
> NOT PUBLISHED, NO PUBLICATION TIMELINE. IN DEVELOPMENT, SUBJECT TO BREAKING
> CHANGES.

## What This Parses

Only transform-list strings. It is **injected** into [`tree-sitter-svg`] over
the `transform`-family attribute values (see that grammar's
[`queries/injections.scm`]); the host keeps the value as one opaque payload
token and this grammar decomposes it:

```text
transform="translate(10 20) rotate(45 50 50) scale(2)"

transform_list
  translate_function  10 20
  rotate_function     45 50 50
  scale_function      2
```

Covers `matrix`, `translate`, `scale`, `rotate`, `skewX`, and `skewY`, with
space/comma-separated numeric arguments and trailing whitespace. The standalone
grammar declares a `conflicts` pair on `transform_list` so a trailing separator
does not require the host's closing quote to disambiguate.

## Development

```sh
tree-sitter generate      # regenerate src/parser.c from grammar.js
tree-sitter test          # corpus tests (test/corpus/)
bun run test              # node binding tests
```

## License

[MIT][LICENSE-MIT] or [Apache-2.0][LICENSE-APACHE] © Kaj Kowalski

[`tree-sitter-svg`]: ../tree-sitter-svg/
[`queries/injections.scm`]: ../tree-sitter-svg/queries/injections.scm
[LICENSE-APACHE]: ./LICENSE-APACHE
[LICENSE-MIT]: ./LICENSE-MIT
[SVG2 transform syntax]: https://www.w3.org/TR/SVG2/coords.html#TransformProperty
[Tree-sitter]: https://tree-sitter.github.io/tree-sitter/
