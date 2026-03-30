use super::{BaselineStatus, CompatOverride, HashMap, RuntimeBrowserSupport, RuntimeCompat};

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
    CompatOverride {
        deprecated: compat
            .pointer("/status/deprecated")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        experimental: compat
            .pointer("/status/experimental")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        baseline: resolve_baseline(compat, wf_features, compat_key),
        browser_support: extract_runtime_browser_support(compat),
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

fn resolve_baseline(
    compat: &serde_json::Value,
    wf_features: Option<&serde_json::Value>,
    compat_key: &str,
) -> Option<BaselineStatus> {
    let wf = wf_features?;
    let tags = compat.get("tags")?.as_array()?;
    let feature_id = tags
        .iter()
        .find_map(|t| t.as_str()?.strip_prefix("web-features:"))?;
    let status = wf.get(feature_id)?.get("status")?;

    if let Some(by_key) = status.get("by_compat_key")
        && let Some(override_status) = by_key.get(compat_key)
    {
        return parse_baseline_value(override_status);
    }

    parse_baseline_value(status)
}

fn parse_baseline_value(status: &serde_json::Value) -> Option<BaselineStatus> {
    match status.get("baseline")? {
        serde_json::Value::Bool(false) => Some(BaselineStatus::Limited),
        serde_json::Value::String(s) if s == "high" => {
            let since = parse_year(status, "baseline_high_date")?;
            Some(BaselineStatus::Widely { since })
        }
        serde_json::Value::String(s) if s == "low" => {
            let since = parse_year(status, "baseline_low_date")?;
            Some(BaselineStatus::Newly { since })
        }
        _ => None,
    }
}

fn parse_year(status: &serde_json::Value, key: &str) -> Option<u16> {
    status.get(key)?.as_str()?.split('-').next()?.parse().ok()
}

fn extract_runtime_browser_support(compat: &serde_json::Value) -> Option<RuntimeBrowserSupport> {
    let support = compat.get("support")?;

    let version_added = |browser: &str| -> Option<String> {
        let entry = support.get(browser)?;
        let stmt = if entry.is_array() {
            entry.get(0)?
        } else {
            entry
        };
        stmt.get("version_added")?.as_str().map(String::from)
    };

    Some(RuntimeBrowserSupport {
        chrome: version_added("chrome"),
        edge: version_added("edge"),
        firefox: version_added("firefox"),
        safari: version_added("safari"),
    })
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
