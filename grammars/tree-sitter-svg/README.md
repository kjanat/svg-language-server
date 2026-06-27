# tree-sitter-svg

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)][LICENSE]

A [Tree-sitter] grammar for **SVG** (Scalable Vector Graphics), built against
the [SVG2 specification].

> [!IMPORTANT]
> THIS GRAMMAR IS NOT PUBLISHED NOR DO I HAVE A TIMELINE FOR PUBLICATION. IT IS
> CURRENTLY IN DEVELOPMENT AND SUBJECT TO BREAKING CHANGES.

## What This Parser Does

Most XML parsers treat SVG as generic markup. This grammar parses SVG-specific
structure in three layers:

1. XML document and tag syntax
2. Typed SVG attribute-value sub-grammars
3. Embedded language regions (CSS, JavaScript, HTML)

Element names stay mostly generic. SVG-specific behavior is split between the
grammar ([`grammar.js`]), scanner ([`src/scanner.c`]), and query files
([`queries/*.scm`]).

### Element Structure

The parser uses five parsing paths for elements:

| Path            | Purpose                                    |
| --------------- | ------------------------------------------ |
| `svg` root gate | Enforces local name `svg` at document root |
| `path` tags     | Dedicated path-element parse path          |
| `script` tags   | Raw text content (no XML parsing inside)   |
| `style` tags    | Raw text content (no XML parsing inside)   |
| `generic` tags  | All other elements as XML structure        |

### Structured Path Data

The `d` attribute on `<path>` elements is parsed into its constituent parts:

```text
d="M 10 20 L 30 40 A 5 5 0 0 1 50 60 Z"

moveto_segment
  command: path_command (M)
  args:    path_coordinate_pair
             path_number (10)
             path_number (20)

lineto_segment
  command: path_command (L)
  args:    path_coordinate_pair
             path_number (30)
             path_number (40)

elliptical_arc_segment
  command:  path_command (A)
  radii:    path_coordinate x2
  rotation: path_rotation
  flags:    path_arc_flag, path_sweep_flag
  target:   path_coordinate_pair

closepath_segment
  command: path_command (Z)
```

This enables queries and tools to operate on individual path segments rather
than treating `d` as an opaque string.

### Typed Attributes

Attributes with meaningful value sub-grammars get dedicated parsing. All others
are parsed as generic `attribute_name`/`quoted_attribute_value` pairs.

| Attribute                   | Sub-grammar                                        |
| --------------------------- | -------------------------------------------------- |
| `d`                         | Full SVG path data (commands, coordinates, arcs)   |
| `viewBox`                   | Four-number box                                    |
| `preserveAspectRatio`       | Optional `defer`, alignment, optional meet/slice   |
| `transform` and variants    | Function list (matrix, translate, rotate, ...)     |
| `points`                    | Coordinate pair list                               |
| `style`                     | CSS injection point                                |
| `on*` events                | JavaScript injection point                         |
| `href`/`xlink:href`         | IRI reference or structured data URI               |
| `id`, `class`               | Identity tokens                                    |
| Paint attributes            | `url()`, keywords, `rgb()`/`hsl()` decomposed      |
| IRI attributes              | `none`, `iri_reference`, or `url(...)` server      |
| `clip`                      | `rect()` function with length arguments            |
| Length attributes           | Length, percentage, or `auto` (24 attribute names) |
| `offset`                    | Number or percentage                               |
| `opacity` and variants      | Number or percentage                               |
| Number attributes           | Pure numeric (pathLength, k1–k4, seed, ...)        |
| `dx`, `dy`                  | Space/comma-separated length lists                 |
| `stroke-dasharray`          | `none`, `inherit`, or length list                  |
| `stdDeviation`, `rotate`, … | Space/comma-separated number lists                 |
| `dur`, `repeatDur`          | Time value, `indefinite`, or `media`               |
| `repeatCount`               | Number or `indefinite`                             |
| `keyTimes`                  | Semicolon-separated numbers                        |
| `keySplines`                | Semicolon-separated control point tuples           |
| `enable-background`         | `new` with optional coords, or `accumulate`        |

### Language Injections

Embedded languages are injected via [`queries/injections.scm`]:

| Context                                      | Injected Language |
| -------------------------------------------- | ----------------- |
| `<style>` element content (including CDATA)  | CSS               |
| Typed `style="..."` attribute value          | CSS               |
| Generic `style="..."` attribute fallback     | CSS               |
| `<script>` element content (including CDATA) | JavaScript        |
| Event handler attribute values               | JavaScript        |
| `<foreignObject>` element children           | HTML              |
| `<foreignObject>` text children              | HTML              |

### Query Files

| File              | Purpose                                            |
| ----------------- | -------------------------------------------------- |
| `highlights.scm`  | Syntax highlighting captures                       |
| `injections.scm`  | Language injection rules (CSS, JS, HTML)           |
| `locals.scm`      | Local scope/reference tracking (id ↔ href/url)     |
| `tags.scm`        | Symbol navigation with `@doc` docstrings           |
| `indents.scm`     | Auto-indentation (Helix, Neovim, Zed)              |
| `textobjects.scm` | Vim-style selections (elements, attributes, paths) |
| `outline.scm`     | Symbol outline / code folding with id context      |
| `brackets.scm`    | Bracket matching and rainbow pairs                 |

<!--
## Installation

### Node.js / Bun

```sh
npm install tree-sitter tree-sitter-svg
```

Use ESM imports (`import ...`) rather than CommonJS `require(...)`.

```js
import Parser from "tree-sitter";
import Svg from "tree-sitter-svg";

const parser = new Parser();
parser.setLanguage(Svg);

const tree = parser.parse(`
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <path d="M 10 80 C 40 10, 65 10, 95 80" stroke="black" fill="none"/>
</svg>
`);

console.log(tree.rootNode.type); // source_file
```

### Rust

```toml
[dependencies]
tree-sitter = "0.26"
tree-sitter-svg = "0.1"
```

```rust
let mut parser = tree_sitter::Parser::new();
parser
    .set_language(&tree_sitter_svg::LANGUAGE.into())
    .expect("Error loading SVG parser");

let source = r#"<svg viewBox="0 0 100 100">
  <circle cx="50" cy="50" r="40" fill="red"/>
</svg>"#;

let tree = parser.parse(source, None).expect("parse failed");
assert_eq!(tree.root_node().kind(), "source_file");
```

### Python

```sh
pip install tree-sitter tree-sitter-svg
```

```python
import tree_sitter_svg
from tree_sitter import Language, Parser

parser = Parser(Language(tree_sitter_svg.language()))
tree = parser.parse(b"<svg xmlns='http://www.w3.org/2000/svg'/>")

assert tree.root_node.type == "source_file"
```

### Go

```sh
go get github.com/tree-sitter/go-tree-sitter github.com/kjanat/svg/grammars/tree-sitter-svg
```

```go
package main

import (
    "fmt"

    tree_sitter_svg "github.com/kjanat/svg/grammars/tree-sitter-svg/bindings/go"
    tree_sitter "github.com/tree-sitter/go-tree-sitter"
)

func main() {
    parser := tree_sitter.NewParser()
    defer parser.Close()

    err := parser.SetLanguage(tree_sitter.NewLanguage(tree_sitter_svg.Language()))
    if err != nil {
        panic(err)
    }

    source := []byte(`<svg><rect width="10" height="10"/></svg>`)
    tree := parser.Parse(source, nil)
    defer tree.Close()

    fmt.Println(tree.RootNode().Kind()) // source_file
}
```

### Swift

Add to `Package.swift`:

```swift
.package(url: "https://github.com/kjanat/tree-sitter-svg", from: "0.1.0")
```

### C/C++

```sh
make && sudo make install
```

```c
#include <tree_sitter/tree-sitter-svg.h>

const TSLanguage *lang = tree_sitter_svg();
```

### Zig

Add to `build.zig.zon` dependencies:

```zig
.@"tree-sitter-svg" = .{
    .url = "https://github.com/kjanat/tree-sitter-svg/archive/refs/heads/master.tar.gz",
},
```

### Java

Via Maven:

```xml
<dependency>
  <groupId>io.github.tree-sitter</groupId>
  <artifactId>jtreesitter-svg</artifactId>
  <version>0.1.0</version>
</dependency>
```

-->

## Development

### Prerequisites

- [Tree-sitter CLI]
- Node.js 22 (grammar generation, Node binding tests)
- C compiler (parser + external scanner)

### Commands

```sh
tree-sitter generate          # regenerate src/parser.c from grammar.js
tree-sitter test              # run corpus + highlight assertions
bun run test                  # Node binding tests
bun run test:regex            # regex sample harness
bun start                     # build WASM + open playground
```

### Project Structure

```text
grammar.js                    # grammar definition (source of truth)
src/
  scanner.c                   # external scanner — tag matching, raw text capture
  parser.c                    # generated (do not edit)
  node-types.json             # generated node type metadata
queries/
  highlights.scm              # syntax highlighting
  injections.scm              # CSS/JS/HTML injection
  locals.scm                  # scope tracking
  tags.scm                    # symbol navigation (with `@doc`)
  indents.scm                 # auto-indentation
  textobjects.scm             # vim-style text object selections
  outline.scm                 # symbol outline / code folding
  brackets.scm                # bracket matching
bindings/
  c/                          # C header + pkg-config
  go/                         # Go binding + test
  java/                       # Java binding + test
  node/                       # Node.js binding + types + test
  python/                     # Python binding + test
  rust/                       # Rust binding + build script
  swift/                      # Swift binding + test
  zig/                        # Zig binding + test
test/corpus/                  # tree-sitter corpus tests
test/highlight/               # highlight query assertions
test/tags/                    # tag query assertions
test/regex_samples/           # regex harness fixtures/tests
```

### Contributing

1. Edit [`grammar.js`] (and [`src/scanner.c`] for tag matching changes)
2. Run `tree-sitter generate && tree-sitter test`
3. Add or update tests in [`test/corpus/`] and [`test/highlight/`]
4. Open a pull request

## Spec Compliance

This parser targets SVG2 syntax and uses SVG 1.1 DTD tables as supporting
reference data.

Element parsing stays mostly XML-generic, with explicit typed coverage for
selected SVG value grammars (for example path data, transforms, and paint/IRI
families).

## License

[MIT][LICENSE-MIT] or [Apache-2.0][LICENSE-APACHE] © Kaj Kowalski

[`grammar.js`]: ./grammar.js
[`queries/*.scm`]: ./queries/
[`queries/injections.scm`]: ./queries/injections.scm
[`src/scanner.c`]: ./src/scanner.c
[`test/corpus/`]: ./test/corpus/
[`test/highlight/`]: ./test/highlight/
[LICENSE]: ./LICENSE-MIT
[LICENSE-APACHE]: ./LICENSE-APACHE
[LICENSE-MIT]: ./LICENSE-MIT
[SVG2 specification]: https://www.w3.org/TR/SVG2/
[Tree-sitter CLI]: https://tree-sitter.github.io/tree-sitter/creating-parsers/tool-overview.html
[Tree-sitter]: https://tree-sitter.github.io/tree-sitter/
