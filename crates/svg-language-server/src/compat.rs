use std::{collections::HashMap, time::Duration};

use svg_data::BaselineStatus;

/// Runtime support state for a single browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeBrowserVersion {
    /// The browser supports the feature, but the first version is unknown.
    Unknown,
    /// The browser supports the feature starting with the given version.
    Version(String),
}

/// Runtime per-browser version data (owned strings, unlike `BrowserSupport`).
#[derive(Clone)]
pub struct RuntimeBrowserSupport {
    pub chrome: Option<RuntimeBrowserVersion>,
    pub edge: Option<RuntimeBrowserVersion>,
    pub firefox: Option<RuntimeBrowserVersion>,
    pub safari: Option<RuntimeBrowserVersion>,
}

/// Runtime compat override for a single element or attribute.
#[derive(Clone)]
pub struct CompatOverride {
    pub deprecated: bool,
    pub experimental: bool,
    pub baseline: Option<BaselineStatus>,
    pub browser_support: Option<RuntimeBrowserSupport>,
}

/// Runtime-fetched compat data, overlays the baked-in catalog.
#[derive(Clone)]
pub struct RuntimeCompat {
    pub elements: HashMap<String, CompatOverride>,
    pub attributes: HashMap<String, CompatOverride>,
}

impl RuntimeCompat {
    /// Convert to lint-crate override maps for compat-aware diagnostics.
    pub fn to_lint_overrides(&self) -> svg_lint::LintOverrides {
        let convert = |map: &HashMap<String, CompatOverride>| {
            map.iter()
                .map(|(name, co)| {
                    (
                        name.clone(),
                        svg_lint::CompatFlags {
                            deprecated: co.deprecated,
                            experimental: co.experimental,
                        },
                    )
                })
                .collect()
        };
        svg_lint::LintOverrides {
            elements: convert(&self.elements),
            attributes: convert(&self.attributes),
        }
    }
}

const BCD_URL: &str = "https://unpkg.com/@mdn/browser-compat-data@latest/data.json";
const WEB_FEATURES_URL: &str = "https://unpkg.com/web-features@latest/data.json";

/// Fetch BCD + web-features from unpkg, parse into a `RuntimeCompat` overlay.
/// Runs synchronously (intended for `spawn_blocking`).
pub fn fetch_runtime_compat() -> Option<RuntimeCompat> {
    let bcd_json = fetch_json(BCD_URL)?;
    let wf_json = fetch_json(WEB_FEATURES_URL).unwrap_or(serde_json::Value::Null);
    let wf_features = wf_json.get("features");
    if wf_features.is_none() {
        tracing::warn!(
            url = WEB_FEATURES_URL,
            "missing /features key in web-features JSON"
        );
    }
    let svg_elements = bcd_json.pointer("/svg/elements");
    if svg_elements.is_none() {
        tracing::warn!(url = BCD_URL, "missing /svg/elements path in BCD JSON");
    }
    let svg_elements = svg_elements?.as_object()?;

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

            let canonical_name = svg_data::xlink::canonical_svg_attribute_name(attribute_name);
            let new_override = compat_override(
                compat,
                wf_features,
                &format!("svg.elements.{element_name}.{attribute_name}"),
            );
            merge_compat_override(attributes.entry(canonical_name.into_owned()), new_override);
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
        tracing::warn!(
            url = BCD_URL,
            "missing /svg/global_attributes path in BCD JSON"
        );
        return;
    };
    let Some(attribute_map) = global_attributes.as_object() else {
        return;
    };

    for (attribute_name, attribute_data) in attribute_map {
        let Some(compat) = attribute_data.pointer("/__compat") else {
            continue;
        };
        let canonical_name = svg_data::xlink::canonical_svg_attribute_name(attribute_name);
        let new_override = compat_override(
            compat,
            wf_features,
            &format!("svg.global_attributes.{attribute_name}"),
        );
        merge_compat_override(attributes.entry(canonical_name.into_owned()), new_override);
    }
}

fn compat_override(
    compat: &serde_json::Value,
    wf_features: Option<&serde_json::Value>,
    compat_key: &str,
) -> CompatOverride {
    let browser_support = svg_data::compat_parse::extract_browser_versions(compat).map(|bv| {
        let map_browser_version = |version| match version {
            Some(svg_data::compat_parse::BrowserVersion::Unknown) => {
                Some(RuntimeBrowserVersion::Unknown)
            }
            Some(svg_data::compat_parse::BrowserVersion::Version(version)) => {
                Some(RuntimeBrowserVersion::Version(version))
            }
            None => None,
        };

        RuntimeBrowserSupport {
            chrome: map_browser_version(bv.chrome),
            edge: map_browser_version(bv.edge),
            firefox: map_browser_version(bv.firefox),
            safari: map_browser_version(bv.safari),
        }
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

    merge_runtime_browser_version(&mut existing.chrome, new.chrome.as_ref());
    merge_runtime_browser_version(&mut existing.edge, new.edge.as_ref());
    merge_runtime_browser_version(&mut existing.firefox, new.firefox.as_ref());
    merge_runtime_browser_version(&mut existing.safari, new.safari.as_ref());
}

fn merge_runtime_browser_version(
    existing: &mut Option<RuntimeBrowserVersion>,
    new: Option<&RuntimeBrowserVersion>,
) {
    let Some(new) = new else {
        *existing = None;
        return;
    };

    let Some(current) = existing.as_ref() else {
        *existing = Some(new.clone());
        return;
    };

    match (current, new) {
        (RuntimeBrowserVersion::Unknown, RuntimeBrowserVersion::Version(version)) => {
            *existing = Some(RuntimeBrowserVersion::Version(version.clone()));
        }
        (RuntimeBrowserVersion::Version(current), RuntimeBrowserVersion::Version(new))
            if compare_browser_versions(new, current).is_gt() =>
        {
            *existing = Some(RuntimeBrowserVersion::Version(new.clone()));
        }
        _ => {}
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
        BaselineStatus, RuntimeBrowserSupport, RuntimeBrowserVersion,
        apply_global_attribute_overrides, collect_element_attribute_overrides, merge_baseline,
        merge_runtime_browser_support,
    };

    fn known(version: &str) -> RuntimeBrowserVersion {
        RuntimeBrowserVersion::Version(version.to_owned())
    }

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
            chrome: Some(known("120")),
            edge: Some(known("120")),
            firefox: Some(known("115")),
            safari: Some(known("≤17.2")),
        });
        let new = RuntimeBrowserSupport {
            chrome: Some(known("127")),
            edge: Some(known("118")),
            firefox: Some(known("115")),
            safari: Some(known("17.2")),
        };

        merge_runtime_browser_support(&mut existing, &new);

        assert_eq!(
            existing
                .as_ref()
                .and_then(|support| support.chrome.as_ref()),
            Some(&known("127"))
        );
        assert_eq!(
            existing.as_ref().and_then(|support| support.edge.as_ref()),
            Some(&known("120"))
        );
        assert_eq!(
            existing
                .as_ref()
                .and_then(|support| support.safari.as_ref()),
            Some(&known("17.2"))
        );
    }

    #[test]
    fn merge_browser_support_keeps_none_as_worst_case() {
        let mut existing = Some(RuntimeBrowserSupport {
            chrome: Some(known("120")),
            edge: Some(known("120")),
            firefox: Some(known("115")),
            safari: Some(known("17.2")),
        });
        let new = RuntimeBrowserSupport {
            chrome: None,
            edge: Some(known("118")),
            firefox: Some(known("114")),
            safari: Some(known("17.0")),
        };

        merge_runtime_browser_support(&mut existing, &new);

        assert_eq!(
            existing
                .as_ref()
                .and_then(|support| support.chrome.as_ref()),
            None
        );
        assert_eq!(
            existing.as_ref().and_then(|support| support.edge.as_ref()),
            Some(&known("120"))
        );
    }

    #[test]
    fn merge_browser_support_keeps_known_version_over_unknown() {
        let mut existing = Some(RuntimeBrowserSupport {
            chrome: Some(known("120")),
            edge: Some(known("120")),
            firefox: Some(known("115")),
            safari: Some(known("17.2")),
        });
        let new = RuntimeBrowserSupport {
            chrome: Some(RuntimeBrowserVersion::Unknown),
            edge: Some(RuntimeBrowserVersion::Unknown),
            firefox: Some(RuntimeBrowserVersion::Unknown),
            safari: Some(RuntimeBrowserVersion::Unknown),
        };

        merge_runtime_browser_support(&mut existing, &new);

        assert_eq!(
            existing
                .as_ref()
                .and_then(|support| support.chrome.as_ref()),
            Some(&known("120"))
        );
    }

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn global_attribute_overlay_merges_with_element_local() -> TestResult {
        let bcd_json = serde_json::json!({
            "svg": {
                "elements": {
                    "rect": {
                        "__compat": {
                            "support": { "chrome": { "version_added": "1" } },
                            "status": { "deprecated": false, "experimental": false }
                        },
                        "fill": {
                            "__compat": {
                                "support": { "chrome": { "version_added": "10" } },
                                "status": { "deprecated": false, "experimental": false }
                            }
                        }
                    }
                },
                "global_attributes": {
                    "fill": {
                        "__compat": {
                            "support": { "chrome": { "version_added": "50" } },
                            "status": { "deprecated": false, "experimental": false }
                        }
                    }
                }
            }
        });

        let svg_elements = bcd_json
            .pointer("/svg/elements")
            .ok_or("missing /svg/elements")?
            .as_object()
            .ok_or("not an object")?;
        let mut attributes = collect_element_attribute_overrides(svg_elements, None);
        apply_global_attribute_overrides(&mut attributes, &bcd_json, None);

        let fill = attributes.get("fill").ok_or("fill should be merged")?;
        // Global says chrome 50, element-local says chrome 10.
        // merge_runtime_browser_support keeps the later (stricter) version.
        assert_eq!(
            fill.browser_support
                .as_ref()
                .and_then(|s| s.chrome.as_ref()),
            Some(&known("50")),
        );
        Ok(())
    }
}
