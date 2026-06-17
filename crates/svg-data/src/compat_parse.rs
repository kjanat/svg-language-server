//! Parsing helpers over MDN browser-compat-data, used to derive baseline and
//! per-browser support facts.
//!
//! These operate on the raw compat JSON so the LSP can reconcile baseline /
//! support at runtime against the same data the catalog was built from.

use crate::BaselineStatus;

/// A single browser's `version_added`, resolved to a comparable form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserVersion {
    /// Supported, but the first version is unknown.
    Unknown,
    /// Supported since the given version string.
    Version(String),
}

/// Per-browser `version_added` for the four tracked engines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserVersions {
    /// Chrome support.
    pub chrome: Option<BrowserVersion>,
    /// Edge support.
    pub edge: Option<BrowserVersion>,
    /// Firefox support.
    pub firefox: Option<BrowserVersion>,
    /// Safari support.
    pub safari: Option<BrowserVersion>,
}

/// Extract per-browser support from a compat record's `support` block.
#[must_use]
pub fn extract_browser_versions(compat: &serde_json::Value) -> Option<BrowserVersions> {
    let support = compat.get("support")?.as_object()?;
    Some(BrowserVersions {
        chrome: support.get("chrome").and_then(browser_version_from_support),
        edge: support.get("edge").and_then(browser_version_from_support),
        firefox: support
            .get("firefox")
            .and_then(browser_version_from_support),
        safari: support.get("safari").and_then(browser_version_from_support),
    })
}

fn browser_version_from_support(value: &serde_json::Value) -> Option<BrowserVersion> {
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .find_map(browser_version_from_support_statement);
    }
    browser_version_from_support_statement(value)
}

fn browser_version_from_support_statement(value: &serde_json::Value) -> Option<BrowserVersion> {
    match value.get("version_added")? {
        serde_json::Value::String(version) => Some(BrowserVersion::Version(version.clone())),
        serde_json::Value::Bool(true) | serde_json::Value::Null => Some(BrowserVersion::Unknown),
        _ => None,
    }
}

/// Resolve a feature's web-platform baseline from compat + web-features data.
#[must_use]
pub fn resolve_baseline(
    compat: &serde_json::Value,
    wf_features: Option<&serde_json::Value>,
    compat_key: &str,
) -> Option<BaselineStatus> {
    let _ = compat;
    let feature = web_feature_for_compat_key(wf_features?, compat_key)?;
    let status = feature
        .get("status")?
        .get("by_compat_key")
        .and_then(serde_json::Value::as_object)
        .and_then(|by_key| by_key.get(compat_key))
        .or_else(|| feature.get("status"))?;
    baseline_status_from_web_features(status)
}

fn web_feature_for_compat_key<'a>(
    wf_features: &'a serde_json::Value,
    compat_key: &str,
) -> Option<&'a serde_json::Value> {
    wf_features.as_object()?.values().find(|feature| {
        feature
            .get("compat_features")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|keys| keys.iter().any(|key| key.as_str() == Some(compat_key)))
    })
}

fn baseline_status_from_web_features(status: &serde_json::Value) -> Option<BaselineStatus> {
    match status.get("baseline")? {
        serde_json::Value::String(value) if value == "high" => {
            year_from_date(status.get("baseline_high_date")?.as_str()?).map(|since| {
                BaselineStatus::Widely {
                    since,
                    qualifier: None,
                }
            })
        }
        serde_json::Value::String(value) if value == "low" => {
            year_from_date(status.get("baseline_low_date")?.as_str()?).map(|since| {
                BaselineStatus::Newly {
                    since,
                    qualifier: None,
                }
            })
        }
        serde_json::Value::String(value) if value == "limited" => Some(BaselineStatus::Limited),
        serde_json::Value::Bool(false) => Some(BaselineStatus::Limited),
        _ => None,
    }
}

fn year_from_date(date: &str) -> Option<u16> {
    date.get(..4)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_browser_versions_from_support_block() {
        let compat = serde_json::json!({
            "support": {
                "chrome": { "version_added": "50" },
                "edge": { "version_added": true },
                "firefox": { "version_added": false },
                "safari": { "version_added": null }
            }
        });

        let Some(versions) = extract_browser_versions(&compat) else {
            panic!("versions should parse");
        };
        assert_eq!(
            versions.chrome,
            Some(BrowserVersion::Version("50".to_owned()))
        );
        assert_eq!(versions.edge, Some(BrowserVersion::Unknown));
        assert_eq!(versions.firefox, None);
        assert_eq!(versions.safari, Some(BrowserVersion::Unknown));
    }

    #[test]
    fn extracts_first_supported_statement_from_arrays() {
        let compat = serde_json::json!({
            "support": {
                "chrome": [
                    { "version_added": false },
                    { "version_added": "80" }
                ]
            }
        });

        let Some(versions) = extract_browser_versions(&compat) else {
            panic!("versions should parse");
        };
        assert_eq!(
            versions.chrome,
            Some(BrowserVersion::Version("80".to_owned()))
        );
    }

    #[test]
    fn resolves_baseline_from_web_features_by_compat_key() {
        let compat = serde_json::json!({});
        let wf = serde_json::json!({
            "svg": {
                "compat_features": ["svg.elements.rect", "svg.elements.rect.width"],
                "status": {
                    "baseline": "high",
                    "baseline_high_date": "2022-07-15",
                    "baseline_low_date": "2020-01-15",
                    "by_compat_key": {
                        "svg.elements.rect.width": {
                            "baseline": "low",
                            "baseline_low_date": "2025-05-01"
                        }
                    }
                }
            }
        });

        assert_eq!(
            resolve_baseline(&compat, Some(&wf), "svg.elements.rect.width"),
            Some(BaselineStatus::Newly {
                since: 2025,
                qualifier: None
            })
        );
        assert_eq!(
            resolve_baseline(&compat, Some(&wf), "svg.elements.rect"),
            Some(BaselineStatus::Widely {
                since: 2022,
                qualifier: None
            })
        );
    }

    #[test]
    fn resolves_limited_baseline_from_false_status() {
        let compat = serde_json::json!({});
        let wf = serde_json::json!({
            "feature": {
                "compat_features": ["svg.elements.a.referrerPolicy"],
                "status": {
                    "baseline": false
                }
            }
        });

        assert_eq!(
            resolve_baseline(&compat, Some(&wf), "svg.elements.a.referrerPolicy"),
            Some(BaselineStatus::Limited)
        );
    }
}
