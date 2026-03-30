use std::collections::HashMap;

use svg_data::BaselineStatus;

/// Runtime per-browser version data (owned strings, unlike `BrowserSupport`).
#[derive(Clone)]
pub(crate) struct RuntimeBrowserSupport {
    pub chrome: Option<String>,
    pub edge: Option<String>,
    pub firefox: Option<String>,
    pub safari: Option<String>,
}

/// Runtime compat override for a single element or attribute.
pub(crate) struct CompatOverride {
    pub deprecated: bool,
    pub experimental: bool,
    pub baseline: Option<BaselineStatus>,
    pub browser_support: Option<RuntimeBrowserSupport>,
}

/// Runtime-fetched compat data, overlays the baked-in catalog.
pub(crate) struct RuntimeCompat {
    pub elements: HashMap<String, CompatOverride>,
    pub attributes: HashMap<String, CompatOverride>,
}

const BCD_URL: &str = "https://unpkg.com/@mdn/browser-compat-data@latest/data.json";
const WEB_FEATURES_URL: &str = "https://unpkg.com/web-features@latest/data.json";

/// Fetch BCD + web-features from unpkg, parse into a `RuntimeCompat` overlay.
/// Runs synchronously (intended for `spawn_blocking`).
pub fn fetch_runtime_compat() -> Option<RuntimeCompat> {
    let bcd_json = fetch_json(BCD_URL)?;
    let wf_json = fetch_json(WEB_FEATURES_URL).unwrap_or(serde_json::Value::Null);
    let wf_features = wf_json.get("features");
    let svg_elements = bcd_json.pointer("/svg/elements")?.as_object()?;

    let elements = collect_element_overrides(svg_elements, wf_features);
    let mut attributes = collect_element_attribute_overrides(svg_elements, wf_features);
    apply_global_attribute_overrides(&mut attributes, &bcd_json, wf_features);

    Some(RuntimeCompat {
        elements,
        attributes,
    })
}

fn fetch_json(url: &str) -> Option<serde_json::Value> {
    let text = ureq::get(url)
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    serde_json::from_str(&text).ok()
}

fn collect_element_overrides(
    svg_elements: &serde_json::Map<String, serde_json::Value>,
    wf_features: Option<&serde_json::Value>,
) -> HashMap<String, CompatOverride> {
    svg_elements
        .iter()
        .filter_map(|(element_name, element_data)| {
            let compat = element_data.pointer("/__compat")?;
            Some((
                element_name.clone(),
                compat_override(compat, wf_features, &format!("svg.elements.{element_name}")),
            ))
        })
        .collect()
}

fn collect_element_attribute_overrides(
    svg_elements: &serde_json::Map<String, serde_json::Value>,
    wf_features: Option<&serde_json::Value>,
) -> HashMap<String, CompatOverride> {
    let mut attributes = HashMap::new();

    for (element_name, element_data) in svg_elements {
        let Some(attribute_map) = element_data.as_object() else {
            continue;
        };

        for (attribute_name, attribute_data) in attribute_map {
            if attribute_name == "__compat" {
                continue;
            }

            let Some(compat) = attribute_data.pointer("/__compat") else {
                continue;
            };

            let new_override = compat_override(
                compat,
                wf_features,
                &format!("svg.elements.{element_name}.{attribute_name}"),
            );
            merge_compat_override(attributes.entry(attribute_name.clone()), new_override);
        }
    }

    attributes
}

fn apply_global_attribute_overrides(
    attributes: &mut HashMap<String, CompatOverride>,
    bcd_json: &serde_json::Value,
    wf_features: Option<&serde_json::Value>,
) {
    let Some(global_attributes) = bcd_json.pointer("/svg/global_attributes") else {
        return;
    };
    let Some(attribute_map) = global_attributes.as_object() else {
        return;
    };

    for (attribute_name, attribute_data) in attribute_map {
        let Some(compat) = attribute_data.pointer("/__compat") else {
            continue;
        };
        attributes.entry(attribute_name.clone()).or_insert_with(|| {
            compat_override(
                compat,
                wf_features,
                &format!("svg.global_attributes.{attribute_name}"),
            )
        });
    }
}

fn compat_override(
    compat: &serde_json::Value,
    wf_features: Option<&serde_json::Value>,
    compat_key: &str,
) -> CompatOverride {
    let browser_support = svg_data::compat_parse::extract_browser_versions(compat).map(
        |(chrome, edge, firefox, safari)| RuntimeBrowserSupport {
            chrome,
            edge,
            firefox,
            safari,
        },
    );
    CompatOverride {
        deprecated: compat
            .pointer("/status/deprecated")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        experimental: compat
            .pointer("/status/experimental")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        baseline: svg_data::compat_parse::resolve_baseline(compat, wf_features, compat_key),
        browser_support,
    }
}

fn merge_compat_override(
    entry: std::collections::hash_map::Entry<'_, String, CompatOverride>,
    new_override: CompatOverride,
) {
    entry
        .and_modify(|existing| {
            if new_override.deprecated {
                existing.deprecated = true;
            }
            if new_override.experimental {
                existing.experimental = true;
            }
            match (&existing.baseline, &new_override.baseline) {
                (None, _) => existing.baseline = new_override.baseline,
                (Some(current), Some(new)) if baseline_rank(*new) < baseline_rank(*current) => {
                    existing.baseline = new_override.baseline;
                }
                _ => {}
            }
            if let Some(new_browser_support) = &new_override.browser_support {
                merge_runtime_browser_support(&mut existing.browser_support, new_browser_support);
            }
        })
        .or_insert(new_override);
}

fn merge_runtime_browser_support(
    existing: &mut Option<RuntimeBrowserSupport>,
    new: &RuntimeBrowserSupport,
) {
    let Some(existing) = existing.as_mut() else {
        *existing = Some(new.clone());
        return;
    };
    if new.chrome.is_none() {
        existing.chrome = None;
    }
    if new.edge.is_none() {
        existing.edge = None;
    }
    if new.firefox.is_none() {
        existing.firefox = None;
    }
    if new.safari.is_none() {
        existing.safari = None;
    }
}

const fn baseline_rank(baseline: BaselineStatus) -> u8 {
    match baseline {
        BaselineStatus::Limited => 0,
        BaselineStatus::Newly { .. } => 1,
        BaselineStatus::Widely { .. } => 2,
    }
}
