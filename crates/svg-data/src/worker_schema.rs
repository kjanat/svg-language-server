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

/// Per-browser support state.
#[derive(Deserialize)]
pub struct WorkerBrowserSupport {
    /// Chrome support state.
    #[serde(default)]
    pub chrome: Option<WorkerBrowserVersion>,
    /// Edge support state.
    #[serde(default)]
    pub edge: Option<WorkerBrowserVersion>,
    /// Firefox support state.
    #[serde(default)]
    pub firefox: Option<WorkerBrowserVersion>,
    /// Safari support state.
    #[serde(default)]
    pub safari: Option<WorkerBrowserVersion>,
}

/// Literal upstream `version_added` value from BCD. One of:
/// version string, `false` (explicitly unsupported),
/// `true` (supported, version unknown), or `null` (no data).
#[derive(Default, Deserialize)]
#[serde(untagged)]
pub enum WorkerRawVersionAdded {
    /// Version string such as `"50"` or `"≤50"`.
    Text(String),
    /// `true` = supported (version unknown), `false` = explicitly unsupported.
    Flag(bool),
    /// Upstream has no data for this browser.
    #[default]
    Null,
}

/// Per-browser support statement mirrored from the worker's `BrowserVersion`.
///
/// The `raw_value_added` field is always present and preserves the exact
/// upstream signal so downstream consumers can distinguish "explicitly
/// unsupported" (`false`) from "no upstream data" (the field itself absent).
#[derive(Deserialize)]
pub struct WorkerBrowserVersion {
    /// Literal upstream `version_added` value (string / bool / null).
    pub raw_value_added: WorkerRawVersionAdded,
    /// Parsed version when `raw_value_added` was a usable string.
    #[serde(default)]
    pub version_added: Option<String>,
    /// Qualifier on `version_added` (`before` / `after` / `approximately`).
    #[serde(default)]
    pub version_qualifier: Option<String>,
    /// `false` when BCD explicitly stated "not supported"; `true` when
    /// supported with unknown version. Absent otherwise.
    #[serde(default)]
    pub supported: Option<bool>,
    /// Upstream `version_removed` — present when support was dropped.
    #[serde(default)]
    pub version_removed: Option<String>,
    /// Qualifier on `version_removed`.
    #[serde(default)]
    pub version_removed_qualifier: Option<String>,
    /// Upstream `partial_implementation` — ships but deviates from spec.
    #[serde(default)]
    pub partial_implementation: Option<bool>,
    /// Vendor prefix required (e.g. `"-webkit-"`).
    #[serde(default)]
    pub prefix: Option<String>,
    /// Alternative name under which the feature ships.
    #[serde(default)]
    pub alternative_name: Option<String>,
    /// Preference / runtime flags gating the feature.
    #[serde(default)]
    pub flags: Option<Vec<WorkerBrowserFlag>>,
    /// Free-form caveats, normalised to a list.
    #[serde(default)]
    pub notes: Option<Vec<String>>,
}

/// A single BCD flag statement (preference or runtime flag).
#[derive(Deserialize)]
pub struct WorkerBrowserFlag {
    /// Flag category (e.g. `"preference"`, `"runtime_flag"`).
    pub r#type: String,
    /// Preference / flag name.
    pub name: String,
    /// Value the flag must be set to for the feature to work.
    #[serde(default)]
    pub value_to_set: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> WorkerOutput {
        let json = FIXTURE_JSON;
        match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => panic!("valid worker JSON should parse: {e}"),
        }
    }

    const FIXTURE_JSON: &str = r#"{
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
                    "browser_support": {
                        "chrome": { "raw_value_added": "1", "version_added": "1" },
                        "edge": { "raw_value_added": "12", "version_added": "12" },
                        "firefox": { "raw_value_added": "1.5", "version_added": "1.5" },
                        "safari": { "raw_value_added": "3", "version_added": "3" }
                    }
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
                    "browser_support": {
                        "chrome": { "raw_value_added": "5", "version_added": "5" },
                        "edge": {
                            "raw_value_added": "≤18",
                            "version_added": "18",
                            "version_qualifier": "before"
                        },
                        "firefox": { "raw_value_added": "3", "version_added": "3" },
                        "safari": { "raw_value_added": "6", "version_added": "6" }
                    }
                }
            },
            "attributes": {
                "fill": {
                    "deprecated": false,
                    "experimental": false,
                    "standard_track": true,
                    "spec_url": [],
                    "baseline": { "status": "limited" },
                    "browser_support": {
                        "chrome": { "raw_value_added": "1", "version_added": "1" }
                    },
                    "elements": ["*"]
                },
                "glyph-orientation-horizontal": {
                    "deprecated": true,
                    "experimental": false,
                    "standard_track": true,
                    "spec_url": [],
                    "browser_support": {
                        "chrome": { "raw_value_added": false, "supported": false },
                        "edge": { "raw_value_added": false, "supported": false },
                        "firefox": { "raw_value_added": false, "supported": false },
                        "safari": {
                            "raw_value_added": "≤13.1",
                            "version_added": "13.1",
                            "version_qualifier": "before"
                        }
                    },
                    "elements": ["*"]
                }
            }
        }"#;

    #[test]
    fn deserialize_fixture_counts() {
        let output = fixture();
        assert_eq!(output.elements.len(), 2);
        assert_eq!(output.attributes.len(), 2);
    }

    #[test]
    fn rect_parses_with_baseline_and_support() {
        let output = fixture();
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
                .and_then(|bs| bs.chrome.as_ref())
                .and_then(|v| v.version_added.as_deref()),
            Some("1"),
        );
    }

    #[test]
    fn fe_gaussian_blur_preserves_baseline_and_edge_qualifiers() {
        let output = fixture();
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
        // Edge carries a ≤ version qualifier — must round-trip.
        let Some(edge) = blur.browser_support.as_ref().and_then(|b| b.edge.as_ref()) else {
            panic!("feGaussianBlur fixture must have edge support");
        };
        assert_eq!(edge.version_added.as_deref(), Some("18"));
        assert_eq!(edge.version_qualifier.as_deref(), Some("before"));
    }

    #[test]
    fn fill_has_limited_baseline_no_year() {
        let output = fixture();
        let fill = &output.attributes["fill"];
        assert_eq!(fill.elements, vec!["*"]);
        assert_eq!(
            fill.baseline.as_ref().map(|b| b.status.as_str()),
            Some("limited"),
        );
        assert_eq!(fill.baseline.as_ref().and_then(|b| b.since), None);
    }

    #[test]
    fn glyph_orientation_horizontal_preserves_explicit_false_plus_qualifier() {
        let output = fixture();
        // Second canary: glyph-orientation-horizontal preserves explicit
        // false for all engines except Safari, where the ≤13.1 qualifier
        // survives end-to-end.
        let goh = &output.attributes["glyph-orientation-horizontal"];
        assert!(goh.deprecated);
        let Some(support) = goh.browser_support.as_ref() else {
            panic!("glyph-orientation-horizontal must carry browser_support");
        };
        let Some(chrome) = support.chrome.as_ref() else {
            panic!("glyph-orientation-horizontal must have chrome support data");
        };
        assert!(matches!(
            chrome.raw_value_added,
            WorkerRawVersionAdded::Flag(false)
        ));
        assert_eq!(chrome.supported, Some(false));
        let Some(safari) = support.safari.as_ref() else {
            panic!("glyph-orientation-horizontal must have safari support data");
        };
        assert_eq!(safari.version_added.as_deref(), Some("13.1"));
        assert_eq!(safari.version_qualifier.as_deref(), Some("before"));
    }
}
