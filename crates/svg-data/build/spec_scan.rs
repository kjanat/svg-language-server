//! SVG 2 spec-source scanner (Rust port of `workers/svg-compat/src/spec_scan.ts`).
//!
//! Reads the **vendored** svgwg checkout under
//! `data/sources/svgwg-<sha>/master/` and extracts machine-readable
//! [`SpecFact`] records about which elements, attributes, and properties
//! SVG 2 defines, removes, or obsoletes. The output mirrors the
//! checked-in `data/reviewed/spec_removals.json`, which the
//! `reconcile_bcd_spec` build check consumes as the THIRD data source
//! alongside BCD flags and snapshot membership.
//!
//! Parser-first, not hand-curated: every fact comes from a structural
//! pattern match against the upstream source files.
//!
//! ## Sources consumed
//!
//! | File | Purpose | Parsing strategy |
//! |---|---|---|
//! | `definitions.xml` | Top-level SVG 2 inventory | `quick-xml` start-tag walk + verbatim-slice regex |
//! | `definitions-filters.xml` | Filter Effects inventory | same |
//! | `definitions-masking.xml` | CSS Masking inventory | same |
//! | `definitions-compositing.xml` | Compositing inventory | same |
//! | `definitions-animations.xml` | SMIL animation inventory (`animate`, `animateMotion`, `animateTransform`, `set`, `mpath`) | same |
//! | `text.html` | Per-property removed/obsoleted overrides | `regex` over h4 sections |
//! | `changes.html` | Changelog removals | `regex` over `<li>Removed the …</li>` |
//!
//! ## Why the XML scan still relies on a verbatim-slice regex
//!
//! The committed `provenance.text` for a definition is the **literal
//! source slice up to the closing quote of the `name` attribute**, e.g.
//! `"<element\n      name='a'"` — internal whitespace and newlines
//! preserved. A structural XML parser normalises attributes and cannot
//! reproduce that byte-exact, truncated slice. So `quick-xml` drives the
//! element/attribute/property *detection* (which tag, which name), and a
//! small regex recovers the exact `<tag … name='x'` provenance slice and
//! its 1-based start line. This keeps the Rust output byte-faithful to
//! the Deno scanner's JSON.
//!
//! ## Known spec quirks the parser handles
//!
//! - `text.html` wraps some status sentences across multiple lines — the
//!   removed/obsoleted detection runs over the whole paragraph body, not
//!   line by line.
//! - `changes.html` lists multiple removals per `<li>` — every classed
//!   `<span>` inside a Removed entry is walked.
//! - `definitions.xml` lists `glyph-orientation-horizontal` /
//!   `glyph-orientation-vertical` as `<property>` even though `text.html`
//!   marks them removed / obsoleted. Both records are emitted (in their
//!   respective lists) so consumers can see the disagreement.

use std::path::Path;

use quick_xml::{Reader, events::Event};
use regex::Regex;

/// Kind of feature the spec scanner emits facts about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FeatureKind {
    Attribute,
    Element,
    Property,
}

impl FeatureKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Attribute => "attribute",
            Self::Element => "element",
            Self::Property => "property",
        }
    }
}

/// Declared status of a feature per the spec prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SpecStatus {
    Defined,
    Removed,
    Obsoleted,
}

/// Provenance for a single scanner fact — file path (relative to
/// `master/`), 1-based line number, and the matched text verbatim.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SpecProvenance {
    pub file: String,
    pub line: u32,
    pub text: String,
}

/// One scanner fact: a feature with a declared status and where we saw it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SpecFact {
    pub name: String,
    pub kind: FeatureKind,
    pub status: SpecStatus,
    pub provenance: SpecProvenance,
}

/// Git metadata identifying the exact spec revision scanned.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourcePin {
    pub repository: String,
    pub commit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_date: Option<String>,
    pub generated_at: String,
}

/// Top-level scanner report — mirrors `spec_removals.json`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SpecReport {
    pub schema_version: u32,
    pub source_pin: SourcePin,
    pub defined_elements: Vec<SpecFact>,
    pub defined_attributes: Vec<SpecFact>,
    pub defined_properties: Vec<SpecFact>,
    pub removed_properties: Vec<SpecFact>,
    pub obsoleted_properties: Vec<SpecFact>,
    pub changelog_removals: Vec<SpecFact>,
}

/// Byte-offset → 1-based line index. `line_starts[i]` is the byte offset
/// of the first character on line `i + 1`.
struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn new(content: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (offset, byte) in content.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset + 1);
            }
        }
        Self { line_starts }
    }

    /// Last line whose start offset is `<= offset`, 1-based. Mirrors the
    /// TS binary search `lineAtOffset`. Returned as `u32` (svgwg source
    /// files are far below `u32::MAX` lines; the saturating conversion is
    /// defensive, never exercised in practice).
    fn line_at(&self, offset: usize) -> u32 {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(index) => index + 1,
            // `Err(insertion)` is the count of starts `<= offset`; the
            // line containing `offset` is `insertion` (1-based), i.e. the
            // start just below it.
            Err(insertion) => insertion,
        };
        u32::try_from(line).unwrap_or(u32::MAX)
    }
}

/// Errors from compiling the scanner's static regex set or reading a
/// vendored source file.
#[derive(Debug)]
pub enum ScanError {
    Regex(regex::Error),
    Io(std::io::Error),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Regex(error) => write!(f, "spec_scan regex compile failed: {error}"),
            Self::Io(error) => write!(f, "spec_scan source read failed: {error}"),
        }
    }
}

impl std::error::Error for ScanError {}

impl From<regex::Error> for ScanError {
    fn from(error: regex::Error) -> Self {
        Self::Regex(error)
    }
}

impl From<std::io::Error> for ScanError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// The scanner's static regex set, compiled once. Holding the compiled
/// `Regex` values avoids re-compiling per call and lets compilation
/// failures surface as a typed error instead of a panic — satisfying the
/// workspace `expect_used`/`unwrap_used` denies.
struct ScanRegexes {
    tag_strip: Regex,
    definition_tag: Regex,
    h4_property: Regex,
    h4_property_generic: Regex,
    paragraph: Regex,
    has_been_removed: Regex,
    has_been_obsoleted: Regex,
    removed_li: Regex,
    removed_span: Regex,
}

impl ScanRegexes {
    fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            tag_strip: Regex::new(r"<[^>]+>")?,
            // `<(element|attribute|property) … name='x'>` verbatim-slice
            // matcher. Group 1 = name; `match.end()` is the truncation
            // point for the provenance `text` (up to and including the
            // closing quote of `name`).
            definition_tag: Regex::new(
                r#"(?s)<(?:element|attribute|property)\s+name=['"]([^'"]+)['"]"#,
            )?,
            // `<h4 id='XxxProperty'>The <span class='property'>'name'</span>`.
            h4_property: Regex::new(
                r#"(?is)<h4\s+id=['"][^'"]*Property['"][^>]*>\s*The\s*<span[^>]*class=['"]property['"][^>]*>'?([^<']+)'?</span>"#,
            )?,
            // Variant h4 whose `id` lacks the `Property` suffix but whose
            // text still names a property span.
            h4_property_generic: Regex::new(
                r#"(?is)<h4\s+[^>]*>\s*The\s*<span[^>]*class=['"]property['"][^>]*>'?([^<']+)'?</span>[^<]*</h4>"#,
            )?,
            paragraph: Regex::new(r"(?is)<p\b[^>]*>(.*?)</p>")?,
            has_been_removed: Regex::new(r"(?is)has\s+been\s+removed\s+in\s+SVG\s*2")?,
            has_been_obsoleted: Regex::new(r"(?is)has\s+been\s+obsoleted")?,
            removed_li: Regex::new(r"(?is)<li[^>]*>\s*Removed\s+the\s+(.*?)</li>")?,
            removed_span: Regex::new(
                r#"(?s)<span\s+class=['"](element|property|attr-name)['"][^>]*>'?([^<']+?)'?</span>"#,
            )?,
        })
    }

    /// Strip HTML tags and collapse whitespace, mirroring the TS
    /// `stripHtml`.
    ///
    /// Tags are removed by replacing them with the **empty string** (not a
    /// space) — matching the TS `input.replace(/<[^>]+>/g, "")`. Replacing
    /// with a space would inject spurious whitespace between an inline
    /// element's close tag and adjacent punctuation (e.g. `</span>,` →
    /// `'name' ,` instead of `'name',`), diverging from the committed JSON.
    fn strip_html(&self, input: &str) -> String {
        let no_tags = self.tag_strip.replace_all(input, "");
        collapse_ws(&no_tags)
    }
}

fn collapse_ws(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse a `definitions*.xml` file for top-level declared features.
///
/// `quick-xml` walks the start/empty tags; for the three tags we care
/// about (`element`, `attribute`, `property`) we recover the byte-exact
/// provenance slice via the `definition_tag` regex anchored at the tag's
/// start offset, reproducing the Deno scanner's verbatim `text`.
fn parse_definitions_xml(re: &ScanRegexes, content: &str, relative_path: &str) -> Vec<SpecFact> {
    let line_index = LineIndex::new(content);
    let mut facts = Vec::new();
    let mut ignored_events = 0_u32;

    let mut reader = Reader::from_str(content);
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::Start(tag) | Event::Empty(tag)) => {
                let kind = match tag.local_name().as_ref() {
                    b"element" => FeatureKind::Element,
                    b"attribute" => FeatureKind::Attribute,
                    b"property" => FeatureKind::Property,
                    _ => continue,
                };
                // `buffer_position()` is the byte just after this tag's
                // closing `>`. The verbatim `<tag … name='x'` slice is the
                // *last* definition-tag regex match that ends at or before
                // this position — its `<` anchor is this tag's start. This
                // reproduces the Deno scanner's byte-exact provenance
                // `text` (truncated at the `name` attribute's quote) and
                // start line, with quick-xml driving the detection.
                let tag_end = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
                let Some(window) = content.get(..tag_end) else {
                    continue;
                };
                let Some(caps) = re.definition_tag.captures_iter(window).last() else {
                    continue;
                };
                let (Some(whole), Some(name)) = (caps.get(0), caps.get(1)) else {
                    continue;
                };
                facts.push(SpecFact {
                    name: name.as_str().to_string(),
                    kind,
                    status: SpecStatus::Defined,
                    provenance: SpecProvenance {
                        file: relative_path.to_string(),
                        line: line_index.line_at(whole.start()),
                        text: whole.as_str().to_string(),
                    },
                });
            }
            Ok(_) => ignored_events = ignored_events.saturating_add(1),
            Err(error) => {
                println!(
                    "cargo::warning=spec_scan: malformed XML in {relative_path} at byte {}: {error}",
                    reader.buffer_position()
                );
            }
        }
    }
    debug_assert!(ignored_events < u32::MAX);

    facts
}

struct Heading {
    name: String,
    start: usize,
}

/// Parse `text.html` for per-property removed/obsoleted status overrides.
fn parse_text_html_overrides(
    re: &ScanRegexes,
    content: &str,
    relative_path: &str,
) -> (Vec<SpecFact>, Vec<SpecFact>) {
    let line_index = LineIndex::new(content);
    let mut removed = Vec::new();
    let mut obsoleted = Vec::new();

    let mut headings: Vec<Heading> = Vec::new();
    for caps in re.h4_property.captures_iter(content) {
        let (Some(full), Some(name)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        headings.push(Heading {
            name: name.as_str().to_string(),
            start: full.start(),
        });
    }
    for caps in re.h4_property_generic.captures_iter(content) {
        let (Some(full), Some(name)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        if headings.iter().any(|h| h.start == full.start()) {
            continue;
        }
        headings.push(Heading {
            name: name.as_str().to_string(),
            start: full.start(),
        });
    }

    headings.sort_by_key(|h| h.start);

    for (index, heading) in headings.iter().enumerate() {
        let next_start = headings
            .get(index + 1)
            .map_or(content.len(), |next| next.start);
        let Some(section_body) = content.get(heading.start..next_start) else {
            continue;
        };

        for caps in re.paragraph.captures_iter(section_body) {
            let (Some(whole), Some(body)) = (caps.get(0), caps.get(1)) else {
                continue;
            };
            let body = body.as_str();
            let absolute_offset = heading.start + whole.start();
            if re.has_been_removed.is_match(body) {
                removed.push(SpecFact {
                    name: heading.name.clone(),
                    kind: FeatureKind::Property,
                    status: SpecStatus::Removed,
                    provenance: SpecProvenance {
                        file: relative_path.to_string(),
                        line: line_index.line_at(absolute_offset),
                        text: re.strip_html(body),
                    },
                });
                break;
            }
            if re.has_been_obsoleted.is_match(body) {
                obsoleted.push(SpecFact {
                    name: heading.name.clone(),
                    kind: FeatureKind::Property,
                    status: SpecStatus::Obsoleted,
                    provenance: SpecProvenance {
                        file: relative_path.to_string(),
                        line: line_index.line_at(absolute_offset),
                        text: re.strip_html(body),
                    },
                });
                break;
            }
        }
    }

    (removed, obsoleted)
}

/// Parse `changes.html` for `<li>Removed the …</li>` changelog entries.
fn parse_changes_log(re: &ScanRegexes, content: &str, relative_path: &str) -> Vec<SpecFact> {
    let line_index = LineIndex::new(content);
    let mut facts = Vec::new();

    for li in re.removed_li.captures_iter(content) {
        let (Some(whole), Some(body)) = (li.get(0), li.get(1)) else {
            continue;
        };
        let body = body.as_str();
        let li_line = line_index.line_at(whole.start());
        let provenance_text = collapse_ws(&re.strip_html(body));

        for span in re.removed_span.captures_iter(body) {
            let (Some(span_class), Some(raw_name)) = (span.get(1), span.get(2)) else {
                continue;
            };
            let kind = match span_class.as_str() {
                "attr-name" => FeatureKind::Attribute,
                "element" => FeatureKind::Element,
                "property" => FeatureKind::Property,
                _ => continue,
            };
            let name = raw_name.as_str().trim().to_string();
            if name.is_empty() {
                continue;
            }
            facts.push(SpecFact {
                name,
                kind,
                status: SpecStatus::Removed,
                provenance: SpecProvenance {
                    file: relative_path.to_string(),
                    line: li_line,
                    text: provenance_text.clone(),
                },
            });
        }
    }

    facts
}

/// Deduplicate facts by `(kind, name)` (first-seen wins), then sort by
/// `(kind, name)` — mirrors the TS `dedupe`. `kind` orders by its
/// lowercase string (`attribute` < `element` < `property`), matching
/// `localeCompare` on the JS string forms.
fn dedupe(mut facts: Vec<SpecFact>) -> Vec<SpecFact> {
    use std::collections::HashSet;
    let mut seen: HashSet<(FeatureKind, String)> = HashSet::new();
    facts.retain(|fact| seen.insert((fact.kind, fact.name.clone())));
    facts.sort_by(|a, b| {
        a.kind
            .as_str()
            .cmp(b.kind.as_str())
            .then_with(|| a.name.cmp(&b.name))
    });
    facts
}

/// Run the full scanner over a vendored svgwg `master/` directory.
///
/// `master` points at `…/svgwg-<sha>/master`. Missing files are skipped
/// (mirroring the TS `safeRead`), so a partial vendor still scans what it
/// has.
pub fn scan_svg2_spec(
    master: &Path,
    repository: &str,
    commit: &str,
    commit_date: Option<&str>,
    generated_at: &str,
) -> Result<SpecReport, ScanError> {
    let re = ScanRegexes::new()?;

    let mut defined_elements = Vec::new();
    let mut defined_attributes = Vec::new();
    let mut defined_properties = Vec::new();

    // The same FIVE definition files the snapshot inventory reads (see
    // `spec_xml::DEFINITION_FILES`): the four core inventories plus
    // `definitions-animations.xml`, which supplies the SMIL animation elements
    // (`animate`, `animateMotion`, `animateTransform`, `set`, `mpath`). Kept as
    // a literal here (not a `use super::spec_xml::…`) because the
    // `tests/spec_scan_repro.rs` reproduction harness includes this file as a
    // standalone `mod spec_scan;` with no sibling `spec_xml` to import from.
    // `safe_read` skips any file a given vendored pin does not carry, so a scan
    // pin without the animations file simply omits SMIL rather than erroring.
    for xml_file in [
        "definitions.xml",
        "definitions-filters.xml",
        "definitions-masking.xml",
        "definitions-compositing.xml",
        "definitions-animations.xml",
    ] {
        let Some(content) = safe_read(&master.join(xml_file))? else {
            continue;
        };
        for fact in parse_definitions_xml(&re, &content, xml_file) {
            match fact.kind {
                FeatureKind::Element => defined_elements.push(fact),
                FeatureKind::Attribute => defined_attributes.push(fact),
                FeatureKind::Property => defined_properties.push(fact),
            }
        }
    }

    let (removed_properties, obsoleted_properties) = safe_read(&master.join("text.html"))?
        .map_or_else(
            || (Vec::new(), Vec::new()),
            |content| parse_text_html_overrides(&re, &content, "text.html"),
        );

    let changelog_removals = safe_read(&master.join("changes.html"))?
        .map_or_else(Vec::new, |content| {
            parse_changes_log(&re, &content, "changes.html")
        });

    Ok(SpecReport {
        schema_version: 1,
        source_pin: SourcePin {
            repository: repository.to_string(),
            commit: commit.to_string(),
            commit_date: commit_date.map(str::to_string),
            generated_at: generated_at.to_string(),
        },
        defined_elements: dedupe(defined_elements),
        defined_attributes: dedupe(defined_attributes),
        defined_properties: dedupe(defined_properties),
        removed_properties: dedupe(removed_properties),
        obsoleted_properties: dedupe(obsoleted_properties),
        changelog_removals: dedupe(changelog_removals),
    })
}

fn safe_read(path: &Path) -> Result<Option<String>, std::io::Error> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
