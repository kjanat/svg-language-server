# CREATING THE BEST SVG TREE-SITTER IN THE WORLD

<!-- markdownlint-disable-file MD013 MD034 -->

For a Tree‑sitter grammar for SVG, treat SVG as a structured XML‑like language
and lean heavily on the official specs and existing XML‑focused Tree‑sitter
resources. [tree-sitter.github](https://tree-sitter.github.io)

## Core SVG specs

- **SVG 1.1 DTD and structure**: The W3C SVG 1.1 Appendix D defines the DTD,
  which is a great starting point for element names, attributes, and content
  models. [w3](https://www.w3.org/TR/SVG11/svgdtd.html)
- **SVG 2 specification (structure)**: The SVG 2 “Document Structure” section
  clarifies how SVG interacts with XML and namespaces, which maps cleanly to a
  Tree‑sitter grammar over XML‑style tags.
  [w3](https://www.w3.org/TR/SVG2/struct.html)
- **W3C Editor’s Draft 14 September 2025**:
  [Scalable Vector Graphics (SVG) 2](https://svgwg.org/svg2-draft/)
  - This version:\
    https://svgwg.org/svg2-draft/
  - Latest version:\
    https://www.w3.org/TR/SVG2/
  - Previous version:\
    https://www.w3.org/TR/2018/CR-SVG2-20180807/
  - Single page version:\
    https://svgwg.org/svg2-draft/single-page.html
  - GitHub repository:\
    https://github.com/w3c/svgwg/
  - Public comments:\
    www-svg@w3.org (archive)

## Tree‑sitter guidance

- **Tree‑sitter official docs**: The “Creating Parsers / Writing the Grammar”
  page explains how to model rules, avoid ambiguities, and keep the grammar
  close to an LR(1)‑style structure.
  [tree-sitter.github](https://tree-sitter.github.io/tree-sitter/creating-parsers/3-writing-the-grammar.html)
- **Examples of XML‑style grammars**: Look at `tree‑sitter‑xml` and its
  `tree‑sitter.json`; that grammar already shows how to model XML tags,
  attributes, and mixed content, which you can reuse for SVG‑specific element
  sets.
  [github](https://github.com/tree-sitter-grammars/tree-sitter-xml/blob/master/tree-sitter.json)

## Example SVG parser references

- **NanoSVG / SVG parser implementations**: Libraries like NanoSVG expose how
  SVG syntax is actually tokenized (paths, transforms, colors, lengths), which
  can inform your lexer‑style rules.
  [github](https://github.com/memononen/nanosvg)
- **Tutorials on writing Tree‑sitter grammars**: Articles that walk through
  building a Tree‑sitter grammar from scratch (e.g., toy‑language or TLA⁺) are
  useful for workflow and testing patterns, even if the language is different.
  [jacopofarina](https://jacopofarina.eu/posts/writing-a-tree-sitter-grammar/)

If you tell me whether you want to parse **plain SVG‑as‑XML** or also handle
**embedded `<style>` / CSS / path‑data grammar**, I can point you to more
targeted sources.

## Embedded languages

To handle embedded `<style>` / CSS and path‑data inside SVG, you’ll want to
combine the SVG spec with the CSS and path‑data grammars, plus Tree‑sitter’s
multi‑language support.
[github](https://github.com/tree-sitter/tree-sitter/issues/121)

### `<style>` and CSS in SVG

- **SVG 2 “Styling”**: The SVG 2 Styling chapter explains how the `<style>`
  element and `style=""` attribute work; it treats the content as a CSS
  `declaration-list`, so you can reuse a real CSS grammar (or a simplified
  subset) for those regions.
  [smashingmagazine](https://www.smashingmagazine.com/2014/11/styling-and-animating-svgs-with-css/)
- **CSS specs**: Use the CSS‑level‑3 / CSSOM specs for the exact grammar of
  selectors, declarations, and values; several Tree‑sitter CSS grammars already
  model this for you.
  [blog.pulsar-edit](https://blog.pulsar-edit.dev/posts/20230925-savetheclocktower-modern-tree-sitter-part-1/)
- **Tree‑sitter multi‑language pattern**: The `subLanguage('css')` pattern (as
  in the HTML‑style issue) lets you delegate `<style>` contents to a
  `tree‑sitter‑css`‑style grammar instead of reinventing the whole CSS parser.
  [github](https://github.com/tree-sitter/tree-sitter/issues/121)

### Path‑data grammar (`d` attribute)

- **SVG Paths spec (EBNF)**: The SVG Paths spec gives a full EBNF grammar for
  `d`‑attribute path data, including `M`, `L`, `C`, `A`, etc., with precise
  numeric and command‑sequence rules. [svgwg](https://svgwg.org/specs/paths/)
- **Path‑data error handling notes**: The same spec explains how to recover from
  malformed path strings (e.g., when tokens don’t match the grammar), which is
  useful for deciding how strict your grammar should be.
  [w3](https://www.w3.org/Graphics/SVG/WG/track/issues/2439)

### How to wire this into your SVG grammar

- Model `<style>` as an XML‑style element whose content is handled by a separate
  CSS grammar (via `subLanguage`‑like constructs or a custom `css_content`
  rule). [w3](https://www.w3.org/TR/SVG2/styling.html)
- Add a dedicated `path_data` rule that mirrors the SVG Paths EBNF, and attach
  it to the `d` attribute on `<path>` so you get a proper subtree for commands
  and coordinates. [w3](https://www.w3.org/TR/SVG2/paths.html)

If you tell me whether you want to **embed** `tree‑sitter‑css` (reuse) or
**inline** CSS rules directly in your SVG grammar, I can sketch a concrete
`grammar.js`/`grammar.json` snippet for each style.

## Injections

Right, you’ll almost certainly want more injections than just CSS in `<style>` /
`style=""`.
[developer.mozilla](https://developer.mozilla.org/en-US/docs/Web/SVG/Reference/Element/foreignObject)

## Other embedded languages worth handling

The big additional injection points in SVG are:

### `<script>` content (JavaScript or similar)

SVG has a `<script>` element equivalent to HTML’s, whose text content is
typically JavaScript (by default `application/ecmascript` / JS). For Tree‑sitter
you’d usually parse it as raw text in the SVG grammar and inject
`tree-sitter-javascript` (or `typescript`) into that content range via
`injections.scm`, same pattern as HTML `<script>`.
[mdn2.netlify](https://mdn2.netlify.app/en-us/docs/web/svg/element/script/)

### `<foreignObject>` content (usually HTML/XHTML)

`<foreignObject>` is designed to include elements from other XML namespaces; in
browsers this is almost always (X)HTML, so you can have a whole HTML subtree
inside your SVG. That’s another classic injection point: keep generic
element/character-data nodes in the SVG tree, and inject `tree-sitter-html` (or
XML/MathML/etc. if you care) inside `<foreignObject>`’s child range.
[developer.mozilla](https://developer.mozilla.org/de/docs/Web/SVG/Reference/Element/foreignObject)

### Optional: path data as its own language

Path `d` attributes follow a proper EBNF grammar for path commands and numeric
sequences, effectively a mini language. You can either inline that grammar into
your SVG language or treat it as a sub‑language and inject a dedicated
`svg-path` Tree‑sitter grammar into `d` attribute values, similar to injecting
CSS into `style`. [w3](https://www.w3.org/wiki/SVG/Animation)

So a “complete” SVG setup generally needs at least three injected languages:
**CSS** (`<style>` and `style=`), **JS** (`<script>` and potentially event
handler attributes), and **HTML/XHTML** inside `<foreignObject>`, plus an
optional dedicated grammar for **path data** if you want a rich AST there.
[github](https://github.com/tree-sitter/tree-sitter/issues/366)

You don’t get a single EBNF for “all of SVG”; the specs define EBNF/BNF **per
data type**, mainly for path data and the shared basic types.
[w3](https://www.w3.org/TR/SVG2/paths.html)

## Where EBNF actually lives

For a Tree‑sitter grammar, the EBNF you care about is mostly in these places:

### 1. Path data (`d` attribute)

- **SVG 2**: *Paths — SVG 2*, section “The grammar for path data” gives a full
  EBNF for `svg_path`, `moveto`, `lineto`, `curveto`, `elliptical_arc`,
  `coordinate`, `number`, `wsp`, etc. [svgwg](https://svgwg.org/svg2-draft/)
- **SVG 1.1**: *Paths — SVG 1.1*, section “The grammar for path data” gives a
  BNF for the same thing (slightly older but widely referenced).
  [w3](https://www.w3.org/TR/SVG11/paths.html)

This is the canonical grammar for the `d` attribute and is what people usually
transliterate directly into parser rules.
[w3](https://www.w3.org/TR/SVG2/paths.html)

### 2. Basic data types (numbers, lengths, lists, etc.)

- **SVG 1.1 / 1.1F2**: *Basic Data Types and Interfaces* defines an EBNF dialect
  used throughout SVG and gives grammars for types like `<integer>`, `<length>`,
  `<angle>`, and the `list-of-Ts` pattern (`comma_wsp`, `wsp`, etc.).
  [dev.w3](https://dev.w3.org/SVG/profiles/1.1F2/master/types.html)
- **SVG 2**: The SVG 2 *Basic Data Types and Interfaces* chapter continues this,
  and the value tables explicitly say “by [EBNF]” when a type’s syntax is
  defined by an EBNF production there.
  [w3](https://www.w3.org/TR/SVG/types.html)

These productions are what the path grammar now references for `wsp` /
`comma_wsp`, and they’re also the right source if you want to be precise about
number/length/list parsing in your grammar.
[lists.w3](https://lists.w3.org/Archives/Public/public-svg-wg/2014AprJun/0069.html)

### 3. Everything else (overall document structure)

The *document structure* (which elements/attributes exist and how they nest) is
defined by the SVG 1.1 DTD and prose, **not** by a global EBNF; SVG 2 is
similar. So for tags/attributes you rely on XML + the DTD/spec tables, and for
the “mini‑languages” inside attributes (`d`, maybe transform/points/etc.) you
lean on the EBNF/BNF in the paths and types chapters.
[w3](https://www.w3.org/TR/SVG11/svgdtd.html)

## Last words

If you ever have any troubles with any implementation, or are surprised by
something being the case, please write it down in
[DISCOVERIES.md](./DISCOVERIES.md) for furure reference.\
Also use it if you learn something that, or come across something new to you.
Whenever you then have an issue you can reference @DISCOVERIES.md for possible
insights.
