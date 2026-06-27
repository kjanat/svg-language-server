# Discoveries

## 2026-06-25: Path-data eviction to injected `svg_path` grammar — modest size, NOT a wasm-build win

Evicted the rich path sub-grammar (11 segment rules, `M/L/C…` command tokens,
`_number_continuation` external) from the host into a sibling
`grammars/tree-sitter-svg-path` grammar, injected over the `d`/`path` attribute
value via `injections.scm`. The host captures the value as one opaque
`path_data_payload` token (`/[^"'<&]+/`); both `d` and `path` are `d_attribute`
(`D_ATTRIBUTE_NAMES = ['d','path']`), so a single injection query covers both.

Measured (apples-to-apples, same toolchain):

- host `parser.c`: 1,159,533 → 1,042,475 B (**−10%**)
- LR states: 1204 → 1040 (**−13.6%**); symbols 373 → 326; tokens 149 → 138 (the
  path-command tokens that made a global `word:` token unsafe are GONE —
  eviction is the prerequisite that *would* let `word:` be added safely)
- host WASM binary: 388,237 → 372,629 B (**−4%**)
- **WASM build time 1:18 → 1:20, RSS 4.80 → 4.79 GB — UNCHANGED.** Eviction does
  NOT reduce wasm build cost; that cost lives in the color-function +
  attribute-bucket machinery, not path data. If build memory/time is the target,
  profile those, not path.

Gotchas hit:

- `tree-sitter parse <file.svg>` resolves the grammar from the global
  `~/.config/tree-sitter/config.json` `parser-directories` (which lists the main
  checkout), NOT the cwd/worktree. A worktree grammar is only exercised by
  passing `--config-path` with a config whose `parser-directories` points at the
  worktree, or by `tree-sitter test` (which compiles the local grammar). Cost me
  several confused parses against a stale parser.
- `tree-sitter generate` does NOT prune unreachable rules from
  `src/grammar.json` (the input dump keeps them), but the parse tables DO drop
  them — judge eviction by `STATE_COUNT`/`SYMBOL_COUNT`, not by grepping
  grammar.json.
- Path data has no comment syntax, so path highlights cannot be unit-tested with
  inline `<!-- ^ capture -->` assertions in a raw `svg_path` file — they were
  historically tested *through* the host's XML comments. After eviction the host
  `test/highlight/path_data.svg` was removed; injected-highlight unit coverage
  needs a separate harness (TODO).
- `:error` corpus tests that asserted host-level path validity ("L without
  preceding M", bad arc flag) no longer error in the host (opaque payload is
  always valid XML); they were relocated to the `svg_path` corpus.

## Build & Tooling

- `tree-sitter.json` should declare `highlights`, `injections`, and `locals`
  query paths in the grammar entry; omitting them triggers a CLI warning and may
  prevent editors from finding the queries
- External scanner enum order must exactly match `externals` order in
  `grammar.js`; mismatches silently corrupt tokenization
- For simple external tokens (tag names, `/>`), avoid `mark_end` unless doing
  lookahead rollback; premature `mark_end` can truncate tokens
- `tree-sitter build --wasm` can look stuck at `Extracting wasi-sdk...`, but for
  this grammar the real stall is later in wasm backend codegen for
  `src/parser.c`; syntax-only and LLVM IR emission finish quickly, object/wasm
  emission does not
- WASM build fails when parser.c exceeds ~100K lines; at 102K lines (458 rules,
  50 externals) the WASM backend cannot complete; at 21K lines (120 rules, 13
  externals) it succeeds instantly
- Zed's extension builder uses wasi-sdk clang-19 to compile grammar WASM;
  102K-line parser.c hung indefinitely (23GB RSS); 21K-line parser.c compiles in
  ~17s
- Scanner stores tag names as `Array(char)`, truncating `int32_t` lookahead to 8
  bits; safe for SVG (ASCII-only names), matches tree-sitter-xml/html; widening
  to `Array(int32_t)` would require serialization format change
- Serialization silently truncates tag stack when 1024-byte buffer exceeded;
  `written` count is patched to reflect actual serialized tags
- `tree-sitter build --reuse-allocator` fails for any grammar whose scanner uses
  `tree_sitter/array.h` — the CLI passes `-DTREE_SITTER_REUSE_ALLOCATOR`
  (mapping `ts_malloc` → `ts_current_malloc`) but doesn't link the runtime that
  defines those symbols; confirmed broken on tree-sitter-rust too (not our bug)
- Scanner should use `ts_calloc`/`ts_free` (from `alloc.h` via `array.h`)
  instead of raw `calloc`/`free` so allocator routing works when the CLI
  eventually fixes `--reuse-allocator`
- CI `run:` using a YAML folded scalar (`>-`) must keep continuation lines at
  the *same* indentation as the first line. Lines indented deeper are treated as
  literal blocks and keep their newlines, so
  `runner install --frozen --keep-going` + deeper-indented task names became two
  shell commands — `runner install` (ran) then bare `typecheck` (exit 127,
  command not found). Fix: dedent task lines to align with `runner`. The
  intended single command is
  `runner install --frozen --keep-going typecheck test:corpus …`
  (install-then-chain; `runner install [TASKS]...` is a real feature, and
  `--keep-going` still returns the first non-zero chain exit so CI stays honest)

## Grammar

- `prec.left` on path segment rules causes the parser to exit after the first
  argument instead of continuing the repeat; subsequent values become
  `implicit_lineto_segment` nodes. Impact splits three ways: (a) moveto/lineto =
  spec-correct (spec itself says trailing pairs after M/L are implicit linetos),
  (b) curveto family C/S/Q/T = semantically wrong (spec says trailing sets after
  C/S/Q/T are implicit cubic/quadratic commands, not linetos), (c) arc/H/V =
  parse ERROR on odd-count tails because implicit_lineto_segment requires a
  coordinate *pair*. Fix: external scanner `_number_continuation` peeks past
  wsp/comma/wsp and commits only if a number-starting char follows — gives the
  GLR parser LR(k) lookahead. Applied to arc, H, V, C, S, Q, T repeats. M/L left
  alone (declared repeat is dead code but spec-equivalent in behavior).
- Keeping `extras` empty preserves XML whitespace as explicit `text` nodes
  (including indentation/newlines)
- Generic XML attributes should require quoted values; allowing
  valueless/unquoted attrs accepts non-XML SVG
- Tag-name matching needs an external scanner stack; CFG-only grammar cannot
  enforce `<a>...</a>` equality
- Using hidden pre/post-root rules keeps document-structure helpers out of the
  visible CST
- For context-specific attributes (e.g. `type` on `<script>`/`<style>`), avoid
  including `generic_attribute` in the same element-specific attribute list or
  strict typing is bypassed
- To enforce content models for selected elements, add name-specialized
  externals + hidden element subrules (e.g. `_path_element`) instead of broad
  generic children
- Category overlap in scanner name predicates is precedence-sensitive (e.g. if
  an element name belongs to two families, first matching branch wins)
- Tight `defs` content models surface omitted common definition elements quickly
  (e.g. `clipPath`); either add explicit families or expect recovery nodes
- Using `$.attribute` (typed + generic) on specialized container tags restores
  extension/custom attribute support without weakening the global XML quoting
  constraint
- Filter primitive conformance needs family-scoped tag tokens and rules
  (`feColorMatrix`, `feTurbulence`, `feComponentTransfer` + `feFunc*`,
  `feMerge` + `feMergeNode`, lighting + light-source) rather than one shared
  primitive bucket
- `text` content should include linking/media (`<a>`) to accept common inline
  link patterns (`<text><a><tspan>…`)
- Zed's `jsx_tag_auto_close` matches exact grammar node names for open/close
  tags; if the SVG root uses distinct tag node kinds from nested elements, root
  auto-close fails even when the visible CST looks equivalent

## Testing

- `:error` corpus sections are best for invalid syntax checks; for recovery-node
  checks without parser error state, keep normal sections with expected trees
- Add dedicated path-data corpus cases for implicit separators and arc-flag
  adjacency (e.g. `A... 01 ...`) to prevent regressions
- Highlight tests for XML-based grammars use `<!-- -->` comments for assertions;
  `<!--` occupies cols 0-3 so `^` carets can only target col 4+; use indented
  arrow tests (`<!-- <- capture -->`) to reach earlier columns
- In highlight tests, child literal captures (`"<?"`, `"<!--"`, `"<!DOCTYPE"` →
  `@punctuation.delimiter`) override parent node captures
  (`(xml_declaration) @keyword`, `(comment) @comment`); test the inner text, not
  the delimiter, for the parent's highlight
- Tag test assertion comments (`<!-- ^ definition.id -->`) are real comment
  nodes in the CST; if an assertion comment appears right before another
  id-bearing element, it becomes that element's `@doc` docstring. Insert a
  non-id element between them to break the adjacency chain

## Tags (code navigation)

- `tree-sitter tags` only allows capture names `@definition.*`, `@reference.*`,
  `@doc`, `@name`, `@local.*`; any other capture (e.g. `@_name` for predicate
  filtering) causes `Invalid capture` error and no output at all
- In `tree-sitter tags`, when multiple patterns match the same
  `@name`/`@definition.*` node, the first matching pattern wins; doc-bearing
  patterns must precede simpler fallback patterns or the docstring is lost
- With `extras: () => []`, explicit `(text)` whitespace nodes appear between
  sibling comments and elements; the `.` anchor requires consecutive named
  siblings, so use `(comment) . (text) . (element)` to bridge. Also need a
  variant without `(text)` for inline placement (`<!-- doc --><el/>`)
- Query child patterns match direct children only, not descendants;
  `(element (self_closing_tag (id_attribute ...)))` is "Impossible pattern"
  because `attribute` wraps `id_attribute` — must write
  `(element (self_closing_tag (attribute (id_attribute ...))))`
- SVG IDs are document-global; `@local.scope` should be on `svg_root_element`
  only, not per-element — a `<linearGradient id="grad1">` inside `<defs>` must
  be referenceable from anywhere

## Bindings

- Bun cannot load Node-API `.node` addons via `import()`; use `require()` (or
  `process.dlopen`) for Bun-specific loading paths
- `bun test` works in this repo when expected prebuild paths exist; test
  bootstrap can create the missing `tree-sitter` runtime prebuild path under
  `node_modules/tree-sitter/prebuilds/<platform>-<arch>/`

## SVG Spec Gotchas

- SVG 2 `svg_path` grammar allows empty/whitespace-only `d` values; treat `d=""`
  and `d="   "` as valid parse cases
- `svg` root detection should accept namespaced forms like `svg:svg` by checking
  local-name segment after the last `:`

## Tree-sitter Quirks

- `<?xml` can be lexically stolen by generic `<?` processing-instruction rules;
  `token(prec(..., '<?xml'))` fixes declaration recognition. The literal 5-char
  token is *insufficient* though: it also matches the `<?xml-stylesheet` PI
  target prefix, causing xml_declaration to commit and then fail at the `-`.
  Require mandatory trailing whitespace in the start token
  (`token(prec(2, /<\?xml[ \t\r\n]+/))`) so the lexer only emits
  `_xml_declaration_start` when a well-formed declaration follows; `<?xml-*` PI
  targets fall through to the generic `<?` + `pi_target_name` path. XML 1.0 §2.8
  guarantees VersionInfo begins with S whitespace, so this is spec-sound
- XML 1.0 §2.8 requires `<?xml ... ?>` to be the absolute first thing in the
  document (no preceding whitespace), but real-world SVGs can gain a leading
  newline/BOM/whitespace via formatters, editors, or copy-paste pipelines (our
  local `samples/w3/arcs01.svg` picked up a leading `\n` from dprint before we
  stripped it; upstream W3C is clean). Strict
  `source_file = optional(xml_declaration) ...` fails these inputs because misc
  cannot appear before xml_declaration. Fix: `source_file` takes
  `optional(seq(repeat($._misc), $.xml_declaration))` so leading misc is
  permitted only when an xml_declaration follows, keeping `source_file_repeat1`
  unambiguous. Regression fixture at `samples/leading-ws-before-xml-decl.svg`
- `tree-sitter` v0.25.0 native addon fails to compile with newer Node/V8
  toolchains in this repo setup; Node 22 LTS works — Volta pin set to 22.22.1
- Non-start grammar rules cannot match empty strings; wrap optional emptiness in
  parent rules instead of making the child nullable
- Overlapping whitespace tokens (e.g. `misc_text` vs path whitespace) can cause
  wrong token choice; assign precedence for context-specific whitespace tokens
- Lexical precedence cannot express "match `base64` only when immediately
  followed by the data-URI comma" without consuming that comma. A
  higher-precedence `;base64` token steals the prefix from valid parameters like
  `;base64=1` and `;base64foo`; fix by folding the comma into the encoding token
  (`/;[Bb][Aa][Ss][Ee]64,/`) so longest-match handles the distinction
- `tree-sitter test -u` will not update sections that still parse with
  `ERROR`/`MISSING`; fix grammar/input first, then rerun update
- If one rule is a strict superset of another (e.g. `length` includes bare
  numbers), avoid including both in the same `choice` without precedence; this
  creates unresolved LR conflicts
- Aliasing a named subrule to `$.element` inside the `element` rule can create
  nested `(element (element ...))`; use hidden subrules (`_foo_element`) to keep
  CST stable
- With many specialized externals, choose token symbol *after* scanning the full
  name against all valid symbols; pairwise ambiguity guards do not scale

## Supertypes & Inline

- A tree-sitter `supertypes: $ => [...]` member is NOT a hidden-but-present
  wrapper — promoting a visible `choice` rule (e.g. `attribute`, `path_segment`,
  `transform_function`, `color_value`) to a supertype makes that wrapper node
  TRANSPARENT: the concrete subtype appears DIRECTLY in the CST with no extra
  level, and node-types.json links it via a `subtypes` array. So the visible
  tree is NOT byte-identical — every corpus expectation loses the wrapper level
  (`(attribute (id_attribute …))` → `(id_attribute …)`), and any query that
  nested a subtype under the supertype must drop that level. The payoff:
  `(attribute) @x` / `(path_segment) @x` single-capture queries match all kinds
  at once, and `(attribute value: (_) @v)` resolves the field through the
  supertype to whichever concrete subtype matched. Queries referencing concrete
  subtype names directly (`(id_attribute …)`, `(href_attribute …)`,
  `*_attribute_name`/`*_attribute_value`) are unaffected. Measured cost of the 4
  SVG supertypes: ZERO extra LR states (1250 → 1250).
- `inline: $ => [...]` is only a net win for SINGLE-USE hidden wrappers. The 9
  single-use color-function wrappers (`_rgb_color`…`_color_mix_function`) inline
  cleanly (−116 states, smaller parser.c). But inlining MULTI-USE hidden
  dispatchers DUPLICATES their body at every use site and EXPLODES the state
  count: adding `_color_component` (×15), `_color_alpha` (×8),
  `_color_hue_component` (×3) drove STATE_COUNT 1250 → 2101 (parser.c 48K → 58K
  lines) — the exact opposite of inline's intended state reduction. Rule of
  thumb: inline single-use indirection; keep multi-use rules as shared hidden
  rules. (None of these change the visible tree — all are `_`-hidden — so the
  choice is purely state-count economics, measure before committing a multi-use
  rule to `inline`.)
- The grammar needs neither `word:` nor extra `token.immediate`. Longest-match
  over the single combined DFA already extracts keywords correctly
  (`fill="context-fill-extra"` → one `named_color`, not literal `context-fill` +
  ERROR; `fill-opacity` → one name, not `fill` + `-opacity`), so `word` is
  redundant. With `extras: () => []` there is no whitespace-skipping for
  `token.immediate` to defend against, and every multi-char XML delimiter is an
  atomic STRING terminal; loose forms (`< /svg>`, `<svg / >`) already ERROR,
  while spec-valid `</svg >` (ETag `S?`) correctly parses — adding immediacy
  would wrongly reject it.

## Architecture: Parse Structure Not Schema

- Encoding SVG element categories × attribute combinations in the grammar causes
  LR state explosion (458 rules → 102K-line parser.c); collapsing to 5 element
  types + 14 typed attributes → 120 rules, 21K-line parser.c (79% reduction)
- Content model constraints (e.g. "path cannot contain child elements") belong
  in linting/query layers, not the parser; grammar should accept structurally
  valid XML
- `raw_text` external token for script/style must guard against error recovery:
  when tree-sitter sets all `valid_symbols` true, check
  `!valid_symbols[START_TAG_NAME] && !valid_symbols[END_TAG_NAME]` to prevent
  raw_text from consuming normal content
- Only 5 element names need scanner recognition: svg (root enforcement), path (d
  attribute), script/style (raw text capture), plus generic fallback
- Attribute sub-grammars worth keeping in the parser are those with genuine
  value syntax (path data, viewBox numbers, transform functions, paint
  functions, URI references) — keyword-only attributes (calcMode, spreadMethod,
  edgeMode) belong in queries
- CDATA text cannot be tokenized correctly with a pure regex when content
  contains `]` before `]]>` (e.g. `a]]]>`); the regex `\]\][^>]` over-matches
  runs of 3+ `]`. External scanner + `repeat1()` chunking is required (same
  approach as tree-sitter-xml)
- When a tree-sitter external scanner returns false, the lexer position resets
  to the scan start — advance() calls are undone. This enables peek-ahead
  patterns: advance past potential delimiters, return false if found (letting
  the grammar match the literal), return true with mark_end if not
- The `svg-data-regen` keyword-bucket classifier must reject scraped spec prose.
  Attributes whose catalog grammar is an English cross-reference or prose
  placeholder (`(see below)`, `(see in attribute)`, `Language-Tag [ABNF]`,
  `space-separated valid non-empty URL tokens [HTML]`) arrive as several
  juxtaposed `keyword` graph nodes with NO alternation operator. A genuine bare
  enum is alternatives joined by `|`/`||` (`anonymous | use-credentials`,
  `normal | [ fill || stroke || markers ]`) or a single keyword. Treating prose
  as an enum routes real values like `type="text/javascript"` / `media="screen"`
  through the single-token keyword rule and ERRORs. Fix lives in
  `GrammarAnalysis::is_bare_keyword_enum` (requires `|`/`||` for 2+ keywords);
  prose falls through to the `css_text` opaque catch-all.
- Union value spaces (keyword | length/number) need the typed value rule to
  accept the keyword arm too. `length` bucket attributes like `baseline-shift`
  (`sub | super | <length-percentage>`), `letter-spacing`/`word-spacing`
  (`normal | <length>`), `refX`/`refY` (`<length> | left | center | …`) carry
  keyword values; `length_attribute_value` adds a `length_keyword_value` arm
  (keyword-led, or length-led with a trailing item) so the plain single-length
  shape is untouched and pure-length attrs keep their exact CST.
- `font-size` (`<absolute-size> | <relative-size> | <length-percentage>`) is a
  three-way union with no clean home in `length` (size keywords break it) or
  `keyword` (lengths break it). It routes to the `css_text` opaque bucket:
  `font-size="16px"`, `"small"`, `"larger"`, `"1.2em"`, `"50%"` all parse
  losslessly as `css_text_attribute` content. Same treatment for other unions
  the catalog cannot disambiguate (MIME `type`, media queries).
- `values`/`tableValues`/`kernelMatrix` are number *lists*, but the spec writes
  their grammar as prose (`list of <number>s`, `(list of <number>s)`,
  `<list of numbers>`) which the scraper used to degrade to a bare `<number>`
  (or garbled keyword tokens). Root-caused in `svg-data-regen`:
  `chapter::number_list_prose_production` canonicalizes those prose idioms to a
  real `<number>+` production (shape-based, no attribute-name allowlist), so the
  catalog now routes all three to the `number_list` bucket. The grammar's
  `number_list_attribute_value` is `choice(number_list, semicolon_number_list)`
  so the same `values` attribute parses both the filter wsp/comma list
  (`values="1 0 0 0 0"`) and the SMIL animation `;` list (`values="0;10;20"`); a
  lone number is the shared list-of-one, declared as a GLR `conflicts` pair.
  Plain `number_attribute_value` is back to a bare single `<number>` — no
  separated tail — because genuine scalars (`bias`, `divisor`, `seed`) never
  list.

## 2026-03-22: Helix 25.07.1 rejects `#strip!` in SVG queries

Helix logged `Failed to compile highlights for 'svg': unknown predicate #strip!`
even though the predicate only appeared in `queries/tags.scm` and
`queries/locals.scm`.

Practical consequence:

- SVG buffers could show no Tree-sitter syntax highlighting at all when Helix
  loaded the query set.

Workaround used here:

- remove `#strip!` from the SVG Helix query set and keep only the `#match? "^#"`
  guards

Tradeoff:

- tag/locals references that rely on stripping the leading `#` may no longer
  resolve as precisely in editors that do not support `#strip!`, but
  highlighting compiles again.
- Downstream editor-specific query packs can still keep a separate `locals.scm`
  with `#strip!` for `#foo` -> `foo` normalization; do not reintroduce it in the
  shared upstream queries unless Helix gains support.
