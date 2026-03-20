# SVG Color Language Server — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a minimal LSP server that provides inline color swatches and color picker for SVG paint attributes.

**Architecture:** Two-crate workspace — `svg-color` (pure color extraction via tree-sitter-svg) and `svg-language-server` (LSP binary via tower-lsp-server). The grammar's `(color_value)` nodes provide precise color locations without regex.

**Tech Stack:** Rust 2024 edition, tree-sitter 0.26, tree-sitter-svg, tower-lsp-server, tokio

**Spec:** `docs/specs/2026-03-20-svg-color-ls-design.md`

---

### Task 1: Wire up `svg-color` dependencies and core types

**Files:**

- Modify: `crates/svg-color/Cargo.toml`
- Modify: `crates/svg-color/src/lib.rs`
- Create: `crates/svg-color/src/types.rs`

- [ ] **Step 1: Add dependencies to svg-color**

In `crates/svg-color/Cargo.toml`, add:

```toml
[dependencies]
tree-sitter.workspace     = true
tree-sitter-svg.workspace = true
```

- [ ] **Step 2: Define core types**

Create `crates/svg-color/src/types.rs`:

```rust
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub struct ColorInfo {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub byte_range: Range<usize>,
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
    pub kind: ColorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorKind {
    Hex,
    Functional,
    Named,
}
```

- [ ] **Step 3: Update lib.rs**

Replace placeholder in `crates/svg-color/src/lib.rs`:

```rust
pub mod types;

pub use types::{ColorInfo, ColorKind};
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p svg-color`
Expected: compiles clean

- [ ] **Step 5: Commit**

```bash
git add crates/svg-color/
git commit -m "feat(svg-color): core types for color extraction"
```

---

### Task 2: Named color lookup table

**Files:**

- Create: `crates/svg-color/src/named_colors.rs`
- Modify: `crates/svg-color/src/lib.rs`

- [ ] **Step 1: Write failing test**

Add to `crates/svg-color/src/named_colors.rs`:

```rust
/// Lookup a CSS named color, returning (r, g, b) as f32 values 0.0–1.0.
/// Returns `None` for unknown names.
pub fn lookup(name: &str) -> Option<(f32, f32, f32)> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_colors() {
        assert_eq!(lookup("red"), Some((1.0, 0.0, 0.0)));
        assert_eq!(lookup("lime"), Some((0.0, 1.0, 0.0)));
        assert_eq!(lookup("blue"), Some((0.0, 0.0, 1.0)));
        assert_eq!(lookup("white"), Some((1.0, 1.0, 1.0)));
        assert_eq!(lookup("black"), Some((0.0, 0.0, 0.0)));
        assert_eq!(
            lookup("coral"),
            Some((255.0 / 255.0, 127.0 / 255.0, 80.0 / 255.0))
        );
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(lookup("Red"), Some((1.0, 0.0, 0.0)));
        assert_eq!(lookup("RED"), Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn unknown_names() {
        assert_eq!(lookup("banana"), None);
        assert_eq!(lookup("notacolor"), None);
        assert_eq!(lookup(""), None);
    }

    #[test]
    fn all_148_colors_present() {
        // Spot check that the table has all CSS named colors
        let expected = [
            "aliceblue",
            "antiquewhite",
            "aqua",
            "aquamarine",
            "azure",
            "beige",
            "bisque",
            "black",
            "blanchedalmond",
            "blue",
            "blueviolet",
            "brown",
            "burlywood",
            "cadetblue",
            "chartreuse",
            "chocolate",
            "coral",
            "cornflowerblue",
            "cornsilk",
            "crimson",
            "cyan",
            "darkblue",
            "darkcyan",
            "darkgoldenrod",
            "darkgray",
            "darkgreen",
            "darkgrey",
            "darkkhaki",
            "darkmagenta",
            "darkolivegreen",
            "darkorange",
            "darkorchid",
            "darkred",
            "darksalmon",
            "darkseagreen",
            "darkslateblue",
            "darkslategray",
            "darkslategrey",
            "darkturquoise",
            "darkviolet",
            "deeppink",
            "deepskyblue",
            "dimgray",
            "dimgrey",
            "dodgerblue",
            "firebrick",
            "floralwhite",
            "forestgreen",
            "fuchsia",
            "gainsboro",
            "ghostwhite",
            "gold",
            "goldenrod",
            "gray",
            "green",
            "greenyellow",
            "grey",
            "honeydew",
            "hotpink",
            "indianred",
            "indigo",
            "ivory",
            "khaki",
            "lavender",
            "lavenderblush",
            "lawngreen",
            "lemonchiffon",
            "lightblue",
            "lightcoral",
            "lightcyan",
            "lightgoldenrodyellow",
            "lightgray",
            "lightgreen",
            "lightgrey",
            "lightpink",
            "lightsalmon",
            "lightseagreen",
            "lightskyblue",
            "lightslategray",
            "lightslategrey",
            "lightsteelblue",
            "lightyellow",
            "lime",
            "limegreen",
            "linen",
            "magenta",
            "maroon",
            "mediumaquamarine",
            "mediumblue",
            "mediumorchid",
            "mediumpurple",
            "mediumseagreen",
            "mediumslateblue",
            "mediumspringgreen",
            "mediumturquoise",
            "mediumvioletred",
            "midnightblue",
            "mintcream",
            "mistyrose",
            "moccasin",
            "navajowhite",
            "navy",
            "oldlace",
            "olive",
            "olivedrab",
            "orange",
            "orangered",
            "orchid",
            "palegoldenrod",
            "palegreen",
            "paleturquoise",
            "palevioletred",
            "papayawhip",
            "peachpuff",
            "peru",
            "pink",
            "plum",
            "powderblue",
            "purple",
            "rebeccapurple",
            "red",
            "rosybrown",
            "royalblue",
            "saddlebrown",
            "salmon",
            "sandybrown",
            "seagreen",
            "seashell",
            "sienna",
            "silver",
            "skyblue",
            "slateblue",
            "slategray",
            "slategrey",
            "snow",
            "springgreen",
            "steelblue",
            "tan",
            "teal",
            "thistle",
            "tomato",
            "turquoise",
            "violet",
            "wheat",
            "white",
            "whitesmoke",
            "yellow",
            "yellowgreen",
        ];
        for name in expected {
            assert!(lookup(name).is_some(), "missing named color: {name}");
        }
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p svg-color -- named_colors`
Expected: FAIL (todo panic)

- [ ] **Step 3: Implement lookup table**

Replace `todo!()` in `lookup()` with a case-insensitive match against
the full CSS Named Colors table (148 entries). Use a `match` on
`name.to_ascii_lowercase().as_str()` returning `Some((r/255.0, g/255.0, b/255.0))`.

Reference: https://www.w3.org/TR/css-color-4/#named-colors

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p svg-color -- named_colors`
Expected: all 4 tests PASS

- [ ] **Step 5: Export module from lib.rs**

Add to `crates/svg-color/src/lib.rs`:

```rust
pub mod named_colors;
```

- [ ] **Step 6: Commit**

```bash
git add crates/svg-color/
git commit -m "feat(svg-color): CSS named color lookup table"
```

---

### Task 3: Hex color parser

**Files:**

- Create: `crates/svg-color/src/parse.rs`
- Modify: `crates/svg-color/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/svg-color/src/parse.rs`:

```rust
use crate::types::{ColorInfo, ColorKind};
use std::ops::Range;

/// Parse a hex color string like `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`.
/// Returns (r, g, b, a) as f32 values 0.0–1.0, or None if invalid.
pub fn parse_hex(text: &str) -> Option<(f32, f32, f32, f32)> {
    todo!()
}

/// Parse a functional color string like `rgb(255, 0, 0)` or `hsl(120, 50%, 50%)`.
/// Returns (r, g, b, a) as f32 values 0.0–1.0, or None if invalid.
pub fn parse_functional(text: &str) -> Option<(f32, f32, f32, f32)> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── hex ────────────────────────────────────────────

    #[test]
    fn hex_6_digit() {
        assert_eq!(parse_hex("#ff0000"), Some((1.0, 0.0, 0.0, 1.0)));
        assert_eq!(parse_hex("#00ff00"), Some((0.0, 1.0, 0.0, 1.0)));
        assert_eq!(parse_hex("#0000ff"), Some((0.0, 0.0, 1.0, 1.0)));
    }

    #[test]
    fn hex_3_digit() {
        assert_eq!(parse_hex("#f00"), Some((1.0, 0.0, 0.0, 1.0)));
        assert_eq!(parse_hex("#fff"), Some((1.0, 1.0, 1.0, 1.0)));
    }

    #[test]
    fn hex_8_digit_alpha() {
        assert_eq!(parse_hex("#ff000080"), Some((1.0, 0.0, 0.0, 128.0 / 255.0)));
        assert_eq!(parse_hex("#ff0000ff"), Some((1.0, 0.0, 0.0, 1.0)));
    }

    #[test]
    fn hex_4_digit_alpha() {
        assert_eq!(
            parse_hex("#f008"),
            Some((1.0, 0.0, 0.0, 0x88 as f32 / 255.0))
        );
    }

    #[test]
    fn hex_case_insensitive() {
        assert_eq!(parse_hex("#FF0000"), Some((1.0, 0.0, 0.0, 1.0)));
        assert_eq!(parse_hex("#Ff0000"), Some((1.0, 0.0, 0.0, 1.0)));
    }

    #[test]
    fn hex_invalid() {
        assert_eq!(parse_hex("#gg0000"), None);
        assert_eq!(parse_hex("#ff0"), None); // 2 digits — invalid length
        assert_eq!(parse_hex("ff0000"), None); // missing #
        assert_eq!(parse_hex(""), None);
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p svg-color -- parse::tests::hex`
Expected: FAIL (todo panic)

- [ ] **Step 3: Implement parse_hex**

Implement hex parsing: strip `#`, match length (3/4/6/8), expand short form,
parse hex digits to u8, convert to f32 0.0–1.0. Return `None` for invalid.

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p svg-color -- parse::tests::hex`
Expected: all 6 tests PASS

- [ ] **Step 5: Export module from lib.rs**

Add to `crates/svg-color/src/lib.rs`:

```rust
pub mod parse;
```

- [ ] **Step 6: Commit**

```bash
git add crates/svg-color/
git commit -m "feat(svg-color): hex color parser"
```

---

### Task 4: Functional color parser (rgb/rgba/hsl/hsla)

**Files:**

- Modify: `crates/svg-color/src/parse.rs`

- [ ] **Step 1: Write failing tests**

Add to `parse.rs` tests module:

```rust
// ─── functional ─────────────────────────────────────

#[test]
fn rgb_integers() {
    assert_eq!(
        parse_functional("rgb(255, 0, 0)"),
        Some((1.0, 0.0, 0.0, 1.0))
    );
    assert_eq!(
        parse_functional("rgb(0,128,255)"),
        Some((0.0, 128.0 / 255.0, 1.0, 1.0))
    );
}

#[test]
fn rgba_with_alpha() {
    let result = parse_functional("rgba(255, 0, 0, 0.5)");
    assert!(result.is_some());
    let (r, _, _, a) = result.unwrap();
    assert!((r - 1.0).abs() < 0.01);
    assert!((a - 0.5).abs() < 0.01);
}

#[test]
fn rgb_percentages() {
    assert_eq!(
        parse_functional("rgb(100%, 0%, 0%)"),
        Some((1.0, 0.0, 0.0, 1.0))
    );
}

#[test]
fn hsl_basic() {
    // hsl(0, 100%, 50%) = red
    let result = parse_functional("hsl(0, 100%, 50%)");
    assert!(result.is_some());
    let (r, g, b, _) = result.unwrap();
    assert!((r - 1.0).abs() < 0.02);
    assert!(g < 0.02);
    assert!(b < 0.02);
}

#[test]
fn hsl_green() {
    // hsl(120, 100%, 50%) = lime green
    let result = parse_functional("hsl(120, 100%, 50%)");
    assert!(result.is_some());
    let (r, g, b, _) = result.unwrap();
    assert!(r < 0.02);
    assert!((g - 1.0).abs() < 0.02);
    assert!(b < 0.02);
}

#[test]
fn hsla_with_alpha() {
    let result = parse_functional("hsla(0, 100%, 50%, 0.5)");
    assert!(result.is_some());
    let (_, _, _, a) = result.unwrap();
    assert!((a - 0.5).abs() < 0.01);
}

#[test]
fn functional_whitespace_variations() {
    assert!(parse_functional("rgb( 255 , 0 , 0 )").is_some());
    assert!(parse_functional("rgb(255,0,0)").is_some());
}

#[test]
fn functional_invalid() {
    assert_eq!(parse_functional("rgb()"), None);
    assert_eq!(parse_functional("rgb(a, b, c)"), None);
    assert_eq!(parse_functional("notafunction(1,2,3)"), None);
    assert_eq!(parse_functional(""), None);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p svg-color -- parse::tests::functional`
Expected: FAIL (todo panic)

- [ ] **Step 3: Implement parse_functional**

Parse format: strip function name, extract args between parens, split on commas,
parse integers or percentages. For HSL, convert to RGB using standard algorithm.

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p svg-color -- parse::tests`
Expected: all parse tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/svg-color/src/parse.rs
git commit -m "feat(svg-color): functional color parser (rgb/hsl)"
```

---

### Task 5: Tree-sitter color extraction

**Files:**

- Create: `crates/svg-color/src/extract.rs`
- Modify: `crates/svg-color/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/svg-color/src/extract.rs`:

```rust
use crate::types::{ColorInfo, ColorKind};
use tree_sitter::{Parser, Tree};

/// Extract all colors from SVG source text.
pub fn extract_colors(source: &[u8]) -> Vec<ColorInfo> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_svg::LANGUAGE.into())
        .expect("failed to load SVG grammar");
    let tree = parser.parse(source, None).expect("failed to parse");
    extract_colors_from_tree(source, &tree)
}

/// Extract colors from an already-parsed tree.
pub fn extract_colors_from_tree(source: &[u8], tree: &Tree) -> Vec<ColorInfo> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_fill() {
        let src = br#"<svg><rect fill="#ff0000"/></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].r, 1.0);
        assert_eq!(colors[0].g, 0.0);
        assert_eq!(colors[0].b, 0.0);
        assert_eq!(colors[0].kind, ColorKind::Hex);
    }

    #[test]
    fn named_stroke() {
        let src = br#"<svg><circle stroke="red"/></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].r, 1.0);
        assert_eq!(colors[0].kind, ColorKind::Named);
    }

    #[test]
    fn functional_fill() {
        let src = br#"<svg><rect fill="rgb(0, 128, 255)"/></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].kind, ColorKind::Functional);
    }

    #[test]
    fn paint_server_fallback() {
        let src = br#"<svg><rect fill="url(#grad) #00ff00"/></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].g, 1.0);
    }

    #[test]
    fn multiple_colors() {
        let src = br#"<svg><rect fill="#ff0000" stroke="#00ff00"/></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 2);
    }

    #[test]
    fn keywords_skipped() {
        let src = br#"<svg><rect fill="none" stroke="currentColor" color="inherit"/></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 0);
    }

    #[test]
    fn invalid_named_color_skipped() {
        let src = br#"<svg><rect fill="banana"/></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 0);
    }

    #[test]
    fn color_in_comment_ignored() {
        let src = br#"<svg><!-- fill="#ff0000" --><rect fill="blue"/></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].b, 1.0); // blue, not the comment's red
    }

    #[test]
    fn stop_color() {
        let src = br#"<svg><stop stop-color="#ff8800"/></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
    }

    #[test]
    fn byte_range_correct() {
        let src = br#"<svg><rect fill="#ff0000"/></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 1);
        let color_text = std::str::from_utf8(&src[colors[0].byte_range.clone()]).unwrap();
        assert_eq!(color_text, "#ff0000");
    }

    #[test]
    fn empty_paint_value() {
        let src = br#"<svg><rect fill=""/></svg>"#;
        let colors = extract_colors(src);
        assert_eq!(colors.len(), 0);
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p svg-color -- extract`
Expected: FAIL (todo panic)

- [ ] **Step 3: Implement extract_colors_from_tree**

Use tree-sitter cursor to walk the tree. For each node with kind `"hex_color"`,
`"functional_color"`, or `"named_color"`:

1. Get node text from source bytes
2. Parse using `parse::parse_hex`, `parse::parse_functional`, or `named_colors::lookup`
3. If valid, create `ColorInfo` with RGBA, byte range, start/end position from
   `node.start_position()` and `node.end_position()`, and kind
4. Collect into `Vec<ColorInfo>`

No tree-sitter query needed — a simple recursive walk checking `node.kind()`
is sufficient and avoids the query compilation overhead.

**Assumption:** In the current grammar, `hex_color`/`functional_color`/`named_color`
only appear under `color_value` inside `paint_attribute` or `paint_server`. If the
grammar ever adds these node types elsewhere, this walk would need filtering.

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p svg-color`
Expected: all tests PASS

- [ ] **Step 5: Export module and public API from lib.rs**

Update `crates/svg-color/src/lib.rs`:

```rust
pub mod extract;
pub mod named_colors;
pub mod parse;
pub mod types;

pub use extract::{extract_colors, extract_colors_from_tree};
pub use types::{ColorInfo, ColorKind};
```

- [ ] **Step 6: Commit**

```bash
git add crates/svg-color/
git commit -m "feat(svg-color): tree-sitter color extraction"
```

---

### Task 6: Color presentation (format conversion)

**Files:**

- Create: `crates/svg-color/src/present.rs`
- Modify: `crates/svg-color/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/svg-color/src/present.rs`:

```rust
use crate::types::ColorKind;

/// Generate color presentation strings for a given RGBA color.
/// Returns a vec of (label, text_edit) pairs.
/// The original format is listed first.
pub fn color_presentations(r: f32, g: f32, b: f32, a: f32, original: ColorKind) -> Vec<String> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_presentations() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Hex);
        assert!(results.iter().any(|s| s == "#ff0000"));
        assert!(results.iter().any(|s| s == "rgb(255, 0, 0)"));
        assert!(results.iter().any(|s| s.starts_with("hsl(")));
        // Original format should be first
        assert!(results[0] == "#ff0000");
    }

    #[test]
    fn hex_with_alpha() {
        let results = color_presentations(1.0, 0.0, 0.0, 0.5, ColorKind::Hex);
        assert!(results.iter().any(|s| s == "#ff000080"));
        assert!(results.iter().any(|s| s == "rgba(255, 0, 0, 0.5)"));
    }

    #[test]
    fn named_color_included() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Named);
        assert!(results.iter().any(|s| s == "red"));
        // Named should be first when original is Named
        assert!(results[0] == "red");
    }

    #[test]
    fn functional_original_first() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Functional);
        assert!(results[0].starts_with("rgb(") || results[0].starts_with("hsl("));
    }

    #[test]
    fn no_alpha_suffix_when_opaque() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Hex);
        // Should NOT include #ff0000ff (redundant alpha)
        assert!(!results.iter().any(|s| s == "#ff0000ff"));
        // Should NOT include rgba when alpha is 1.0
        assert!(!results.iter().any(|s| s.starts_with("rgba(")));
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p svg-color -- present`
Expected: FAIL (todo panic)

- [ ] **Step 3: Implement color_presentations**

Generate format strings: hex, rgb/rgba, hsl/hsla, and reverse-lookup named color.
Order by original format first. Include alpha variants only when `a < 1.0`.

RGB→HSL conversion: standard algorithm (max/min channel, hue from sector, saturation from lightness).

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p svg-color -- present`
Expected: all 5 tests PASS

- [ ] **Step 5: Export from lib.rs**

Add to `crates/svg-color/src/lib.rs`:

```rust
pub mod present;
pub use present::color_presentations;
```

- [ ] **Step 6: Commit**

```bash
git add crates/svg-color/
git commit -m "feat(svg-color): color presentation format conversion"
```

---

### Task 7: LSP server scaffolding

**Files:**

- Modify: `crates/svg-language-server/Cargo.toml`
- Modify: `crates/svg-language-server/src/main.rs`

- [ ] **Step 1: Add LSP dependencies**

Update `crates/svg-language-server/Cargo.toml`:

```toml
[dependencies]
svg-color.workspace       = true
tree-sitter.workspace     = true
tree-sitter-svg.workspace = true
tower-lsp-server          = "0.23"
tokio                     = { version = "1", features = ["io-std", "macros", "rt-multi-thread"] }
```

Add to workspace `Cargo.toml` `[workspace.dependencies]`:

```toml
tower-lsp-server = "0.23"
tokio            = { version = "1", features = ["io-std", "macros", "rt-multi-thread"] }
```

Then update the crate Cargo.toml to use workspace refs.

- [ ] **Step 2: Implement minimal LSP server**

Replace `crates/svg-language-server/src/main.rs`.

**Note:** `tower-lsp-server` 0.23 uses `ls_types` (not `lsp_types`), `Uri`
(not `Url`), and native async (no `#[async_trait]`). Verify exact API against
the crate docs at build time — the snippets below are guidance, not copy-paste.

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

struct SvgLanguageServer {
    client: Client,
    documents: Arc<RwLock<HashMap<Uri, String>>>,
}

impl SvgLanguageServer {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl LanguageServer for SvgLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                color_provider: Some(ColorProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let mut docs = self.documents.write().await;
        docs.insert(params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let mut docs = self.documents.write().await;
        if let Some(change) = params.content_changes.into_iter().last() {
            docs.insert(params.text_document.uri, change.text);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut docs = self.documents.write().await;
        docs.remove(&params.text_document.uri);
    }

    async fn document_color(&self, params: DocumentColorParams) -> Result<Vec<ColorInformation>> {
        // Stub — next task fills this in
        Ok(vec![])
    }

    async fn color_presentation(
        &self,
        params: ColorPresentationParams,
    ) -> Result<Vec<ColorPresentation>> {
        // Stub — next task fills this in
        Ok(vec![])
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(SvgLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p svg-language-server`
Expected: compiles clean (may need to adjust tower-lsp-server API to match exact version)

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/svg-language-server/
git commit -m "feat(lsp): server scaffolding with tower-lsp-server"
```

---

### Task 8: Wire document_color and color_presentation

**Files:**

- Modify: `crates/svg-language-server/src/main.rs`

- [ ] **Step 1: Implement document_color**

Replace the stub `document_color` method:

```rust
async fn document_color(&self, params: DocumentColorParams) -> Result<Vec<ColorInformation>> {
    let docs = self.documents.read().await;
    let Some(source) = docs.get(&params.text_document.uri) else {
        return Ok(vec![]);
    };
    let source_bytes = source.as_bytes();
    let colors = svg_color::extract_colors(source_bytes);

    Ok(colors
        .into_iter()
        .map(|c| ColorInformation {
            range: Range {
                start: Position {
                    line: c.start_row as u32,
                    character: byte_col_to_utf16(source_bytes, c.start_row, c.start_col),
                },
                end: Position {
                    line: c.end_row as u32,
                    character: byte_col_to_utf16(source_bytes, c.end_row, c.end_col),
                },
            },
            color: Color {
                red: c.r as f64,
                green: c.g as f64,
                blue: c.b as f64,
                alpha: c.a as f64,
            },
        })
        .collect())
}
```

Need a helper `byte_col_to_utf16(source, row, byte_col) -> u32` that counts
UTF-16 code units for the byte column offset. For ASCII SVG this is 1:1,
but multi-byte chars need proper conversion.

Also store `ColorKind` alongside each color in a side map for `color_presentation`.

- [ ] **Step 2: Implement color_presentation**

Replace the stub `color_presentation` method:

```rust
async fn color_presentation(
    &self,
    params: ColorPresentationParams,
) -> Result<Vec<ColorPresentation>> {
    let r = params.color.red as f32;
    let g = params.color.green as f32;
    let b = params.color.blue as f32;
    let a = params.color.alpha as f32;

    // Default to Hex if we don't know the original format
    let kind = /* look up original kind from cached data, default ColorKind::Hex */;

    let presentations = svg_color::color_presentations(r, g, b, a, kind);
    Ok(presentations
        .into_iter()
        .map(|label| ColorPresentation {
            label: label.clone(),
            text_edit: Some(TextEdit {
                range: params.range,
                new_text: label,
            }),
            additional_text_edits: None,
        })
        .collect())
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p svg-language-server`
Expected: compiles clean

- [ ] **Step 4: Manual smoke test**

Run: `cargo build -p svg-language-server`
Then test with an LSP client (e.g., `echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}' | cargo run -p svg-language-server`).
Expected: server responds with capabilities including `colorProvider: true`

- [ ] **Step 5: Commit**

```bash
git add crates/svg-language-server/
git commit -m "feat(lsp): wire document_color and color_presentation"
```

---

### Task 9: End-to-end integration test

**Files:**

- Create: `crates/svg-language-server/tests/integration.rs`

- [ ] **Step 1: Write integration test**

Create `crates/svg-language-server/tests/integration.rs` that:

1. Spawns the LSP binary as a subprocess
2. Sends `initialize` request
3. Sends `textDocument/didOpen` with SVG content containing colors
4. Sends `textDocument/documentColor` request
5. Asserts the response contains the expected color information
6. Sends `textDocument/colorPresentation` request
7. Asserts format alternatives are returned

Use `std::process::Command` to spawn and communicate via stdin/stdout
with JSON-RPC framing (`Content-Length: N\r\n\r\n{...}`).

- [ ] **Step 2: Run integration test**

Run: `cargo test -p svg-language-server -- --test integration`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/svg-language-server/tests/
git commit -m "test(lsp): end-to-end integration test"
```

---

### Task 10: Verify and clean up

**Files:**

- Modify: `README.md`

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass across both crates

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 3: Update README**

Add to `README.md`:

- What the server does (documentColor + colorPresentation for SVG)
- Installation: `cargo install svg-language-server`
- Editor setup: Zed config snippet
- Supported attributes and color formats

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: README with install and usage"
```

---

## Verification

After all tasks complete:

1. `cargo test --workspace` — all unit + integration tests pass
2. `cargo clippy --workspace -- -D warnings` — no warnings
3. `cargo build --release -p svg-language-server` — release binary builds
4. Manual test in Zed:
   - Install binary to PATH
   - Add `[language_servers.svg-language-server]` to zed-svg extension
   - Open an SVG with `fill="#ff0000"` — color swatch appears
   - Click swatch — color picker shows hex/rgb/hsl alternatives

## File Map

| File                                              | Responsibility                      |
| ------------------------------------------------- | ----------------------------------- |
| `crates/svg-color/src/types.rs`                   | `ColorInfo`, `ColorKind` data types |
| `crates/svg-color/src/named_colors.rs`            | 148-entry CSS named color lookup    |
| `crates/svg-color/src/parse.rs`                   | Hex + functional color parsing      |
| `crates/svg-color/src/extract.rs`                 | Tree-sitter walk → `Vec<ColorInfo>` |
| `crates/svg-color/src/present.rs`                 | Color format conversion for picker  |
| `crates/svg-color/src/lib.rs`                     | Public API re-exports               |
| `crates/svg-language-server/src/main.rs`          | LSP server (tower-lsp-server)       |
| `crates/svg-language-server/tests/integration.rs` | E2E LSP test                        |

## Deferred from Spec

- **Incremental re-parse**: The spec mentions tree-sitter's `tree.edit()` API
  for incremental updates. This plan uses full re-parse on `didChange` instead.
  SVGs are small enough that full re-parse is negligible. Add incremental
  parsing if profiling shows it matters.
- **npm distribution**: Deferred to Phase 2. Ship via `cargo install` first.
