use std::{collections::HashMap, time::Duration};

use svg_data::BaselineStatus;

/// Runtime per-browser version data (owned strings, unlike `BrowserSupport`).
#[derive(Clone)]
pub struct RuntimeBrowserSupport {
    pub chrome: Option<String>,
    pub edge: Option<String>,
    pub firefox: Option<String>,
    pub safari: Option<String>,
}

/// Runtime compat override for a single element or attribute.
pub struct CompatOverride {
    pub deprecated: bool,
    pub experimental: bool,
    pub baseline: Option<BaselineStatus>,
    pub browser_support: Option<RuntimeBrowserSupport>,
}

/// Runtime-fetched compat data, overlays the baked-in catalog.
pub struct RuntimeCompat {
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
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build(),
    );
    let text = agent
        .get(url)
        .call()
        .map_err(|err| tracing::warn!(url, error = %err, "HTTP request failed"))
        .ok()?
        .body_mut()
        .read_to_string()
        .map_err(|err| tracing::warn!(url, error = %err, "failed to read response body"))
        .ok()?;
    serde_json::from_str(&text)
        .map_err(|err| tracing::warn!(url, error = %err, "failed to parse JSON"))
        .ok()
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
        let new_override = compat_override(
            compat,
            wf_features,
            &format!("svg.global_attributes.{attribute_name}"),
        );
        merge_compat_override(attributes.entry(attribute_name.clone()), new_override);
    }
}

fn compat_override(
    compat: &serde_json::Value,
    wf_features: Option<&serde_json::Value>,
    compat_key: &str,
) -> CompatOverride {
    let browser_support =
        svg_data::compat_parse::extract_browser_versions(compat).map(|bv| RuntimeBrowserSupport {
            chrome: bv.chrome,
            edge: bv.edge,
            firefox: bv.firefox,
            safari: bv.safari,
        });
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
            merge_baseline(&mut existing.baseline, new_override.baseline);
            if let Some(new_browser_support) = &new_override.browser_support {
                merge_runtime_browser_support(&mut existing.browser_support, new_browser_support);
            }
        })
        .or_insert(new_override);
}

const fn merge_baseline(existing: &mut Option<BaselineStatus>, new: Option<BaselineStatus>) {
    let Some(current) = *existing else {
        *existing = new;
        return;
    };
    let Some(new) = new else {
        return;
    };

    let current_rank = baseline_rank(current);
    let new_rank = baseline_rank(new);
    if new_rank < current_rank
        || (new_rank == current_rank && baseline_since(new) > baseline_since(current))
    {
        *existing = Some(new);
    }
}

fn merge_runtime_browser_support(
    existing: &mut Option<RuntimeBrowserSupport>,
    new: &RuntimeBrowserSupport,
) {
    let Some(existing) = existing.as_mut() else {
        *existing = Some(new.clone());
        return;
    };

    merge_runtime_browser_version(&mut existing.chrome, new.chrome.as_deref());
    merge_runtime_browser_version(&mut existing.edge, new.edge.as_deref());
    merge_runtime_browser_version(&mut existing.firefox, new.firefox.as_deref());
    merge_runtime_browser_version(&mut existing.safari, new.safari.as_deref());
}

fn merge_runtime_browser_version(existing: &mut Option<String>, new: Option<&str>) {
    let Some(current) = existing.as_deref() else {
        if let Some(new) = new {
            *existing = Some(new.to_owned());
        }
        return;
    };
    let Some(new) = new else {
        *existing = None;
        return;
    };

    if compare_browser_versions(new, current).is_gt() {
        *existing = Some(new.to_owned());
    }
}

fn compare_browser_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let Some((left_upper_bound, left_parts)) = parse_browser_version(left) else {
        tracing::debug!(version = left, "failed to parse browser version");
        return std::cmp::Ordering::Equal;
    };
    let Some((right_upper_bound, right_parts)) = parse_browser_version(right) else {
        tracing::debug!(version = right, "failed to parse browser version");
        return std::cmp::Ordering::Equal;
    };

    let max_len = left_parts.len().max(right_parts.len());
    for idx in 0..max_len {
        let left_part = left_parts.get(idx).copied().unwrap_or(0);
        let right_part = right_parts.get(idx).copied().unwrap_or(0);
        match left_part.cmp(&right_part) {
            std::cmp::Ordering::Equal => {}
            non_eq => return non_eq,
        }
    }

    (!left_upper_bound).cmp(&!right_upper_bound)
}

fn parse_browser_version(version: &str) -> Option<(bool, Vec<u32>)> {
    let (upper_bound, version) = version
        .strip_prefix('≤')
        .map_or((false, version), |version| (true, version));
    let parts = version
        .split('.')
        .map(str::parse)
        .collect::<Result<Vec<u32>, _>>()
        .ok()?;
    Some((upper_bound, parts))
}

const fn baseline_rank(baseline: BaselineStatus) -> u8 {
    match baseline {
        BaselineStatus::Limited => 0,
        BaselineStatus::Newly { .. } => 1,
        BaselineStatus::Widely { .. } => 2,
    }
}

const fn baseline_since(baseline: BaselineStatus) -> u16 {
    match baseline {
        BaselineStatus::Widely { since } | BaselineStatus::Newly { since } => since,
        BaselineStatus::Limited => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BaselineStatus, RuntimeBrowserSupport, merge_baseline, merge_runtime_browser_support,
    };

    #[test]
    fn merge_baseline_prefers_worse_rank() {
        let mut existing = Some(BaselineStatus::Widely { since: 2020 });
        merge_baseline(&mut existing, Some(BaselineStatus::Limited));
        assert_eq!(existing, Some(BaselineStatus::Limited));
    }

    #[test]
    fn merge_baseline_tightens_equal_rank_year() {
        let mut existing = Some(BaselineStatus::Newly { since: 2024 });
        merge_baseline(&mut existing, Some(BaselineStatus::Newly { since: 2025 }));
        assert_eq!(existing, Some(BaselineStatus::Newly { since: 2025 }));
    }

    #[test]
    fn merge_browser_support_prefers_later_and_stricter_versions() {
        let mut existing = Some(RuntimeBrowserSupport {
            chrome: Some("120".to_owned()),
            edge: Some("120".to_owned()),
            firefox: Some("115".to_owned()),
            safari: Some("≤17.2".to_owned()),
        });
        let new = RuntimeBrowserSupport {
            chrome: Some("127".to_owned()),
            edge: Some("118".to_owned()),
            firefox: Some("115".to_owned()),
            safari: Some("17.2".to_owned()),
        };

        merge_runtime_browser_support(&mut existing, &new);

        assert_eq!(
            existing
                .as_ref()
                .and_then(|support| support.chrome.as_deref()),
            Some("127")
        );
        assert_eq!(
            existing
                .as_ref()
                .and_then(|support| support.edge.as_deref()),
            Some("120")
        );
        assert_eq!(
            existing
                .as_ref()
                .and_then(|support| support.safari.as_deref()),
            Some("17.2")
        );
    }

    #[test]
    fn merge_browser_support_keeps_none_as_worst_case() {
        let mut existing = Some(RuntimeBrowserSupport {
            chrome: Some("120".to_owned()),
            edge: Some("120".to_owned()),
            firefox: Some("115".to_owned()),
            safari: Some("17.2".to_owned()),
        });
        let new = RuntimeBrowserSupport {
            chrome: None,
            edge: Some("118".to_owned()),
            firefox: Some("114".to_owned()),
            safari: Some("17.0".to_owned()),
        };

        merge_runtime_browser_support(&mut existing, &new);

        assert_eq!(
            existing
                .as_ref()
                .and_then(|support| support.chrome.as_deref()),
            None
        );
        assert_eq!(
            existing
                .as_ref()
                .and_then(|support| support.edge.as_deref()),
            Some("120")
        );
    }
}
