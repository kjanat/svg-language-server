use super::{BaselineStatus, CompatOverride, HashMap, RuntimeBrowserSupport, RuntimeCompat};

const BCD_URL: &str = "https://unpkg.com/@mdn/browser-compat-data@latest/data.json";
const WEB_FEATURES_URL: &str = "https://unpkg.com/web-features@latest/data.json";

/// Fetch BCD + web-features from unpkg, parse into a `RuntimeCompat` overlay.
/// Runs synchronously (intended for `spawn_blocking`).
pub fn fetch_runtime_compat() -> Option<RuntimeCompat> {
    let bcd_text = ureq::get(BCD_URL)
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    let bcd_json: serde_json::Value = serde_json::from_str(&bcd_text).ok()?;

    let wf_json: serde_json::Value = ureq::get(WEB_FEATURES_URL)
        .call()
        .ok()
        .and_then(|mut r| r.body_mut().read_to_string().ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);

    let wf_features = wf_json.get("features");
    let svg_elements = bcd_json.pointer("/svg/elements")?.as_object()?;

    let mut elements = HashMap::new();
    let mut attributes = HashMap::new();

    for (el_name, el_data) in svg_elements {
        if let Some(compat) = el_data.pointer("/__compat") {
            let deprecated = compat
                .pointer("/status/deprecated")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let experimental = compat
                .pointer("/status/experimental")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let compat_key = format!("svg.elements.{el_name}");
            let baseline = resolve_baseline(compat, wf_features, &compat_key);
            let browser_support = extract_runtime_browser_support(compat);
            elements.insert(
                el_name.clone(),
                CompatOverride {
                    deprecated,
                    experimental,
                    baseline,
                    browser_support,
                },
            );
        }

        if let Some(obj) = el_data.as_object() {
            for (key, val) in obj {
                if key == "__compat" {
                    continue;
                }
                if let Some(compat) = val.pointer("/__compat") {
                    let deprecated = compat
                        .pointer("/status/deprecated")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    let experimental = compat
                        .pointer("/status/experimental")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    let compat_key = format!("svg.elements.{el_name}.{key}");
                    let baseline = resolve_baseline(compat, wf_features, &compat_key);
                    let browser_support = extract_runtime_browser_support(compat);
                    attributes
                        .entry(key.clone())
                        .and_modify(|existing: &mut CompatOverride| {
                            if deprecated {
                                existing.deprecated = true;
                            }
                            if experimental {
                                existing.experimental = true;
                            }
                            match (&existing.baseline, &baseline) {
                                (None, _) => existing.baseline = baseline,
                                (Some(current), Some(new))
                                    if baseline_rank(*new) < baseline_rank(*current) =>
                                {
                                    existing.baseline = baseline;
                                }
                                _ => {}
                            }
                            if let Some(new_bs) = &browser_support {
                                merge_runtime_browser_support(
                                    &mut existing.browser_support,
                                    new_bs,
                                );
                            }
                        })
                        .or_insert(CompatOverride {
                            deprecated,
                            experimental,
                            baseline,
                            browser_support,
                        });
                }
            }
        }
    }

    if let Some(global_attrs) = bcd_json.pointer("/svg/global_attributes")
        && let Some(obj) = global_attrs.as_object()
    {
        for (attr_name, attr_data) in obj {
            if let Some(compat) = attr_data.pointer("/__compat") {
                let deprecated = compat
                    .pointer("/status/deprecated")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                let experimental = compat
                    .pointer("/status/experimental")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                let compat_key = format!("svg.global_attributes.{attr_name}");
                let baseline = resolve_baseline(compat, wf_features, &compat_key);
                let browser_support = extract_runtime_browser_support(compat);
                attributes
                    .entry(attr_name.clone())
                    .or_insert(CompatOverride {
                        deprecated,
                        experimental,
                        baseline,
                        browser_support,
                    });
            }
        }
    }

    Some(RuntimeCompat {
        elements,
        attributes,
    })
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

fn baseline_rank(baseline: BaselineStatus) -> u8 {
    match baseline {
        BaselineStatus::Limited => 0,
        BaselineStatus::Newly { .. } => 1,
        BaselineStatus::Widely { .. } => 2,
    }
}
