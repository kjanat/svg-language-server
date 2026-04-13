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

pub fn format_element_hover_with_profile(
    el: &svg_data::ElementDef,
    profile_lifecycle: Option<String>,
    rt: Option<&CompatOverride>,
) -> String {
    let deprecated = rt.map_or(el.deprecated, |r| r.deprecated);
    let experimental = rt.map_or(el.experimental, |r| r.experimental);
    let baseline = rt
        .and_then(|r| r.baseline.as_ref())
        .or(el.baseline.as_ref());
    let show_unsupported = experimental || matches!(baseline, Some(BaselineStatus::Limited));

    let mut parts = Vec::new();

    if deprecated {
        parts.push(format!("~~{}~~", el.description));
        parts.push(String::new());
        parts.push("**Deprecated**".to_owned());
    } else if experimental {
        parts.push(el.description.to_owned());
        parts.push(String::new());
        parts.push("**Experimental**".to_owned());
    } else {
        parts.push(el.description.to_owned());
    }

    if let Some(profile_lifecycle) = profile_lifecycle {
        parts.push(String::new());
        parts.push(profile_lifecycle);
    }

    if show_unsupported
        && let Some(line) = format_unsupported_browsers_line(
            el.browser_support.as_ref(),
            rt.and_then(|r| r.browser_support.as_ref()),
        )
    {
        parts.push(String::new());
        parts.push(line);
    }

    if let Some(baseline) = baseline {
        parts.push(String::new());
        parts.push(format_baseline(*baseline));
    }

    parts.push(String::new());
    parts.push(format_browser_support_line(
        el.browser_support.as_ref(),
        rt.and_then(|r| r.browser_support.as_ref()),
    ));

    parts.push(String::new());
    let mut links = vec![format!("[MDN Reference]({})", el.mdn_url)];
    if let Some(spec_url) = el.spec_url {
        links.push(format!("[Spec]({spec_url})"));
    }
    parts.push(links.join(" | "));

    parts.join("\n")
}

pub fn format_attribute_hover_with_profile(
    attr: &svg_data::AttributeDef,
    profile_lifecycle: Option<String>,
    rt: Option<&CompatOverride>,
) -> String {
    let deprecated = rt.map_or(attr.deprecated, |r| r.deprecated);
    let experimental = rt.map_or(attr.experimental, |r| r.experimental);
    let baseline = rt
        .and_then(|r| r.baseline.as_ref())
        .or(attr.baseline.as_ref());
    let show_unsupported = experimental || matches!(baseline, Some(BaselineStatus::Limited));

    let mut parts = Vec::new();

    if deprecated {
        parts.push(format!("~~{}~~", attr.description));
        parts.push(String::new());
        parts.push("**Deprecated**".to_owned());
    } else if experimental {
        parts.push(attr.description.to_owned());
        parts.push(String::new());
        parts.push("**Experimental**".to_owned());
    } else {
        parts.push(attr.description.to_owned());
    }

    if let Some(profile_lifecycle) = profile_lifecycle {
        parts.push(String::new());
        parts.push(profile_lifecycle);
    }

    if show_unsupported
        && let Some(line) = format_unsupported_browsers_line(
            attr.browser_support.as_ref(),
            rt.and_then(|r| r.browser_support.as_ref()),
        )
    {
        parts.push(String::new());
        parts.push(line);
    }

    match &attr.values {
        svg_data::AttributeValues::Enum(vals) => {
            parts.push(String::new());
            parts.push(format!("Values: `{}`", vals.join("` | `")));
        }
        svg_data::AttributeValues::Transform(funcs) => {
            parts.push(String::new());
            parts.push(format!("Functions: `{}`", funcs.join("` | `")));
        }
        svg_data::AttributeValues::PreserveAspectRatio {
            alignments,
            meet_or_slice,
        } => {
            parts.push(String::new());
            parts.push(format!("Alignments: `{}`", alignments.join("` | `")));
            parts.push(format!("Scaling: `{}`", meet_or_slice.join("` | `")));
        }
        _ => {}
    }

    if let Some(baseline) = baseline {
        parts.push(String::new());
        parts.push(format_baseline(*baseline));
    }

    parts.push(String::new());
    parts.push(format_browser_support_line(
        attr.browser_support.as_ref(),
        rt.and_then(|r| r.browser_support.as_ref()),
    ));

    parts.push(String::new());
    let mut links = vec![format!("[MDN Reference]({})", attr.mdn_url)];
    if let Some(spec_url) = attr.spec_url {
        links.push(format!("[Spec]({spec_url})"));
    }
    parts.push(links.join(" | "));

    parts.join("\n")
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

fn format_unsupported_browsers_line(
    baked: Option<&BrowserSupport>,
    runtime: Option<&RuntimeBrowserSupport>,
) -> Option<String> {
    if baked.is_none() && runtime.is_none() {
        return None;
    }
    let mut unsupported = Vec::new();
    if matches!(
        effective_browser_version(
            baked.and_then(|b| b.chrome),
            runtime_browser_override(runtime, |support| support.chrome.as_ref()),
        ),
        BrowserVersionView::Unsupported
    ) {
        unsupported.push("Chrome");
    }
    if matches!(
        effective_browser_version(
            baked.and_then(|b| b.edge),
            runtime_browser_override(runtime, |support| support.edge.as_ref()),
        ),
        BrowserVersionView::Unsupported
    ) {
        unsupported.push("Edge");
    }
    if matches!(
        effective_browser_version(
            baked.and_then(|b| b.firefox),
            runtime_browser_override(runtime, |support| support.firefox.as_ref()),
        ),
        BrowserVersionView::Unsupported
    ) {
        unsupported.push("Firefox");
    }
    if matches!(
        effective_browser_version(
            baked.and_then(|b| b.safari),
            runtime_browser_override(runtime, |support| support.safari.as_ref()),
        ),
        BrowserVersionView::Unsupported
    ) {
        unsupported.push("Safari");
    }
    if unsupported.is_empty() {
        None
    } else {
        Some(format!("Not supported in: {}", unsupported.join(", ")))
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
            BrowserVersionView::Version(version) => format!("{name} {version}"),
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

    format!("{chrome} | {edge} | {firefox} | {safari}")
}

#[derive(Clone, Copy)]
enum BrowserVersionView<'a> {
    Unsupported,
    SupportedUnknown,
    Version(&'a str),
}

#[derive(Clone, Copy)]
enum RuntimeBrowserOverride<'a> {
    Missing,
    Unsupported,
    Supported(&'a RuntimeBrowserVersion),
}

const fn baked_browser_version(version: Option<BrowserVersion>) -> BrowserVersionView<'static> {
    match version {
        None => BrowserVersionView::Unsupported,
        Some(BrowserVersion::Unknown) => BrowserVersionView::SupportedUnknown,
        Some(BrowserVersion::Version(version)) => BrowserVersionView::Version(version),
    }
}

fn effective_browser_version(
    baked: Option<BrowserVersion>,
    runtime: RuntimeBrowserOverride<'_>,
) -> BrowserVersionView<'_> {
    match runtime {
        RuntimeBrowserOverride::Missing => baked_browser_version(baked),
        RuntimeBrowserOverride::Unsupported => BrowserVersionView::Unsupported,
        RuntimeBrowserOverride::Supported(RuntimeBrowserVersion::Version(version)) => {
            BrowserVersionView::Version(version)
        }
        RuntimeBrowserOverride::Supported(RuntimeBrowserVersion::Unknown) => match baked {
            Some(BrowserVersion::Version(version)) => BrowserVersionView::Version(version),
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

    #[test]
    fn unknown_browser_version_is_shown_as_supported() {
        let baked = BrowserSupport {
            chrome: Some(BrowserVersion::Unknown),
            edge: None,
            firefox: None,
            safari: None,
        };

        assert_eq!(
            format_unsupported_browsers_line(Some(&baked), None),
            Some("Not supported in: Edge, Firefox, Safari".to_owned())
        );
        assert_eq!(
            format_browser_support_line(Some(&baked), None),
            "Chrome supported | Edge ✗ | Firefox ✗ | Safari ✗"
        );
    }

    #[test]
    fn runtime_unknown_version_keeps_baked_known_version() {
        let baked = BrowserSupport {
            chrome: Some(BrowserVersion::Version("120")),
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
            "Chrome 120 | Edge ✗ | Firefox ✗ | Safari ✗"
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
