//! Deserialization types for the svg-compat worker JSON output.
//!
//! These types mirror the worker's `SvgCompatOutput` schema and are used by
//! both the build script (via `#[path]` include) and the crate itself.
//! Keep self-contained — no imports from sibling modules.

use std::collections::HashMap;

use serde::Deserialize;

/// Root response from the svg-compat worker `/data.json` endpoint.
#[derive(Deserialize)]
pub struct WorkerOutput {
    /// SVG element compat entries keyed by tag name.
    pub elements: HashMap<String, WorkerElement>,
    /// SVG attribute compat entries keyed by attribute name.
    pub attributes: HashMap<String, WorkerAttribute>,
}

/// Browser compatibility entry for an SVG element.
#[derive(Deserialize)]
pub struct WorkerElement {
    /// Whether this element is deprecated.
    pub deprecated: bool,
    /// Whether this element is experimental.
    pub experimental: bool,
    /// Specification URLs (may be empty).
    pub spec_url: Vec<String>,
    /// Web-platform baseline status, if resolvable.
    pub baseline: Option<WorkerBaseline>,
    /// Minimum browser versions that support this element.
    pub browser_support: Option<WorkerBrowserSupport>,
}

/// Browser compatibility entry for an SVG attribute, with element scope.
#[derive(Deserialize)]
pub struct WorkerAttribute {
    /// Whether this attribute is deprecated.
    pub deprecated: bool,
    /// Whether this attribute is experimental.
    pub experimental: bool,
    /// Specification URLs (may be empty).
    pub spec_url: Vec<String>,
    /// Web-platform baseline status, if resolvable.
    pub baseline: Option<WorkerBaseline>,
    /// Minimum browser versions that support this attribute.
    pub browser_support: Option<WorkerBrowserSupport>,
    /// Element names this attribute applies to. `["*"]` means global.
    pub elements: Vec<String>,
}

/// Baseline support tier from web-features.
#[derive(Deserialize)]
pub struct WorkerBaseline {
    /// One of `"widely"`, `"newly"`, or `"limited"`.
    pub status: String,
    /// Set when the upstream baseline value was unrecognised; the
    /// original is preserved here and `status` falls back to
    /// `"limited"` on the worker side.
    #[serde(default)]
    pub raw_status: Option<String>,
    /// Year the feature reached this baseline tier.
    #[serde(default)]
    pub since: Option<u16>,
    /// Qualifier on the year: `"before"` / `"after"` / `"approximately"`.
    ///
    /// Mirrors the prefix on the date that `since` was derived from
    /// (e.g. web-features `"≤2021-04-02"` → `"before"` + `since: 2021`).
    #[serde(default)]
    pub since_qualifier: Option<String>,
    /// Full date when the feature first reached baseline (low tier).
    #[serde(default)]
    pub low_date: Option<WorkerBaselineDate>,
    /// Full date when the feature reached baseline high tier.
    #[serde(default)]
    pub high_date: Option<WorkerBaselineDate>,
}

/// Parsed baseline date plus the original upstream string.
///
/// `raw` is always present so no upstream byte is ever lost; `date`
/// and `qualifier` are best-effort outputs from the worker's parser.
#[derive(Deserialize)]
pub struct WorkerBaselineDate {
    /// Original upstream value, byte-for-byte.
    pub raw: String,
    /// ISO `YYYY-MM-DD` extracted from `raw`. Absent if unparseable.
    #[serde(default)]
    pub date: Option<String>,
    /// `"before"` / `"after"` / `"approximately"` — absent for exact dates.
    #[serde(default)]
    pub qualifier: Option<String>,
}

/// Per-browser minimum version strings.
#[derive(Deserialize)]
pub struct WorkerBrowserSupport {
    /// Minimum Chrome version (e.g. `"88"`, `"≤50"`).
    pub chrome: Option<String>,
    /// Minimum Edge version.
    pub edge: Option<String>,
    /// Minimum Firefox version.
    pub firefox: Option<String>,
    /// Minimum Safari version.
    pub safari: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_worker_output() {
        let json = r#"{
            "generated_at": "2025-01-01T00:00:00Z",
            "sources": {
                "bcd": { "package": "x", "requested": "1", "resolved": "1", "mode": "default", "source_url": "x" },
                "web_features": { "package": "x", "requested": "1", "resolved": "1", "mode": "default", "source_url": "x" }
            },
            "elements": {
                "rect": {
                    "deprecated": false,
                    "experimental": false,
                    "standard_track": true,
                    "spec_url": ["https://svgwg.org/svg2-draft/shapes.html#RectElement"],
                    "baseline": {
                        "status": "widely",
                        "since": 2015,
                        "low_date": { "raw": "2015-07-29", "date": "2015-07-29" },
                        "high_date": { "raw": "2018-01-29", "date": "2018-01-29" }
                    },
                    "browser_support": { "chrome": "1", "edge": "12", "firefox": "1.5", "safari": "3" }
                },
                "feGaussianBlur": {
                    "deprecated": false,
                    "experimental": false,
                    "standard_track": true,
                    "spec_url": [],
                    "baseline": {
                        "status": "widely",
                        "since": 2021,
                        "since_qualifier": "before",
                        "low_date": { "raw": "≤2018-10-02", "date": "2018-10-02", "qualifier": "before" },
                        "high_date": { "raw": "≤2021-04-02", "date": "2021-04-02", "qualifier": "before" }
                    },
                    "browser_support": { "chrome": "5", "edge": "≤18", "firefox": "3", "safari": "6" }
                }
            },
            "attributes": {
                "fill": {
                    "deprecated": false,
                    "experimental": false,
                    "standard_track": true,
                    "spec_url": [],
                    "baseline": { "status": "limited" },
                    "browser_support": { "chrome": "1" },
                    "elements": ["*"]
                }
            }
        }"#;

        let output: WorkerOutput = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => panic!("valid worker JSON should parse: {e}"),
        };
        assert_eq!(output.elements.len(), 2);
        assert_eq!(output.attributes.len(), 1);

        let rect = &output.elements["rect"];
        assert!(!rect.deprecated);
        assert_eq!(rect.spec_url.len(), 1);
        assert_eq!(
            rect.baseline.as_ref().map(|b| b.status.as_str()),
            Some("widely"),
        );
        assert_eq!(rect.baseline.as_ref().and_then(|b| b.since), Some(2015));
        // Exact dates: qualifier absent.
        assert!(
            rect.baseline
                .as_ref()
                .and_then(|b| b.high_date.as_ref())
                .and_then(|d| d.qualifier.as_deref())
                .is_none()
        );
        assert_eq!(
            rect.browser_support
                .as_ref()
                .and_then(|bs| bs.chrome.as_deref()),
            Some("1"),
        );

        // The canary: feGaussianBlur's qualifier must survive end-to-end.
        let blur = &output.elements["feGaussianBlur"];
        assert_eq!(blur.baseline.as_ref().and_then(|b| b.since), Some(2021),);
        assert_eq!(
            blur.baseline
                .as_ref()
                .and_then(|b| b.since_qualifier.as_deref()),
            Some("before"),
        );
        let Some(high) = blur.baseline.as_ref().and_then(|b| b.high_date.as_ref()) else {
            panic!("feGaussianBlur fixture must have a high_date");
        };
        assert_eq!(high.raw, "≤2021-04-02");
        assert_eq!(high.date.as_deref(), Some("2021-04-02"));
        assert_eq!(high.qualifier.as_deref(), Some("before"));

        let fill = &output.attributes["fill"];
        assert_eq!(fill.elements, vec!["*"]);
        assert_eq!(
            fill.baseline.as_ref().map(|b| b.status.as_str()),
            Some("limited"),
        );
        assert_eq!(fill.baseline.as_ref().and_then(|b| b.since), None);
    }
}
