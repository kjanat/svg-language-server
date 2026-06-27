# tree-sitter-svg-path

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)][LICENSE-MIT]

A [Tree-sitter] grammar for **SVG path data** — the value of the `d` (and
`path`) attribute — built against the [SVG2 path grammar].

> [!IMPORTANT]
> NOT PUBLISHED, NO PUBLICATION TIMELINE. IN DEVELOPMENT, SUBJECT TO BREAKING
> CHANGES.

## What This Parses

This grammar parses only path-data strings. It is **injected** into
[`tree-sitter-svg`] over the `d`/`path` attribute value (see that grammar's
[`queries/injections.scm`]), so the host keeps path data as one opaque payload
token and this grammar gives it structure:

```text
d="M 10 20 L 30 40 A 5 5 0 0 1 50 60 Z"

moveto_segment         command M, path_coordinate_pair (10 20)
lineto_segment         command L, path_coordinate_pair (30 40)
elliptical_arc_segment command A, radii, rotation, arc/sweep flags, target
closepath_segment      command Z
```

It handles the hostile bits that do not belong in an XML host DFA: command
letters glued to numbers (`M10-20`), implicit repeated segments, and arc-flag
adjacency (`A5 5 0 0150 60`). An external scanner (`_number_continuation`) gives
the parser the lookahead needed to keep repeated coordinate sets attached to the
right command.

Empty and whitespace-only path data (`d=""`, `d="   "`) are valid per spec.

## Development

```sh
tree-sitter generate      # regenerate src/parser.c from grammar.js
tree-sitter test          # corpus tests (test/corpus/)
bun run test              # node binding + drift-guard tests
```

`constants.js` exports `PATH_COMMAND_LETTERS`; `grammar.test.ts` asserts it
against the shared catalog so the command set cannot drift silently.

## License

[MIT][LICENSE-MIT] or [Apache-2.0][LICENSE-APACHE] © Kaj Kowalski

[`tree-sitter-svg`]: ../tree-sitter-svg/
[`queries/injections.scm`]: ../tree-sitter-svg/queries/injections.scm
[LICENSE-APACHE]: ./LICENSE-APACHE
[LICENSE-MIT]: ./LICENSE-MIT
[SVG2 path grammar]: https://www.w3.org/TR/SVG2/paths.html#PathDataBNF
[Tree-sitter]: https://tree-sitter.github.io/tree-sitter/
