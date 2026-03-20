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

/// Resolved baseline + deprecated for one element.
struct CompatEntry {
    deprecated: bool,
    baseline: Option<BaselineValue>,
}

enum BaselineValue {
    Widely { since: u16 },
    Newly { since: u16 },
    Limited,
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

struct CompatData {
    elements: HashMap<String, CompatEntry>,
    attributes: HashMap<String, CompatEntry>,
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

        // Extract deprecated status
        let deprecated = compat
            .pointer("/status/deprecated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Extract web-features tag: look for "web-features:XXX" in tags array
        let baseline = extract_baseline(compat, &wf_features);

        map.insert(
            el_name.clone(),
            CompatEntry {
                deprecated,
                baseline,
            },
        );
    }

    println!(
        "cargo::warning=compat: loaded {} element entries from BCD",
        map.len()
    );

    // Also process svg.global_attributes for attribute baseline/deprecated
    let mut attr_map = HashMap::new();
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
            let baseline = extract_baseline(compat, &wf_features);
            attr_map.insert(
                attr_name.clone(),
                CompatEntry {
                    deprecated,
                    baseline,
                },
            );
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

/// Given a BCD __compat object and the web-features data, resolve baseline status.
fn extract_baseline(
    compat: &serde_json::Value,
    wf_features: &Option<serde_json::Value>,
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

    let baseline_val = status.get("baseline")?;

    match baseline_val {
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
                "void" => "ContentModel::Void".to_string(),
                "text" => "ContentModel::Text".to_string(),
                other => panic!("unknown simple content_model: {other}"),
            },
            ContentModelJson::Children { .. } => {
                format!("ContentModel::Children(EL_{id}_CHILDREN)")
            }
        };

        // Use compat data if available, otherwise fall back to JSON values
        let (deprecated, baseline_str) = match compat.elements.get(&el.name) {
            Some(entry) => (entry.deprecated, format_baseline(entry.baseline.as_ref())),
            None => (el.deprecated, "None".to_string()),
        };

        writeln!(out, "    ElementDef {{").unwrap();
        writeln!(out, "        name: \"{}\",", escape(&el.name)).unwrap();
        writeln!(out, "        description: \"{}\",", escape(&el.description)).unwrap();
        writeln!(out, "        mdn_url: \"{}\",", escape(&el.mdn_url)).unwrap();
        writeln!(out, "        deprecated: {deprecated},").unwrap();
        writeln!(out, "        baseline: {baseline_str},").unwrap();
        writeln!(out, "        content_model: {content_model},").unwrap();
        writeln!(out, "        required_attrs: EL_{id}_REQUIRED_ATTRS,").unwrap();
        writeln!(out, "        attrs: EL_{id}_ATTRS,").unwrap();
        writeln!(out, "        global_attrs: {},", el.global_attrs).unwrap();
        writeln!(out, "    }},").unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    // ---- ATTRIBUTES array ----

    writeln!(out, "pub(crate) static ATTRIBUTES: &[AttributeDef] = &[").unwrap();
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
        let (deprecated, baseline_str) = match compat.attributes.get(&attr.name) {
            Some(entry) => (entry.deprecated, format_baseline(entry.baseline.as_ref())),
            None => (attr.deprecated, "None".to_string()),
        };
        writeln!(out, "        deprecated: {deprecated},").unwrap();
        writeln!(out, "        baseline: {baseline_str},").unwrap();
        writeln!(out, "        values: {values},").unwrap();
        writeln!(out, "        elements: ATTR_{id}_ELEMENTS,").unwrap();
        writeln!(out, "    }},").unwrap();
    }
    writeln!(out, "];").unwrap();

    fs::write(&out_path, out).expect("failed to write catalog.rs");
}
