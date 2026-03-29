use super::{BaselineValue, BrowserSupportValue, CompatEntry, ensure_cached};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const BCD_URL: &str = "https://unpkg.com/@mdn/browser-compat-data@latest/data.json";
const WEB_FEATURES_URL: &str = "https://unpkg.com/web-features@latest/data.json";

/// Which elements a BCD-discovered attribute applies to.
pub(super) struct BcdAttribute {
    pub(super) compat: CompatEntry,
    pub(super) elements: Vec<String>,
}

pub(super) fn bcd_attr_applies_globally(elements: &[String]) -> bool {
    elements.iter().any(|element| element == "*")
}

pub(super) struct CompatData {
    pub(super) elements: HashMap<String, CompatEntry>,
    /// Attributes from BCD (global + element-specific, merged).
    pub(super) attributes: HashMap<String, BcdAttribute>,
}

/// Build lookup maps for elements + attributes.
/// On any failure, prints a cargo warning and returns empty maps.
pub(super) fn fetch_compat_data(out_dir: &Path) -> CompatData {
    let offline = std::env::var("SVG_DATA_OFFLINE").is_ok();

    let bcd_path = out_dir.join("bcd-data.json");
    let wf_path = out_dir.join("web-features-data.json");

    let bcd_ok = match ensure_cached(BCD_URL, &bcd_path, offline) {
        Ok(v) => v,
        Err(e) => {
            println!("cargo::warning=compat: BCD fetch failed: {e}");
            false
        }
    };

    let wf_ok = match ensure_cached(WEB_FEATURES_URL, &wf_path, offline) {
        Ok(v) => v,
        Err(e) => {
            println!("cargo::warning=compat: web-features fetch failed: {e}");
            false
        }
    };

    let empty = CompatData {
        elements: HashMap::new(),
        attributes: HashMap::new(),
    };

    if !bcd_ok {
        println!("cargo::warning=compat: no BCD data — all entries get baseline: None");
        return empty;
    }

    // Parse BCD: we only need svg.elements
    let bcd_raw = match fs::read_to_string(&bcd_path) {
        Ok(s) => s,
        Err(e) => {
            println!("cargo::warning=compat: failed to read BCD cache: {e}");
            return empty;
        }
    };

    let bcd_root: serde_json::Value = match serde_json::from_str(&bcd_raw) {
        Ok(v) => v,
        Err(e) => {
            println!("cargo::warning=compat: failed to parse BCD JSON: {e}");
            return empty;
        }
    };

    let svg_elements = match bcd_root.pointer("/svg/elements") {
        Some(v) => v,
        None => {
            println!("cargo::warning=compat: BCD missing /svg/elements path");
            return empty;
        }
    };

    // Parse web-features (optional — only needed for baseline mapping)
    let wf_features: Option<serde_json::Value> = if wf_ok {
        match fs::read_to_string(&wf_path) {
            Ok(s) => match serde_json::from_str::<serde_json::Value>(&s) {
                Ok(v) => v.get("features").cloned(),
                Err(e) => {
                    println!("cargo::warning=compat: failed to parse web-features JSON: {e}");
                    None
                }
            },
            Err(e) => {
                println!("cargo::warning=compat: failed to read web-features cache: {e}");
                None
            }
        }
    } else {
        None
    };

    let svg_elements_obj = match svg_elements.as_object() {
        Some(o) => o,
        None => {
            println!("cargo::warning=compat: /svg/elements is not an object");
            return empty;
        }
    };

    let mut map = HashMap::new();

    for (el_name, el_data) in svg_elements_obj {
        let compat = match el_data.pointer("/__compat") {
            Some(c) => c,
            None => continue,
        };

        // Extract status flags
        let deprecated = compat
            .pointer("/status/deprecated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let experimental = compat
            .pointer("/status/experimental")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let spec_url = extract_spec_url(compat);
        let browser_support = extract_browser_support(compat);
        let compat_key = format!("svg.elements.{el_name}");
        let baseline = extract_baseline(compat, &wf_features, &compat_key);

        map.insert(
            el_name.clone(),
            CompatEntry {
                deprecated,
                experimental,
                spec_url,
                baseline,
                browser_support,
            },
        );
    }

    println!(
        "cargo::warning=compat: loaded {} element entries from BCD",
        map.len()
    );

    // Collect attributes from BCD: global + element-specific
    let mut attr_map: HashMap<String, BcdAttribute> = HashMap::new();

    // 1) Global attributes (apply to all elements)
    if let Some(global_attrs) = bcd_root.pointer("/svg/global_attributes")
        && let Some(obj) = global_attrs.as_object()
    {
        for (attr_name, attr_data) in obj {
            let Some(compat) = attr_data.pointer("/__compat") else {
                continue;
            };
            let deprecated = compat
                .pointer("/status/deprecated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let experimental = compat
                .pointer("/status/experimental")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let spec_url = extract_spec_url(compat);
            let browser_support = extract_browser_support(compat);
            let compat_key = format!("svg.global_attributes.{attr_name}");
            let baseline = extract_baseline(compat, &wf_features, &compat_key);
            attr_map.insert(
                attr_name.clone(),
                BcdAttribute {
                    compat: CompatEntry {
                        deprecated,
                        experimental,
                        spec_url,
                        baseline,
                        browser_support,
                    },
                    elements: vec!["*".to_string()],
                },
            );
        }
    }

    // 2) Element-specific attributes (e.g. svg.elements.svg.baseProfile)
    for (el_name, el_data) in svg_elements_obj {
        let Some(el_obj) = el_data.as_object() else {
            continue;
        };
        for (key, val) in el_obj {
            if key == "__compat" {
                continue;
            }
            let Some(compat) = val.pointer("/__compat") else {
                continue;
            };
            let deprecated = compat
                .pointer("/status/deprecated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let experimental = compat
                .pointer("/status/experimental")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let spec_url = extract_spec_url(compat);
            let browser_support = extract_browser_support(compat);
            let compat_key = format!("svg.elements.{el_name}.{key}");
            let baseline = extract_baseline(compat, &wf_features, &compat_key);

            attr_map
                .entry(key.clone())
                .and_modify(|existing| {
                    // Merge: promote deprecated if any source says so
                    if deprecated {
                        existing.compat.deprecated = true;
                    }
                    // Promote experimental if any source says so
                    if experimental {
                        existing.compat.experimental = true;
                    }
                    // Keep first spec URL
                    if existing.compat.spec_url.is_none() {
                        existing.compat.spec_url = spec_url.clone();
                    }
                    // Add element association if not already global
                    if !bcd_attr_applies_globally(&existing.elements)
                        && !existing.elements.iter().any(|element| element == el_name)
                    {
                        existing.elements.push(el_name.clone());
                    }
                    // Conservative baseline merge: keep the worst (most limited)
                    match (&existing.compat.baseline, &baseline) {
                        (None, _) => existing.compat.baseline = baseline.clone(),
                        (Some(current), Some(new)) if new.rank() < current.rank() => {
                            existing.compat.baseline = baseline.clone();
                        }
                        _ => {}
                    }
                    // Conservative browser support merge: keep the lowest version per browser
                    if let Some(new_bs) = &browser_support {
                        merge_browser_support(&mut existing.compat.browser_support, new_bs);
                    }
                })
                .or_insert(BcdAttribute {
                    compat: CompatEntry {
                        deprecated,
                        experimental,
                        spec_url,
                        baseline,
                        browser_support,
                    },
                    elements: vec![el_name.clone()],
                });
        }
    }

    println!(
        "cargo::warning=compat: loaded {} attribute entries from BCD",
        attr_map.len()
    );

    CompatData {
        elements: map,
        attributes: attr_map,
    }
}

/// Given a BCD `__compat` object, web-features data, and BCD compat key,
/// resolve baseline status.
fn extract_baseline(
    compat: &serde_json::Value,
    wf_features: &Option<serde_json::Value>,
    compat_key: &str,
) -> Option<BaselineValue> {
    let wf = wf_features.as_ref()?;

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

    Some(BrowserSupportValue {
        chrome: version_added("chrome"),
        edge: version_added("edge"),
        firefox: version_added("firefox"),
        safari: version_added("safari"),
    })
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
