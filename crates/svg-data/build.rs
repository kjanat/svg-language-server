use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

// ---- JSON schema types ----

#[derive(Deserialize)]
struct JsonElement {
    name: String,
    description: String,
    mdn_url: String,
    deprecated: bool,
    content_model: ContentModelJson,
    required_attrs: Vec<String>,
    attrs: Vec<String>,
    global_attrs: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ContentModelJson {
    Simple(String),
    Children { children: Vec<String> },
}

#[derive(Deserialize)]
struct JsonAttribute {
    name: String,
    description: String,
    mdn_url: String,
    deprecated: bool,
    values: ValuesJson,
    elements: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ValuesJson {
    Enum {
        values: Vec<String>,
    },
    FreeText,
    Color,
    Length,
    Url,
    NumberOrPercentage,
    Transform {
        functions: Vec<String>,
    },
    Viewbox,
    PreserveAspectRatio {
        alignments: Vec<String>,
        meet_or_slice: Vec<String>,
    },
    Points,
    PathData,
}

// ---- Compat data types ----

/// Per-browser `version_added` for the four major desktop browsers.
#[derive(Clone, Default)]
struct BrowserSupportValue {
    chrome: Option<String>,
    edge: Option<String>,
    firefox: Option<String>,
    safari: Option<String>,
}

/// Resolved compat data for one element or attribute.
struct CompatEntry {
    deprecated: bool,
    experimental: bool,
    spec_url: Option<String>,
    baseline: Option<BaselineValue>,
    browser_support: Option<BrowserSupportValue>,
}

#[derive(Clone)]
enum BaselineValue {
    Widely { since: u16 },
    Newly { since: u16 },
    Limited,
}

impl BaselineValue {
    /// Ordering for conservative merge: lower = worse support.
    fn rank(&self) -> u8 {
        match self {
            Self::Limited => 0,
            Self::Newly { .. } => 1,
            Self::Widely { .. } => 2,
        }
    }
}

// ---- Codegen helpers ----

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn write_static_str_slice(out: &mut String, name: &str, items: &[String]) {
    write!(out, "static {name}: &[&str] = &[").unwrap();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write!(out, "\"{}\"", escape(item)).unwrap();
    }
    writeln!(out, "];").unwrap();
}

fn ident_from(name: &str) -> String {
    name.replace('-', "_").to_uppercase()
}

// ---- Compat data fetching ----

const BCD_URL: &str = "https://unpkg.com/@mdn/browser-compat-data@latest/data.json";
const WEB_FEATURES_URL: &str = "https://unpkg.com/web-features@latest/data.json";

const CACHE_MAX_AGE_SECS: u64 = 24 * 60 * 60; // 24 hours

/// Download `url` to `dest` if the file is missing or older than 24h.
/// Returns `Ok(true)` if file is ready, `Ok(false)` if skipped (offline mode),
/// `Err` on failure.
fn ensure_cached(url: &str, dest: &Path, offline: bool) -> Result<bool, String> {
    if offline {
        if dest.exists() {
            println!(
                "cargo::warning=compat: using existing cache (offline mode): {}",
                dest.display()
            );
            return Ok(true);
        }
        println!(
            "cargo::warning=compat: no cache and offline mode — skipping {}",
            dest.display()
        );
        return Ok(false);
    }

    // Check if existing cache is fresh enough.
    if dest.exists()
        && let Ok(meta) = fs::metadata(dest)
        && let Ok(modified) = meta.modified()
        && let Ok(age) = SystemTime::now().duration_since(modified)
        && age.as_secs() < CACHE_MAX_AGE_SECS
    {
        println!(
            "cargo::warning=compat: using cached {} (age {}s)",
            dest.display(),
            age.as_secs()
        );
        return Ok(true);
    }

    println!("cargo::warning=compat: downloading {url}");

    let mut response = ureq::get(url)
        .call()
        .map_err(|e| format!("fetch {url}: {e}"))?;

    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("read body {url}: {e}"))?;

    fs::write(dest, &body).map_err(|e| format!("write {}: {e}", dest.display()))?;

    Ok(true)
}

/// Which elements a BCD-discovered attribute applies to.
struct BcdAttribute {
    compat: CompatEntry,
    elements: Vec<String>,
}

fn bcd_attr_applies_globally(elements: &[String]) -> bool {
    elements.iter().any(|element| element == "*")
}

struct CompatData {
    elements: HashMap<String, CompatEntry>,
    /// Attributes from BCD (global + element-specific, merged).
    attributes: HashMap<String, BcdAttribute>,
}

/// Build lookup maps for elements + attributes.
/// On any failure, prints a cargo warning and returns empty maps.
fn fetch_compat_data(out_dir: &Path) -> CompatData {
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
///
/// Resolution order:
/// 1. `by_compat_key[compat_key]` — per-key override (most precise)
/// 2. Feature-level `status.baseline` — fallback
fn extract_baseline(
    compat: &serde_json::Value,
    wf_features: &Option<serde_json::Value>,
    compat_key: &str,
) -> Option<BaselineValue> {
    let wf = wf_features.as_ref()?;

    // Find the web-features tag
    let tags = compat.get("tags")?.as_array()?;
    let feature_id = tags.iter().find_map(|tag| {
        let s = tag.as_str()?;
        s.strip_prefix("web-features:")
    })?;

    // Look up in web-features data
    let feature = wf.get(feature_id)?;
    let status = feature.get("status")?;

    // Try per-compat-key override first
    if let Some(by_key) = status.get("by_compat_key")
        && let Some(override_status) = by_key.get(compat_key)
    {
        return parse_baseline_value(override_status);
    }

    // Fall back to feature-level baseline
    parse_baseline_value(status)
}

/// Parse a baseline value from a status object containing `baseline`,
/// `baseline_high_date`, and `baseline_low_date` fields.
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

/// Conservative merge of browser support: if any source lacks support for
/// a browser (None), the merged result is None for that browser.
fn merge_browser_support(existing: &mut Option<BrowserSupportValue>, new: &BrowserSupportValue) {
    let Some(existing) = existing.as_mut() else {
        *existing = Some(new.clone());
        return;
    };

    // Conservative: if the new source doesn't support a browser, drop it
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

/// Extract `version_added` for Chrome, Edge, Firefox, Safari from a BCD
/// `__compat.support` object.
fn extract_browser_support(compat: &serde_json::Value) -> Option<BrowserSupportValue> {
    let support = compat.get("support")?;

    let version_added = |browser: &str| -> Option<String> {
        let entry = support.get(browser)?;
        // Handle both single statement and array (take first entry)
        let stmt = if entry.is_array() {
            entry.get(0)?
        } else {
            entry
        };
        // version_added is either a string or false
        stmt.get("version_added")?.as_str().map(String::from)
    };

    Some(BrowserSupportValue {
        chrome: version_added("chrome"),
        edge: version_added("edge"),
        firefox: version_added("firefox"),
        safari: version_added("safari"),
    })
}

/// Extract the first `spec_url` from a BCD `__compat` object.
///
/// BCD `spec_url` can be a single string or an array of strings.
fn extract_spec_url(compat: &serde_json::Value) -> Option<String> {
    match compat.get("spec_url")? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(arr) => arr.first().and_then(|v| v.as_str()).map(String::from),
        _ => None,
    }
}

/// Extract the year from a date string like "2020-01-15".
fn extract_year(status: &serde_json::Value, key: &str) -> Option<u16> {
    let date_str = status.get(key)?.as_str()?;
    let year_str = date_str.split('-').next()?;
    year_str.parse::<u16>().ok()
}

fn format_baseline(baseline: Option<&BaselineValue>) -> String {
    match baseline {
        None => "None".to_string(),
        Some(BaselineValue::Widely { since }) => {
            format!("Some(BaselineStatus::Widely {{ since: {since} }})")
        }
        Some(BaselineValue::Newly { since }) => {
            format!("Some(BaselineStatus::Newly {{ since: {since} }})")
        }
        Some(BaselineValue::Limited) => "Some(BaselineStatus::Limited)".to_string(),
    }
}

fn format_browser_support(bs: Option<&BrowserSupportValue>) -> String {
    let Some(bs) = bs else {
        return "None".to_string();
    };
    format!(
        "Some(BrowserSupport {{ chrome: {}, edge: {}, firefox: {}, safari: {} }})",
        format_option_str(bs.chrome.as_deref()),
        format_option_str(bs.edge.as_deref()),
        format_option_str(bs.firefox.as_deref()),
        format_option_str(bs.safari.as_deref()),
    )
}

fn format_option_str(value: Option<&str>) -> String {
    match value {
        None => "None".to_string(),
        Some(s) => format!("Some(\"{}\")", escape(s)),
    }
}

fn main() {
    println!("cargo::rerun-if-changed=data/elements.json");
    println!("cargo::rerun-if-changed=data/attributes.json");
    println!("cargo::rerun-if-env-changed=SVG_DATA_OFFLINE");

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);
    let out_path = out_dir.join("catalog.rs");

    let elements_json = fs::read_to_string(manifest_dir.join("data/elements.json"))
        .expect("failed to read elements.json");
    let attributes_json = fs::read_to_string(manifest_dir.join("data/attributes.json"))
        .expect("failed to read attributes.json");

    let elements: Vec<JsonElement> =
        serde_json::from_str(&elements_json).expect("failed to parse elements.json");
    let attributes: Vec<JsonAttribute> =
        serde_json::from_str(&attributes_json).expect("failed to parse attributes.json");

    // Fetch compat data (graceful fallback on failure)
    let compat = fetch_compat_data(out_dir);

    let mut out = String::with_capacity(16384);

    writeln!(out, "// @generated by build.rs -- do not edit").unwrap();
    writeln!(out).unwrap();

    // ---- Element statics ----

    // Build a map of element name -> index for unique static names
    let el_idents: HashMap<&str, String> = elements
        .iter()
        .map(|e| (e.name.as_str(), ident_from(&e.name)))
        .collect();

    for el in &elements {
        let id = &el_idents[el.name.as_str()];

        write_static_str_slice(
            &mut out,
            &format!("EL_{id}_REQUIRED_ATTRS"),
            &el.required_attrs,
        );
        write_static_str_slice(&mut out, &format!("EL_{id}_ATTRS"), &el.attrs);

        // Children categories (if applicable)
        if let ContentModelJson::Children { children } = &el.content_model {
            write!(out, "static EL_{id}_CHILDREN: &[ElementCategory] = &[").unwrap();
            for (i, cat) in children.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write!(out, "ElementCategory::{cat}").unwrap();
            }
            writeln!(out, "];").unwrap();
        }

        writeln!(out).unwrap();
    }

    // ---- Attribute statics ----

    let attr_idents: HashMap<&str, String> = attributes
        .iter()
        .map(|a| (a.name.as_str(), ident_from(&a.name)))
        .collect();

    for attr in &attributes {
        let id = &attr_idents[attr.name.as_str()];

        write_static_str_slice(&mut out, &format!("ATTR_{id}_ELEMENTS"), &attr.elements);

        match &attr.values {
            ValuesJson::Enum { values } => {
                write_static_str_slice(&mut out, &format!("ATTR_{id}_VALUES"), values);
            }
            ValuesJson::Transform { functions } => {
                write_static_str_slice(&mut out, &format!("ATTR_{id}_FUNCTIONS"), functions);
            }
            ValuesJson::PreserveAspectRatio {
                alignments,
                meet_or_slice,
            } => {
                write_static_str_slice(&mut out, &format!("ATTR_{id}_ALIGNMENTS"), alignments);
                write_static_str_slice(
                    &mut out,
                    &format!("ATTR_{id}_MEET_OR_SLICE"),
                    meet_or_slice,
                );
            }
            _ => {}
        }

        writeln!(out).unwrap();
    }

    // ---- ELEMENTS array ----

    writeln!(out, "pub(crate) static ELEMENTS: &[ElementDef] = &[").unwrap();
    for el in &elements {
        let id = &el_idents[el.name.as_str()];
        let content_model = match &el.content_model {
            ContentModelJson::Simple(s) => match s.as_str() {
                "foreign" => "ContentModel::Foreign".to_string(),
                "void" => "ContentModel::Void".to_string(),
                "text" => "ContentModel::Text".to_string(),
                other => panic!("unknown simple content_model: {other}"),
            },
            ContentModelJson::Children { .. } => {
                format!("ContentModel::Children(EL_{id}_CHILDREN)")
            }
        };

        // Use compat data if available, otherwise fall back to JSON values
        let (deprecated, experimental, spec_url_str, baseline_str, browser_support_str) =
            match compat.elements.get(&el.name) {
                Some(entry) => (
                    entry.deprecated,
                    entry.experimental,
                    format_option_str(entry.spec_url.as_deref()),
                    format_baseline(entry.baseline.as_ref()),
                    format_browser_support(entry.browser_support.as_ref()),
                ),
                None => (
                    el.deprecated,
                    false,
                    "None".to_string(),
                    "None".to_string(),
                    "None".to_string(),
                ),
            };

        writeln!(out, "    ElementDef {{").unwrap();
        writeln!(out, "        name: \"{}\",", escape(&el.name)).unwrap();
        writeln!(out, "        description: \"{}\",", escape(&el.description)).unwrap();
        writeln!(out, "        mdn_url: \"{}\",", escape(&el.mdn_url)).unwrap();
        writeln!(out, "        deprecated: {deprecated},").unwrap();
        writeln!(out, "        experimental: {experimental},").unwrap();
        writeln!(out, "        spec_url: {spec_url_str},").unwrap();
        writeln!(out, "        baseline: {baseline_str},").unwrap();
        writeln!(out, "        browser_support: {browser_support_str},").unwrap();
        writeln!(out, "        content_model: {content_model},").unwrap();
        writeln!(out, "        required_attrs: EL_{id}_REQUIRED_ATTRS,").unwrap();
        writeln!(out, "        attrs: EL_{id}_ATTRS,").unwrap();
        writeln!(out, "        global_attrs: {},", el.global_attrs).unwrap();
        writeln!(out, "    }},").unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    // ---- BCD-only attribute statics ----

    // Collect BCD attributes not in the curated set
    let curated_names: std::collections::HashSet<&str> =
        attributes.iter().map(|a| a.name.as_str()).collect();
    let mut bcd_only: Vec<(&str, &BcdAttribute)> = compat
        .attributes
        .iter()
        .filter(|(name, _)| !curated_names.contains(name.as_str()))
        .map(|(name, bcd)| (name.as_str(), bcd))
        .collect();
    bcd_only.sort_by_key(|(name, _)| *name);

    for (name, bcd) in &bcd_only {
        let id = ident_from(name);
        write_static_str_slice(&mut out, &format!("ATTR_{id}_ELEMENTS"), &bcd.elements);
        writeln!(out).unwrap();
    }

    println!(
        "cargo::warning=compat: merged {} BCD-only attributes into catalog",
        bcd_only.len()
    );

    // ---- ATTRIBUTES array ----

    writeln!(out, "pub(crate) static ATTRIBUTES: &[AttributeDef] = &[").unwrap();

    // 1) Curated attributes (rich types, descriptions)
    for attr in &attributes {
        let id = &attr_idents[attr.name.as_str()];
        let values = match &attr.values {
            ValuesJson::Enum { .. } => format!("AttributeValues::Enum(ATTR_{id}_VALUES)"),
            ValuesJson::FreeText => "AttributeValues::FreeText".to_string(),
            ValuesJson::Color => "AttributeValues::Color".to_string(),
            ValuesJson::Length => "AttributeValues::Length".to_string(),
            ValuesJson::Url => "AttributeValues::Url".to_string(),
            ValuesJson::NumberOrPercentage => "AttributeValues::NumberOrPercentage".to_string(),
            ValuesJson::Transform { .. } => {
                format!("AttributeValues::Transform(ATTR_{id}_FUNCTIONS)")
            }
            ValuesJson::Viewbox => "AttributeValues::ViewBox".to_string(),
            ValuesJson::PreserveAspectRatio { .. } => {
                format!(
                    "AttributeValues::PreserveAspectRatio {{ alignments: ATTR_{id}_ALIGNMENTS, meet_or_slice: ATTR_{id}_MEET_OR_SLICE }}"
                )
            }
            ValuesJson::Points => "AttributeValues::Points".to_string(),
            ValuesJson::PathData => "AttributeValues::PathData".to_string(),
        };
        writeln!(out, "    AttributeDef {{").unwrap();
        writeln!(out, "        name: \"{}\",", escape(&attr.name)).unwrap();
        writeln!(
            out,
            "        description: \"{}\",",
            escape(&attr.description)
        )
        .unwrap();
        writeln!(out, "        mdn_url: \"{}\",", escape(&attr.mdn_url)).unwrap();
        let (deprecated, experimental, spec_url_str, baseline_str, browser_support_str) =
            match compat.attributes.get(&attr.name) {
                Some(bcd) => (
                    bcd.compat.deprecated,
                    bcd.compat.experimental,
                    format_option_str(bcd.compat.spec_url.as_deref()),
                    format_baseline(bcd.compat.baseline.as_ref()),
                    format_browser_support(bcd.compat.browser_support.as_ref()),
                ),
                None => (
                    attr.deprecated,
                    false,
                    "None".to_string(),
                    "None".to_string(),
                    "None".to_string(),
                ),
            };
        writeln!(out, "        deprecated: {deprecated},").unwrap();
        writeln!(out, "        experimental: {experimental},").unwrap();
        writeln!(out, "        spec_url: {spec_url_str},").unwrap();
        writeln!(out, "        baseline: {baseline_str},").unwrap();
        writeln!(out, "        browser_support: {browser_support_str},").unwrap();
        writeln!(out, "        values: {values},").unwrap();
        writeln!(out, "        elements: ATTR_{id}_ELEMENTS,").unwrap();
        writeln!(out, "    }},").unwrap();
    }

    // 2) BCD-only attributes (auto-generated, FreeText values)
    for (name, bcd) in &bcd_only {
        let id = ident_from(name);
        let mdn_url = format!(
            "https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/{}",
            name
        );
        let description = format!("The {} SVG attribute.", name);
        let deprecated = bcd.compat.deprecated;
        let experimental = bcd.compat.experimental;
        let spec_url_str = format_option_str(bcd.compat.spec_url.as_deref());
        let baseline_str = format_baseline(bcd.compat.baseline.as_ref());
        let browser_support_str = format_browser_support(bcd.compat.browser_support.as_ref());
        writeln!(out, "    AttributeDef {{").unwrap();
        writeln!(out, "        name: \"{}\",", escape(name)).unwrap();
        writeln!(out, "        description: \"{}\",", escape(&description)).unwrap();
        writeln!(out, "        mdn_url: \"{}\",", escape(&mdn_url)).unwrap();
        writeln!(out, "        deprecated: {deprecated},").unwrap();
        writeln!(out, "        experimental: {experimental},").unwrap();
        writeln!(out, "        spec_url: {spec_url_str},").unwrap();
        writeln!(out, "        baseline: {baseline_str},").unwrap();
        writeln!(out, "        browser_support: {browser_support_str},").unwrap();
        writeln!(out, "        values: AttributeValues::FreeText,").unwrap();
        writeln!(out, "        elements: ATTR_{id}_ELEMENTS,").unwrap();
        writeln!(out, "    }},").unwrap();
    }

    writeln!(out, "];").unwrap();

    fs::write(&out_path, out).expect("failed to write catalog.rs");
}
