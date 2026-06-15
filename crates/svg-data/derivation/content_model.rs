//! Derive SVG 2 element content models from the vendored `definitions*.xml`.
//!
//! This is the single source of truth for the child-content model of every
//! element present in the SVG 2 Editor's Draft. It is shared (via `#[path]`)
//! by the regeneration example (`examples/derive_content_models.rs`) and the
//! reproducibility gate (`tests/content_model_spec_derived.rs`) so the
//! committed `data/specs/Svg2EditorsDraft/elements.json` content models can
//! never silently drift from the spec.
//!
//! The svgwg `definitions*.xml` files are self-contained: `<elementcategory>`
//! tags define each content-model category's member elements, and every
//! `<element>` declares its `contentmodel` plus the child `elementcategories`
//! and explicit child `elements` it accepts. The spec's `anyof` /
//! `textoranyof` model is the *union* of those categories and explicit
//! elements, so we resolve each category through the spec's own membership
//! table — never through `svg_data::ElementCategory`, whose grouping is an
//! internal taxonomy that deliberately differs from the spec's.
//!
//! The result is a flattened, self-describing `ElementSet` per element that is
//! immune to category-membership drift. Four elements carry prose content
//! models (`<x:contentmodel>` instead of machine-readable attributes) and are
//! encoded from the spec text in [`prose_override`].

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use quick_xml::{Reader, events::Event};
use svg_data::snapshot_schema::ElementContentModel;

/// The five vendored svgwg definition files. Element definitions and category
/// memberships are spread across all of them (filter primitives live in
/// `-filters`, animation elements in `-animations`, etc.). They are read from
/// the vendored directory resolved from the active pin (see [`svgwg_master_dir`])
/// rather than `include_str!`ed, so re-vendoring at a new commit follows the
/// pin automatically without touching this file.
const DEFINITION_FILES: &[&str] = &[
    "definitions.xml",
    "definitions-filters.xml",
    "definitions-masking.xml",
    "definitions-compositing.xml",
    "definitions-animations.xml",
];

/// One element's raw content-model declaration as scraped from a single
/// `<element>` start tag.
struct RawElement {
    /// `contentmodel` attribute (`anyof`, `textoranyof`, `any`, `text`,
    /// `empty`), or `None` when the element uses a prose `<x:contentmodel>`.
    content_model: Option<String>,
    /// Child element categories the element accepts (`elementcategories`).
    categories: Vec<String>,
    /// Explicit child element names the element accepts (`elements`).
    elements: Vec<String>,
}

/// Parse failure while reading the vendored XML. Carries a human-readable
/// reason so the example and the gate test both surface actionable errors.
#[derive(Debug)]
pub struct DeriveError(String);

impl std::fmt::Display for DeriveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "content-model derivation failed: {}", self.0)
    }
}

impl std::error::Error for DeriveError {}

/// Derive the content model for every element defined in the SVG 2 Editor's
/// Draft, keyed by element name.
///
/// # Errors
/// Returns [`DeriveError`] if the vendored XML cannot be parsed, an element
/// references a content-model category with no `<elementcategory>` definition,
/// or a prose-content-model element is encountered without an override.
pub fn derive_ed_content_models() -> Result<BTreeMap<String, ElementContentModel>, DeriveError> {
    let mut categories: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut elements: BTreeMap<String, RawElement> = BTreeMap::new();

    let dir = svgwg_master_dir()?;
    for file in DEFINITION_FILES {
        let path = dir.join(file);
        let source = std::fs::read_to_string(&path)
            .map_err(|error| DeriveError(format!("read {}: {error}", path.display())))?;
        scan_definitions(&source, &mut categories, &mut elements)?;
    }

    let mut out = BTreeMap::new();
    for (name, raw) in &elements {
        out.insert(name.clone(), resolve(name, raw, &categories)?);
    }
    Ok(out)
}

/// Scan one definitions file, accumulating `<elementcategory>` memberships and
/// `<element>` content-model declarations into the shared maps.
fn scan_definitions(
    source: &str,
    categories: &mut BTreeMap<String, Vec<String>>,
    elements: &mut BTreeMap<String, RawElement>,
) -> Result<(), DeriveError> {
    let mut reader = Reader::from_str(source);
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::Start(tag) | Event::Empty(tag)) => match tag.name().as_ref() {
                b"elementcategory" => {
                    let Some(name) = attr(&tag, b"name")? else {
                        continue;
                    };
                    categories.insert(name, csv(attr(&tag, b"elements")?.as_deref()));
                }
                b"element" => {
                    let Some(name) = attr(&tag, b"name")? else {
                        continue;
                    };
                    elements.entry(name).or_insert(RawElement {
                        content_model: attr(&tag, b"contentmodel")?,
                        categories: csv(attr(&tag, b"elementcategories")?.as_deref()),
                        elements: csv(attr(&tag, b"elements")?.as_deref()),
                    });
                }
                _ => {}
            },
            Ok(_) => {}
            Err(error) => return Err(DeriveError(error.to_string())),
        }
    }
    Ok(())
}

/// Resolve one element's raw declaration into a typed content model.
fn resolve(
    name: &str,
    raw: &RawElement,
    categories: &BTreeMap<String, Vec<String>>,
) -> Result<ElementContentModel, DeriveError> {
    if let Some(model) = prose_override(name) {
        return Ok(model);
    }
    match raw.content_model.as_deref() {
        Some("anyof" | "textoranyof") => element_set(name, raw, categories),
        Some("any") => Ok(ElementContentModel::AnySvg),
        Some("text") => Ok(ElementContentModel::TextOnly),
        Some("empty") => Ok(ElementContentModel::Empty),
        Some(other) => Err(DeriveError(format!(
            "element {name} has unknown contentmodel {other:?}"
        ))),
        None => Err(DeriveError(format!(
            "element {name} has a prose content model but no override"
        ))),
    }
}

/// Flatten an `anyof` / `textoranyof` model into a sorted, de-duplicated
/// element set: the union of every listed category's spec members and the
/// explicitly listed child elements. Text-bearing (`textoranyof`) elements are
/// non-empty containers, so the only behavioural distinction that matters
/// downstream — not being [`ElementContentModel::Empty`] — is preserved.
fn element_set(
    name: &str,
    raw: &RawElement,
    categories: &BTreeMap<String, Vec<String>>,
) -> Result<ElementContentModel, DeriveError> {
    let mut members: BTreeSet<String> = raw.elements.iter().cloned().collect();
    for category in &raw.categories {
        let resolved = categories.get(category).ok_or_else(|| {
            DeriveError(format!(
                "element {name} references content-model category {category:?} \
                 with no <elementcategory> definition"
            ))
        })?;
        members.extend(resolved.iter().cloned());
    }
    Ok(ElementContentModel::ElementSet {
        elements: members.into_iter().collect(),
    })
}

/// Content models the spec expresses as prose (`<x:contentmodel>`) rather than
/// machine-readable attributes, encoded from the spec text. `foreignObject`
/// declares `contentmodel='any'` but hosts a foreign (HTML) namespace, so it is
/// pinned to [`ElementContentModel::ForeignNamespace`] here rather than
/// resolving as `AnySvg`.
fn prose_override(name: &str) -> Option<ElementContentModel> {
    let set = |names: &[&str]| ElementContentModel::ElementSet {
        elements: names.iter().map(|name| (*name).to_string()).collect(),
    };
    match name {
        // "any element or text allowed by its parent's content model, except
        // for another a" — a transparent wrapper; AnySvg is the faithful fit.
        "a" => Some(ElementContentModel::AnySvg),
        // contentmodel='any' in the source, but the element is the foreign
        // (HTML) content host; SVG child validation must not apply.
        "foreignObject" => Some(ElementContentModel::ForeignNamespace),
        // "Any number of descriptive elements, 'script' and mpath."
        "animateMotion" => Some(set(&["desc", "metadata", "mpath", "script", "title"])),
        // "Any number of descriptive elements, 'script' and exactly one light
        // source element, in any order."
        "feDiffuseLighting" | "feSpecularLighting" => Some(set(&[
            "desc",
            "feDistantLight",
            "fePointLight",
            "feSpotLight",
            "metadata",
            "script",
            "title",
        ])),
        _ => None,
    }
}

/// Resolve the vendored svgwg `master` directory from the active pin recorded
/// in `Svg2EditorsDraft/snapshot.json`. The vendored sources live under
/// `data/sources/svgwg-<commit[..8]>/master`, so reading the pin keeps this in
/// lockstep with `build.rs` and the re-vendor tooling without a hardcoded
/// commit prefix.
fn svgwg_master_dir() -> Result<PathBuf, DeriveError> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let snapshot_path = manifest_dir.join("data/specs/Svg2EditorsDraft/snapshot.json");
    let raw = std::fs::read(&snapshot_path)
        .map_err(|error| DeriveError(format!("read {}: {error}", snapshot_path.display())))?;
    let snapshot: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|error| DeriveError(error.to_string()))?;

    let commit = snapshot
        .get("pinned_sources")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|source| source.get("pin"))
        .find(|pin| pin.get("kind").and_then(serde_json::Value::as_str) == Some("git_commit"))
        .and_then(|pin| pin.get("commit"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| DeriveError(format!("no git_commit pin in {}", snapshot_path.display())))?;

    let prefix = commit
        .get(..8)
        .ok_or_else(|| DeriveError(format!("git_commit pin {commit:?} is too short")))?;
    Ok(manifest_dir.join(format!("data/sources/svgwg-{prefix}/master")))
}

/// Read an attribute value off a start tag. The svgwg definition attributes we
/// read (`name`, `contentmodel`, `elementcategories`, `elements`) are plain
/// element/category identifiers with no XML entity references, so the raw bytes
/// decode directly as UTF-8 — no unescaping pass is required.
fn attr(
    tag: &quick_xml::events::BytesStart<'_>,
    key: &[u8],
) -> Result<Option<String>, DeriveError> {
    match tag.try_get_attribute(key) {
        Ok(Some(found)) => std::str::from_utf8(&found.value)
            .map(|value| Some(value.to_string()))
            .map_err(|error| DeriveError(error.to_string())),
        Ok(None) => Ok(None),
        Err(error) => Err(DeriveError(error.to_string())),
    }
}

/// Split a comma-separated attribute value into trimmed, non-empty tokens.
fn csv(value: Option<&str>) -> Vec<String> {
    value
        .into_iter()
        .flat_map(|raw| raw.split(','))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}
