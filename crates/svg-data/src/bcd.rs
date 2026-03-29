//! Rust equivalents of the [`@mdn/browser-compat-data`] TypeScript types.
//!
//! Generated from the [BCD JSON-Schema] type definitions. These types mirror
//! the npm package's `types.d.ts` so the full BCD dataset can be deserialized
//! into strongly-typed Rust structures.
//!
//! [`@mdn/browser-compat-data`]: https://github.com/mdn/browser-compat-data
//! [BCD JSON-Schema]: https://github.com/mdn/browser-compat-data/tree/main/schemas

use std::collections::HashMap;

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Browser identifiers & enums
// ---------------------------------------------------------------------------

/// The names of the known browsers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserName {
    Bun,
    Chrome,
    ChromeAndroid,
    Deno,
    Edge,
    Firefox,
    FirefoxAndroid,
    Ie,
    Nodejs,
    Oculus,
    Opera,
    OperaAndroid,
    Safari,
    SafariIos,
    SamsunginternetAndroid,
    WebviewAndroid,
    WebviewIos,
}

/// A version string (`"85"`) or `false` (not supported).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum VersionValue {
    /// The browser version in which the feature was added (e.g. `"85"`).
    Version(String),
    /// `false` — the feature is not supported in this browser.
    NotSupported(bool),
}

/// The platform a browser runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserType {
    Desktop,
    Mobile,
    Xr,
    Server,
}

/// Name of a browser's rendering/JS engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum BrowserEngine {
    Blink,
    EdgeHTML,
    Gecko,
    Presto,
    Trident,
    WebKit,
    V8,
}

/// Where in the lifecycle a browser release is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserReleaseStatus {
    Retired,
    Current,
    Beta,
    Nightly,
    Esr,
    Planned,
}

// ---------------------------------------------------------------------------
// Browser & release data
// ---------------------------------------------------------------------------

/// All known browsers keyed by [`BrowserName`].
pub type Browsers = HashMap<BrowserName, BrowserStatement>;

/// Metadata for a single browser.
#[derive(Debug, Clone, Deserialize)]
pub struct BrowserStatement {
    /// The browser brand name (e.g. "Firefox", "Chrome Android").
    pub name: String,
    /// The platform the browser runs on.
    #[serde(rename = "type")]
    pub browser_type: BrowserType,
    /// The upstream browser this one derives from (e.g. Edge → Chrome).
    pub upstream: Option<BrowserName>,
    /// Name of the browser's preview channel (e.g. "Nightly", "TP").
    pub preview_name: Option<String>,
    /// URL where feature flags can be changed (e.g. `about:config`).
    pub pref_url: Option<String>,
    /// Whether the browser supports user-toggleable feature flags.
    pub accepts_flags: bool,
    /// Whether the browser supports extensions.
    pub accepts_webextensions: bool,
    /// Known versions of this browser.
    pub releases: HashMap<String, ReleaseStatement>,
}

/// Metadata for a single browser release.
#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseStatement {
    /// Release date formatted as `YYYY-MM-DD`.
    pub release_date: Option<String>,
    /// Link to release notes or changelog.
    pub release_notes: Option<String>,
    /// Lifecycle status of this release.
    pub status: BrowserReleaseStatus,
    /// Name of the underlying engine.
    pub engine: Option<BrowserEngine>,
    /// Engine version corresponding to this browser version.
    pub engine_version: Option<String>,
}

// ---------------------------------------------------------------------------
// Compat data
// ---------------------------------------------------------------------------

/// A recursive tree node in the BCD hierarchy.
///
/// Each identifier may carry its own `__compat` statement and may have
/// child identifiers keyed by arbitrary names.
///
/// Mirrors the TypeScript `Identifier` type:
/// ```typescript
/// type Identifier = {[key: string]: Identifier} & {__compat?: CompatStatement};
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct Identifier {
    /// Compat statement for this node, if present.
    #[serde(rename = "__compat")]
    pub compat: Option<CompatStatement>,
    /// Child identifiers (every key except `__compat`).
    #[serde(flatten)]
    pub children: HashMap<String, serde_json::Value>,
}

/// Per-browser support data for a feature.
///
/// Maps [`BrowserName`] → [`SupportStatement`].
pub type SupportBlock = HashMap<BrowserName, SupportStatement>;

/// Support information: either a single statement or an array of statements
/// (e.g. when a feature was removed and re-added).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SupportStatement {
    /// A single support entry.
    Single(SimpleSupportStatement),
    /// Multiple support entries (e.g. feature removed then re-added).
    Multiple(Vec<SimpleSupportStatement>),
}

/// Compat statement for a single BCD feature.
#[derive(Debug, Clone, Deserialize)]
pub struct CompatStatement {
    /// Human-readable description of the feature.
    pub description: Option<String>,
    /// URL to the MDN reference page (language-agnostic).
    pub mdn_url: Option<String>,
    /// Specification URL(s), each containing a fragment identifier.
    pub spec_url: Option<SpecUrl>,
    /// Tags assigned to this feature (e.g. `web-features:svg`).
    pub tags: Option<Vec<String>>,
    /// Path to the source file in the BCD repository (auto-generated).
    pub source_file: Option<String>,
    /// Per-browser support data.
    pub support: SupportBlock,
    /// Stability status flags.
    pub status: Option<StatusBlock>,
}

/// A single spec URL or an array of spec URLs.
///
/// Mirrors the TypeScript type `string | [string, string, ...string[]]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SpecUrl {
    /// A single specification URL.
    One(String),
    /// Multiple specification URLs.
    Many(Vec<String>),
}

impl SpecUrl {
    /// Returns the first spec URL.
    #[must_use]
    pub fn first(&self) -> &str {
        match self {
            Self::One(url) => url,
            Self::Many(urls) => &urls[0],
        }
    }

    /// Returns all spec URLs as a slice-like iterator.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        match self {
            Self::One(url) => std::slice::from_ref(url).iter().map(String::as_str),
            Self::Many(urls) => urls.iter().map(String::as_str),
        }
    }
}

/// Stability status of a feature.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct StatusBlock {
    /// Usually `true` for single-implementer features.
    ///
    /// *Deprecated in BCD — prefer Baseline calculations instead.*
    pub experimental: bool,
    /// Whether the feature is part of an active specification.
    pub standard_track: bool,
    /// Whether the feature is no longer recommended.
    pub deprecated: bool,
}

/// Support details for a single browser.
#[derive(Debug, Clone, Deserialize)]
pub struct SimpleSupportStatement {
    /// Browser version that added this feature, or `false` if unsupported.
    pub version_added: VersionValue,
    /// Browser version that removed this feature.
    pub version_removed: Option<String>,
    /// Last browser version that supported this feature (auto-generated).
    pub version_last: Option<String>,
    /// Vendor prefix (e.g. `"-webkit-"`). Leading/trailing `-` included.
    pub prefix: Option<String>,
    /// Alternative name when the feature uses an entirely different name.
    pub alternative_name: Option<String>,
    /// Flags that must be configured for support.
    pub flags: Option<Vec<FlagStatement>>,
    /// Changeset/commit URL or bug tracker URL for the implementation.
    pub impl_url: Option<StringOrArray>,
    /// `true` when the implementation deviates from the spec.
    pub partial_implementation: Option<bool>,
    /// Additional notes about this support entry.
    pub notes: Option<StringOrArray>,
}

/// A string or array of strings.
///
/// Used for `notes`, `impl_url`, and similar BCD fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum StringOrArray {
    /// A single string value.
    One(String),
    /// Multiple string values.
    Many(Vec<String>),
}

/// A flag/preference that must be set for a feature to work.
#[derive(Debug, Clone, Deserialize)]
pub struct FlagStatement {
    /// The flag type.
    #[serde(rename = "type")]
    pub flag_type: FlagType,
    /// Name of the flag or preference.
    pub name: String,
    /// Value the flag must be set to.
    pub value_to_set: Option<String>,
}

/// The type of a browser feature flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagType {
    Preference,
    RuntimeFlag,
}

// ---------------------------------------------------------------------------
// Top-level structures
// ---------------------------------------------------------------------------

/// Package metadata embedded in BCD's `__meta` key.
#[derive(Debug, Clone, Deserialize)]
pub struct MetaBlock {
    /// The BCD package version.
    pub version: String,
    /// Build timestamp.
    pub timestamp: String,
}

/// The top-level BCD data structure.
///
/// Mirrors the TypeScript `CompatData` interface. Each field except
/// `__meta` and `browsers` is an [`Identifier`] tree.
#[derive(Debug, Clone, Deserialize)]
pub struct CompatData {
    /// Package metadata (version, build timestamp).
    #[serde(rename = "__meta")]
    pub meta: MetaBlock,

    /// [Web API](https://developer.mozilla.org/docs/Web/API) interfaces.
    pub api: Identifier,

    /// Known browsers and runtimes.
    pub browsers: Browsers,

    /// [CSS](https://developer.mozilla.org/docs/Web/CSS) properties, selectors, at-rules.
    pub css: Identifier,

    /// [HTML](https://developer.mozilla.org/docs/Web/HTML) elements, attributes, globals.
    pub html: Identifier,

    /// [HTTP](https://developer.mozilla.org/docs/Web/HTTP) headers, statuses, methods.
    pub http: Identifier,

    /// [JavaScript](https://developer.mozilla.org/docs/Web/JavaScript) built-ins, statements, operators.
    pub javascript: Identifier,

    /// Manifest data (e.g. [Web App Manifest](https://developer.mozilla.org/docs/Web/Progressive_web_apps/manifest)).
    pub manifests: Identifier,

    /// [MathML](https://developer.mozilla.org/docs/Web/MathML) elements, attributes, globals.
    pub mathml: Identifier,

    /// [Media types](https://developer.mozilla.org/docs/Web/HTTP/Guides/MIME_types).
    pub mediatypes: Identifier,

    /// [SVG](https://developer.mozilla.org/docs/Web/SVG) elements, attributes, globals.
    pub svg: Identifier,

    /// [WebAssembly](https://developer.mozilla.org/docs/WebAssembly) features.
    pub webassembly: Identifier,

    /// [WebDriver](https://developer.mozilla.org/docs/Web/WebDriver) commands.
    pub webdriver: Identifier,

    /// [WebExtensions](https://developer.mozilla.org/Add-ons/WebExtensions) APIs and manifest keys.
    pub webextensions: Identifier,
}
