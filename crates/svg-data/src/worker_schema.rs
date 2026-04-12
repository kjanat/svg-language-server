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
    /// Year the feature reached this baseline tier.
    pub since: Option<u16>,
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
                    "baseline": { "status": "widely", "since": 2015 },
                    "browser_support": { "chrome": "1", "edge": "12", "firefox": "1.5", "safari": "3" }
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

        let output: WorkerOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.elements.len(), 1);
        assert_eq!(output.attributes.len(), 1);

        let rect = &output.elements["rect"];
        assert!(!rect.deprecated);
        assert_eq!(rect.spec_url.len(), 1);
        assert_eq!(rect.baseline.as_ref().unwrap().status, "widely");
        assert_eq!(rect.baseline.as_ref().unwrap().since, Some(2015));
        assert_eq!(
            rect.browser_support.as_ref().unwrap().chrome.as_deref(),
            Some("1"),
        );

        let fill = &output.attributes["fill"];
        assert_eq!(fill.elements, vec!["*"]);
        assert_eq!(fill.baseline.as_ref().unwrap().status, "limited");
        assert!(fill.baseline.as_ref().unwrap().since.is_none());
    }
}
