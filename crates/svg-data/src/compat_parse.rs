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
    {
        return parse_baseline_value(override_status);
    }

    parse_baseline_value(status)
}

/// Per-browser `version_added` strings extracted from BCD support data.
pub struct BrowserVersions {
    /// Chrome `version_added`.
    pub chrome: Option<String>,
    /// Edge `version_added`.
    pub edge: Option<String>,
    /// Firefox `version_added`.
    pub firefox: Option<String>,
    /// Safari `version_added`.
    pub safari: Option<String>,
}

/// Extract `support/{browser}/version_added` for the four major browsers.
///
/// Returns `None` for browsers without support data or where the version is
/// not a string (e.g. `version_added: true` without a concrete version).
#[must_use]
pub fn extract_browser_versions(compat: &serde_json::Value) -> Option<BrowserVersions> {
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

    let versions = BrowserVersions {
        chrome: version_added("chrome"),
        edge: version_added("edge"),
        firefox: version_added("firefox"),
        safari: version_added("safari"),
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
        assert_eq!(v.and_then(|v| v.chrome.as_deref()), Some("45"));
        assert_eq!(v.and_then(|v| v.firefox.as_deref()), Some("52"));
        assert_eq!(v.and_then(|v| v.edge.as_deref()), None);
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
        assert_eq!(versions.and_then(|v| v.chrome), Some("80".to_owned()));
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
}
