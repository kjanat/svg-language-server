//! Shared BCD JSON parsing helpers for runtime compat overlays.
//!
//! Build-time compat parsing in `build/bcd.rs` uses structurally identical
//! logic but cannot import from this crate (build scripts compile separately).
//! When modifying these helpers, keep `build/bcd.rs` in sync.

use crate::BaselineStatus;

/// Parse a baseline status value from a web-features `status` JSON object.
///
/// Expects the `{ "baseline": "high"|"low"|false, "baseline_high_date": "YYYY-...", ... }` shape.
#[must_use]
pub fn parse_baseline_value(status: &serde_json::Value) -> Option<BaselineStatus> {
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

/// Resolve baseline status for a BCD compat entry via web-features tags.
///
/// Looks up the feature ID from BCD `tags`, finds the web-features entry,
/// and checks for compat-key-specific overrides before falling back to the
/// top-level status.
#[must_use]
pub fn resolve_baseline(
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
        && let Some(parsed) = parse_baseline_value(override_status)
    {
        return Some(parsed);
    }

    parse_baseline_value(status)
}

/// Parsed `version_added` support state for a single browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserVersion {
    /// The browser supports the feature, but the first version is unknown.
    Unknown,
    /// The browser supports the feature starting with the given version.
    Version(String),
}

/// Per-browser support data extracted from BCD support data.
pub struct BrowserVersions {
    /// Chrome support state.
    pub chrome: Option<BrowserVersion>,
    /// Edge support state.
    pub edge: Option<BrowserVersion>,
    /// Firefox support state.
    pub firefox: Option<BrowserVersion>,
    /// Safari support state.
    pub safari: Option<BrowserVersion>,
}

/// Extract `support/{browser}/version_added` for the four major browsers.
///
/// Returns `None` for browsers without support data. `version_added: true`
/// maps to [`BrowserVersion::Unknown`].
#[must_use]
pub fn extract_browser_versions(compat: &serde_json::Value) -> Option<BrowserVersions> {
    let support = compat.get("support")?;

    let version_added = |browser: &str| -> Option<String> {
        let entry = support.get(browser)?;
        if entry.is_array() {
            for stmt in entry.as_array()? {
                if let Some(version) = stmt
                    .get("version_added")
                    .and_then(serde_json::Value::as_str)
                {
                    return Some(version.to_owned());
                }
            }
            None
        } else {
            let stmt = entry;
            stmt.get("version_added")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        }
    };

    let browser_version = |browser: &str| -> Option<BrowserVersion> {
        version_added(browser)
            .map(BrowserVersion::Version)
            .or_else(|| {
                let entry = support.get(browser)?;
                let stmt = if entry.is_array() {
                    entry.get(0)?
                } else {
                    entry
                };
                match stmt.get("version_added")? {
                    serde_json::Value::Bool(true) => Some(BrowserVersion::Unknown),
                    _ => None,
                }
            })
    };

    let versions = BrowserVersions {
        chrome: browser_version("chrome"),
        edge: browser_version("edge"),
        firefox: browser_version("firefox"),
        safari: browser_version("safari"),
    };
    if versions.chrome.is_none()
        && versions.edge.is_none()
        && versions.firefox.is_none()
        && versions.safari.is_none()
    {
        None
    } else {
        Some(versions)
    }
}

/// Extract the first calendar year from a date string like `"2023-03-27"`.
#[must_use]
pub fn parse_year(status: &serde_json::Value, key: &str) -> Option<u16> {
    status.get(key)?.as_str()?.split('-').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::BaselineStatus;

    #[test]
    fn parse_baseline_value_widely() {
        let status = json!({
            "baseline": "high",
            "baseline_high_date": "2020-01-01"
        });
        let result = parse_baseline_value(&status);
        assert_eq!(result, Some(BaselineStatus::Widely { since: 2020 }));
    }

    #[test]
    fn parse_baseline_value_newly() {
        let status = json!({
            "baseline": "low",
            "baseline_low_date": "2023-06-15"
        });
        let result = parse_baseline_value(&status);
        assert_eq!(result, Some(BaselineStatus::Newly { since: 2023 }));
    }

    #[test]
    fn parse_baseline_value_limited() {
        let status = json!({ "baseline": false });
        assert_eq!(parse_baseline_value(&status), Some(BaselineStatus::Limited));
    }

    #[test]
    fn parse_baseline_value_missing() {
        let status = json!({});
        assert_eq!(parse_baseline_value(&status), None);
    }

    #[test]
    fn extract_browser_versions_with_data() {
        let compat = json!({
            "support": {
                "chrome": { "version_added": "45" },
                "firefox": { "version_added": "52" }
            }
        });
        let versions = extract_browser_versions(&compat);
        assert!(versions.is_some());
        let v = versions.as_ref();
        assert_eq!(
            v.and_then(|v| v.chrome.as_ref()),
            Some(&BrowserVersion::Version("45".to_owned()))
        );
        assert_eq!(
            v.and_then(|v| v.firefox.as_ref()),
            Some(&BrowserVersion::Version("52".to_owned()))
        );
        assert_eq!(v.and_then(|v| v.edge.as_ref()), None);
    }

    #[test]
    fn extract_browser_versions_all_none() {
        let compat = json!({ "support": {} });
        assert!(extract_browser_versions(&compat).is_none());
    }

    #[test]
    fn extract_browser_versions_array_entry() {
        let compat = json!({
            "support": {
                "chrome": [
                    { "version_added": "80" },
                    { "version_added": "45", "flags": [] }
                ]
            }
        });
        let versions = extract_browser_versions(&compat);
        assert_eq!(
            versions.and_then(|v| v.chrome),
            Some(BrowserVersion::Version("80".to_owned()))
        );
    }

    #[test]
    fn extract_browser_versions_array_uses_first_non_null_string() {
        let compat = json!({
            "support": {
                "chrome": [
                    { "version_added": null },
                    { "version_added": "45", "flags": [] }
                ]
            }
        });
        let versions = extract_browser_versions(&compat);
        assert_eq!(
            versions.and_then(|v| v.chrome),
            Some(BrowserVersion::Version("45".to_owned()))
        );
    }

    #[test]
    fn extract_browser_versions_true_means_supported_unknown_version() {
        let compat = json!({
            "support": {
                "chrome": { "version_added": true }
            }
        });
        let versions = extract_browser_versions(&compat);
        assert_eq!(
            versions.and_then(|v| v.chrome),
            Some(BrowserVersion::Unknown)
        );
    }

    #[test]
    fn parse_year_valid() {
        let status = json!({ "date": "2023-03-27" });
        assert_eq!(parse_year(&status, "date"), Some(2023));
    }

    #[test]
    fn parse_year_missing() {
        let status = json!({});
        assert_eq!(parse_year(&status, "date"), None);
    }

    /// Helper: build a BCD compat object tagged with a web-features ID.
    fn compat_with_tag(feature_id: &str) -> serde_json::Value {
        json!({ "tags": [format!("web-features:{feature_id}")] })
    }

    #[test]
    fn resolve_baseline_uses_by_compat_key_override() {
        let compat = compat_with_tag("svg");
        let wf = json!({
            "svg": {
                "status": {
                    "baseline": "low",
                    "baseline_low_date": "2024-01-01",
                    "by_compat_key": {
                        "svg.elements.rect": {
                            "baseline": "high",
                            "baseline_high_date": "2020-06-01"
                        }
                    }
                }
            }
        });
        assert_eq!(
            resolve_baseline(&compat, Some(&wf), "svg.elements.rect"),
            Some(BaselineStatus::Widely { since: 2020 }),
        );
    }

    #[test]
    fn resolve_baseline_falls_back_without_override() {
        let compat = compat_with_tag("svg");
        let wf = json!({
            "svg": {
                "status": {
                    "baseline": "low",
                    "baseline_low_date": "2024-01-01"
                }
            }
        });
        assert_eq!(
            resolve_baseline(&compat, Some(&wf), "svg.elements.rect"),
            Some(BaselineStatus::Newly { since: 2024 }),
        );
    }

    #[test]
    fn resolve_baseline_malformed_override_returns_none() {
        let compat = compat_with_tag("svg");
        let wf = json!({
            "svg": {
                "status": {
                    "baseline": "low",
                    "baseline_low_date": "2024-01-01",
                    "by_compat_key": {
                        "svg.elements.rect": {
                            "baseline": "high"
                            // missing baseline_high_date → parse_baseline_value returns None
                        }
                    }
                }
            }
        });
        // Malformed override: present but unparseable → falls back to top-level status
        assert_eq!(
            resolve_baseline(&compat, Some(&wf), "svg.elements.rect"),
            Some(BaselineStatus::Newly { since: 2024 }),
        );
    }

    #[test]
    fn resolve_baseline_none_without_wf_features() {
        let compat = compat_with_tag("svg");
        assert_eq!(resolve_baseline(&compat, None, "svg.elements.rect"), None);
    }
}
