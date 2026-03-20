# SVG Intelligence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add completions, hover documentation, and structural diagnostics to the SVG language server.

**Architecture:** Two new crates (`svg-data` for the SVG element/attribute catalog, `svg-lint` for diagnostics) plus modifications to the existing `svg-language-server` LSP binary. The catalog is baked in at compile time from a curated JSON file and `@mdn/browser-compat-data`. All consumers share a single parsed tree per document.

**Tech Stack:** Rust 2024, tree-sitter 0.26, tree-sitter-svg, tower-lsp-server 0.23, tokio, serde/serde_json

**Spec:** `docs/specs/2026-03-20-svg-intelligence-design.md`

---

## File Map

| File                                     | Responsibility                                                                                       |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `crates/svg-data/Cargo.toml`             | New crate manifest                                                                                   |
| `crates/svg-data/src/lib.rs`             | Public API: `element()`, `attribute()`, `allowed_children()`, etc.                                   |
| `crates/svg-data/src/types.rs`           | `ElementDef`, `AttributeDef`, `ContentModel`, `BaselineStatus`, `ElementCategory`, `AttributeValues` |
| `crates/svg-data/src/catalog.rs`         | Static element/attribute tables (generated or hand-written)                                          |
| `crates/svg-data/src/categories.rs`      | `elements_in_category()` mapping                                                                     |
| `crates/svg-data/data/elements.json`     | Curated element catalog source (descriptions, content models, attrs)                                 |
| `crates/svg-data/data/attributes.json`   | Curated attribute catalog source (descriptions, values, elements)                                    |
| `crates/svg-data/build.rs`               | Build script: reads JSON catalog + browser-compat-data, generates `catalog.rs`                       |
| `crates/svg-lint/Cargo.toml`             | New crate manifest                                                                                   |
| `crates/svg-lint/src/lib.rs`             | Public API: `lint()`, `lint_tree()`                                                                  |
| `crates/svg-lint/src/types.rs`           | `SvgDiagnostic`, `Severity`, `DiagnosticCode`                                                        |
| `crates/svg-lint/src/rules.rs`           | Individual diagnostic rule implementations                                                           |
| `crates/svg-language-server/src/main.rs` | Modified: shared tree, hover, completions, diagnostics                                               |

---

### Task 1: Scaffold `svg-data` crate with types

**Files:**

- Create: `crates/svg-data/Cargo.toml`
- Create: `crates/svg-data/src/lib.rs`
- Create: `crates/svg-data/src/types.rs`
- Modify: `Cargo.toml` (workspace deps)

- [ ] **Step 1: Create crate directory and Cargo.toml**

```bash
mkdir -p crates/svg-data/src
```

`crates/svg-data/Cargo.toml`:

```toml
[package]
name                  = "svg-data"
version               = "0.1.0"
edition.workspace     = true
description.workspace = true
readme.workspace      = true
repository.workspace  = true
```

- [ ] **Step 2: Add workspace dependency**

In root `Cargo.toml`, add to `[workspace.dependencies]`:

```toml
[workspace.dependencies.svg-data]
path = "crates/svg-data"
```

- [ ] **Step 3: Define core types**

Create `crates/svg-data/src/types.rs`:

```rust
/// Definition of an SVG element.
#[derive(Debug, Clone)]
pub struct ElementDef {
    pub name: &'static str,
    pub description: &'static str,
    pub mdn_url: &'static str,
    pub deprecated: bool,
    pub baseline: Option<BaselineStatus>,
    pub content_model: ContentModel,
    pub required_attrs: &'static [&'static str],
    pub attrs: &'static [&'static str],
    pub global_attrs: bool,
}

/// Whether an element is a container, void, or text-content element.
#[derive(Debug, Clone)]
pub enum ContentModel {
    /// Can contain child elements from these categories.
    Children(&'static [ElementCategory]),
    /// Self-closing / void element (e.g. `<rect/>`, `<circle/>`).
    Void,
    /// Contains raw text content, no child elements (e.g. `<title>`, `<desc>`).
    Text,
}

/// Definition of an SVG attribute.
#[derive(Debug, Clone)]
pub struct AttributeDef {
    pub name: &'static str,
    pub description: &'static str,
    pub mdn_url: &'static str,
    pub deprecated: bool,
    pub baseline: Option<BaselineStatus>,
    pub values: AttributeValues,
    pub elements: &'static [&'static str],
}

/// What kind of values an attribute accepts, for completions.
#[derive(Debug, Clone)]
pub enum AttributeValues {
    Enum(&'static [&'static str]),
    FreeText,
    Color,
    Length,
    Url,
    NumberOrPercentage,
    Transform(&'static [&'static str]),
    ViewBox,
    PreserveAspectRatio {
        alignments: &'static [&'static str],
        meet_or_slice: &'static [&'static str],
    },
    Points,
    PathData,
}

/// Baseline browser support status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineStatus {
    Widely { since: u16 },
    Newly { since: u16 },
    Limited,
}

/// SVG element categories for content model grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementCategory {
    Container,
    Shape,
    Text,
    Gradient,
    Filter,
    Descriptive,
    Structural,
    Animation,
    PaintServer,
    ClipMask,
    LightSource,
    FilterPrimitive,
}
```

- [ ] **Step 4: Create lib.rs**

Create `crates/svg-data/src/lib.rs`:

```rust
pub mod types;

pub use types::{
    AttributeDef, AttributeValues, BaselineStatus, ContentModel, ElementCategory, ElementDef,
};
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p svg-data`
Expected: compiles clean

- [ ] **Step 6: Commit**

```bash
git add crates/svg-data/ Cargo.toml
git commit -m "feat(svg-data): scaffold crate with core types"
```

---

### Task 2: Create curated element catalog (JSON)

**Files:**

- Create: `crates/svg-data/data/elements.json`

This is the authoritative source of SVG element knowledge. Start with a
representative subset (~30 most common elements), expand later.

- [ ] **Step 1: Create data directory and elements.json**

```bash
mkdir -p crates/svg-data/data
```

Create `crates/svg-data/data/elements.json`. Each element entry has:

- `name`: element tag name
- `description`: one-sentence description (from MDN)
- `mdn_url`: full MDN URL
- `deprecated`: boolean
- `content_model`: `"void"`, `"text"`, or `{"children": ["Category1", ...]}`
- `required_attrs`: array of attribute names
- `attrs`: element-specific attribute names
- `global_attrs`: boolean

Example structure (include at least these elements: `svg`, `g`, `defs`,
`use`, `rect`, `circle`, `ellipse`, `line`, `polyline`, `polygon`, `path`,
`text`, `tspan`, `textPath`, `a`, `image`, `title`, `desc`, `metadata`,
`symbol`, `clipPath`, `mask`, `linearGradient`, `radialGradient`, `stop`,
`pattern`, `filter`, `feGaussianBlur`, `foreignObject`, `style`, `script`):

```json
[
	{
		"name": "svg",
		"description": "The svg element is a container for SVG graphics.",
		"mdn_url": "https://developer.mozilla.org/en-US/docs/Web/SVG/Element/svg",
		"deprecated": false,
		"content_model": { "children": ["Container", "Shape", "Text", "Gradient", "Filter", "Descriptive", "Structural", "Animation", "PaintServer", "ClipMask"] },
		"required_attrs": [],
		"attrs": ["xmlns", "viewBox", "width", "height", "x", "y", "preserveAspectRatio"],
		"global_attrs": true
	},
	{
		"name": "rect",
		"description": "The rect element is a basic SVG shape that draws rectangles.",
		"mdn_url": "https://developer.mozilla.org/en-US/docs/Web/SVG/Element/rect",
		"deprecated": false,
		"content_model": "void",
		"required_attrs": [],
		"attrs": ["x", "y", "width", "height", "rx", "ry"],
		"global_attrs": true
	},
	{
		"name": "text",
		"description": "The text element defines a graphics element consisting of text.",
		"mdn_url": "https://developer.mozilla.org/en-US/docs/Web/SVG/Element/text",
		"deprecated": false,
		"content_model": { "children": ["Text", "Descriptive"] },
		"required_attrs": [],
		"attrs": ["x", "y", "dx", "dy", "textLength", "lengthAdjust"],
		"global_attrs": true
	},
	{
		"name": "tspan",
		"description": "The tspan element defines a subtext within a text element.",
		"mdn_url": "https://developer.mozilla.org/en-US/docs/Web/SVG/Element/tspan",
		"deprecated": false,
		"content_model": { "children": ["Text", "Descriptive"] },
		"required_attrs": [],
		"attrs": ["x", "y", "dx", "dy", "textLength", "lengthAdjust"],
		"global_attrs": true
	}
]
```

Include all ~30 elements listed above, with accurate descriptions from MDN,
correct content models from the SVG 2 spec, and element-specific attributes.

Reference: https://developer.mozilla.org/en-US/docs/Web/SVG/Element
Reference: https://www.w3.org/TR/SVG2/struct.html (content models)

- [ ] **Step 2: Commit**

```bash
git add crates/svg-data/data/
git commit -m "feat(svg-data): curated element catalog JSON"
```

---

### Task 3: Create curated attribute catalog (JSON)

**Files:**

- Create: `crates/svg-data/data/attributes.json`

- [ ] **Step 1: Create attributes.json**

Each attribute entry has:

- `name`: attribute name
- `description`: one-sentence description
- `mdn_url`: full MDN URL
- `deprecated`: boolean
- `values`: one of:
  - `{"type": "enum", "values": ["v1", "v2"]}` — enumerated keywords
  - `{"type": "free_text"}` — free-form string
  - `{"type": "color"}` — color value
  - `{"type": "length"}` — length or percentage
  - `{"type": "url"}` — URL/IRI reference
  - `{"type": "number_or_percentage"}` — number or percentage
  - `{"type": "transform", "functions": ["translate", "rotate", ...]}` — transform functions
  - `{"type": "viewbox"}` — four numbers
  - `{"type": "preserve_aspect_ratio", "alignments": [...], "meet_or_slice": [...]}` — alignment
  - `{"type": "points"}` — coordinate pair list
  - `{"type": "path_data"}` — SVG path data
- `elements`: array of element names that accept this attribute, or `["*"]` for global

Include at least these attributes: `id`, `class`, `style`, `fill`, `stroke`,
`stroke-width`, `stroke-linecap`, `stroke-linejoin`, `stroke-dasharray`,
`opacity`, `fill-opacity`, `stroke-opacity`, `transform`, `d`, `viewBox`,
`preserveAspectRatio`, `x`, `y`, `width`, `height`, `cx`, `cy`, `r`, `rx`,
`ry`, `x1`, `y1`, `x2`, `y2`, `points`, `href`, `xlink:href`,
`font-family`, `font-size`, `font-weight`, `text-anchor`,
`dominant-baseline`, `stop-color`, `stop-opacity`, `offset`,
`gradientUnits`, `gradientTransform`, `clip-path`, `mask`,
`filter`, `display`, `visibility`, `color`.

Example:

```json
[
	{
		"name": "fill",
		"description": "Defines the color used to paint the interior of the element.",
		"mdn_url": "https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/fill",
		"deprecated": false,
		"values": { "type": "color" },
		"elements": ["*"]
	},
	{
		"name": "stroke-linecap",
		"description": "Defines the shape to be used at the end of open subpaths.",
		"mdn_url": "https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/stroke-linecap",
		"deprecated": false,
		"values": { "type": "enum", "values": ["butt", "round", "square"] },
		"elements": ["*"]
	},
	{
		"name": "d",
		"description": "Defines a path to be drawn.",
		"mdn_url": "https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/d",
		"deprecated": false,
		"values": { "type": "path_data" },
		"elements": ["path"]
	}
]
```

- [ ] **Step 2: Commit**

```bash
git add crates/svg-data/data/
git commit -m "feat(svg-data): curated attribute catalog JSON"
```

---

### Task 4: Build script — generate catalog from JSON

**Files:**

- Create: `crates/svg-data/build.rs`
- Create: `crates/svg-data/src/catalog.rs`
- Create: `crates/svg-data/src/categories.rs`
- Modify: `crates/svg-data/Cargo.toml` (add build deps)
- Modify: `crates/svg-data/src/lib.rs`

- [ ] **Step 1: Add build dependencies**

In `crates/svg-data/Cargo.toml` (pinned inline, not workspace-inherited,
since serde is only needed at build time for this crate):

```toml
[build-dependencies]
serde      = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Write build.rs**

Create `crates/svg-data/build.rs` that:

1. Reads `data/elements.json` and `data/attributes.json`
2. Generates `src/catalog.rs` with:
   - `pub(crate) static ELEMENTS: &[ElementDef] = &[...]`
   - `pub(crate) static ATTRIBUTES: &[AttributeDef] = &[...]`
3. Maps JSON content model values to Rust `ContentModel` enum variants
4. Maps JSON attribute value types to Rust `AttributeValues` enum variants
5. Prints `cargo::rerun-if-changed=data/elements.json` and
   `cargo::rerun-if-changed=data/attributes.json`

The generated code should use `&'static` references throughout. For arrays,
generate static slices: `static RECT_ATTRS: &[&str] = &["x", "y", ...];`

```rust
use serde::Deserialize;
use std::io::Write;
use std::path::Path;
use std::{env, fs};

#[derive(Deserialize)]
struct JsonElement {
    name: String,
    description: String,
    mdn_url: String,
    deprecated: bool,
    content_model: serde_json::Value,
    required_attrs: Vec<String>,
    attrs: Vec<String>,
    global_attrs: bool,
}

#[derive(Deserialize)]
struct JsonAttribute {
    name: String,
    description: String,
    mdn_url: String,
    deprecated: bool,
    values: serde_json::Value,
    elements: Vec<String>,
}

fn main() {
    println!("cargo::rerun-if-changed=data/elements.json");
    println!("cargo::rerun-if-changed=data/attributes.json");

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("catalog.rs");

    let elements_json = fs::read_to_string("data/elements.json").expect("read data/elements.json");
    let elements: Vec<JsonElement> =
        serde_json::from_str(&elements_json).expect("parse elements.json");

    let attrs_json = fs::read_to_string("data/attributes.json").expect("read data/attributes.json");
    let attributes: Vec<JsonAttribute> =
        serde_json::from_str(&attrs_json).expect("parse attributes.json");

    let mut out = fs::File::create(&out_path).expect("create catalog.rs");

    // Generate element auxiliary slices
    for el in &elements {
        let upper = el.name.to_uppercase().replace('-', "_");
        writeln!(
            out,
            "static {upper}_REQUIRED: &[&str] = &[{}];",
            el.required_attrs
                .iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        writeln!(
            out,
            "static {upper}_ATTRS: &[&str] = &[{}];",
            el.attrs
                .iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        // Content model children categories
        if let Some(obj) = el.content_model.as_object() {
            if let Some(children) = obj.get("children") {
                let cats: Vec<String> = children
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| format!("ElementCategory::{}", v.as_str().unwrap()))
                    .collect();
                writeln!(
                    out,
                    "static {upper}_CHILDREN: &[ElementCategory] = &[{}];",
                    cats.join(", ")
                )
                .unwrap();
            }
        }
    }

    // Generate ELEMENTS array
    writeln!(out, "\npub(crate) static ELEMENTS: &[ElementDef] = &[").unwrap();
    for el in &elements {
        let upper = el.name.to_uppercase().replace('-', "_");
        let cm = match el.content_model.as_str() {
            Some("void") => "ContentModel::Void".to_string(),
            Some("text") => "ContentModel::Text".to_string(),
            _ => format!("ContentModel::Children({upper}_CHILDREN)"),
        };
        writeln!(out, "    ElementDef {{").unwrap();
        writeln!(out, "        name: \"{}\",", el.name).unwrap();
        writeln!(
            out,
            "        description: \"{}\",",
            el.description.replace('"', "\\\"")
        )
        .unwrap();
        writeln!(out, "        mdn_url: \"{}\",", el.mdn_url).unwrap();
        writeln!(out, "        deprecated: {},", el.deprecated).unwrap();
        writeln!(out, "        baseline: None,").unwrap(); // filled by compat overlay
        writeln!(out, "        content_model: {cm},").unwrap();
        writeln!(out, "        required_attrs: {upper}_REQUIRED,").unwrap();
        writeln!(out, "        attrs: {upper}_ATTRS,").unwrap();
        writeln!(out, "        global_attrs: {},", el.global_attrs).unwrap();
        writeln!(out, "    }},").unwrap();
    }
    writeln!(out, "];").unwrap();

    // Generate attribute auxiliary slices and ATTRIBUTES array (similar pattern)
    // ... (generate ATTRIBUTES array analogously)

    // Implementation for attributes follows the same pattern:
    // static slices for elements lists, then the ATTRIBUTES array.
    for attr in &attributes {
        let upper = attr.name.to_uppercase().replace('-', "_").replace(':', "_");
        writeln!(
            out,
            "static {upper}_ELEMENTS: &[&str] = &[{}];",
            attr.elements
                .iter()
                .map(|e| format!("\"{e}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        // Enum values if applicable
        if let Some(obj) = attr.values.as_object() {
            if obj.get("type").and_then(|t| t.as_str()) == Some("enum") {
                if let Some(vals) = obj.get("values").and_then(|v| v.as_array()) {
                    let items: Vec<String> = vals
                        .iter()
                        .map(|v| format!("\"{}\"", v.as_str().unwrap()))
                        .collect();
                    writeln!(
                        out,
                        "static {upper}_VALUES: &[&str] = &[{}];",
                        items.join(", ")
                    )
                    .unwrap();
                }
            }
            if obj.get("type").and_then(|t| t.as_str()) == Some("transform") {
                if let Some(fns) = obj.get("functions").and_then(|v| v.as_array()) {
                    let items: Vec<String> = fns
                        .iter()
                        .map(|v| format!("\"{}\"", v.as_str().unwrap()))
                        .collect();
                    writeln!(
                        out,
                        "static {upper}_FUNCTIONS: &[&str] = &[{}];",
                        items.join(", ")
                    )
                    .unwrap();
                }
            }
            if obj.get("type").and_then(|t| t.as_str()) == Some("preserve_aspect_ratio") {
                if let Some(aligns) = obj.get("alignments").and_then(|v| v.as_array()) {
                    let items: Vec<String> = aligns
                        .iter()
                        .map(|v| format!("\"{}\"", v.as_str().unwrap()))
                        .collect();
                    writeln!(
                        out,
                        "static {upper}_ALIGNMENTS: &[&str] = &[{}];",
                        items.join(", ")
                    )
                    .unwrap();
                }
                if let Some(mos) = obj.get("meet_or_slice").and_then(|v| v.as_array()) {
                    let items: Vec<String> = mos
                        .iter()
                        .map(|v| format!("\"{}\"", v.as_str().unwrap()))
                        .collect();
                    writeln!(
                        out,
                        "static {upper}_MEET_OR_SLICE: &[&str] = &[{}];",
                        items.join(", ")
                    )
                    .unwrap();
                }
            }
        }
    }

    writeln!(out, "\npub(crate) static ATTRIBUTES: &[AttributeDef] = &[").unwrap();
    for attr in &attributes {
        let upper = attr.name.to_uppercase().replace('-', "_").replace(':', "_");
        let values = if let Some(obj) = attr.values.as_object() {
            match obj.get("type").and_then(|t| t.as_str()) {
                Some("enum") => format!("AttributeValues::Enum({upper}_VALUES)"),
                Some("free_text") => "AttributeValues::FreeText".to_string(),
                Some("color") => "AttributeValues::Color".to_string(),
                Some("length") => "AttributeValues::Length".to_string(),
                Some("url") => "AttributeValues::Url".to_string(),
                Some("number_or_percentage") => "AttributeValues::NumberOrPercentage".to_string(),
                Some("transform") => format!("AttributeValues::Transform({upper}_FUNCTIONS)"),
                Some("viewbox") => "AttributeValues::ViewBox".to_string(),
                Some("preserve_aspect_ratio") => format!(
                    "AttributeValues::PreserveAspectRatio {{ alignments: {upper}_ALIGNMENTS, meet_or_slice: {upper}_MEET_OR_SLICE }}"
                ),
                Some("points") => "AttributeValues::Points".to_string(),
                Some("path_data") => "AttributeValues::PathData".to_string(),
                _ => "AttributeValues::FreeText".to_string(),
            }
        } else {
            "AttributeValues::FreeText".to_string()
        };

        writeln!(out, "    AttributeDef {{").unwrap();
        writeln!(out, "        name: \"{}\",", attr.name).unwrap();
        writeln!(
            out,
            "        description: \"{}\",",
            attr.description.replace('"', "\\\"")
        )
        .unwrap();
        writeln!(out, "        mdn_url: \"{}\",", attr.mdn_url).unwrap();
        writeln!(out, "        deprecated: {},", attr.deprecated).unwrap();
        writeln!(out, "        baseline: None,").unwrap();
        writeln!(out, "        values: {values},").unwrap();
        writeln!(out, "        elements: {upper}_ELEMENTS,").unwrap();
        writeln!(out, "    }},").unwrap();
    }
    writeln!(out, "];").unwrap();
}
```

- [ ] **Step 3: Create catalog.rs (include generated code)**

Create `crates/svg-data/src/catalog.rs`:

```rust
use crate::types::*;

include!(concat!(env!("OUT_DIR"), "/catalog.rs"));
```

- [ ] **Step 4: Create categories.rs**

Create `crates/svg-data/src/categories.rs`:

```rust
use crate::catalog::ELEMENTS;
use crate::types::{ContentModel, ElementCategory};

/// Return all element names that belong to the given category.
pub fn elements_in_category(cat: ElementCategory) -> Vec<&'static str> {
    // An element belongs to a category based on its nature:
    // Shape elements have Void content models and are shapes, etc.
    // For simplicity, we derive category membership from a static map.
    // This is populated from the catalog: any element whose content_model
    // includes a category as parent can be that category's child.
    match cat {
        ElementCategory::Container => vec![
            "svg", "g", "defs", "symbol", "marker", "clipPath", "mask", "pattern", "a",
        ],
        ElementCategory::Shape => vec![
            "rect", "circle", "ellipse", "line", "polyline", "polygon", "path",
        ],
        ElementCategory::Text => vec!["text", "tspan", "textPath"],
        ElementCategory::Gradient => vec!["linearGradient", "radialGradient", "stop"],
        ElementCategory::Filter => vec!["filter"],
        ElementCategory::Descriptive => vec!["title", "desc", "metadata"],
        ElementCategory::Structural => vec!["use", "image", "foreignObject", "switch"],
        ElementCategory::Animation => vec!["animate", "animateMotion", "animateTransform", "set"],
        ElementCategory::PaintServer => vec!["linearGradient", "radialGradient", "pattern"],
        ElementCategory::ClipMask => vec!["clipPath", "mask"],
        ElementCategory::LightSource => vec!["feDistantLight", "fePointLight", "feSpotLight"],
        ElementCategory::FilterPrimitive => vec![
            "feBlend",
            "feColorMatrix",
            "feComponentTransfer",
            "feComposite",
            "feConvolveMatrix",
            "feDiffuseLighting",
            "feDisplacementMap",
            "feFlood",
            "feGaussianBlur",
            "feImage",
            "feMerge",
            "feMorphology",
            "feOffset",
            "feSpecularLighting",
            "feTile",
            "feTurbulence",
        ],
    }
}

/// Return concrete element names allowed as children of `parent`.
pub fn allowed_children(parent: &str) -> Vec<&'static str> {
    let Some(el) = ELEMENTS.iter().find(|e| e.name == parent) else {
        return Vec::new();
    };
    match &el.content_model {
        ContentModel::Children(cats) => {
            let mut names: Vec<&'static str> = cats
                .iter()
                .flat_map(|cat| elements_in_category(*cat))
                .collect();
            names.sort_unstable();
            names.dedup();
            names
        }
        ContentModel::Void | ContentModel::Text => Vec::new(),
    }
}
```

- [ ] **Step 5: Update lib.rs with public API**

Replace `crates/svg-data/src/lib.rs`:

```rust
mod catalog;
pub mod categories;
pub mod types;

pub use types::{
    AttributeDef, AttributeValues, BaselineStatus, ContentModel, ElementCategory, ElementDef,
};

use catalog::{ATTRIBUTES, ELEMENTS};

/// Lookup an element definition by name.
pub fn element(name: &str) -> Option<&'static ElementDef> {
    ELEMENTS.iter().find(|e| e.name == name)
}

/// Lookup an attribute definition by name.
pub fn attribute(name: &str) -> Option<&'static AttributeDef> {
    ATTRIBUTES.iter().find(|a| a.name == name)
}

/// All known SVG element definitions.
pub fn elements() -> &'static [ElementDef] {
    ELEMENTS
}

/// All known SVG attribute definitions.
pub fn attributes() -> &'static [AttributeDef] {
    ATTRIBUTES
}

/// Concrete element names allowed as children of `parent`.
pub fn allowed_children(parent: &str) -> Vec<&'static str> {
    categories::allowed_children(parent)
}

/// All attributes valid for the given element (element-specific + globals).
pub fn attributes_for(element_name: &str) -> Vec<&'static AttributeDef> {
    let el = match element(element_name) {
        Some(e) => e,
        None => return Vec::new(),
    };
    let mut result: Vec<&'static AttributeDef> = Vec::new();
    for attr in ATTRIBUTES {
        let applies = attr.elements.contains(&"*") || attr.elements.contains(&element_name);
        if applies {
            result.push(attr);
        }
    }
    result
}

/// Map an ElementCategory to concrete element names.
pub fn elements_in_category(cat: ElementCategory) -> Vec<&'static str> {
    categories::elements_in_category(cat)
}
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p svg-data`
Expected: compiles clean

- [ ] **Step 7: Commit**

```bash
git add crates/svg-data/
git commit -m "feat(svg-data): build script + catalog codegen from JSON"
```

---

### Task 5: `svg-data` tests

**Files:**

- Modify: `crates/svg-data/src/lib.rs` (add tests module)

- [ ] **Step 1: Write tests**

Add to `crates/svg-data/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_lookup() {
        let rect = element("rect").expect("rect should exist");
        assert_eq!(rect.name, "rect");
        assert!(!rect.deprecated);
        assert!(matches!(rect.content_model, ContentModel::Void));
    }

    #[test]
    fn element_not_found() {
        assert!(element("notanelement").is_none());
    }

    #[test]
    fn text_content_model() {
        let text = element("text").expect("text should exist");
        assert!(matches!(text.content_model, ContentModel::Children(_)));
    }

    #[test]
    fn allowed_children_text() {
        let children = allowed_children("text");
        assert!(children.contains(&"tspan"), "text should allow tspan");
        assert!(!children.contains(&"rect"), "text should not allow rect");
    }

    #[test]
    fn allowed_children_void() {
        let children = allowed_children("rect");
        assert!(children.is_empty(), "void element should have no children");
    }

    #[test]
    fn attribute_lookup() {
        let fill = attribute("fill").expect("fill should exist");
        assert!(matches!(fill.values, AttributeValues::Color));
    }

    #[test]
    fn attribute_d_on_path() {
        let d = attribute("d").expect("d should exist");
        assert!(d.elements.contains(&"path"));
        assert!(matches!(d.values, AttributeValues::PathData));
    }

    #[test]
    fn attributes_for_rect() {
        let attrs = attributes_for("rect");
        let names: Vec<&str> = attrs.iter().map(|a| a.name).collect();
        assert!(names.contains(&"fill"), "rect should accept fill");
        assert!(names.contains(&"x"), "rect should accept x");
        assert!(!names.contains(&"d"), "rect should not accept d");
    }

    #[test]
    fn elements_in_shape_category() {
        let shapes = elements_in_category(ElementCategory::Shape);
        assert!(shapes.contains(&"rect"));
        assert!(shapes.contains(&"circle"));
        assert!(shapes.contains(&"path"));
        assert!(!shapes.contains(&"g"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p svg-data`
Expected: all tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/svg-data/
git commit -m "test(svg-data): catalog lookup and content model tests"
```

---

### Task 6: Scaffold `svg-lint` crate with types

**Files:**

- Create: `crates/svg-lint/Cargo.toml`
- Create: `crates/svg-lint/src/lib.rs`
- Create: `crates/svg-lint/src/types.rs`
- Modify: `Cargo.toml` (workspace deps)

- [ ] **Step 1: Create crate**

```bash
mkdir -p crates/svg-lint/src
```

`crates/svg-lint/Cargo.toml`:

```toml
[package]
name                  = "svg-lint"
version               = "0.1.0"
edition.workspace     = true
description.workspace = true
readme.workspace      = true
repository.workspace  = true

[dependencies]
svg-data.workspace        = true
tree-sitter.workspace     = true
tree-sitter-svg.workspace = true
```

Add to root `Cargo.toml` `[workspace.dependencies]`:

```toml
[workspace.dependencies.svg-lint]
path = "crates/svg-lint"
```

- [ ] **Step 2: Define diagnostic types**

Create `crates/svg-lint/src/types.rs`:

```rust
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub struct SvgDiagnostic {
    pub byte_range: Range<usize>,
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    InvalidChild,
    MissingRequiredAttr,
    DeprecatedElement,
    DeprecatedAttribute,
    UnknownElement,
    UnknownAttribute,
    DuplicateId,
}
```

- [ ] **Step 3: Create lib.rs**

Create `crates/svg-lint/src/lib.rs`:

```rust
pub mod types;

pub use types::{DiagnosticCode, Severity, SvgDiagnostic};
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p svg-lint`
Expected: compiles clean

- [ ] **Step 5: Commit**

```bash
git add crates/svg-lint/ Cargo.toml
git commit -m "feat(svg-lint): scaffold crate with diagnostic types"
```

---

### Task 7: Implement lint rules (TDD)

**Files:**

- Create: `crates/svg-lint/src/rules.rs`
- Modify: `crates/svg-lint/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Add to `crates/svg-lint/src/lib.rs`:

```rust
mod rules;

use tree_sitter::Parser;

/// Parse source and lint.
pub fn lint(source: &[u8]) -> Vec<SvgDiagnostic> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_svg::LANGUAGE.into())
        .expect("failed to load SVG grammar");
    let tree = parser.parse(source, None).expect("failed to parse");
    lint_tree(source, &tree)
}

/// Lint an already-parsed tree.
pub fn lint_tree(source: &[u8], tree: &tree_sitter::Tree) -> Vec<SvgDiagnostic> {
    rules::check_all(source, tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_svg_no_diagnostics() {
        let src = br#"<svg><rect x="0" y="0" width="10" height="10"/></svg>"#;
        let diags = lint(src);
        assert!(
            diags.is_empty(),
            "valid SVG should have no diagnostics: {diags:?}"
        );
    }

    #[test]
    fn invalid_child_text_in_tspan() {
        let src = br#"<svg><tspan><text>hello</text></tspan></svg>"#;
        let diags = lint(src);
        assert!(
            diags.iter().any(|d| d.code == DiagnosticCode::InvalidChild),
            "tspan containing text should be InvalidChild: {diags:?}"
        );
    }

    #[test]
    fn unknown_element() {
        let src = br#"<svg><banana/></svg>"#;
        let diags = lint(src);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::UnknownElement),
            "unknown element should be flagged: {diags:?}"
        );
    }

    #[test]
    fn deprecated_element() {
        // Add a deprecated element to the catalog (e.g. cursor) for this test
        // If cursor is not in the catalog yet, this test documents the intent
        let src = br#"<svg><cursor/></svg>"#;
        let diags = lint(src);
        // Either deprecated or unknown — both are acceptable for now
        assert!(
            !diags.is_empty(),
            "deprecated element should produce a diagnostic"
        );
    }

    #[test]
    fn duplicate_id() {
        let src = br#"<svg><rect id="a"/><rect id="a"/></svg>"#;
        let diags = lint(src);
        assert!(
            diags.iter().any(|d| d.code == DiagnosticCode::DuplicateId),
            "duplicate ids should be flagged: {diags:?}"
        );
    }

    #[test]
    fn rect_in_svg_is_valid() {
        let src = br#"<svg><rect/></svg>"#;
        let diags = lint(src);
        let invalid_child = diags.iter().any(|d| d.code == DiagnosticCode::InvalidChild);
        assert!(!invalid_child, "rect in svg should be valid: {diags:?}");
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p svg-lint`
Expected: FAIL (compilation error — `rules` module doesn't exist yet)

- [ ] **Step 3: Implement rules**

Create `crates/svg-lint/src/rules.rs`:

```rust
use crate::types::{DiagnosticCode, Severity, SvgDiagnostic};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub fn check_all(source: &[u8], tree: &Tree) -> Vec<SvgDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut id_map: HashMap<String, usize> = HashMap::new();

    walk_elements(source, tree.root_node(), &mut diagnostics, &mut id_map);

    diagnostics
}

fn walk_elements(
    source: &[u8],
    node: Node,
    diagnostics: &mut Vec<SvgDiagnostic>,
    id_map: &mut HashMap<String, usize>,
) {
    let kind = node.kind();

    if kind == "element" || kind == "svg_root_element" {
        check_element(source, node, diagnostics, id_map);
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_elements(source, child, diagnostics, id_map);
    }
}

fn check_element(
    source: &[u8],
    node: Node,
    diagnostics: &mut Vec<SvgDiagnostic>,
    id_map: &mut HashMap<String, usize>,
) {
    // Find the tag node (start_tag or self_closing_tag)
    let mut tag_cursor = node.walk();
    let tag_node = node
        .children(&mut tag_cursor)
        .find(|c| c.kind() == "start_tag" || c.kind() == "self_closing_tag");

    let Some(tag) = tag_node else { return };

    // Extract element name
    let name_node = tag.child_by_field_name("name");
    let Some(name_node) = name_node else { return };
    let name = &source[name_node.byte_range()];
    let name_str = std::str::from_utf8(name).unwrap_or("");

    // Check 1: Unknown element
    let element_def = svg_data::element(name_str);
    if element_def.is_none() {
        diagnostics.push(SvgDiagnostic {
            byte_range: name_node.byte_range(),
            start_row: name_node.start_position().row,
            start_col: name_node.start_position().column,
            end_row: name_node.end_position().row,
            end_col: name_node.end_position().column,
            severity: Severity::Warning,
            code: DiagnosticCode::UnknownElement,
            message: format!("Unknown SVG element: <{name_str}>"),
        });
        return; // Can't check further without a definition
    }

    let Some(def) = element_def else { return };

    // Check 2: Deprecated element
    if def.deprecated {
        diagnostics.push(SvgDiagnostic {
            byte_range: name_node.byte_range(),
            start_row: name_node.start_position().row,
            start_col: name_node.start_position().column,
            end_row: name_node.end_position().row,
            end_col: name_node.end_position().column,
            severity: Severity::Warning,
            code: DiagnosticCode::DeprecatedElement,
            message: format!("<{name_str}> is deprecated"),
        });
    }

    // Check 3: Duplicate id
    check_duplicate_id(source, tag, name_str, diagnostics, id_map);

    // Check 4: Invalid children
    check_children(source, node, name_str, diagnostics);
}

fn check_duplicate_id(
    source: &[u8],
    tag: Node,
    _element_name: &str,
    diagnostics: &mut Vec<SvgDiagnostic>,
    id_map: &mut HashMap<String, usize>,
) {
    // Walk attributes looking for id
    let mut cursor = tag.walk();
    for attr_node in tag.children(&mut cursor) {
        if attr_node.kind() != "attribute" {
            continue;
        }

        // Look for id_attribute specifically
        let mut attr_cursor = attr_node.walk();
        for child in attr_node.children(&mut attr_cursor) {
            if child.kind() == "id_attribute" {
                // Get the id value
                if let Some(value_node) = child.child_by_field_name("value") {
                    // id_attribute_value contains id_token
                    let mut vc = value_node.walk();
                    for v in value_node.children(&mut vc) {
                        if v.kind() == "id_token" {
                            let id_text =
                                std::str::from_utf8(&source[v.byte_range()]).unwrap_or("");
                            if let Some(&first_line) = id_map.get(id_text) {
                                diagnostics.push(SvgDiagnostic {
                                    byte_range: v.byte_range(),
                                    start_row: v.start_position().row,
                                    start_col: v.start_position().column,
                                    end_row: v.end_position().row,
                                    end_col: v.end_position().column,
                                    severity: Severity::Warning,
                                    code: DiagnosticCode::DuplicateId,
                                    message: format!(
                                        "Duplicate id \"{id_text}\" (first defined on line {})",
                                        first_line + 1
                                    ),
                                });
                            } else {
                                id_map.insert(id_text.to_string(), v.start_position().row);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn check_children(
    source: &[u8],
    parent_node: Node,
    parent_name: &str,
    diagnostics: &mut Vec<SvgDiagnostic>,
) {
    let allowed = svg_data::allowed_children(parent_name);
    if allowed.is_empty() {
        // Void or text element — check handled by content model
        return;
    }

    let mut cursor = parent_node.walk();
    for child in parent_node.children(&mut cursor) {
        if child.kind() != "element" {
            continue;
        }

        // Get child element name
        let mut child_cursor = child.walk();
        let child_tag = child
            .children(&mut child_cursor)
            .find(|c| c.kind() == "start_tag" || c.kind() == "self_closing_tag");
        let Some(ct) = child_tag else { continue };
        let Some(cn) = ct.child_by_field_name("name") else {
            continue;
        };
        let child_name = std::str::from_utf8(&source[cn.byte_range()]).unwrap_or("");

        if !allowed.contains(&child_name) {
            diagnostics.push(SvgDiagnostic {
                byte_range: cn.byte_range(),
                start_row: cn.start_position().row,
                start_col: cn.start_position().column,
                end_row: cn.end_position().row,
                end_col: cn.end_position().column,
                severity: Severity::Error,
                code: DiagnosticCode::InvalidChild,
                message: format!("<{child_name}> is not allowed as a child of <{parent_name}>"),
            });
        }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p svg-lint`
Expected: all tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/svg-lint/
git commit -m "feat(svg-lint): diagnostic rules with TDD"
```

---

### Task 8: Refactor LSP server — shared tree + diagnostics

**Files:**

- Modify: `crates/svg-language-server/Cargo.toml`
- Modify: `crates/svg-language-server/src/main.rs`

- [ ] **Step 1: Add dependencies**

In `crates/svg-language-server/Cargo.toml`, add:

```toml
svg-data.workspace = true
svg-lint.workspace = true
```

- [ ] **Step 2: Refactor document storage to DocumentState**

In `main.rs`, replace `HashMap<Uri, String>` with `HashMap<Uri, DocumentState>`:

```rust
use tree_sitter::Parser;

struct DocumentState {
    source: String,
    tree: tree_sitter::Tree,
}

struct SvgLanguageServer {
    client: Client, // rename from _client
    documents: Arc<RwLock<HashMap<Uri, DocumentState>>>,
    parser: Arc<RwLock<Parser>>,
    color_kinds: ColorKindCache,
}
```

Update `new()`:

```rust
fn new(client: Client) -> Self {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_svg::LANGUAGE.into())
        .expect("failed to load SVG grammar");
    Self {
        client,
        documents: Arc::new(RwLock::new(HashMap::new())),
        parser: Arc::new(RwLock::new(parser)),
        color_kinds: Arc::new(RwLock::new(HashMap::new())),
    }
}
```

- [ ] **Step 3: Update did_open / did_change to parse + publish diagnostics**

```rust
async fn did_open(&self, params: DidOpenTextDocumentParams) {
    let uri = params.text_document.uri;
    let source = params.text_document.text;
    self.update_document(uri, source).await;
}

async fn did_change(&self, params: DidChangeTextDocumentParams) {
    if let Some(change) = params.content_changes.into_iter().last() {
        let uri = params.text_document.uri;
        self.update_document(uri, change.text).await;
    }
}
```

Add helper method on `SvgLanguageServer`:

```rust
async fn update_document(&self, uri: Uri, source: String) {
    let tree = {
        let mut parser = self.parser.write().await;
        parser
            .parse(source.as_bytes(), None)
            .expect("failed to parse")
    };

    // Run diagnostics
    let diags = svg_lint::lint_tree(source.as_bytes(), &tree);
    let source_bytes = source.as_bytes();
    let lsp_diags: Vec<Diagnostic> = diags
        .into_iter()
        .map(|d| Diagnostic {
            range: Range::new(
                Position::new(
                    d.start_row as u32,
                    byte_col_to_utf16(source_bytes, d.start_row, d.start_col),
                ),
                Position::new(
                    d.end_row as u32,
                    byte_col_to_utf16(source_bytes, d.end_row, d.end_col),
                ),
            ),
            severity: Some(match d.severity {
                svg_lint::Severity::Error => DiagnosticSeverity::ERROR,
                svg_lint::Severity::Warning => DiagnosticSeverity::WARNING,
                svg_lint::Severity::Information => DiagnosticSeverity::INFORMATION,
                svg_lint::Severity::Hint => DiagnosticSeverity::HINT,
            }),
            source: Some("svg".to_string()),
            message: d.message,
            ..Default::default()
        })
        .collect();

    self.client
        .publish_diagnostics(uri.clone(), lsp_diags, None)
        .await;

    // Store document
    self.documents
        .write()
        .await
        .insert(uri, DocumentState { source, tree });
}
```

- [ ] **Step 4: Update document_color to use shared tree**

```rust
async fn document_color(&self, params: DocumentColorParams) -> Result<Vec<ColorInformation>> {
    let docs = self.documents.read().await;
    let Some(doc) = docs.get(&params.text_document.uri) else {
        return Ok(Vec::new());
    };
    let source_bytes = doc.source.as_bytes();
    let colors = svg_color::extract_colors_from_tree(source_bytes, &doc.tree);

    // Preserve existing color_kinds cache logic
    let mut kinds = self.color_kinds.write().await;
    kinds.retain(|(uri, _, _), _| *uri != params.text_document.uri);

    let result = colors
        .into_iter()
        .map(|c| {
            let start_char = byte_col_to_utf16(source_bytes, c.start_row, c.start_col);
            let end_char = byte_col_to_utf16(source_bytes, c.end_row, c.end_col);

            kinds.insert(
                (
                    params.text_document.uri.clone(),
                    c.start_row as u32,
                    start_char,
                ),
                c.kind,
            );

            ColorInformation {
                range: Range::new(
                    Position::new(c.start_row as u32, start_char),
                    Position::new(c.end_row as u32, end_char),
                ),
                color: Color {
                    red: c.r,
                    green: c.g,
                    blue: c.b,
                    alpha: c.a,
                },
            }
        })
        .collect();

    Ok(result)
}
```

- [ ] **Step 5: Update did_close**

```rust
async fn did_close(&self, params: DidCloseTextDocumentParams) {
    self.documents
        .write()
        .await
        .remove(&params.text_document.uri);
    // Clear diagnostics for closed document
    self.client
        .publish_diagnostics(params.text_document.uri, vec![], None)
        .await;
}
```

- [ ] **Step 6: Add necessary imports**

Add to the import block:

```rust
use tower_lsp_server::ls_types::{Diagnostic, DiagnosticSeverity /* existing imports... */};
```

- [ ] **Step 7: Verify compilation**

Run: `cargo check -p svg-language-server`
Expected: compiles clean

- [ ] **Step 8: Commit**

```bash
git add crates/svg-language-server/
git commit -m "feat(lsp): shared tree lifecycle + diagnostics publishing"
```

---

### Task 9: Implement hover

**Files:**

- Modify: `crates/svg-language-server/src/main.rs`

- [ ] **Step 1: Advertise hover capability**

In `initialize`, add to `ServerCapabilities`:

```rust
hover_provider: Some(HoverProviderCapability::Simple(true)),
```

- [ ] **Step 2: Implement hover handler**

Add the `hover` method to the `LanguageServer` impl:

```rust
async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    let docs = self.documents.read().await;
    let Some(doc) = docs.get(&params.text_document_position_params.text_document.uri) else {
        return Ok(None);
    };

    let pos = params.text_document_position_params.position;
    let source_bytes = doc.source.as_bytes();

    // Convert LSP position (UTF-16) to tree-sitter point (byte offset)
    let byte_col = utf16_to_byte_col(source_bytes, pos.line as usize, pos.character as u32);
    let point = tree_sitter::Point::new(pos.line as usize, byte_col);

    let root = doc.tree.root_node();
    let Some(node) = root.descendant_for_point_range(point, point) else {
        return Ok(None);
    };

    // Determine what we're hovering over
    let hover_content = if node.kind() == "name" || node.kind() == "attribute_name" {
        // Element name — check parent is a tag
        if let Some(parent) = node.parent() {
            let parent_kind = parent.kind();
            if parent_kind == "start_tag"
                || parent_kind == "end_tag"
                || parent_kind == "self_closing_tag"
            {
                let name = std::str::from_utf8(&source_bytes[node.byte_range()]).unwrap_or("");
                format_element_hover(name)
            } else {
                None
            }
        } else {
            None
        }
    } else if is_attribute_name_node(node) {
        // Typed attribute name nodes: paint_attribute_name, length_attribute_name, etc.
        let name = std::str::from_utf8(&source_bytes[node.byte_range()]).unwrap_or("");
        format_attribute_hover(name)
    } else {
        None
    };

    Ok(hover_content.map(|content| Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: Some(Range::new(
            Position::new(
                node.start_position().row as u32,
                byte_col_to_utf16(
                    source_bytes,
                    node.start_position().row,
                    node.start_position().column,
                ),
            ),
            Position::new(
                node.end_position().row as u32,
                byte_col_to_utf16(
                    source_bytes,
                    node.end_position().row,
                    node.end_position().column,
                ),
            ),
        )),
    }))
}
```

- [ ] **Step 3: Add hover formatting functions**

```rust
// Verified node kinds against tree-sitter-svg (refactor/structural-tiers).
// Generic attrs use "attribute_name"; there is no "generic_attribute_name".
fn is_attribute_name_node(node: tree_sitter::Node) -> bool {
    let kind = node.kind();
    kind == "attribute_name"
        || kind == "paint_attribute_name"
        || kind == "length_attribute_name"
        || kind == "transform_attribute_name"
        || kind == "viewbox_attribute_name"
        || kind == "id_attribute_name"
}

fn format_element_hover(name: &str) -> Option<String> {
    let def = svg_data::element(name)?;
    let mut parts = Vec::new();

    if def.deprecated {
        parts.push(format!("~~{}~~", def.description));
        parts.push(String::new());
        parts.push("⚠️ **Deprecated**".to_string());
    } else {
        parts.push(def.description.to_string());
    }

    if let Some(baseline) = &def.baseline {
        parts.push(String::new());
        parts.push(format_baseline(baseline));
    }

    parts.push(String::new());
    parts.push(format!("[MDN Reference]({})", def.mdn_url));

    Some(parts.join("\n"))
}

fn format_attribute_hover(name: &str) -> Option<String> {
    let def = svg_data::attribute(name)?;
    let mut parts = Vec::new();

    if def.deprecated {
        parts.push(format!("~~{}~~", def.description));
        parts.push(String::new());
        parts.push("⚠️ **Deprecated**".to_string());
    } else {
        parts.push(def.description.to_string());
    }

    // Show allowed values for enum attributes
    if let svg_data::AttributeValues::Enum(vals) = &def.values {
        parts.push(String::new());
        parts.push(format!("Values: `{}`", vals.join("` | `")));
    }

    if let Some(baseline) = &def.baseline {
        parts.push(String::new());
        parts.push(format_baseline(baseline));
    }

    parts.push(String::new());
    parts.push(format!("[MDN Reference]({})", def.mdn_url));

    Some(parts.join("\n"))
}

fn format_baseline(status: &svg_data::BaselineStatus) -> String {
    match status {
        svg_data::BaselineStatus::Widely { since } => {
            format!("◇◇ Widely available across major browsers (Baseline since {since})")
        }
        svg_data::BaselineStatus::Newly { since } => {
            format!("◇ Newly available across major browsers (Baseline since {since})")
        }
        svg_data::BaselineStatus::Limited => {
            "Limited availability across major browsers".to_string()
        }
    }
}
```

- [ ] **Step 4: Add utf16_to_byte_col helper**

```rust
/// Convert a UTF-16 code unit column to a byte offset within a given row.
/// Inverse of `byte_col_to_utf16`.
fn utf16_to_byte_col(source: &[u8], row: usize, utf16_col: u32) -> usize {
    let line_start: usize = source
        .split(|&b| b == b'\n')
        .take(row)
        .map(|line| line.len() + 1)
        .sum();

    let line_end = source[line_start..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(source.len(), |p| line_start + p);

    let line_bytes = &source[line_start..line_end];
    let line_str = String::from_utf8_lossy(line_bytes);

    let mut utf16_count = 0u32;
    let mut byte_offset = 0usize;
    for ch in line_str.chars() {
        if utf16_count >= utf16_col {
            break;
        }
        utf16_count += ch.len_utf16() as u32;
        byte_offset += ch.len_utf8();
    }

    byte_offset
}
```

- [ ] **Step 5: Add hover-related imports**

Add to imports:

```rust
use tower_lsp_server::ls_types::{
    Hover,
    HoverContents,
    HoverParams,
    HoverProviderCapability,
    MarkupContent,
    MarkupKind,
    // ... existing imports
};
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p svg-language-server`
Expected: compiles clean

- [ ] **Step 7: Commit**

```bash
git add crates/svg-language-server/
git commit -m "feat(lsp): hover documentation for elements and attributes"
```

---

### Task 10: Implement completions

**Files:**

- Modify: `crates/svg-language-server/src/main.rs`

- [ ] **Step 1: Advertise completion capability**

In `initialize`, add to `ServerCapabilities`:

```rust
completion_provider: Some(CompletionOptions {
    trigger_characters: Some(vec![
        "<".to_string(),
        " ".to_string(),
        "\"".to_string(),
        "'".to_string(),
    ]),
    ..Default::default()
}),
```

- [ ] **Step 2: Implement completion handler**

```rust
async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
    let docs = self.documents.read().await;
    let Some(doc) = docs.get(&params.text_document_position.text_document.uri) else {
        return Ok(None);
    };

    let pos = params.text_document_position.position;
    let source_bytes = doc.source.as_bytes();
    let byte_col = utf16_to_byte_col(source_bytes, pos.line as usize, pos.character as u32);
    let point = tree_sitter::Point::new(pos.line as usize, byte_col);

    let root = doc.tree.root_node();
    let Some(node) = root.descendant_for_point_range(point, point) else {
        return Ok(None);
    };

    let items = match detect_completion_context(node, source_bytes) {
        CompletionContext::ElementName { parent } => complete_elements(&parent),
        CompletionContext::AttributeName { element } => complete_attributes(&element),
        CompletionContext::AttributeValue { attribute } => complete_attribute_values(&attribute),
        CompletionContext::None => return Ok(None),
    };

    Ok(Some(CompletionResponse::Array(items)))
}
```

- [ ] **Step 3: Implement context detection**

```rust
enum CompletionContext {
    ElementName { parent: String },
    AttributeName { element: String },
    AttributeValue { attribute: String },
    None,
}

fn detect_completion_context(node: tree_sitter::Node, source: &[u8]) -> CompletionContext {
    let mut current = node;
    loop {
        let kind = current.kind();

        // Inside a start_tag or self_closing_tag → attribute completion
        if kind == "start_tag" || kind == "self_closing_tag" {
            if let Some(name_node) = current.child_by_field_name("name") {
                let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
                return CompletionContext::AttributeName {
                    element: name.to_string(),
                };
            }
        }

        // Inside an attribute value → value completion
        if kind.ends_with("_attribute_value") || kind == "quoted_attribute_value" {
            // Walk up to find the attribute name
            if let Some(attr_parent) = current.parent() {
                if let Some(name_node) = attr_parent.child_by_field_name("name") {
                    let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
                    return CompletionContext::AttributeValue {
                        attribute: name.to_string(),
                    };
                }
            }
        }

        // Inside an element body (between tags) → element completion
        if kind == "element" || kind == "svg_root_element" {
            if let Some(tag) = current
                .children(&mut current.walk())
                .find(|c| c.kind() == "start_tag" || c.kind() == "self_closing_tag")
            {
                if let Some(name_node) = tag.child_by_field_name("name") {
                    let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
                    return CompletionContext::ElementName {
                        parent: name.to_string(),
                    };
                }
            }
        }

        // Walk up
        match current.parent() {
            Some(parent) => current = parent,
            None => return CompletionContext::None,
        }
    }
}
```

- [ ] **Step 4: Implement completion generators**

```rust
fn complete_elements(parent: &str) -> Vec<CompletionItem> {
    let allowed = svg_data::allowed_children(parent);
    if allowed.is_empty() {
        // If no content model or unknown parent, show all elements
        return svg_data::elements()
            .iter()
            .map(|def| element_completion_item(def))
            .collect();
    }
    allowed
        .iter()
        .filter_map(|name| svg_data::element(name).map(element_completion_item))
        .collect()
}

fn element_completion_item(def: &svg_data::ElementDef) -> CompletionItem {
    let insert = match def.content_model {
        svg_data::ContentModel::Void => format!("{} />", def.name),
        _ => format!("{}>$0</{}>", def.name, def.name),
    };
    CompletionItem {
        label: def.name.to_string(),
        kind: Some(CompletionItemKind::PROPERTY), // closest to "element"
        detail: Some(def.description.to_string()),
        deprecated: if def.deprecated { Some(true) } else { None },
        insert_text: Some(insert),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

fn complete_attributes(element: &str) -> Vec<CompletionItem> {
    svg_data::attributes_for(element)
        .into_iter()
        .map(|def| CompletionItem {
            label: def.name.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(def.description.to_string()),
            deprecated: if def.deprecated { Some(true) } else { None },
            insert_text: Some(format!("{}=\"$0\"", def.name)),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        })
        .collect()
}

fn complete_attribute_values(attribute: &str) -> Vec<CompletionItem> {
    let Some(def) = svg_data::attribute(attribute) else {
        return Vec::new();
    };
    match &def.values {
        svg_data::AttributeValues::Enum(vals) => vals
            .iter()
            .map(|v| CompletionItem {
                label: v.to_string(),
                kind: Some(CompletionItemKind::VALUE),
                ..Default::default()
            })
            .collect(),
        svg_data::AttributeValues::Transform(funcs) => funcs
            .iter()
            .map(|f| CompletionItem {
                label: f.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                insert_text: Some(format!("{f}($0)")),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            })
            .collect(),
        svg_data::AttributeValues::PreserveAspectRatio {
            alignments,
            meet_or_slice,
        } => {
            let mut items: Vec<CompletionItem> = alignments
                .iter()
                .map(|a| CompletionItem {
                    label: a.to_string(),
                    kind: Some(CompletionItemKind::VALUE),
                    ..Default::default()
                })
                .collect();
            items.extend(meet_or_slice.iter().map(|m| CompletionItem {
                label: m.to_string(),
                kind: Some(CompletionItemKind::VALUE),
                ..Default::default()
            }));
            items
        }
        // For Color, Length, etc. — no enumerable values to complete
        _ => Vec::new(),
    }
}
```

- [ ] **Step 5: Add completion-related imports**

```rust
use tower_lsp_server::ls_types::{
    CompletionItem,
    CompletionItemKind,
    CompletionOptions,
    CompletionParams,
    CompletionResponse,
    InsertTextFormat,
    // ... existing imports
};
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p svg-language-server`
Expected: compiles clean

- [ ] **Step 7: Commit**

```bash
git add crates/svg-language-server/
git commit -m "feat(lsp): context-aware completions for elements, attributes, values"
```

---

### Task 11: Integration tests for new LSP features

**Files:**

- Modify: `crates/svg-language-server/tests/integration.rs`

- [ ] **Step 1: Add hover integration test**

Add to the existing integration test file, after the color presentation test
and before shutdown:

```rust
    // --- hover test: element name ---
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": "file:///test.svg" },
                "position": { "line": 0, "character": 7 }  // over "rect"
            }
        }),
    );

    let hover_resp = recv_response(&rx, 10, timeout);
    let hover_result = &hover_resp["result"];
    assert!(hover_result.get("contents").is_some(), "hover should return contents");
    let hover_value = hover_result["contents"]["value"].as_str().unwrap_or("");
    assert!(hover_value.contains("MDN Reference"), "hover should contain MDN link: {hover_value}");
```

- [ ] **Step 2: Add completion integration test**

```rust
    // --- completion test: after < ---
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": "file:///test.svg" },
                "position": { "line": 0, "character": 5 }  // inside <svg>, after <
            }
        }),
    );

    let comp_resp = recv_response(&rx, 11, timeout);
    let comp_items = comp_resp["result"]
        .as_array()
        .expect("completion result should be an array");
    assert!(!comp_items.is_empty(), "should return completion items");
    let labels: Vec<&str> = comp_items.iter()
        .filter_map(|i| i["label"].as_str())
        .collect();
    assert!(labels.contains(&"rect"), "completions should include rect: {labels:?}");
```

- [ ] **Step 3: Add diagnostics integration test**

Diagnostics are push-based notifications (no request id). The `recv_response`
helper skips notifications, so we must drain `rx` directly. **Important:**
`publishDiagnostics` for `test.svg` may arrive after `didOpen` (from Step 1),
so drain any queued notifications before opening the invalid file.

```rust
    // --- diagnostics test ---
    // Drain any buffered notifications (e.g. diagnostics for test.svg)
    while rx.try_recv().is_ok() {}

    // Open a file with invalid nesting to trigger diagnostics
    let invalid_svg = r##"<svg><tspan><text>hello</text></tspan></svg>"##;
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///invalid.svg",
                    "languageId": "svg",
                    "version": 1,
                    "text": invalid_svg
                }
            }
        }),
    );

    // Read messages until we find publishDiagnostics for invalid.svg
    let diag_deadline = std::time::Instant::now() + timeout;
    let mut found_diags = false;
    while std::time::Instant::now() < diag_deadline {
        let remaining = diag_deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(msg) => {
                if msg.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics") {
                    let params = &msg["params"];
                    if params["uri"].as_str() == Some("file:///invalid.svg") {
                        let diags = params["diagnostics"].as_array()
                            .expect("diagnostics should be array");
                        assert!(!diags.is_empty(),
                            "invalid SVG should produce diagnostics: {diags:?}");
                        found_diags = true;
                        break;
                    }
                }
                // Skip other messages (responses, other notifications)
            }
            Err(_) => break,
        }
    }
    assert!(found_diags, "should have received publishDiagnostics notification");
```

- [ ] **Step 4: Run integration tests**

Run: `cargo test -p svg-language-server --test integration`
Expected: all tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/svg-language-server/tests/
git commit -m "test(lsp): integration tests for hover, completion, diagnostics"
```

---

### Task 12: Final verification and cleanup

**Files:**

- Modify: `README.md`

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings. Fix any issues.

- [ ] **Step 3: Update README**

Add new features to `README.md`:

```markdown
## Features

- `textDocument/documentColor` — color swatches for paint attributes
- `textDocument/colorPresentation` — convert between hex, rgb(), hsl(), named
- `textDocument/hover` — element/attribute documentation with MDN links and baseline status
- `textDocument/completion` — context-aware completions for elements, attributes, and values
- `textDocument/publishDiagnostics` — structural validation (invalid nesting, unknown elements, deprecated usage, duplicate IDs)
```

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: update README with intelligence features"
```

---

## Deferred to Follow-Up Tasks

- **CompatOverlay / runtime refresh**: The spec describes fetching latest
  `@mdn/browser-compat-data` at runtime. This plan bakes in `baseline: None`
  and defers runtime fetch to a follow-up task — the core features work
  without it, and adding HTTP + caching is orthogonal to the catalog/lint/LSP work.
- **Close-tag completion (`</│`)**: The spec lists this as a trigger point.
  This plan's `detect_completion_context` does not handle `end_tag` nodes.
  Add as a follow-up — the tree-sitter parent element name is available.
- **`elements_in_category` return type**: Spec says `&'static [&'static str]`
  but plan uses `Vec` for simplicity. Can be optimized to static slices
  in a follow-up (would require codegen or `once_cell`).

---

## Verification

After all tasks complete:

1. `cargo test --workspace` — all tests pass
2. `cargo clippy --workspace -- -D warnings` — no warnings
3. `cargo build --release -p svg-language-server` — release binary builds
4. Manual test in Zed:
   - Open SVG with `<rect fill="#ff0000"/>` — color swatch + hover docs
   - Type `<` inside `<svg>` — element completion list
   - Type `fill="` — attribute value completions
   - Write `<tspan><text>` — diagnostic squiggly on `<text>`
   - Hover `<rect>` — description + baseline + MDN link
