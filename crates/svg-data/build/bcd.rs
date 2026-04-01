use std::{collections::HashMap, fs, path::Path};

use super::{BaselineValue, BrowserSupportValue, CompatEntry, ensure_cached, xlink};

const BCD_URL: &str = "https://unpkg.com/@mdn/browser-compat-data@latest/data.json";
const WEB_FEATURES_URL: &str = "https://unpkg.com/web-features@latest/data.json";

/// BCD-discovered attribute with compat metadata and element applicability.
pub struct BcdAttribute {
    pub compat: CompatEntry,
    /// Element names this attribute applies to. Contains `"*"` if global.
    pub elements: Vec<String>,
}

pub fn bcd_attr_applies_globally(elements: &[String]) -> bool {
    elements.iter().any(|element| element == "*")
}

pub struct CompatData {
    pub elements: HashMap<String, CompatEntry>,
    /// Attributes from BCD (global + element-specific, merged).
    pub attributes: HashMap<String, BcdAttribute>,
}

/// Build lookup maps for elements + attributes.
/// On any failure, prints a cargo warning and returns empty maps.
pub fn fetch_compat_data(out_dir: &Path) -> CompatData {
    let offline = std::env::var("SVG_DATA_OFFLINE").is_ok();
    let (bcd_ok, wf_ok, bcd_path, wf_path) = prepare_cache_paths(out_dir, offline);
    let empty_data = CompatData {
        elements: HashMap::new(),
        attributes: HashMap::new(),
    };

    if !bcd_ok {
        println!("cargo::warning=compat: no BCD data — all entries get baseline: None");
        return empty_data;
    }

    let Some(bcd_root) = read_cached_json(&bcd_path, "BCD") else {
        return empty_data;
    };
    let Some(svg_elements_obj) = svg_elements_object(&bcd_root) else {
        return empty_data;
    };
    let wf_features = read_web_features(&wf_path, wf_ok);
    let elements = collect_element_entries(svg_elements_obj, wf_features.as_ref());

    println!(
        "cargo::warning=compat: loaded {} element entries from BCD",
        elements.len()
    );

    let attributes = collect_attribute_entries(&bcd_root, svg_elements_obj, wf_features.as_ref());

    println!(
        "cargo::warning=compat: loaded {} attribute entries from BCD",
        attributes.len()
    );

    CompatData {
        elements,
        attributes,
    }
}

fn prepare_cache_paths(
    out_dir: &Path,
    offline: bool,
) -> (bool, bool, std::path::PathBuf, std::path::PathBuf) {
    let bcd_path = out_dir.join("bcd-data.json");
    let wf_path = out_dir.join("web-features-data.json");
    let bcd_ok = ensure_cached_with_warning(BCD_URL, &bcd_path, offline, "BCD");
    let wf_ok = ensure_cached_with_warning(WEB_FEATURES_URL, &wf_path, offline, "web-features");
    (bcd_ok, wf_ok, bcd_path, wf_path)
}

fn ensure_cached_with_warning(url: &str, path: &Path, offline: bool, label: &str) -> bool {
    match ensure_cached(url, path, offline) {
        Ok(value) => value,
        Err(error) => {
            println!("cargo::warning=compat: {label} fetch failed: {error}");
            false
        }
    }
}

fn read_cached_json(path: &Path, label: &str) -> Option<serde_json::Value> {
    let raw = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            println!("cargo::warning=compat: failed to read {label} cache: {error}");
            return None;
        }
    };

    match serde_json::from_str(&raw) {
        Ok(json) => Some(json),
        Err(error) => {
            println!("cargo::warning=compat: failed to parse {label} JSON: {error}");
            None
        }
    }
}

fn svg_elements_object(
    bcd_root: &serde_json::Value,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    let Some(svg_elements) = bcd_root.pointer("/svg/elements") else {
        println!("cargo::warning=compat: BCD missing /svg/elements path");
        return None;
    };
    let Some(elements) = svg_elements.as_object() else {
        println!("cargo::warning=compat: /svg/elements is not an object");
        return None;
    };
    Some(elements)
}

fn read_web_features(path: &Path, wf_ok: bool) -> Option<serde_json::Value> {
    if !wf_ok {
        return None;
    }

    let json = read_cached_json(path, "web-features")?;
    let features = json.get("features").cloned();
    if features.is_none() {
        println!("cargo::warning=compat: web-features JSON missing \"features\" key");
    }
    features
}

fn collect_element_entries(
    svg_elements_obj: &serde_json::Map<String, serde_json::Value>,
    wf_features: Option<&serde_json::Value>,
) -> HashMap<String, CompatEntry> {
    svg_elements_obj
        .iter()
        .filter_map(|(element_name, element_data)| {
            let compat = element_data.pointer("/__compat")?;
            Some((
                element_name.clone(),
                compat_entry(compat, wf_features, &format!("svg.elements.{element_name}")),
            ))
        })
        .collect()
}

fn collect_attribute_entries(
    bcd_root: &serde_json::Value,
    svg_elements_obj: &serde_json::Map<String, serde_json::Value>,
    wf_features: Option<&serde_json::Value>,
) -> HashMap<String, BcdAttribute> {
    let mut attributes = HashMap::new();
    collect_global_attributes(&mut attributes, bcd_root, wf_features);
    collect_element_specific_attributes(&mut attributes, svg_elements_obj, wf_features);
    attributes
}

fn collect_global_attributes(
    attributes: &mut HashMap<String, BcdAttribute>,
    bcd_root: &serde_json::Value,
    wf_features: Option<&serde_json::Value>,
) {
    let Some(global_attributes) = bcd_root.pointer("/svg/global_attributes") else {
        println!("cargo::warning=compat: BCD missing /svg/global_attributes path");
        return;
    };
    let Some(attribute_map) = global_attributes.as_object() else {
        println!("cargo::warning=compat: /svg/global_attributes is not an object");
        return;
    };

    for (attribute_name, attribute_data) in attribute_map {
        let Some(compat) = attribute_data.pointer("/__compat") else {
            continue;
        };
        let canonical_name = xlink::canonical_svg_attribute_name(attribute_name);
        attributes.insert(
            canonical_name.into_owned(),
            BcdAttribute {
                compat: compat_entry(
                    compat,
                    wf_features,
                    &format!("svg.global_attributes.{attribute_name}"),
                ),
                elements: vec!["*".to_string()],
            },
        );
    }
}

fn collect_element_specific_attributes(
    attributes: &mut HashMap<String, BcdAttribute>,
    svg_elements_obj: &serde_json::Map<String, serde_json::Value>,
    wf_features: Option<&serde_json::Value>,
) {
    for (element_name, element_data) in svg_elements_obj {
        let Some(element_map) = element_data.as_object() else {
            continue;
        };

        for (attribute_name, attribute_data) in element_map {
            if attribute_name == "__compat" {
                continue;
            }

            let Some(compat) = attribute_data.pointer("/__compat") else {
                continue;
            };

            let compat_entry = compat_entry(
                compat,
                wf_features,
                &format!("svg.elements.{element_name}.{attribute_name}"),
            );
            let canonical_name = xlink::canonical_svg_attribute_name(attribute_name);
            merge_attribute_entry(
                attributes,
                canonical_name.as_ref(),
                element_name,
                compat_entry,
            );
        }
    }
}

fn merge_attribute_entry(
    attributes: &mut HashMap<String, BcdAttribute>,
    attribute_name: &str,
    element_name: &str,
    compat: CompatEntry,
) {
    attributes
        .entry(attribute_name.to_string())
        .and_modify(|existing| {
            if compat.deprecated {
                existing.compat.deprecated = true;
            }
            if compat.experimental {
                existing.compat.experimental = true;
            }
            if existing.compat.spec_url.is_none() {
                existing.compat.spec_url.clone_from(&compat.spec_url);
            }
            if !bcd_attr_applies_globally(&existing.elements)
                && !existing
                    .elements
                    .iter()
                    .any(|element| element == element_name)
            {
                existing.elements.push(element_name.to_string());
            }
            match (&existing.compat.baseline, &compat.baseline) {
                (None, _) => existing.compat.baseline.clone_from(&compat.baseline),
                (Some(current), Some(new)) if new.rank() < current.rank() => {
                    existing.compat.baseline.clone_from(&compat.baseline);
                }
                _ => {}
            }
            if let Some(browser_support) = &compat.browser_support {
                merge_browser_support(&mut existing.compat.browser_support, browser_support);
            }
        })
        .or_insert_with(|| BcdAttribute {
            compat,
            elements: vec![element_name.to_string()],
        });
}

fn compat_entry(
    compat: &serde_json::Value,
    wf_features: Option<&serde_json::Value>,
    compat_key: &str,
) -> CompatEntry {
    CompatEntry {
        deprecated: compat
            .pointer("/status/deprecated")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        experimental: compat
            .pointer("/status/experimental")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        spec_url: extract_spec_url(compat),
        baseline: extract_baseline(compat, wf_features, compat_key),
        browser_support: extract_browser_support(compat),
    }
}

/// Given a BCD `__compat` object, web-features data, and BCD compat key,
/// resolve baseline status.
fn extract_baseline(
    compat: &serde_json::Value,
    wf_features: Option<&serde_json::Value>,
    compat_key: &str,
) -> Option<BaselineValue> {
    let wf = wf_features?;

    let tags = compat.get("tags")?.as_array()?;
    let feature_id = tags.iter().find_map(|tag| {
        let s = tag.as_str()?;
        s.strip_prefix("web-features:")
    })?;

    let feature = wf.get(feature_id)?;
    let status = feature.get("status")?;

    if let Some(by_key) = status.get("by_compat_key")
        && let Some(override_status) = by_key.get(compat_key)
    {
        return parse_baseline_value(override_status);
    }

    parse_baseline_value(status)
}

fn parse_baseline_value(status: &serde_json::Value) -> Option<BaselineValue> {
    match status.get("baseline")? {
        serde_json::Value::Bool(false) => Some(BaselineValue::Limited),
        serde_json::Value::String(s) if s == "high" => {
            let year = extract_year(status, "baseline_high_date")?;
            Some(BaselineValue::Widely { since: year })
        }
        serde_json::Value::String(s) if s == "low" => {
            let year = extract_year(status, "baseline_low_date")?;
            Some(BaselineValue::Newly { since: year })
        }
        _ => None,
    }
}

fn merge_browser_support(existing: &mut Option<BrowserSupportValue>, new: &BrowserSupportValue) {
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

fn extract_browser_support(compat: &serde_json::Value) -> Option<BrowserSupportValue> {
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

    let value = BrowserSupportValue {
        chrome: version_added("chrome"),
        edge: version_added("edge"),
        firefox: version_added("firefox"),
        safari: version_added("safari"),
    };
    if value.chrome.is_none()
        && value.edge.is_none()
        && value.firefox.is_none()
        && value.safari.is_none()
    {
        None
    } else {
        Some(value)
    }
}

fn extract_spec_url(compat: &serde_json::Value) -> Option<String> {
    match compat.get("spec_url")? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(arr) => arr.first().and_then(|v| v.as_str()).map(String::from),
        _ => None,
    }
}

fn extract_year(status: &serde_json::Value, key: &str) -> Option<u16> {
    let date_str = status.get(key)?.as_str()?;
    let year_str = date_str.split('-').next()?;
    year_str.parse::<u16>().ok()
}
