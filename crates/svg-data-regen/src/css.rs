//! External CSS/SVG property-definition extraction.
//!
//! SVG 2's definitions graph points some presentation attributes at CSS specs
//! instead of local SVGWG chapters. Those hrefs are part of the SVG source
//! graph, so follow the exact URLs the pinned SVGWG definitions reference and
//! extract only the property definitions SVG asked for.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    chapter::{self, PropertyValueDef},
    extract::{AttributeRef, Definitions},
    fetch,
};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// Property definitions extracted from external pages.
pub struct ExternalPropertyDefinitions {
    /// Extracted property value grammars.
    pub properties: Vec<PropertyValueDef>,
    /// Per-page extraction report.
    pub pages: Vec<ExternalPropertyPageReport>,
    /// Number of distinct external property names requested by SVGWG.
    pub requested_count: usize,
}

/// Extraction stats for one external property page.
pub struct ExternalPropertyPageReport {
    /// Page URL without fragment.
    pub url: String,
    /// Number of requested property names for this page.
    pub requested: usize,
    /// Number of requested property names found as propdef tables.
    pub matched: usize,
}

/// Fetch and extract external property definitions referenced by SVGWG.
///
/// # Errors
/// Returns an error if a referenced page cannot be fetched or parsed.
pub fn fetch_external_property_defs(
    modules: &[Definitions],
    editors_draft_base: &str,
) -> Fallible<ExternalPropertyDefinitions> {
    let pages = external_property_pages(modules, editors_draft_base);
    let requested_count = pages.values().map(BTreeSet::len).sum();
    // Key on `(name, bearer element)` so element-scoped attribute definitions
    // whose value grammar diverges by bearer (e.g. `operator` on `feComposite`
    // vs `feMorphology`) are all retained; same-name definitions that share a
    // bearer (or carry none) still collapse to one.
    let mut properties_by_key: BTreeMap<(String, Option<String>), PropertyValueDef> =
        BTreeMap::new();
    let mut reports = Vec::new();

    for (url, requested_names) in pages {
        let html = fetch::url_text(&url, "text/html")?;
        let properties = extract_requested_properties(&html, &requested_names);
        let missing = missing_requested_names(&requested_names, &properties);
        if !missing.is_empty() {
            return Err(format!(
                "external property extraction missed {} requested definition(s) from {url}: {}",
                missing.len(),
                missing.join(", ")
            )
            .into());
        }
        let matched = properties.len();
        for property in properties {
            properties_by_key.insert((property.name.clone(), property.dfn_for.clone()), property);
        }
        reports.push(ExternalPropertyPageReport {
            url,
            requested: requested_names.len(),
            matched,
        });
    }

    Ok(ExternalPropertyDefinitions {
        properties: properties_by_key.into_values().collect(),
        pages: reports,
        requested_count,
    })
}

fn external_property_pages(
    modules: &[Definitions],
    editors_draft_base: &str,
) -> BTreeMap<String, BTreeSet<String>> {
    let presentation_names = presentation_property_names(modules);
    let mut requested_by_name: BTreeMap<String, String> = BTreeMap::new();
    for module in modules {
        let base = module.anchor_base.as_deref().unwrap_or(editors_draft_base);
        for property in &module.properties {
            if !presentation_names.contains(property.name.as_str()) {
                continue;
            }
            let Some(href) = property.href.as_deref() else {
                continue;
            };
            let url = resolve_url(base, href);
            if should_fetch_external_property_url(&url, editors_draft_base) {
                requested_by_name.insert(normalized_property_name(&property.name), url);
            } else if let Some(url) =
                prose_backed_external_property_url(property.name.as_str(), href)
            {
                requested_by_name.insert(normalized_property_name(&property.name), url.to_owned());
            }
        }
        for attribute in external_attribute_refs(module) {
            let Some(href) = attribute.href.as_deref() else {
                continue;
            };
            let url = resolve_url(base, href);
            if should_fetch_external_attribute_url(&url, editors_draft_base) {
                requested_by_name.insert(normalized_property_name(&attribute.name), url);
            }
        }
    }

    let mut pages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, url) in requested_by_name {
        pages.entry(page_url(&url)).or_default().insert(name);
    }
    pages
}

fn external_attribute_refs(module: &Definitions) -> impl Iterator<Item = &AttributeRef> {
    module
        .global_attributes
        .iter()
        .chain(
            module
                .attribute_categories
                .iter()
                .flat_map(|category| category.attributes.iter()),
        )
        .chain(
            module
                .elements
                .iter()
                .flat_map(|element| element.attributes.iter()),
        )
}

fn presentation_property_names(modules: &[Definitions]) -> BTreeSet<&str> {
    modules
        .iter()
        .flat_map(|module| &module.attribute_categories)
        .filter(|category| category.name == "presentation")
        .flat_map(|category| category.presentation_attributes.iter().map(String::as_str))
        .collect()
}

fn extract_requested_properties(
    html: &str,
    requested_names: &BTreeSet<String>,
) -> Vec<PropertyValueDef> {
    let extracted = chapter::extract_property_definitions(html);
    let requested: BTreeSet<&str> = requested_names.iter().map(String::as_str).collect();
    extracted
        .into_iter()
        .filter(|property| requested.contains(normalized_property_name(&property.name).as_str()))
        .map(|mut property| {
            property.name = normalized_property_name(&property.name);
            property
        })
        .collect()
}

fn missing_requested_names(
    requested_names: &BTreeSet<String>,
    properties: &[PropertyValueDef],
) -> Vec<String> {
    let matched: BTreeSet<String> = properties
        .iter()
        .map(|property| normalized_property_name(&property.name))
        .collect();
    requested_names
        .difference(&matched)
        .cloned()
        .collect::<Vec<_>>()
}

fn should_fetch_external_property_url(url: &str, editors_draft_base: &str) -> bool {
    is_absolute_url(url)
        && (editors_draft_base.is_empty() || !same_editor_draft_base(url, editors_draft_base))
}

fn should_fetch_external_attribute_url(url: &str, editors_draft_base: &str) -> bool {
    should_fetch_external_property_url(url, editors_draft_base) && is_css_spec_url(url)
}

fn is_css_spec_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.contains("wai-aria") || lower.contains("/svg") {
        return false;
    }
    lower.contains("drafts.csswg.org") || (lower.contains("w3.org/tr/") && lower.contains("css"))
}

fn same_editor_draft_base(url: &str, editors_draft_base: &str) -> bool {
    strip_scheme(url).starts_with(strip_scheme(editors_draft_base))
}

fn strip_scheme(url: &str) -> &str {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
}

fn prose_backed_external_property_url(name: &str, href: &str) -> Option<&'static str> {
    // SVG 2 render.html keeps these definitions local in definitions.xml but
    // the prose delegates their value definitions to CSS 2.1:
    // "See the CSS 2.1 specification for the definitions of display and
    // visibility", and "`overflow` ... has the same parameter values ... as
    // defined in CSS 2.1".
    match (name, href) {
        ("display", "render.html#VisibilityControl") => {
            Some("https://www.w3.org/TR/2011/REC-CSS2-20110607/visuren.html#propdef-display")
        }
        ("visibility", "render.html#VisibilityControl") => {
            Some("https://www.w3.org/TR/2011/REC-CSS2-20110607/visufx.html#propdef-visibility")
        }
        ("overflow", "render.html#OverflowAndClipProperties") => {
            Some("https://www.w3.org/TR/2011/REC-CSS2-20110607/visufx.html#propdef-overflow")
        }
        _ => None,
    }
}

fn is_absolute_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn resolve_url(base: &str, href: &str) -> String {
    if is_absolute_url(href) {
        href.to_owned()
    } else {
        format!("{base}{href}")
    }
}

fn page_url(url: &str) -> String {
    url.split_once('#')
        .map_or(url, |(page, _fragment)| page)
        .to_owned()
}

fn normalized_property_name(name: &str) -> String {
    name.trim().trim_matches('\'').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{AttributeCategory, Definitions, PropertyDef};

    #[test]
    fn groups_external_property_hrefs_by_page() {
        let modules = [
            Definitions {
                anchor_base: None,
                properties: vec![
                    PropertyDef {
                        name: "fill".to_owned(),
                        href: Some("painting.html#FillProperty".to_owned()),
                    },
                    PropertyDef {
                        name: "font-style".to_owned(),
                        href: Some("https://www.w3.org/TR/css-fonts-3/#font-style-prop".to_owned()),
                    },
                    PropertyDef {
                        name: "display".to_owned(),
                        href: Some("render.html#VisibilityControl".to_owned()),
                    },
                ],
                attribute_categories: vec![AttributeCategory {
                    name: "presentation".to_owned(),
                    href: None,
                    attributes: Vec::new(),
                    presentation_attributes: vec![
                        "display".to_owned(),
                        "fill".to_owned(),
                        "font-style".to_owned(),
                    ],
                }],
                ..Definitions::default()
            },
            Definitions {
                anchor_base: Some("https://drafts.csswg.org/css-masking-1/".to_owned()),
                properties: vec![PropertyDef {
                    name: "clip-rule".to_owned(),
                    href: Some("#propdef-clip-rule".to_owned()),
                }],
                attribute_categories: vec![AttributeCategory {
                    name: "presentation".to_owned(),
                    href: None,
                    attributes: Vec::new(),
                    presentation_attributes: vec!["clip-rule".to_owned()],
                }],
                ..Definitions::default()
            },
        ];

        let pages = external_property_pages(&modules, "https://w3c.github.io/svgwg/svg2-draft/");
        assert_eq!(pages.len(), 3);
        assert!(
            pages
                .get("https://www.w3.org/TR/css-fonts-3/")
                .is_some_and(|names| names.contains("font-style"))
        );
        assert!(
            pages
                .get("https://www.w3.org/TR/2011/REC-CSS2-20110607/visuren.html")
                .is_some_and(|names| names.contains("display"))
        );
        assert!(
            pages
                .get("https://drafts.csswg.org/css-masking-1/")
                .is_some_and(|names| names.contains("clip-rule"))
        );
    }

    #[test]
    fn extracts_only_requested_properties_from_external_page() {
        let html = r#"
<table class="propdef">
  <tr><td>Name:</td><td><dfn id="propdef-font-style">font-style</dfn></td></tr>
  <tr><td>Value:</td><td>normal | italic | oblique</td></tr>
</table>
<table class="def propdef">
  <tr><th>Name:</th><td><dfn id="propdef-font-weight">font-weight</dfn></td></tr>
  <tr><th>Value:</th><td>normal | bold | bolder | lighter</td></tr>
</table>"#;
        let requested = BTreeSet::from(["font-style".to_owned()]);
        let properties = extract_requested_properties(html, &requested);

        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].name, "font-style");
        assert_eq!(
            properties[0].value.as_deref(),
            Some("normal | italic | oblique")
        );
    }

    #[test]
    fn reports_requested_external_properties_that_were_not_extracted() {
        let html = r#"
<table class="propdef">
  <tr><td>Name:</td><td><dfn id="propdef-font-style">font-style</dfn></td></tr>
  <tr><td>Value:</td><td>normal | italic | oblique</td></tr>
</table>"#;
        let requested = BTreeSet::from(["font-style".to_owned(), "font-weight".to_owned()]);
        let properties = extract_requested_properties(html, &requested);

        assert_eq!(
            missing_requested_names(&requested, &properties),
            ["font-weight"]
        );
    }
}
