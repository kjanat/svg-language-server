use super::ensure_cached;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Base URL for raw svgwg spec HTML files on GitHub.
const SVGWG_RAW: &str = "https://raw.githubusercontent.com/w3c/svgwg/main/master";

/// Spec HTML files and the elements they document.
const SVGWG_SPEC_FILES: &[&str] = &[
    "shapes.html",
    "struct.html",
    "text.html",
    "paths.html",
    "painting.html",
    "pservers.html",
    "linking.html",
    "interact.html",
    "embedded.html",
    "masking.html",
];

/// SVG Animations spec (separate module in the same repo).
const SVGWG_ANIM_URL: &str =
    "https://raw.githubusercontent.com/w3c/svgwg/main/specs/animations/master/Overview.html";

/// Fetch element descriptions from the W3C svgwg spec sources.
/// Returns a map of element name → spec description (HTML stripped).
pub(super) fn fetch_spec_descriptions(out_dir: &Path, offline: bool) -> HashMap<String, String> {
    let mut descriptions = HashMap::new();

    // Fetch each spec file and extract descriptions
    for file in SVGWG_SPEC_FILES {
        let url = format!("{SVGWG_RAW}/{file}");
        let cache_name = format!("svgwg-{file}");
        let cache_path = out_dir.join(&cache_name);

        match ensure_cached(&url, &cache_path, offline) {
            Ok(true) => {
                if let Ok(html) = fs::read_to_string(&cache_path) {
                    extract_element_descriptions(&html, &mut descriptions);
                }
            }
            Ok(false) => {}
            Err(e) => {
                println!("cargo::warning=spec: failed to fetch {file}: {e}");
            }
        }
    }

    // Fetch animations spec
    let anim_cache = out_dir.join("svgwg-animations.html");
    match ensure_cached(SVGWG_ANIM_URL, &anim_cache, offline) {
        Ok(true) => {
            if let Ok(html) = fs::read_to_string(&anim_cache) {
                extract_element_descriptions(&html, &mut descriptions);
            }
        }
        Ok(false) => {}
        Err(e) => {
            println!("cargo::warning=spec: failed to fetch animations spec: {e}");
        }
    }

    println!(
        "cargo::warning=spec: loaded {} element descriptions from svgwg",
        descriptions.len()
    );
    descriptions
}

/// Extract element descriptions from an svgwg spec HTML file.
///
/// Uses two strategies:
/// 1. Heading id="XxxElement" → first `<p>` after heading (most reliable)
/// 2. `<edit:with element='name'>` → first `<p>` (fallback, validated)
fn extract_element_descriptions(html: &str, out: &mut HashMap<String, String>) {
    // Strategy 1 (primary): heading id="XxxElement" followed by <p>
    let mut search_from = 0;
    while let Some(id_pos) = html[search_from..].find("Element\">") {
        let abs_pos = search_from + id_pos;
        let prefix = &html[search_from..abs_pos];
        if let Some(id_start) = prefix.rfind("id=\"") {
            let name_start = search_from + id_start + 4;
            let raw_id = &html[name_start..abs_pos + "Element".len()];
            if let Some(elem_name) = raw_id.strip_suffix("Element") {
                let elem_name = uncapitalize_element_name(elem_name);
                if !elem_name.is_empty()
                    && !out.contains_key(&elem_name)
                    && let Some(desc) =
                        extract_first_paragraph(&html[abs_pos + "Element\">".len()..])
                {
                    let clean = strip_html_tags(&desc);
                    if is_element_description(&clean, &elem_name) {
                        out.insert(elem_name, truncate_description(&clean));
                    }
                }
            }
        }
        search_from = abs_pos + "Element\">".len();
    }

    // Strategy 2 (fallback): <edit:with element='name'> followed by <p>
    let mut search_from = 0;
    while let Some(edit_pos) = html[search_from..].find("<edit:with element='") {
        let abs_pos = search_from + edit_pos;
        let after_tag = abs_pos + "<edit:with element='".len();
        let Some(quote_end) = html[after_tag..].find('\'') else {
            search_from = after_tag;
            continue;
        };
        let elem_name = &html[after_tag..after_tag + quote_end];

        if !out.contains_key(elem_name)
            && let Some(desc) = extract_first_paragraph(&html[after_tag + quote_end..])
        {
            let clean = strip_html_tags(&desc);
            if is_element_description(&clean, elem_name) {
                out.insert(elem_name.to_string(), truncate_description(&clean));
            }
        }
        search_from = after_tag + quote_end;
    }
}

/// Check if a description paragraph is actually about the element itself
/// (not about an attribute that applies to it or some other context).
fn is_element_description(text: &str, elem_name: &str) -> bool {
    // Reject clearly non-element descriptions
    if text.starts_with("The following")
        || text.starts_with("This attribute")
        || text.starts_with("The outline of")
        || text.starts_with("Except for")
    {
        return false;
    }
    // Best: description mentions the element by name
    if text.contains(&format!("'{elem_name}'")) || text.contains(&format!("<{elem_name}>")) {
        return true;
    }
    // Good: starts with typical spec description patterns
    if text.starts_with("The '") || text.starts_with("A ") || text.starts_with("An ") {
        return true;
    }
    false
}

/// Truncate a description to its first two sentences for conciseness.
fn truncate_description(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut sentences = 0;
    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] == b'.' && bytes[i + 1] == b' ' && bytes[i + 2].is_ascii_uppercase() {
            sentences += 1;
            if sentences >= 2 {
                return text[..=i].to_string();
            }
        }
    }
    text.to_string()
}

/// Extract text content of the first `<p>...</p>` block in the given HTML slice.
fn extract_first_paragraph(html: &str) -> Option<String> {
    // Skip whitespace, comments, edit: tags to find the first <p>
    let p_start = html.find("<p>")?;
    // Don't look too far (skip if >2000 chars away — probably not the description)
    if p_start > 2000 {
        return None;
    }
    let content_start = p_start + 3;
    let p_end = html[content_start..].find("</p>")?;
    Some(html[content_start..content_start + p_end].to_string())
}

/// Strip HTML tags from a string, leaving only text content.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Normalize whitespace: collapse runs of whitespace into single spaces
    let collapsed: String = result.split_whitespace().collect::<Vec<_>>().join(" ");
    // Remove surrounding single quotes from element references like 'rect'
    collapsed
        .replace("\u{2018}", "'") // left single quote
        .replace("\u{2019}", "'") // right single quote
}

/// Convert a PascalCase element ID to the actual element name.
/// e.g. "Rect" → "rect", "LinearGradient" → "linearGradient",
///      "FeGaussianBlur" → "feGaussianBlur"
fn uncapitalize_element_name(id: &str) -> String {
    let mut chars = id.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => format!("{}{}", first.to_lowercase(), chars.as_str()),
    }
}
