use std::{fmt::Write as _, sync::LazyLock};

use svg_data::{
    BaselineQualifier, BaselineStatus, BrowserSupport, BrowserVersion, ProfileLookup,
    SpecLifecycle, SpecSnapshotId,
};
use tower_lsp_server::ls_types::Uri;
use url::Url;

use crate::{
    clipboard::svg_data_uri,
    compat::{CompatOverride, RuntimeBrowserSupport, RuntimeBrowserVersion},
    positions::byte_offset_for_row_col,
    stylesheets::{ClassDefinitionHover, CustomPropertyDefinitionHover},
};

struct HoverSourceLink {
    label: String,
    target: String,
}

fn direct_hover_source_link(uri: &Uri, line: usize) -> HoverSourceLink {
    HoverSourceLink {
        label: format!("{}:{line}", uri.as_str()),
        target: uri.as_str().to_owned(),
    }
}

pub fn format_class_hover(class_name: &str, definitions: &[ClassDefinitionHover]) -> String {
    format_definition_hover(
        definitions.iter().map(|definition| {
            (
                css_rule_snippet(&definition.source, &definition.definition.span),
                hover_source_link(&definition.uri, definition.definition.span.start_row),
            )
        }),
        &format!(".{class_name}"),
    )
}

pub fn format_custom_property_hover(
    property_name: &str,
    definitions: &[CustomPropertyDefinitionHover],
) -> String {
    format_definition_hover(
        definitions.iter().map(|definition| {
            (
                css_declaration_snippet(&definition.source, &definition.definition.span),
                hover_source_link(&definition.uri, definition.definition.span.start_row),
            )
        }),
        property_name,
    )
}

fn format_definition_hover(
    definitions: impl Iterator<Item = (String, HoverSourceLink)>,
    fallback_label: &str,
) -> String {
    let sections: Vec<String> = definitions
        .map(|(snippet, source)| {
            let trimmed = snippet.trim();
            let mut section = String::new();
            if trimmed.is_empty() {
                let _ = write!(section, "`{fallback_label}`");
            } else {
                section.push_str("```css\n");
                section.push_str(trimmed);
                section.push_str("\n```");
            }
            section.push_str("\nDefined in [");
            section.push_str(&source.label);
            section.push_str("](");
            section.push_str(&source.target);
            section.push(')');
            section
        })
        .collect();

    sections.join("\n\n---\n\n")
}

fn hover_source_link(uri: &Uri, start_row: usize) -> HoverSourceLink {
    let line = start_row + 1;
    let Ok(url) = Url::parse(uri.as_str()) else {
        return direct_hover_source_link(uri, line);
    };

    match url.scheme() {
        "file" => {
            let Ok(path) = url.to_file_path() else {
                return direct_hover_source_link(uri, line);
            };

            let target = format!("{url}#L{line}");
            if let Ok(cwd) = std::env::current_dir()
                && let Ok(relative) = path.strip_prefix(&cwd)
            {
                return HoverSourceLink {
                    label: format!("{}:{line}", relative.display()),
                    target,
                };
            }

            if let Some(file_name) = path.file_name() {
                return HoverSourceLink {
                    label: format!("{}:{line}", file_name.to_string_lossy()),
                    target,
                };
            }

            HoverSourceLink {
                label: format!("{}:{line}", path.display()),
                target,
            }
        }
        "http" | "https" => {
            let host = url.host_str().unwrap_or_default();
            HoverSourceLink {
                label: format!("{host}{}:{line}", url.path()),
                target: format!("{url}#L{line}"),
            }
        }
        _ => direct_hover_source_link(uri, line),
    }
}

fn css_rule_snippet(source: &str, span: &svg_references::Span) -> String {
    let source_bytes = source.as_bytes();
    let start = byte_offset_for_row_col(source_bytes, span.start_row, span.start_col);
    if start >= source_bytes.len() {
        return String::new();
    }

    if let Some(block_open) = source_bytes[start..]
        .iter()
        .position(|&byte| byte == b'{')
        .map(|offset| start + offset)
    {
        let selector_start = source_bytes[..start]
            .iter()
            .rposition(|&byte| byte == b'}')
            .map_or(0, |idx| idx + 1);

        if let Some(block_end) = matching_brace_end(source_bytes, block_open) {
            return source[selector_start..block_end].trim().to_owned();
        }
    }

    line_text_at(source, span.start_row)
}

fn css_declaration_snippet(source: &str, span: &svg_references::Span) -> String {
    let source_bytes = source.as_bytes();
    let start = byte_offset_for_row_col(source_bytes, span.start_row, span.start_col);
    if start >= source_bytes.len() {
        return String::new();
    }

    let declaration_start = source_bytes[..start]
        .iter()
        .rposition(|&byte| matches!(byte, b';' | b'{'))
        .map_or(0, |idx| idx + 1);
    let declaration_end = source_bytes[start..]
        .iter()
        .position(|&byte| matches!(byte, b';' | b'}'))
        .map_or(source_bytes.len(), |idx| start + idx);

    source[declaration_start..declaration_end].trim().to_owned()
}

// `matching_brace_end` is a cheap brace counter for hover snippets. It does
// not handle braces inside CSS strings/comments, so edge cases may truncate the
// snippet, but that is preferable here to a full CSS reparse on hover.
fn matching_brace_end(source: &[u8], open_index: usize) -> Option<usize> {
    let mut depth = 0usize;

    for (idx, byte) in source.iter().enumerate().skip(open_index) {
        match *byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(idx + 1);
                }
            }
            _ => {}
        }
    }

    None
}

fn line_text_at(source: &str, row: usize) -> String {
    source
        .lines()
        .nth(row)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Typed sections of a compatibility hover payload. Each variant renders
/// to its own self-contained block of markdown; [`CompatMarkdownBuilder`]
/// joins them with exactly one blank line between each, so sections never
/// need to push their own leading/trailing whitespace.
///
/// The enum replaces the loose `parts.push(String::new())` pattern: the
/// blank-line discipline is encoded once in `build()` rather than repeated
/// at every call site, and the set of legal sections is closed, so future
/// additions must land here (visible to every reviewer) rather than as
/// ad-hoc string pushes.
enum HoverSection {
    /// Blockquote-quoted verdict headline. Rendered markdown looks
    /// like `> ✗ baseProfile — removed from the current SVG profile`.
    Headline(String),
    /// Plain-text MDN-style description. First prose block.
    Description(String),
    /// Consolidated `**Status:** reason · reason` line, or legacy profile-lifecycle fallback.
    Status(String),
    /// Attribute value constraints (`Values: ...`, `Functions: ...`, or paired `Alignments:`/`Scaling:`).
    /// Rendered as consecutive lines with NO blank between them.
    ValueConstraints(Vec<String>),
    /// Pre-rendered baseline row with icon data-URI.
    Baseline(String),
    /// Single-line `Chrome ≤80 · Edge ≤80 · Firefox ✗ · Safari ≤13.1` chip row.
    BrowserChips(String),
    /// Per-browser sub-bullets for partial/prefix/flags/notes caveats.
    /// Rendered as consecutive lines with NO blank between them.
    BrowserNotes(Vec<String>),
    /// Footer links (MDN · Spec), joined with ` · ` into a single line.
    Links(Vec<String>),
}

/// Structured builder for compatibility-hover markdown. Call sites push
/// [`HoverSection`]s in display order; [`Self::build`] renders each
/// section and joins with `\n\n`.
///
/// The builder is deliberately thin — it owns *ordering* and *spacing*,
/// not section content. Each `push_*` helper only accepts a pre-formatted
/// payload so that the heavy formatting (verdict glyphs, baseline icons,
/// per-browser notes) stays in its dedicated function and can be unit-
/// tested in isolation.
struct CompatMarkdownBuilder {
    sections: Vec<HoverSection>,
}

impl CompatMarkdownBuilder {
    const fn new() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    fn headline(&mut self, line: String) -> &mut Self {
        self.sections.push(HoverSection::Headline(line));
        self
    }

    fn description(&mut self, line: String) -> &mut Self {
        self.sections.push(HoverSection::Description(line));
        self
    }

    fn status(&mut self, line: String) -> &mut Self {
        self.sections.push(HoverSection::Status(line));
        self
    }

    fn value_constraints(&mut self, lines: Vec<String>) -> &mut Self {
        if !lines.is_empty() {
            self.sections.push(HoverSection::ValueConstraints(lines));
        }
        self
    }

    fn baseline(&mut self, line: String) -> &mut Self {
        self.sections.push(HoverSection::Baseline(line));
        self
    }

    fn browser_chips(&mut self, line: String) -> &mut Self {
        self.sections.push(HoverSection::BrowserChips(line));
        self
    }

    fn browser_notes(&mut self, lines: Vec<String>) -> &mut Self {
        if !lines.is_empty() {
            self.sections.push(HoverSection::BrowserNotes(lines));
        }
        self
    }

    fn links(&mut self, lines: Vec<String>) -> &mut Self {
        if !lines.is_empty() {
            self.sections.push(HoverSection::Links(lines));
        }
        self
    }

    fn build(self) -> String {
        let rendered: Vec<String> = self
            .sections
            .into_iter()
            .map(|section| match section {
                HoverSection::Headline(line)
                | HoverSection::Description(line)
                | HoverSection::Status(line)
                | HoverSection::Baseline(line)
                | HoverSection::BrowserChips(line) => line,
                HoverSection::ValueConstraints(lines) | HoverSection::BrowserNotes(lines) => {
                    lines.join("\n")
                }
                HoverSection::Links(lines) => lines.join(" · "),
            })
            .collect();
        rendered.join("\n\n")
    }
}

/// Build the `[MDN Reference](…) · [Spec](…)` link list. Keeps both call
/// sites from duplicating the tiny `spec_url` fallback.
fn hover_link_list(mdn_url: &str, spec_url: Option<&str>) -> Vec<String> {
    let mut links = vec![format!("[MDN Reference]({mdn_url})")];
    if let Some(spec_url) = spec_url {
        links.push(format!("[Spec]({spec_url})"));
    }
    links
}

/// Render the attribute value-constraint block as zero, one, or two lines
/// depending on which [`AttributeValues`] variant is present.
fn value_constraints_lines(values: &svg_data::AttributeValues) -> Vec<String> {
    match values {
        svg_data::AttributeValues::Enum(vals) => {
            vec![format!("Values: `{}`", vals.join("` | `"))]
        }
        svg_data::AttributeValues::Transform(funcs) => {
            vec![format!("Functions: `{}`", funcs.join("` | `"))]
        }
        svg_data::AttributeValues::PreserveAspectRatio {
            alignments,
            meet_or_slice,
        } => vec![
            format!("Alignments: `{}`", alignments.join("` | `")),
            format!("Scaling: `{}`", meet_or_slice.join("` | `")),
        ],
        _ => Vec::new(),
    }
}

pub fn format_element_hover_with_profile(
    el: &svg_data::ElementDef,
    profile: SpecSnapshotId,
    profile_lifecycle: Option<String>,
    rt: Option<&CompatOverride>,
) -> String {
    let baseline = rt
        .and_then(|r| r.baseline.as_ref())
        .or(el.baseline.as_ref());
    // The pre-computed verdict is the single source of truth for
    // headline + status.
    let verdict = svg_data::compat_verdict_for_element(el, profile);

    let mut builder = CompatMarkdownBuilder::new();

    if let Some(v) = verdict {
        builder.headline(format_verdict_headline(v, el.name));
    }
    builder.description(el.description.to_owned());

    if let Some(v) = verdict
        && let Some(status) = format_verdict_status(v)
    {
        builder.status(status);
    } else if let Some(profile_lifecycle) = profile_lifecycle {
        // Only fall back to the legacy profile lifecycle line when the
        // verdict layer has nothing to say — avoids contradictions like
        // "**Deprecated**" + "**Stable in Svg2EditorsDraft20250914**".
        builder.status(profile_lifecycle);
    }

    if let Some(baseline) = baseline {
        builder.baseline(format_baseline(*baseline));
    }

    builder.browser_chips(format_browser_support_line(
        el.browser_support.as_ref(),
        rt.and_then(|r| r.browser_support.as_ref()),
    ));

    if let Some(notes) = format_browser_notes_list(el.browser_support.as_ref()) {
        builder.browser_notes(notes);
    }

    builder.links(hover_link_list(el.mdn_url, el.spec_url));

    builder.build()
}

pub fn format_attribute_hover_with_profile(
    attr: &svg_data::AttributeDef,
    profile: SpecSnapshotId,
    profile_lifecycle: Option<String>,
    rt: Option<&CompatOverride>,
) -> String {
    let baseline = rt
        .and_then(|r| r.baseline.as_ref())
        .or(attr.baseline.as_ref());
    let verdict = svg_data::compat_verdict_for_attribute(attr, profile);

    let mut builder = CompatMarkdownBuilder::new();

    if let Some(v) = verdict {
        builder.headline(format_verdict_headline(v, attr.name));
    }
    builder.description(attr.description.to_owned());

    if let Some(v) = verdict
        && let Some(status) = format_verdict_status(v)
    {
        builder.status(status);
    } else if let Some(profile_lifecycle) = profile_lifecycle {
        builder.status(profile_lifecycle);
    }

    builder.value_constraints(value_constraints_lines(&attr.values));

    if let Some(baseline) = baseline {
        builder.baseline(format_baseline(*baseline));
    }

    builder.browser_chips(format_browser_support_line(
        attr.browser_support.as_ref(),
        rt.and_then(|r| r.browser_support.as_ref()),
    ));

    if let Some(notes) = format_browser_notes_list(attr.browser_support.as_ref()) {
        builder.browser_notes(notes);
    }

    builder.links(hover_link_list(attr.mdn_url, attr.spec_url));

    builder.build()
}

pub fn profile_lifecycle_hover_line<T>(
    profile: SpecSnapshotId,
    lookup: &ProfileLookup<T>,
) -> Option<String> {
    match lookup {
        ProfileLookup::Present { lifecycle, .. } => {
            Some(format_profile_lifecycle_line(profile, *lifecycle))
        }
        ProfileLookup::UnsupportedInProfile { known_in } => {
            Some(format_unsupported_profile_lifecycle_line(profile, known_in))
        }
        ProfileLookup::Unknown => None,
    }
}

fn format_profile_lifecycle_line(profile: SpecSnapshotId, lifecycle: SpecLifecycle) -> String {
    let label = match lifecycle {
        SpecLifecycle::Stable => "Stable",
        SpecLifecycle::Experimental => "Experimental",
        SpecLifecycle::Deprecated => "Deprecated",
        SpecLifecycle::Obsolete => "Obsolete",
    };
    format!("**{label} in {}**", profile.as_str())
}

fn format_unsupported_profile_lifecycle_line(
    profile: SpecSnapshotId,
    known_in: &'static [SpecSnapshotId],
) -> String {
    let Some(selected_index) = snapshot_index(profile) else {
        return format!("**Not in {}**", profile.as_str());
    };
    let first_known = known_in.first().copied();
    let last_known = known_in.last().copied();

    if let Some(last_known) = last_known
        && snapshot_index(last_known).is_some_and(|known_index| known_index < selected_index)
    {
        return format!("**Obsolete after {}**", last_known.as_str());
    }

    if let Some(first_known) = first_known
        && snapshot_index(first_known).is_some_and(|known_index| known_index > selected_index)
    {
        return format!("**Experimental in {}**", first_known.as_str());
    }

    format!("**Not in {}**", profile.as_str())
}

fn snapshot_index(snapshot: SpecSnapshotId) -> Option<usize> {
    svg_data::spec_snapshots()
        .iter()
        .position(|candidate| *candidate == snapshot)
}

fn format_external_attribute_hover(
    description: impl AsRef<str>,
    reference_label: &str,
    reference_url: &str,
) -> String {
    format!(
        "{}\n\n[{}]({})",
        description.as_ref(),
        reference_label,
        reference_url
    )
}

fn format_deprecated_external_attribute_hover(
    description: impl AsRef<str>,
    replacement: Option<&str>,
    reference_label: &str,
    reference_url: &str,
) -> String {
    let mut parts = vec![format!("~~{}~~", description.as_ref())];
    parts.push(String::new());
    parts.push("**Deprecated**".to_owned());
    if let Some(r) = replacement {
        parts.push(String::new());
        parts.push(format!("Use `{r}` instead."));
    }
    parts.push(String::new());
    parts.push(format!("[{reference_label}]({reference_url})"));
    parts.join("\n")
}

pub fn external_attribute_hover(kind: &str, attr_name: &str) -> Option<String> {
    const XML_NAMES_URL: &str = "https://www.w3.org/TR/REC-xml-names/";
    const XML_DECL_URL: &str = "https://www.w3.org/TR/xml/";

    if let Some(markdown) = xml_declaration_attribute_hover(kind, XML_DECL_URL) {
        return Some(markdown);
    }

    if let Some(markdown) = namespace_attribute_hover(attr_name, XML_NAMES_URL) {
        return Some(markdown);
    }

    let mdn_reference_url = |name: &str| {
        format!("https://developer.mozilla.org/docs/Web/SVG/Reference/Attribute/{name}")
    };

    legacy_svg_attribute_hover(attr_name, &mdn_reference_url)
}

fn xml_declaration_attribute_hover(kind: &str, reference_url: &str) -> Option<String> {
    let description = match kind {
        "xml_version_attribute_name" => {
            "Specifies the XML version used by the document declaration."
        }
        "xml_encoding_attribute_name" => {
            "Specifies the character encoding declared for the XML document."
        }
        "xml_standalone_attribute_name" => {
            "Declares whether the XML document relies on external markup declarations."
        }
        _ => return None,
    };

    Some(format_external_attribute_hover(
        description,
        "W3C XML Reference",
        reference_url,
    ))
}

fn namespace_attribute_hover(attr_name: &str, reference_url: &str) -> Option<String> {
    if attr_name == "xmlns" {
        return Some(format_external_attribute_hover(
            "Declares the default XML namespace for this element and its descendants.",
            "W3C Namespaces in XML",
            reference_url,
        ));
    }

    attr_name.strip_prefix("xmlns:").map(|prefix| {
        format_external_attribute_hover(
            format!(
                "Declares the `{prefix}` XML namespace prefix for this element and its descendants."
            ),
            "W3C Namespaces in XML",
            reference_url,
        )
    })
}

fn legacy_svg_attribute_hover(
    attr_name: &str,
    mdn_reference_url: &impl Fn(&str) -> String,
) -> Option<String> {
    match attr_name {
        "xml:lang" => Some(format_external_attribute_hover(
            "Specifies the natural language used by the element's text content and attribute values.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xml:space" => Some(format_external_attribute_hover(
            "Controls how XML whitespace is handled for the element's character data.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xml:base" => Some(format_external_attribute_hover(
            "Specifies the base URI used to resolve relative URLs within the element.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:href" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink form of `href` used to point at linked resources in SVG.",
            Some("href"),
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:arcrole" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink attribute that identifies the semantic role of the link arc.",
            None,
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:role" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink attribute that identifies the semantic role of the linked resource.",
            None,
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:show" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink attribute that hints how the linked resource should be presented.",
            None,
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:title" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink attribute that provides a human-readable title for the link.",
            None,
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:type" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink attribute that declares the XLink link type.",
            None,
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:actuate" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink attribute that hints when the linked resource should be traversed.",
            None,
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        _ => None,
    }
}

static BASELINE_HIGH: LazyLock<String> =
    LazyLock::new(|| svg_data_uri(include_str!("../assets/baseline-high.svg")));
static BASELINE_LOW: LazyLock<String> =
    LazyLock::new(|| svg_data_uri(include_str!("../assets/baseline-low.svg")));
static BASELINE_LIMITED: LazyLock<String> =
    LazyLock::new(|| svg_data_uri(include_str!("../assets/baseline-limited.svg")));

/// Glyph to render before a baseline year when the upstream date
/// carried a qualifier prefix — mirrors the worker's `BaselineBadge`
/// so the LSP hover surfaces `≤2021` instead of silently lying.
const fn format_baseline_qualifier(qualifier: Option<BaselineQualifier>) -> &'static str {
    match qualifier {
        Some(BaselineQualifier::Before) => "≤",
        Some(BaselineQualifier::After) => "≥",
        Some(BaselineQualifier::Approximately) => "~",
        None => "",
    }
}

fn format_baseline(baseline: BaselineStatus) -> String {
    match baseline {
        BaselineStatus::Widely { since, qualifier } => {
            let icon = &*BASELINE_HIGH;
            let q = format_baseline_qualifier(qualifier);
            format!(
                "![Baseline icon]({icon}) _Widely available across major browsers (Baseline since {q}{since})_"
            )
        }
        BaselineStatus::Newly { since, qualifier } => {
            let icon = &*BASELINE_LOW;
            let q = format_baseline_qualifier(qualifier);
            format!(
                "![Baseline icon]({icon}) _Newly available across major browsers (Baseline since {q}{since})_"
            )
        }
        BaselineStatus::Limited => {
            let icon = &*BASELINE_LIMITED;
            format!("![Baseline icon]({icon}) _Limited availability across major browsers_")
        }
    }
}

/// Sub-bullet lines describing per-browser caveats the `format_browser_support_line`
/// chip row can't express: partial implementations, vendor prefixes, version
/// removals, alternative names, runtime flags, and upstream notes.
///
/// Returns `None` when no browser has any caveat to display — callers omit
/// the whole section in that case.
fn format_browser_notes_list(baked: Option<&BrowserSupport>) -> Option<Vec<String>> {
    let support = baked?;
    let mut lines = Vec::new();
    for (name, version) in [
        ("Chrome", support.chrome),
        ("Edge", support.edge),
        ("Firefox", support.firefox),
        ("Safari", support.safari),
    ] {
        let Some(v) = version else { continue };
        // Explicit-false is already covered by the chip row's `✗`.
        if matches!(v.supported, Some(false)) {
            continue;
        }
        let mut segments: Vec<String> = Vec::new();
        if v.partial_implementation {
            // First note, if any, carries the "why" for the partial impl.
            let detail = v.notes.first().copied().unwrap_or("");
            if detail.is_empty() {
                segments.push("partial implementation".to_string());
            } else {
                segments.push(format!("partial — {detail}"));
            }
        } else if !v.notes.is_empty() {
            segments.push(v.notes.join(" · "));
        }
        if let Some(prefix) = v.prefix {
            segments.push(format!("requires `{prefix}` prefix"));
        }
        if let Some(alt) = v.alternative_name {
            segments.push(format!("ships as `{alt}`"));
        }
        if !v.flags.is_empty() {
            let names: Vec<String> = v.flags.iter().map(|f| format!("`{}`", f.name)).collect();
            segments.push(format!("behind flag {}", names.join(", ")));
        }
        if let Some(removed) = v.version_removed {
            let glyph = format_baseline_qualifier(v.version_removed_qualifier);
            segments.push(format!("removed in {glyph}{removed}"));
        }
        if !segments.is_empty() {
            lines.push(format!("- {name}: {}", segments.join("; ")));
        }
    }
    if lines.is_empty() { None } else { Some(lines) }
}

/// Render the verdict headline as a markdown blockquote.
///
/// Glyph choice maps directly to [`VerdictRecommendation`]:
///
/// | Recommendation | Glyph | Semantics |
/// |---|---|---|
/// | `Safe`    | `✓` | Use it |
/// | `Caution` | `⚠` | Use with care |
/// | `Avoid`   | `⊘` | Avoid in new work |
/// | `Forbid`  | `✗` | Do not use |
///
/// The blockquote is rendered markdown — LSP clients that support it show
/// a left border + muted background, a clean attention-grabber. Clients
/// that strip quoting still get the glyph + feature name + template text
/// on the first line.
fn format_verdict_headline(verdict: svg_data::CompatVerdict, feature_name: &str) -> String {
    let glyph = match verdict.recommendation {
        svg_data::VerdictRecommendation::Safe => "\u{2713}", // ✓
        svg_data::VerdictRecommendation::Caution => "\u{26A0}", // ⚠
        svg_data::VerdictRecommendation::Avoid => "\u{2298}", // ⊘
        svg_data::VerdictRecommendation::Forbid => "\u{2717}", // ✗
    };
    let template = if verdict.headline_template.is_empty() {
        match verdict.recommendation {
            svg_data::VerdictRecommendation::Safe => "safe to use",
            svg_data::VerdictRecommendation::Caution => "use with care",
            svg_data::VerdictRecommendation::Avoid => "avoid in new work",
            svg_data::VerdictRecommendation::Forbid => "do not use",
        }
    } else {
        verdict.headline_template
    };
    format!("> {glyph} `{feature_name}` — {template}")
}

/// Render the verdict status line — one or more reason tags joined by
/// ` · `. This consolidates the old split between `**Deprecated**` and
/// `**Stable in Svg2EditorsDraft20250914**` into a single non-contradictory
/// phrase sourced from the pre-reconciled verdict.
fn format_verdict_status(verdict: svg_data::CompatVerdict) -> Option<String> {
    if verdict.reasons.is_empty() {
        return None;
    }
    let parts: Vec<String> = verdict
        .reasons
        .iter()
        .map(|reason| format_verdict_reason(*reason))
        .collect();
    Some(format!("**Status:** {}", parts.join(" · ")))
}

fn format_verdict_reason(reason: svg_data::VerdictReason) -> String {
    match reason {
        svg_data::VerdictReason::BcdDeprecated => "deprecated".to_string(),
        svg_data::VerdictReason::BcdExperimental => "experimental".to_string(),
        svg_data::VerdictReason::ProfileObsolete { last_seen } => {
            format!("removed after `{}`", last_seen.as_str())
        }
        svg_data::VerdictReason::ProfileExperimental => "draft-only in profile".to_string(),
        svg_data::VerdictReason::BaselineLimited => "limited baseline".to_string(),
        svg_data::VerdictReason::BaselineNewly { since, qualifier } => {
            let glyph = format_baseline_qualifier(qualifier);
            format!("newly available since {glyph}{since}")
        }
        svg_data::VerdictReason::PartialImplementationIn(browser) => {
            format!("partial in {browser}")
        }
        svg_data::VerdictReason::PrefixRequiredIn { browser, prefix } => {
            format!("`{prefix}` prefix in {browser}")
        }
        svg_data::VerdictReason::BehindFlagIn(browser) => {
            format!("flagged in {browser}")
        }
        svg_data::VerdictReason::UnsupportedIn(browser) => {
            format!("no support in {browser}")
        }
        svg_data::VerdictReason::RemovedIn {
            browser,
            version,
            qualifier,
        } => {
            let glyph = format_baseline_qualifier(qualifier);
            format!("removed in {browser} {glyph}{version}")
        }
    }
}

fn format_browser_support_line(
    baked: Option<&BrowserSupport>,
    runtime: Option<&RuntimeBrowserSupport>,
) -> String {
    let fmt = |name: &str,
               baked_ver: Option<BrowserVersion>,
               rt_ver: RuntimeBrowserOverride<'_>|
     -> String {
        match effective_browser_version(baked_ver, rt_ver) {
            BrowserVersionView::Unsupported => format!("{name} \u{2717}"),
            BrowserVersionView::SupportedUnknown => format!("{name} supported"),
            BrowserVersionView::Version { version, qualifier } => {
                let glyph = format_baseline_qualifier(qualifier);
                format!("{name} {glyph}{version}")
            }
        }
    };

    let chrome = fmt(
        "Chrome",
        baked.and_then(|b| b.chrome),
        runtime_browser_override(runtime, |support| support.chrome.as_ref()),
    );
    let edge = fmt(
        "Edge",
        baked.and_then(|b| b.edge),
        runtime_browser_override(runtime, |support| support.edge.as_ref()),
    );
    let firefox = fmt(
        "Firefox",
        baked.and_then(|b| b.firefox),
        runtime_browser_override(runtime, |support| support.firefox.as_ref()),
    );
    let safari = fmt(
        "Safari",
        baked.and_then(|b| b.safari),
        runtime_browser_override(runtime, |support| support.safari.as_ref()),
    );

    // Prose-friendly bullet separator. The earlier `|` worked for a
    // fixed-width grid but reads as table syntax in rendered markdown.
    format!("{chrome} · {edge} · {firefox} · {safari}")
}

#[derive(Clone, Copy)]
enum BrowserVersionView<'a> {
    Unsupported,
    SupportedUnknown,
    Version {
        version: &'a str,
        /// `≤`/`≥`/`~` qualifier from the baked catalog. Always `None`
        /// on the runtime-override path because `RuntimeBrowserVersion`
        /// doesn't preserve the qualifier.
        qualifier: Option<BaselineQualifier>,
    },
}

#[derive(Clone, Copy)]
enum RuntimeBrowserOverride<'a> {
    Missing,
    Unsupported,
    Supported(&'a RuntimeBrowserVersion),
}

const fn baked_browser_version(version: Option<BrowserVersion>) -> BrowserVersionView<'static> {
    let Some(v) = version else {
        return BrowserVersionView::Unsupported;
    };
    if matches!(v.supported, Some(false)) {
        return BrowserVersionView::Unsupported;
    }
    if let Some(version) = v.version_added {
        return BrowserVersionView::Version {
            version,
            qualifier: v.version_qualifier,
        };
    }
    if matches!(v.supported, Some(true)) {
        return BrowserVersionView::SupportedUnknown;
    }
    BrowserVersionView::Unsupported
}

fn effective_browser_version(
    baked: Option<BrowserVersion>,
    runtime: RuntimeBrowserOverride<'_>,
) -> BrowserVersionView<'_> {
    match runtime {
        RuntimeBrowserOverride::Missing => baked_browser_version(baked),
        RuntimeBrowserOverride::Unsupported => BrowserVersionView::Unsupported,
        RuntimeBrowserOverride::Supported(RuntimeBrowserVersion::Version(version)) => {
            // Runtime overrides don't carry the qualifier; caller sees a
            // bare version string.
            BrowserVersionView::Version {
                version,
                qualifier: None,
            }
        }
        RuntimeBrowserOverride::Supported(RuntimeBrowserVersion::Unknown) => match baked {
            Some(v) if v.version_added.is_some() => BrowserVersionView::Version {
                version: v.version_added.unwrap_or(""),
                qualifier: v.version_qualifier,
            },
            _ => BrowserVersionView::SupportedUnknown,
        },
    }
}

fn runtime_browser_override<'a>(
    runtime: Option<&'a RuntimeBrowserSupport>,
    get: impl FnOnce(&'a RuntimeBrowserSupport) -> Option<&'a RuntimeBrowserVersion>,
) -> RuntimeBrowserOverride<'a> {
    runtime.map_or(RuntimeBrowserOverride::Missing, |runtime| {
        get(runtime).map_or(RuntimeBrowserOverride::Unsupported, |version| {
            RuntimeBrowserOverride::Supported(version)
        })
    })
}

#[cfg(test)]
mod tests {
    use svg_data::ProfileLookup;

    use super::*;

    fn bv_unknown() -> BrowserVersion {
        BrowserVersion {
            raw_value_added: svg_data::RawVersionAdded::Flag(true),
            supported: Some(true),
            ..BrowserVersion::EMPTY
        }
    }

    fn bv_version(version: &'static str) -> BrowserVersion {
        BrowserVersion {
            raw_value_added: svg_data::RawVersionAdded::Text(version),
            version_added: Some(version),
            ..BrowserVersion::EMPTY
        }
    }

    #[test]
    fn unknown_browser_version_is_shown_as_supported() {
        let baked = BrowserSupport {
            chrome: Some(bv_unknown()),
            edge: None,
            firefox: None,
            safari: None,
        };

        // New separator is ` · ` (prose bullet) instead of ` | `.
        assert_eq!(
            format_browser_support_line(Some(&baked), None),
            "Chrome supported · Edge ✗ · Firefox ✗ · Safari ✗"
        );
    }

    #[test]
    fn runtime_unknown_version_keeps_baked_known_version() {
        let baked = BrowserSupport {
            chrome: Some(bv_version("120")),
            edge: None,
            firefox: None,
            safari: None,
        };
        let runtime = RuntimeBrowserSupport {
            chrome: Some(RuntimeBrowserVersion::Unknown),
            edge: None,
            firefox: None,
            safari: None,
        };

        assert_eq!(
            format_browser_support_line(Some(&baked), Some(&runtime)),
            "Chrome 120 · Edge ✗ · Firefox ✗ · Safari ✗"
        );
    }

    #[test]
    fn unsupported_profile_hover_line_marks_obsolete_after_last_known_snapshot() {
        assert_eq!(
            profile_lifecycle_hover_line(
                SpecSnapshotId::Svg2EditorsDraft20250914,
                &ProfileLookup::<()>::UnsupportedInProfile {
                    known_in: &[
                        SpecSnapshotId::Svg11Rec20030114,
                        SpecSnapshotId::Svg11Rec20110816,
                    ],
                },
            ),
            Some("**Obsolete after Svg11Rec20110816**".to_owned())
        );
    }

    #[test]
    fn present_profile_hover_line_uses_selected_profile_lifecycle() {
        assert_eq!(
            profile_lifecycle_hover_line(
                SpecSnapshotId::Svg2EditorsDraft20250914,
                &ProfileLookup::Present {
                    value: (),
                    lifecycle: SpecLifecycle::Experimental,
                },
            ),
            Some("**Experimental in Svg2EditorsDraft20250914**".to_owned())
        );
    }
}
