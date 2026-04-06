use std::{collections::HashMap, fs, path::Path};

use super::ensure_cached;

/// Pinned `w3c/svgwg` revision for reproducible spec scraping.
///
/// Update workflow:
/// 1. `git ls-remote https://github.com/w3c/svgwg refs/heads/main`
/// 2. Replace this SHA with the new commit hash.
/// 3. Rebuild/test `svg-data` to refresh generated descriptions.
const SVGWG_SHA: &str = "bd0b7819e8ce69d06e08b4710a18d46ac7252787";

/// Base URL for raw svgwg spec HTML files on GitHub.
const SVGWG_RAW: &str = "https://raw.githubusercontent.com/w3c/svgwg";

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

/// SVG Animations spec path (separate module in the same repo).
const SVGWG_ANIM_URL: &str = "specs/animations/master/Overview.html";

/// Fetch element descriptions from the W3C svgwg spec sources.
/// Returns a map of element name → spec description (HTML stripped).
pub fn fetch_spec_descriptions(out_dir: &Path, offline: bool) -> HashMap<String, String> {
    let mut descriptions = HashMap::new();

    // Fetch each spec file and extract descriptions
    for file in SVGWG_SPEC_FILES {
        let url = format!("{SVGWG_RAW}/{SVGWG_SHA}/master/{file}");
        let cache_name = format!("svgwg-{file}");
        let cache_path = out_dir.join(&cache_name);

        match ensure_cached(&url, &cache_path, offline) {
            Ok(true) => match fs::read_to_string(&cache_path) {
                Ok(html) => extract_element_descriptions(&html, &mut descriptions),
                Err(e) => println!(
                    "cargo::warning=spec: failed to read cache {}: {e}",
                    cache_path.display()
                ),
            },
            Ok(false) => {}
            Err(e) => {
                println!("cargo::warning=spec: failed to fetch {file}: {e}");
            }
        }
    }

    // Fetch animations spec
    let anim_url = format!("{SVGWG_RAW}/{SVGWG_SHA}/{SVGWG_ANIM_URL}");
    let anim_cache = out_dir.join("svgwg-animations.html");
    match ensure_cached(&anim_url, &anim_cache, offline) {
        Ok(true) => match fs::read_to_string(&anim_cache) {
            Ok(html) => extract_element_descriptions(&html, &mut descriptions),
            Err(e) => println!(
                "cargo::warning=spec: failed to read cache {}: {e}",
                anim_cache.display()
            ),
        },
        Ok(false) => {}
        Err(e) => {
            println!("cargo::warning=spec: failed to fetch animations spec: {e}");
        }
    }

    println!(
        "svg-data: loaded {} element descriptions from svgwg",
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
            if is_interface_svg_identifier(raw_id) {
                search_from = abs_pos + "Element\">".len();
                continue;
            }
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
        if is_interface_svg_identifier(elem_name) {
            search_from = after_tag + quote_end;
            continue;
        }

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
    let lower = text.to_ascii_lowercase();

    // Reject clearly non-element descriptions
    if text.starts_with("The following")
        || text.starts_with("This attribute")
        || text.starts_with("The outline of")
        || text.starts_with("Except for")
        || ((text.starts_with("A ") || text.starts_with("An ")) && lower.contains(" object"))
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

/// Truncate scraped prose to the first two likely sentences for conciseness.
///
/// This uses a lightweight heuristic intended for build-time HTML scraping: it
/// looks for a period followed by whitespace and then an uppercase character,
/// including non-ASCII uppercase letters. The heuristic is intentionally
/// imperfect and can still produce false positives around abbreviations such as
/// `e.g.`, punctuation followed by quotes, or other editorial edge cases, but
/// it keeps generated descriptions short without adding a heavy sentence parser.
fn truncate_description(text: &str) -> String {
    let mut sentences = 0;

    for (idx, ch) in text.char_indices() {
        if ch != '.' {
            continue;
        }

        let mut saw_whitespace = false;
        for next in text[idx + ch.len_utf8()..].chars() {
            if next.is_whitespace() {
                saw_whitespace = true;
                continue;
            }

            if saw_whitespace && next.is_uppercase() {
                sentences += 1;
                if sentences >= 2 {
                    return text[..=idx].to_string();
                }
            }
            break;
        }
    }

    text.to_string()
}

// If the first paragraph starts much later than this, we've likely wandered
// into examples/notes rather than the short element description.
const MAX_DESCRIPTION_SEARCH_OFFSET: usize = 2_000;

/// Extract text content of the first `<p>...</p>` block in the given HTML slice.
fn extract_first_paragraph(html: &str) -> Option<String> {
    // Call sites sometimes slice right after `...Element">`, which means
    // we are already inside the target paragraph content.
    if let Some(p_end) = html.find("</p>") {
        let first_open = html.find("<p");
        if first_open.is_none_or(|open| p_end < open) {
            if p_end <= MAX_DESCRIPTION_SEARCH_OFFSET {
                return Some(html[..p_end].to_string());
            }
            return None;
        }
    }

    // Otherwise find the first explicit <p...> start tag in this slice.
    // Accept both `<p>` and `<p ...>` while excluding tags like `<path>`.
    let mut search_from = 0;
    while let Some(rel_start) = html[search_from..].find("<p") {
        let p_start = search_from + rel_start;
        if p_start > MAX_DESCRIPTION_SEARCH_OFFSET {
            return None;
        }

        let tag_tail_start = p_start + 2;
        let next = *html.as_bytes().get(tag_tail_start)?;
        if next != b'>' && !next.is_ascii_whitespace() {
            search_from = tag_tail_start;
            continue;
        }

        let open_end = html[tag_tail_start..].find('>')?;
        let content_start = tag_tail_start + open_end + 1;
        let p_end = html[content_start..].find("</p>")?;
        return Some(html[content_start..content_start + p_end].to_string());
    }

    None
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

/// Convert a `PascalCase` element ID to the actual element name.
/// e.g. `Rect` → `rect`, `LinearGradient` → `linearGradient`,
///      `FeGaussianBlur` → `feGaussianBlur`
fn uncapitalize_element_name(id: &str) -> String {
    let mut chars = id.chars();
    chars.next().map_or_else(String::new, |first| {
        format!("{}{}", first.to_lowercase(), chars.as_str())
    })
}

fn is_interface_svg_identifier(raw_id: &str) -> bool {
    raw_id.contains("InterfaceSVG")
}
