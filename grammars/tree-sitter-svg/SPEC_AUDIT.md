# SVG Spec Audit Notes

Date: 2026-03-20

This file records the concrete spec excerpts used for this parser pass and the
implemented grammar/test changes tied to those excerpts.

## Sources Consulted

- SVG 2 Structure: https://www.w3.org/TR/SVG2/struct.html
- SVG 2 Paths: https://www.w3.org/TR/SVG2/paths.html
- SVG 2 Types: https://www.w3.org/TR/SVG/types.html
- SVG 1.1 DTD appendix: https://www.w3.org/TR/SVG11/svgdtd.html

## Evidence -> Implementation Mapping

### 1) Root/fragment structure must be under `svg`

- Evidence:
  [SVG 2 §5.1.1](https://www.w3.org/TR/SVG2/struct.html#NewDocumentOverview) "An
  SVG document fragment consists of any number of SVG elements contained within
  an `svg` element."
- Evidence:
  [SVG 2 §5.1.3 (SVG document fragment)](https://www.w3.org/TR/SVG2/struct.html#TermSVGDocumentFragment)
  "A document sub-tree which starts with an `svg` element which is either the
  root element of the document or whose parent element is not in the SVG
  namespace."
- Implementation:
  - `source_file` now requires `svg_root_element`.
  - Scanner adds dedicated root-name externals to enforce local-name `svg` for
    root.
  - Non-`svg` root now produces parse error corpus case.

### 2) Path data grammar and separators

- Evidence: [SVG 2 §9.3.9](https://www.w3.org/TR/SVG2/paths.html#PathDataBNF)
  "SVG path data matches the following EBNF grammar." Includes
  `flag::=("0"|"1")` and `comma_wsp::=(wsp+ ","? wsp*) | ("," wsp*)`.
- Evidence: [SVG 2 §9.3.9](https://www.w3.org/TR/SVG2/paths.html#PathDataBNF)
  "Note that the EBNF allows the path data string in the d property to be empty.
  This is not an error, instead it disables rendering of the path."
- Implementation:
  - Grammar contains typed nodes for all major command families
    (`M/L/H/V/C/S/Q/T/A/Z`).
  - Arc args enforce binary flags and structured argument shape.
  - `d=""` and `d="   "` accepted and covered in corpus.
  - Added path stress corpus for all commands, implicit separators,
    exponent/sign forms.

### 3) Types chapter: CSS/EBNF parsing and invalid handling

- Evidence: [SVG 2 §4.2](https://www.w3.org/TR/SVG/types.html#syntax) "There are
  six methods for describing an attribute's syntax:"
- Evidence: [SVG 2 §4.2](https://www.w3.org/TR/SVG/types.html#syntax) "When a
  presentation attribute defined using the CSS Value Definition Syntax is
  parsed, this is done as follows:"
- Evidence: [SVG 2 §4.2](https://www.w3.org/TR/SVG/types.html#syntax) "When an
  attribute fails to parse ... the attribute is assumed to have been specified
  as the given initial value."
- Implementation:
  - Added typed attribute grammars for high-value SVG attrs: `viewBox`,
    `preserveAspectRatio`, `transform`, `points`, paint/opacity, length/number
    families, rendering enums, id/class/lang/xml attrs, href/IRI.
  - Added invalid corpus cases for malformed typed values.

### 4) DTD defines concrete element/attribute collections

- Evidence:
  [SVG 1.1 Appendix A.1](https://www.w3.org/TR/SVG11/svgdtd.html#Introduction)
  "This appendix is normative." and "This appendix defines a DTD for SVG 1.1,
  which is used as part of determining whether a given document or document
  fragment is conforming."
- Evidence:
  [SVG 1.1 Appendix A.2.1](https://www.w3.org/TR/SVG11/svgdtd.html#ElementAndAttributeCollections)
  "Most modules define a named collection of elements or attributes."
- Implementation:
  - Root narrowed from generic XML to `svg` root entrypoint.
  - Attribute model moved from mostly generic to mostly typed for common SVG
    domains.

### 5) Embedded language surfaces (`style`, `script`, `foreignObject`, handlers)

- Evidence: [SVG 2 §5.1.4](https://www.w3.org/TR/SVG2/struct.html#SVGElement)
  includes
  [`foreignObject`](https://www.w3.org/TR/SVG2/embedded.html#ForeignObjectElement),
  [`script`](https://www.w3.org/TR/SVG2/interact.html#ScriptElement), and
  [`style`](https://www.w3.org/TR/SVG2/styling.html#StyleElement) in `svg`
  content model.
- Evidence: [SVG 2 §5.1.4](https://www.w3.org/TR/SVG2/struct.html#SVGElement)
  "The `svg` element exposes as event handler content attributes ... a number of
  the event handlers of the Window object."
- Evidence: [SVG 2 §5.1.3](https://www.w3.org/TR/SVG2/struct.html#Definitions)
  marks `foreignObject` and `script` as structurally external elements when
  `href` is present.
- Implementation:
  - Existing CSS/JS/HTML injections retained for `<style>`, `<script>`,
    `<foreignObject>`.
  - Added JS injections for typed event-handler attributes (`on*=` values).
  - Updated tags/locals queries for typed id/href/IRI nodes.

## Verification Commands

- `tree-sitter generate`
- `tree-sitter test`
- `node --test bindings/node/*_test.js`

## Coverage Snapshot

- SVG2 attribute-index comparison source:
  https://www.w3.org/TR/SVG2/attindex.html
- Current local coverage report: `UNTYPED_SVG2_ATTRIBUTES.md`
- Result in this pass: `Total untyped attributes: 0` (excluding dynamically
  typed `on*` event attributes)
