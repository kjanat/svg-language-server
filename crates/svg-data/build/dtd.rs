//! Deterministic flat-DTD parser for the vendored SVG 1.1 grammars.
//!
//! The W3C SVG 1.1 Recommendations ship a *flattened* DTD (`svg11-flat.dtd`):
//! every modular `.mod` file inlined into a single document. Despite being
//! "flat" it still carries the full XML modular-DTD machinery —
//! parameter-entity definitions (`<!ENTITY % NAME "replacement">`), conditional
//! marked sections (`<![ %gate; [ … ]]>`), and pervasive `%NAME;` references —
//! that an `<!ELEMENT>`/`<!ATTLIST>` reader must resolve before it can see the
//! real element names, content models, and attribute lists.
//!
//! This module is a small, self-contained, deterministic resolver for exactly
//! that. It performs **no** network or filesystem I/O of its own (the caller
//! hands in the vendored DTD text) and produces an order-stable
//! [`DtdInventory`]:
//!
//! 1. **element presence** — every `<!ELEMENT name …>` after qname expansion;
//! 2. **content models** — each element's raw, entity-expanded content model
//!    (kept for later content-model work; presence + edges are what the
//!    inventory bakes today);
//! 3. the **element ↔ attribute matrix** — every `(element, attribute)` edge
//!    from the expanded `<!ATTLIST>` bodies (the matrix);
//! 4. **enumerated keyword sets** — for any attribute whose declared TYPE is an
//!    enumeration `( a | b | c )`, the ordered keyword set;
//! 5. **provenance** — the `%…attrib;` parameter-entity *group* each attribute
//!    was pulled in under, so a downstream classifier can bucket attributes by
//!    the SVG attribute-collection they belong to (Core, Presentation, …).
//!
//! ## Resolution model
//!
//! XML DTD semantics: **the first declaration of a parameter entity wins**; any
//! later `<!ENTITY % NAME "…">` for an already-defined `NAME` is ignored. The
//! flat SVG DTD leans on this heavily — each module emits empty *default*
//! placeholders (`<!ENTITY % SVG.Core.attrib "">`) that are only effective if
//! they are seen *before* the real definition; because the real definition is
//! emitted earlier in the flattened file, first-wins selects it and the
//! placeholders are inert.
//!
//! Conditional sections gate content on an `INCLUDE`/`IGNORE` keyword that is
//! itself usually an expanded parameter entity (`<![%svg-foo.module;[ … ]]>`).
//! `IGNORE` bodies are dropped wholesale before any declaration is read, so an
//! ignored module never contributes elements, attributes, or entities.

use std::collections::{BTreeMap, BTreeSet};

use super::classification::Classification;

/// Map one raw DTD attribute-collection group name (e.g. `SVG.Core.attrib`,
/// `SVG.Presentation.attrib` for SVG 1.1, or `stdAttrs`,
/// `PresentationAttributes-Color` for SVG 1.0) onto the shared
/// [`Classification`] taxonomy used by the SVG 2 ED inventory, so all editions
/// expose attributes under one normalized bucket set.
///
/// ## SVG 1.0 collections
///
/// SVG 1.0 (2001-09-04) predates the SVG 1.1 modular-DTD `%…attrib;` naming
/// scheme: its attribute-collection parameter entities use a flat,
/// non-namespaced convention (`stdAttrs`, `langSpaceAttrs`, `testAttrs`, the
/// `xlinkRefAttrs*`, the `*Events`, and the `PresentationAttributes-*` family).
/// They are mapped here onto the **same** buckets as their SVG 1.1
/// counterparts, so the SVG 1.0 inventory is classified just like the later
/// editions. The non-presentation, non-event SVG 1.0 collections (the `anim*`
/// and `filter_primitive*`/`component_transfer_function` collections) keep
/// their raw name in [`Classification::Other`], matching how SVG 1.1's
/// `Animation*`/`FilterPrimitive*` collections are handled.
///
/// The mapping is derived purely from the (stable, well-documented) modular-DTD
/// group names:
///
/// - the `core` collection plus its `id`/`base`/`lang`/`xmlns` leaves, and the
///   `Style` collection (`style`/`class`) → [`Classification::Core`];
/// - the `Conditional` collection (`requiredFeatures`, `requiredExtensions`,
///   `systemLanguage`) → [`Classification::ConditionalProcessing`];
/// - every `XLink` collection (`XLink`, `XLinkEmbed`, `XLinkReplace`,
///   `XLinkRequired`) → [`Classification::Xlink`];
/// - the graphical/document/animation **event** collections →
///   [`Classification::EventHandler`];
/// - the presentation-attribute collections (`Presentation` umbrella plus the
///   `Color`, `Paint`, `Opacity`, `Graphics`, `Viewport`, `Text`,
///   `TextContent`, `Font`, `Marker`, `Gradient`, `Clip`, `Mask`, `Filter`,
///   `FilterColor`, `Cursor`, `ColorProfile`, `Container` collections) →
///   [`Classification::Presentation`];
/// - everything else (the `External` resource hint, the non-event `Animation*`
///   collections, the filter-primitive collections) keeps its raw group name
///   verbatim in [`Classification::Other`] so no provenance is lost.
///
/// SVG 1.1 has no ARIA attribute collection, so [`Classification::Aria`] never
/// arises here; that is faithful to the edition.
#[must_use]
pub fn classify_group(group: &str) -> Classification {
    // Each arm carries both the SVG 1.1 modular-DTD `%…attrib;` collections and
    // their SVG 1.0 (2001-09-04) flat-naming counterparts (see
    // [`SVG10_ATTRIB_GROUPS`]), so one taxonomy classifies every edition.
    match group {
        // Core: SVG 1.1 core/id/base/lang/xmlns/Style; SVG 1.0 `stdAttrs`
        // (`id`/`xml:base`) and `langSpaceAttrs` (`xml:lang`/`xml:space`).
        "SVG.Core.attrib" | "SVG.id.attrib" | "SVG.base.attrib" | "SVG.lang.attrib"
        | "SVG.xmlns.attrib" | "SVG.Style.attrib" | "stdAttrs" | "langSpaceAttrs" => {
            Classification::Core
        }
        // Conditional processing: SVG 1.1 `Conditional`; SVG 1.0 `testAttrs`.
        "SVG.Conditional.attrib" | "testAttrs" => Classification::ConditionalProcessing,
        // XLink: SVG 1.1 `XLink*`; SVG 1.0 `xlinkRefAttrs*`.
        "SVG.XLink.attrib"
        | "SVG.XLinkEmbed.attrib"
        | "SVG.XLinkReplace.attrib"
        | "SVG.XLinkRequired.attrib"
        | "xlinkRefAttrs"
        | "xlinkRefAttrsEmbed" => Classification::Xlink,
        // Event handlers: SVG 1.1 `*Events`; SVG 1.0 `*Events`.
        "SVG.GraphicalEvents.attrib"
        | "SVG.DocumentEvents.attrib"
        | "SVG.AnimationEvents.attrib"
        | "graphicsElementEvents"
        | "documentEvents"
        | "animationEvents" => Classification::EventHandler,
        // Presentation: SVG 1.1 `Presentation` umbrella + leaf collections;
        // SVG 1.0 `PresentationAttributes-*` family.
        "SVG.Presentation.attrib"
        | "SVG.Container.attrib"
        | "SVG.Viewport.attrib"
        | "SVG.Text.attrib"
        | "SVG.TextContent.attrib"
        | "SVG.Font.attrib"
        | "SVG.Paint.attrib"
        | "SVG.Color.attrib"
        | "SVG.ColorProfile.attrib"
        | "SVG.Opacity.attrib"
        | "SVG.Graphics.attrib"
        | "SVG.Marker.attrib"
        | "SVG.Gradient.attrib"
        | "SVG.Clip.attrib"
        | "SVG.Mask.attrib"
        | "SVG.Filter.attrib"
        | "SVG.FilterColor.attrib"
        | "SVG.Cursor.attrib"
        | "PresentationAttributes-All"
        | "PresentationAttributes-Color"
        | "PresentationAttributes-FillStroke"
        | "PresentationAttributes-Graphics"
        | "PresentationAttributes-Viewports"
        | "PresentationAttributes-TextElements"
        | "PresentationAttributes-TextContentElements"
        | "PresentationAttributes-FontSpecification"
        | "PresentationAttributes-Markers"
        | "PresentationAttributes-Gradients"
        | "PresentationAttributes-Images"
        | "PresentationAttributes-LightingEffects"
        | "PresentationAttributes-feFlood"
        | "PresentationAttributes-FilterPrimitives"
        | "PresentationAttributes-Containers" => Classification::Presentation,
        // Everything else keeps its raw name verbatim (SVG 1.0 `anim*`,
        // `filter_primitive*`, `component_transfer_function_attributes`;
        // SVG 1.1 `Animation*`/`FilterPrimitive*`/`External`).
        other => Classification::Other(other.to_string()),
    }
}

/// A single enumerated attribute TYPE: its ordered keyword set, exactly as the
/// DTD wrote them (including `inherit` where present — callers strip it when
/// cross-checking against the `inherit`-free property-index enums).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumType {
    /// Ordered, de-duplicated keyword alternatives from `( a | b | c )`.
    pub keywords: Vec<String>,
}

/// Whether an attribute was declared `#REQUIRED`, `#IMPLIED`, or neither
/// (`#FIXED`/literal default). Captured cheaply from the token following the
/// TYPE; the SVG DTD only ever uses `#REQUIRED` and `#IMPLIED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Defaulting {
    /// `#REQUIRED` — the attribute must be present.
    Required,
    /// `#IMPLIED` — the attribute is optional.
    Implied,
    /// `#FIXED "…"` or a bare literal default — neither required nor implied.
    Other,
}

/// One resolved attribute on one element: its declared TYPE shape and
/// defaulting, plus the `%…attrib;` group provenance it was expanded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttlistEntry {
    /// Attribute name, for example `fill` or `gradientUnits`.
    pub name: String,
    /// Enumerated keyword set when the TYPE is `( a | b | c )`, else `None`
    /// (the TYPE was `CDATA`, `ID`, a `%…datatype;` expansion to `CDATA`, …).
    pub enumeration: Option<EnumType>,
    /// `#REQUIRED` / `#IMPLIED` / other.
    pub defaulting: Defaulting,
}

/// The deterministic inventory derived from one flat SVG 1.1 DTD.
#[derive(Debug, Clone, Default)]
pub struct DtdInventory {
    /// Every `<!ELEMENT>` name, sorted and de-duplicated.
    pub elements: BTreeSet<String>,
    /// Element name → its raw (entity-expanded) content-model string. Kept for
    /// downstream content-model work; not baked into the presence inventory.
    pub content_models: BTreeMap<String, String>,
    /// Element → resolved attribute entries, keyed by attribute name. A later
    /// `<!ATTLIST>` redeclaration for an element merges; per XML rules the
    /// first declaration of a given attribute on an element wins, which the
    /// merge preserves (existing entries are not overwritten).
    pub element_attributes: BTreeMap<String, BTreeMap<String, AttlistEntry>>,
    /// Attribute name → the set of `%…attrib;` parameter-entity group names it
    /// was expanded under (provenance for classification). An attribute
    /// declared inline on an element (not via a group) has no entry here.
    pub attribute_groups: BTreeMap<String, BTreeSet<String>>,
    /// Attribute name → its enumerated keyword set, when any `<!ATTLIST>`
    /// declared it as an enumeration. The DTD is internally consistent, so a
    /// name maps to a single enum; the first non-empty enum seen wins.
    pub attribute_enums: BTreeMap<String, EnumType>,
}

impl DtdInventory {
    /// The set of `(element, attribute)` edges — the matrix in flat form.
    #[must_use]
    pub fn edges(&self) -> BTreeSet<(String, String)> {
        let mut edges = BTreeSet::new();
        for (element, attributes) in &self.element_attributes {
            for attribute in attributes.keys() {
                edges.insert((element.clone(), attribute.clone()));
            }
        }
        edges
    }

    /// Every distinct attribute name attached to at least one element.
    #[must_use]
    pub fn attribute_names(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for attributes in self.element_attributes.values() {
            names.extend(attributes.keys().cloned());
        }
        names
    }
}

/// Parse a flat SVG 1.1 DTD into a deterministic [`DtdInventory`].
///
/// The input is the full vendored DTD text. Resolution is pure and
/// order-stable: comment stripping, then conditional-section resolution against
/// first-wins parameter entities, then `<!ENTITY>`/`<!ELEMENT>`/`<!ATTLIST>`
/// extraction with recursive `%NAME;` expansion under a cycle guard.
#[must_use]
pub fn parse(dtd: &str) -> DtdInventory {
    let without_comments = strip_comments(dtd);
    let mut entities = Entities::default();
    // First pass: collect every parameter entity (first-wins) so conditional
    // gates and later expansions can resolve. Entities inside an IGNORE body
    // are excluded because the body is dropped here.
    let resolved = resolve_conditionals(&without_comments, &mut entities);
    // Second pass over the IGNORE-pruned text: read the structural
    // declarations, expanding parameter-entity references as we go.
    let mut inventory = DtdInventory::default();
    read_declarations(&resolved, &entities, &mut inventory);
    inventory
}

/// First-wins parameter-entity table: `NAME -> replacement text`.
#[derive(Debug, Default)]
struct Entities {
    map: BTreeMap<String, String>,
}

impl Entities {
    /// Record `name = value` only if `name` is not already defined (XML
    /// first-declaration-wins).
    fn define(&mut self, name: &str, value: &str) {
        self.map
            .entry(name.to_string())
            .or_insert_with(|| value.to_string());
    }

    /// Look up a parameter entity's replacement text.
    fn get(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(String::as_str)
    }
}

/// Remove every `<!-- … -->` comment. SGML/XML comments do not nest, so a flat
/// scan for the next `-->` after each `<!--` is correct.
fn strip_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        match after.find("-->") {
            Some(end) => rest = &after[end + 3..],
            None => {
                // Unterminated comment: drop the remainder.
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Walk the comment-stripped DTD, recording parameter entities and emitting a
/// copy of the text with `IGNORE` marked sections removed.
///
/// Marked sections are `<![ KEYWORD [ … ]]>`, where `KEYWORD` is the expansion
/// of a parameter entity (`<![%svg-foo.module;[`) or a literal `INCLUDE` /
/// `IGNORE`. Sections nest. `<!ENTITY % …>` declarations encountered outside an
/// ignored body are folded into `entities` as we go, so a gate later in the
/// file resolves against entities defined earlier (the order the flat DTD
/// relies on).
fn resolve_conditionals(input: &str, entities: &mut Entities) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if input[idx..].starts_with("<![") {
            let (section_end, body, keyword) = read_marked_section(input, idx, entities);
            if keyword == "INCLUDE" {
                // Recurse into the included body so nested sections and entity
                // definitions inside it are processed in order.
                let inner = resolve_conditionals(body, entities);
                out.push_str(&inner);
            }
            // IGNORE: drop the body entirely.
            idx = section_end;
        } else if input[idx..].starts_with("<!ENTITY") {
            let decl_end = declaration_end(input, idx);
            let decl = &input[idx..decl_end];
            record_entity(decl, entities);
            out.push_str(decl);
            idx = decl_end;
        } else {
            let ch = input[idx..].chars().next().unwrap_or('<');
            out.push(ch);
            idx += ch.len_utf8();
        }
    }
    out
}

/// Locate the byte index just past the `>` that closes the declaration starting
/// at `start`. Quoted strings (single or double) are skipped so a `>` inside a
/// replacement-text literal does not end the declaration early.
fn declaration_end(input: &str, start: usize) -> usize {
    let bytes = input.as_bytes();
    let mut idx = start;
    let mut quote: Option<u8> = None;
    while idx < bytes.len() {
        let byte = bytes[idx];
        match quote {
            Some(q) => {
                if byte == q {
                    quote = None;
                }
            }
            None => match byte {
                b'"' | b'\'' => quote = Some(byte),
                b'>' => return idx + 1,
                _ => {}
            },
        }
        idx += 1;
    }
    bytes.len()
}

/// Read the marked section that starts at `start` (`input[start..]` begins with
/// `<![`). Returns `(end_index, body, keyword)` where `end_index` is just past
/// the closing `]]>`, `body` is the inner text between `[` and the matching
/// `]]>`, and `keyword` is the resolved `INCLUDE`/`IGNORE` gate (anything that
/// does not resolve to `IGNORE` is treated as `INCLUDE`, the lenient default).
fn read_marked_section<'a>(
    input: &'a str,
    start: usize,
    entities: &Entities,
) -> (usize, &'a str, String) {
    // Status keyword sits between `<![` and the body-opening `[`.
    let after_open = start + 3;
    let Some(rel_bracket) = input[after_open..].find('[') else {
        // Malformed: no body bracket. Consume to end, treat as IGNORE so we
        // never accidentally pull in garbage.
        return (input.len(), "", "IGNORE".to_string());
    };
    let keyword_raw = &input[after_open..after_open + rel_bracket];
    let keyword = resolve_status_keyword(keyword_raw, entities);
    let body_start = after_open + rel_bracket + 1;

    // Find the matching `]]>`, honoring nested `<![ … ]]>` sections.
    let body_bytes = input.as_bytes();
    let mut idx = body_start;
    let mut depth = 1usize;
    while idx < body_bytes.len() {
        if input[idx..].starts_with("<![") {
            depth += 1;
            idx += 3;
        } else if input[idx..].starts_with("]]>") {
            depth -= 1;
            if depth == 0 {
                let body = &input[body_start..idx];
                return (idx + 3, body, keyword);
            }
            idx += 3;
        } else {
            idx += 1;
        }
    }
    // Unterminated section: take the rest as the body.
    (input.len(), &input[body_start..], keyword)
}

/// Resolve a marked-section status keyword. It is either a literal
/// `INCLUDE`/`IGNORE` or a `%NAME;` reference chain whose first-wins expansion
/// is the keyword (the SVG DTD chains `%SVG.prefixed;` -> `%NS.prefixed;` ->
/// `IGNORE`, so the reference must be expanded *recursively*). Unknown/empty
/// resolves to `INCLUDE` (lenient).
fn resolve_status_keyword(raw: &str, entities: &Entities) -> String {
    let mut active = Vec::new();
    let expanded = expand(raw.trim(), entities, &mut active);
    if expanded.trim().eq_ignore_ascii_case("IGNORE") {
        "IGNORE".to_string()
    } else {
        "INCLUDE".to_string()
    }
}

/// Parse one `<!ENTITY % NAME "replacement">` declaration and fold it into the
/// first-wins table. External-identifier entities (`PUBLIC`/`SYSTEM`, no
/// literal replacement) are skipped: the flat DTD never relies on them having a
/// usable replacement (their `.mod` bodies are already inlined).
fn record_entity(decl: &str, entities: &mut Entities) {
    // decl looks like: <!ENTITY % NAME "value"> (possibly multi-line).
    let body = decl
        .trim_start_matches("<!ENTITY")
        .trim_end_matches('>')
        .trim();
    let Some(rest) = body.strip_prefix('%') else {
        // A general (non-parameter) entity — irrelevant to expansion here.
        return;
    };
    let rest = rest.trim_start();
    // NAME is the first whitespace-delimited token.
    let mut chars = rest.char_indices();
    let mut name_end = rest.len();
    for (offset, ch) in chars.by_ref() {
        if ch.is_whitespace() {
            name_end = offset;
            break;
        }
    }
    let name = &rest[..name_end];
    if name.is_empty() {
        return;
    }
    let after_name = rest[name_end..].trim_start();
    // Only literal-replacement entities carry a value we can expand. A
    // PUBLIC/SYSTEM external id has no quoted literal as its first token.
    let value = match after_name.chars().next() {
        Some(quote @ ('"' | '\'')) => {
            let body = &after_name[1..];
            body.find(quote).map_or(body, |end| &body[..end])
        }
        _ => return,
    };
    entities.define(name, value);
}

/// Recursively expand every `%NAME;` reference in `text` against `entities`,
/// guarding against entity cycles via the `active` stack. An unknown reference
/// expands to empty (the SVG DTD's optional `*.extra.*` hooks are deliberately
/// undefined). A reference already on the `active` stack expands to empty,
/// breaking the cycle.
fn expand(text: &str, entities: &Entities, active: &mut Vec<String>) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%'
            && let Some((name, after)) = read_reference(text, idx)
        {
            if active.iter().any(|n| n == name) {
                // Cycle: skip this reference, keep scanning past it.
                idx = after;
                continue;
            }
            if let Some(value) = entities.get(name) {
                let value = value.to_string();
                active.push(name.to_string());
                let expanded = expand(&value, entities, active);
                active.pop();
                out.push_str(&expanded);
            } // else: unknown reference -> empty.
            idx = after;
            continue;
        }
        let ch = text[idx..].chars().next().unwrap_or('%');
        out.push(ch);
        idx += ch.len_utf8();
    }
    out
}

/// If a parameter reference `%NAME;` starts at `pos`, return `(NAME, end)`
/// where `end` is the byte index just past the `;`. A bare `%` not forming a
/// valid reference returns `None` so it is emitted literally.
fn read_reference(text: &str, pos: usize) -> Option<(&str, usize)> {
    debug_assert_eq!(text.as_bytes().get(pos), Some(&b'%'));
    let after_percent = pos + 1;
    let rest = &text[after_percent..];
    // A parameter-entity name is `Name` per XML: letters, digits, and
    // `-_.:` — terminated by `;`. Whitespace after `%` means it is not a
    // reference (e.g. a stray `%` in prose).
    let semicolon = rest.find(';')?;
    let name = &rest[..semicolon];
    if name.is_empty()
        || name.chars().any(|ch| {
            !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' || ch == ':')
        })
    {
        return None;
    }
    Some((name, after_percent + semicolon + 1))
}

/// Read every `<!ELEMENT>` / `<!ATTLIST>` declaration from the IGNORE-pruned
/// text, expanding parameter references and folding the results into
/// `inventory`. `<!ENTITY>` declarations are skipped here (already collected).
fn read_declarations(input: &str, entities: &Entities, inventory: &mut DtdInventory) {
    let bytes = input.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if input[idx..].starts_with("<!ELEMENT") {
            let end = declaration_end(input, idx);
            handle_element(&input[idx..end], entities, inventory);
            idx = end;
        } else if input[idx..].starts_with("<!ATTLIST") {
            let end = declaration_end(input, idx);
            handle_attlist(&input[idx..end], entities, inventory);
            idx = end;
        } else {
            idx += 1;
        }
    }
}

/// Fold one `<!ELEMENT name content>` declaration into the inventory. Both the
/// name and content are parameter-expanded first (the name is `%SVG.foo.qname;`
/// and the content is `%SVG.foo.content;`).
fn handle_element(decl: &str, entities: &Entities, inventory: &mut DtdInventory) {
    let body = decl
        .trim_start_matches("<!ELEMENT")
        .trim_end_matches('>')
        .trim();
    let mut active = Vec::new();
    let expanded = expand(body, entities, &mut active);
    let expanded = expanded.trim();
    // First token is the element name; the remainder is the content model.
    let (name, content) = split_first_token(expanded);
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    inventory.elements.insert(name.to_string());
    inventory
        .content_models
        .entry(name.to_string())
        .or_insert_with(|| collapse_ws(content));
}

/// Fold one `<!ATTLIST element attr TYPE default …>` declaration into the
/// inventory. The element name and the entire attribute body are
/// parameter-expanded first (the body is a stack of `%…attrib;` group
/// references plus inline `attr TYPE default` triples).
fn handle_attlist(decl: &str, entities: &Entities, inventory: &mut DtdInventory) {
    let body = decl
        .trim_start_matches("<!ATTLIST")
        .trim_end_matches('>')
        .trim();
    // Expand the element name first (it is the leading `%SVG.foo.qname;`),
    // capturing group provenance for the attribute body separately.
    let (raw_name, raw_attrs) = split_first_token(body);
    let mut active = Vec::new();
    let element = expand(raw_name, entities, &mut active).trim().to_string();
    if element.is_empty() {
        return;
    }
    inventory.elements.insert(element.clone());

    let entries = parse_attlist_body(raw_attrs, entities);
    let element_entry = inventory.element_attributes.entry(element).or_default();
    for (entry, groups) in entries {
        for group in &groups {
            inventory
                .attribute_groups
                .entry(entry.name.clone())
                .or_default()
                .insert(group.clone());
        }
        if let Some(enumeration) = &entry.enumeration {
            inventory
                .attribute_enums
                .entry(entry.name.clone())
                .or_insert_with(|| enumeration.clone());
        }
        // First declaration of an attribute on an element wins.
        element_entry.entry(entry.name.clone()).or_insert(entry);
    }
}

/// Expand and tokenize an `<!ATTLIST>` attribute body into `(entry, groups)`
/// pairs. `groups` is the set of `%…attrib;` parameter-entity names the entry
/// was expanded from (empty for an inline attribute), tracked so the caller can
/// classify by attribute-collection provenance.
fn parse_attlist_body(raw_attrs: &str, entities: &Entities) -> Vec<(AttlistEntry, Vec<String>)> {
    // Expand top-level `%…attrib;` references one at a time so each produced
    // triple keeps the originating group name as provenance. References nested
    // inside a group expansion inherit that top-level group.
    let segments = expand_with_groups(raw_attrs, entities);
    let mut entries = Vec::new();
    for segment in segments {
        for entry in tokenize_attlist_segment(&segment.text) {
            entries.push((entry, segment.groups.clone()));
        }
    }
    entries
}

/// One expanded attribute-body segment plus the top-level `%…attrib;` group(s)
/// it originated from.
struct GroupedSegment {
    text: String,
    groups: Vec<String>,
}

/// Expand an `<!ATTLIST>` body, attributing each chunk of expanded text to the
/// top-level `%NAME;` group reference it came from. Inline text between group
/// references (the per-element `attr TYPE default` triples) is attributed to no
/// group.
fn expand_with_groups(raw: &str, entities: &Entities) -> Vec<GroupedSegment> {
    let bytes = raw.as_bytes();
    let mut segments = Vec::new();
    let mut inline = String::new();
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%'
            && let Some((name, after)) = read_reference(raw, idx)
        {
            if is_attribute_group(name) {
                // A `%…attrib;` attribute-collection group: flush any pending
                // inline triples, then emit the group's expansion as its own
                // provenance-bearing segment.
                if inline.trim().is_empty() {
                    inline.clear();
                } else {
                    segments.push(GroupedSegment {
                        text: std::mem::take(&mut inline),
                        groups: Vec::new(),
                    });
                }
                let mut active = vec![name.to_string()];
                let expanded = entities
                    .get(name)
                    .map(|value| expand(value, entities, &mut active))
                    .unwrap_or_default();
                segments.push(GroupedSegment {
                    text: expanded,
                    groups: vec![name.to_string()],
                });
            } else {
                // A datatype (or other non-group) reference inside an inline
                // `attr TYPE default` triple: expand it in place so the triple
                // stays intact (e.g. `x1 %Coordinate.datatype; #IMPLIED` ->
                // `x1 CDATA #IMPLIED`).
                let mut active = vec![name.to_string()];
                let expanded = entities
                    .get(name)
                    .map(|value| expand(value, entities, &mut active))
                    .unwrap_or_default();
                inline.push_str(&expanded);
            }
            idx = after;
            continue;
        }
        let ch = raw[idx..].chars().next().unwrap_or('%');
        inline.push(ch);
        idx += ch.len_utf8();
    }
    if !inline.trim().is_empty() {
        segments.push(GroupedSegment {
            text: inline,
            groups: Vec::new(),
        });
    }
    segments
}

/// The SVG 1.0 (2001-09-04) attribute-collection parameter entities, which
/// predate the SVG 1.1 `%…attrib;` naming convention and so must be recognized
/// by an explicit allowlist instead of a suffix test.
///
/// Each of these expands to one or more whole `attr TYPE default` triples (a
/// *collection* of attributes), exactly like an SVG 1.1 `%…attrib;` group — so
/// the parser attributes the triples they produce to the group for provenance.
/// Every **other** parameter entity referenced inside an SVG 1.0 `<!ATTLIST>`
/// body is a *datatype* (`%Coordinate;`, `%Length;`, `%Boolean;`, `%URI;`, …)
/// that expands to a single TYPE token belonging inside the current triple, so
/// it is deliberately excluded here. This split is what lets the same flat-DTD
/// reader bake the SVG 1.0 inventory with the same group classification it gives
/// SVG 1.1. The names are stable (frozen REC) and taken verbatim from the
/// vendored `svg10.dtd`.
const SVG10_ATTRIB_GROUPS: &[&str] = &[
    "stdAttrs",
    "langSpaceAttrs",
    "testAttrs",
    "xlinkRefAttrs",
    "xlinkRefAttrsEmbed",
    "graphicsElementEvents",
    "documentEvents",
    "animationEvents",
    "animElementAttrs",
    "animAttributeAttrs",
    "animTimingAttrs",
    "animValueAttrs",
    "animAdditionAttrs",
    "filter_primitive_attributes",
    "filter_primitive_attributes_with_in",
    "component_transfer_function_attributes",
    "PresentationAttributes-All",
    "PresentationAttributes-Color",
    "PresentationAttributes-FillStroke",
    "PresentationAttributes-Graphics",
    "PresentationAttributes-Viewports",
    "PresentationAttributes-TextElements",
    "PresentationAttributes-TextContentElements",
    "PresentationAttributes-FontSpecification",
    "PresentationAttributes-Markers",
    "PresentationAttributes-Gradients",
    "PresentationAttributes-Images",
    "PresentationAttributes-LightingEffects",
    "PresentationAttributes-feFlood",
    "PresentationAttributes-FilterPrimitives",
    "PresentationAttributes-Containers",
];

/// `true` if a parameter-entity name designates an SVG attribute-collection
/// group.
///
/// Two naming conventions are recognized so one reader serves every vendored
/// edition:
///
/// - **SVG 1.1 / SVG 1.1 PR** use the modular-DTD `%…attrib;` convention
///   (`SVG.Core.attrib`, `SVG.Presentation.attrib`), matched by suffix.
/// - **SVG 1.0** predates that scheme and names its collections flatly
///   (`stdAttrs`, `PresentationAttributes-Color`, …), matched against the
///   explicit [`SVG10_ATTRIB_GROUPS`] allowlist.
///
/// Every other `<!ATTLIST>`-body reference is a datatype expansion
/// (`%Coordinate.datatype;`, `%Length;`) that belongs inside the current
/// `attr TYPE default` triple, not a group of its own.
fn is_attribute_group(name: &str) -> bool {
    name.ends_with(".attrib") || SVG10_ATTRIB_GROUPS.contains(&name)
}

/// Tokenize one fully-expanded attribute-list fragment into [`AttlistEntry`]s.
///
/// Each entry is `NAME TYPE DEFAULT`, where TYPE is either a parenthesized
/// enumeration `( a | b | c )` or a single token (`CDATA`, `ID`, `IDREF`, …),
/// and DEFAULT is `#REQUIRED`, `#IMPLIED`, `#FIXED "…"`, or a literal.
fn tokenize_attlist_segment(text: &str) -> Vec<AttlistEntry> {
    let mut tokens = AttTokenizer::new(text);
    let mut entries = Vec::new();
    while let Some(name) = tokens.next_token() {
        // A stray `>` or empty token guards against trailing noise.
        if name.is_empty() {
            continue;
        }
        let Some(type_token) = tokens.next_type() else {
            break;
        };
        let enumeration = match type_token {
            TypeToken::Enumeration(keywords) => Some(EnumType { keywords }),
            TypeToken::Name => None,
        };
        let defaulting = tokens.read_default();
        entries.push(AttlistEntry {
            name,
            enumeration,
            defaulting,
        });
    }
    entries
}

/// A declared attribute TYPE: either an enumeration or a single name token
/// (`CDATA`, `ID`, …). The name token's text is irrelevant to the inventory —
/// only the enumerated keyword sets are captured — so the [`Name`](Self::Name)
/// variant carries no payload.
enum TypeToken {
    Enumeration(Vec<String>),
    Name,
}

/// Hand-rolled tokenizer over a fully-expanded `<!ATTLIST>` fragment. The
/// grammar is regular enough (whitespace-separated tokens, `( … )`
/// enumerations, `#…` defaults, quoted literals) that a small explicit scanner
/// is clearer and more deterministic than a regex soup.
struct AttTokenizer<'a> {
    bytes: &'a [u8],
    text: &'a str,
    pos: usize,
}

impl<'a> AttTokenizer<'a> {
    const fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            text,
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    /// Read the next whitespace-delimited bare token (the attribute name). Stops
    /// before `(`, `#`, or quotes, which begin a TYPE or DEFAULT.
    fn next_token(&mut self) -> Option<String> {
        self.skip_ws();
        if self.pos >= self.bytes.len() {
            return None;
        }
        // A name never starts with `(`/`#`/quote; if we see one here the
        // stream is misaligned, so bail to avoid mis-pairing.
        match self.bytes[self.pos] {
            b'(' | b'#' | b'"' | b'\'' => return None,
            _ => {}
        }
        let start = self.pos;
        while self.pos < self.bytes.len() {
            let byte = self.bytes[self.pos];
            if byte.is_ascii_whitespace() || byte == b'(' || byte == b'#' {
                break;
            }
            self.pos += 1;
        }
        Some(self.text[start..self.pos].to_string())
    }

    /// Read the TYPE following an attribute name: an enumeration `( … )` or a
    /// single name token (`CDATA`, `ID`, …).
    fn next_type(&mut self) -> Option<TypeToken> {
        self.skip_ws();
        if self.pos >= self.bytes.len() {
            return None;
        }
        if self.bytes[self.pos] == b'(' {
            return Some(TypeToken::Enumeration(self.read_enumeration()));
        }
        // A NOTATION enumeration is `NOTATION ( … )`; the SVG DTD never uses it,
        // but handle it so a misread cannot swallow following entries.
        let start = self.pos;
        while self.pos < self.bytes.len() {
            let byte = self.bytes[self.pos];
            if byte.is_ascii_whitespace() || byte == b'(' {
                break;
            }
            self.pos += 1;
        }
        let name = &self.text[start..self.pos];
        if name.eq_ignore_ascii_case("NOTATION") {
            self.skip_ws();
            if self.pos < self.bytes.len() && self.bytes[self.pos] == b'(' {
                return Some(TypeToken::Enumeration(self.read_enumeration()));
            }
        }
        Some(TypeToken::Name)
    }

    /// Read a parenthesized enumeration body `( a | b | c )` into its ordered,
    /// de-duplicated keyword list. Assumes the cursor is on the opening `(`.
    fn read_enumeration(&mut self) -> Vec<String> {
        // Consume `(`.
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b')' {
            self.pos += 1;
        }
        let inner = &self.text[start..self.pos];
        // Consume `)`.
        if self.pos < self.bytes.len() {
            self.pos += 1;
        }
        let mut keywords = Vec::new();
        for token in inner.split('|') {
            let keyword = token.trim();
            if !keyword.is_empty() && !keywords.iter().any(|k| k == keyword) {
                keywords.push(keyword.to_string());
            }
        }
        keywords
    }

    /// Read the DEFAULT following a TYPE: `#REQUIRED`, `#IMPLIED`,
    /// `#FIXED "…"`, or a bare quoted literal. Consumes any trailing quoted
    /// value so the cursor lands on the next attribute name.
    fn read_default(&mut self) -> Defaulting {
        self.skip_ws();
        if self.pos >= self.bytes.len() {
            return Defaulting::Other;
        }
        if self.bytes[self.pos] == b'#' {
            let start = self.pos;
            self.pos += 1;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_alphabetic() {
                self.pos += 1;
            }
            let keyword = &self.text[start..self.pos];
            let defaulting = match keyword {
                "#REQUIRED" => Defaulting::Required,
                "#IMPLIED" => Defaulting::Implied,
                _ => Defaulting::Other,
            };
            // `#FIXED "value"` carries a literal after it; consume it.
            if keyword.eq_ignore_ascii_case("#FIXED") {
                self.consume_quoted_literal();
            }
            return defaulting;
        }
        // A bare literal default (`"value"`): consume and report Other.
        self.consume_quoted_literal();
        Defaulting::Other
    }

    /// If the cursor is on a quoted literal, consume it (through the closing
    /// quote). No-op otherwise.
    fn consume_quoted_literal(&mut self) {
        self.skip_ws();
        if self.pos >= self.bytes.len() {
            return;
        }
        let quote = self.bytes[self.pos];
        if quote != b'"' && quote != b'\'' {
            return;
        }
        self.pos += 1;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != quote {
            self.pos += 1;
        }
        if self.pos < self.bytes.len() {
            self.pos += 1;
        }
    }
}

/// Split `text` into its first whitespace-delimited token and the remainder.
fn split_first_token(text: &str) -> (&str, &str) {
    let trimmed = text.trim_start();
    trimmed
        .find(char::is_whitespace)
        .map_or((trimmed, ""), |end| {
            (&trimmed[..end], trimmed[end..].trim_start())
        })
}

/// Collapse all runs of whitespace to single spaces and trim.
fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_comments_without_nesting() {
        let input = "a<!-- c1 -->b<!-- c2 -->c";
        assert_eq!(strip_comments(input), "abc");
    }

    #[test]
    fn first_entity_definition_wins() {
        let mut entities = Entities::default();
        entities.define("X", "first");
        entities.define("X", "second");
        assert_eq!(entities.get("X"), Some("first"));
    }

    #[test]
    fn expands_nested_references() {
        let mut entities = Entities::default();
        entities.define("inner", "VALUE");
        entities.define("outer", "%inner;");
        let mut active = Vec::new();
        assert_eq!(expand("%outer;", &entities, &mut active), "VALUE");
    }

    #[test]
    fn cycle_guard_breaks_self_reference() {
        let mut entities = Entities::default();
        entities.define("loop", "a%loop;b");
        let mut active = Vec::new();
        // The self-reference is dropped, the surrounding literal survives.
        assert_eq!(expand("%loop;", &entities, &mut active), "ab");
    }

    #[test]
    fn ignore_section_is_dropped() {
        let dtd = "<![IGNORE[<!ELEMENT gone EMPTY>]]><!ELEMENT kept EMPTY>";
        let inv = parse(dtd);
        assert!(inv.elements.contains("kept"));
        assert!(!inv.elements.contains("gone"));
    }

    #[test]
    fn include_section_is_kept() {
        let dtd = "<![INCLUDE[<!ELEMENT here EMPTY>]]>";
        let inv = parse(dtd);
        assert!(inv.elements.contains("here"));
    }

    #[test]
    fn entity_gated_section_resolves() {
        let dtd = "<!ENTITY % gate \"INCLUDE\"><![%gate;[<!ELEMENT on EMPTY>]]>\
                   <!ENTITY % off \"IGNORE\"><![%off;[<!ELEMENT hidden EMPTY>]]>";
        let inv = parse(dtd);
        assert!(inv.elements.contains("on"));
        assert!(!inv.elements.contains("hidden"));
    }

    #[test]
    fn parses_enumerated_attribute_with_provenance() {
        let dtd = "<!ENTITY % Core.attrib \"id ID #IMPLIED\">\
                   <!ENTITY % rect.qname \"rect\">\
                   <!ELEMENT %rect.qname; EMPTY>\
                   <!ATTLIST %rect.qname; \
                       %Core.attrib; \
                       fill-rule ( nonzero | evenodd | inherit ) #IMPLIED \
                       width CDATA #REQUIRED>";
        let inv = parse(dtd);
        assert!(inv.elements.contains("rect"));
        let attrs = &inv.element_attributes["rect"];
        // From the group:
        assert!(attrs.contains_key("id"));
        // Inline enumerated + required:
        let fill_rule = &attrs["fill-rule"];
        assert_eq!(
            fill_rule.enumeration.as_ref().map(|e| e.keywords.clone()),
            Some(vec![
                "nonzero".to_string(),
                "evenodd".to_string(),
                "inherit".to_string()
            ])
        );
        assert_eq!(attrs["width"].defaulting, Defaulting::Required);
        assert_eq!(fill_rule.defaulting, Defaulting::Implied);
        // Provenance: `id` came from the Core.attrib group.
        assert!(inv.attribute_groups["id"].contains("Core.attrib"));
        // Enum surfaced at the inventory level too.
        assert!(inv.attribute_enums.contains_key("fill-rule"));
    }

    #[test]
    fn qname_first_wins_keeps_prefixless_name() {
        // Mirrors the real DTD: an earlier `%pfx;`-built qname must win over a
        // later bare redefinition.
        let dtd = "<!ENTITY % pfx \"\">\
                   <!ENTITY % svg.qname \"%pfx;svg\">\
                   <!ENTITY % svg.qname \"svg\">\
                   <!ELEMENT %svg.qname; EMPTY>";
        let inv = parse(dtd);
        assert!(inv.elements.contains("svg"));
    }
}
