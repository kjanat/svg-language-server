use std::{collections::HashMap, fs, path::Path};

use super::{
    BaselineQualifierValue, BaselineValue, BrowserSupportValue, BrowserVersionValue, CompatEntry,
    ensure_cached, worker_schema,
};

const SVG_COMPAT_URL: &str = "https://svg-compat.kjanat.com/data.json";

/// BCD-discovered attribute with compat metadata and element applicability.
pub struct BcdAttribute {
    pub compat: CompatEntry,
    /// Element names this attribute applies to. Contains `"*"` if global.
    pub elements: Vec<String>,
}

pub struct CompatData {
    pub elements: HashMap<String, CompatEntry>,
    /// Attributes from the worker (global + element-specific, pre-merged).
    pub attributes: HashMap<String, BcdAttribute>,
}

/// Fetch pre-processed compat data from the svg-compat worker.
/// On any failure, prints a cargo warning and returns empty maps.
pub fn fetch_compat_data(out_dir: &Path) -> CompatData {
    let offline = std::env::var("SVG_DATA_OFFLINE").is_ok();
    let url = std::env::var("SVG_COMPAT_URL").unwrap_or_else(|_| SVG_COMPAT_URL.to_string());
    let cache_path = out_dir.join("svg-compat-data.json");

    let empty = CompatData {
        elements: HashMap::new(),
        attributes: HashMap::new(),
    };

    match ensure_cached(&url, &cache_path, offline) {
        Ok(true) => {}
        Ok(false) => {
            println!("cargo::warning=compat: no cached data and offline — skipping");
            return empty;
        }
        Err(error) => {
            println!("cargo::warning=compat: fetch failed: {error}");
            return empty;
        }
    }

    let raw = match fs::read_to_string(&cache_path) {
        Ok(source) => source,
        Err(error) => {
            println!("cargo::warning=compat: failed to read cache: {error}");
            return empty;
        }
    };

    let output: worker_schema::WorkerOutput = match serde_json::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(error) => {
            println!("cargo::warning=compat: failed to parse worker JSON: {error}");
            return empty;
        }
    };

    let elements: HashMap<String, CompatEntry> = output
        .elements
        .iter()
        .map(|(name, entry)| (name.clone(), convert_element(entry)))
        .collect();

    let attributes: HashMap<String, BcdAttribute> = output
        .attributes
        .iter()
        .map(|(name, entry)| (name.clone(), convert_attribute(entry)))
        .collect();

    println!(
        "svg-data: loaded {} element entries from worker",
        elements.len()
    );
    println!(
        "svg-data: loaded {} attribute entries from worker",
        attributes.len()
    );

    CompatData {
        elements,
        attributes,
    }
}

fn convert_element(entry: &worker_schema::WorkerElement) -> CompatEntry {
    CompatEntry {
        deprecated: entry.deprecated,
        experimental: entry.experimental,
        spec_url: entry.spec_url.first().cloned(),
        baseline: entry.baseline.as_ref().and_then(convert_baseline),
        browser_support: entry.browser_support.as_ref().map(convert_browser_support),
    }
}

fn convert_attribute(entry: &worker_schema::WorkerAttribute) -> BcdAttribute {
    BcdAttribute {
        compat: CompatEntry {
            deprecated: entry.deprecated,
            experimental: entry.experimental,
            spec_url: entry.spec_url.first().cloned(),
            baseline: entry.baseline.as_ref().and_then(convert_baseline),
            browser_support: entry.browser_support.as_ref().map(convert_browser_support),
        },
        elements: entry.elements.clone(),
    }
}

fn convert_baseline(b: &worker_schema::WorkerBaseline) -> Option<BaselineValue> {
    let qualifier = convert_qualifier(b.since_qualifier.as_deref());
    match b.status.as_str() {
        "widely" => Some(BaselineValue::Widely {
            since: b.since?,
            qualifier,
        }),
        "newly" => Some(BaselineValue::Newly {
            since: b.since?,
            qualifier,
        }),
        "limited" => Some(BaselineValue::Limited),
        other => {
            // Match the worker's "warn loudly on unknown" rule so an
            // unexpected upstream status doesn't silently drop the entry.
            println!(
                "cargo::warning=svg-data: unknown baseline status {other:?}, treating as Limited"
            );
            Some(BaselineValue::Limited)
        }
    }
}

fn convert_qualifier(raw: Option<&str>) -> Option<BaselineQualifierValue> {
    match raw {
        Some("before") => Some(BaselineQualifierValue::Before),
        Some("after") => Some(BaselineQualifierValue::After),
        Some("approximately") => Some(BaselineQualifierValue::Approximately),
        Some(other) => {
            println!(
                "cargo::warning=svg-data: unknown baseline qualifier {other:?}, treating as Approximately"
            );
            Some(BaselineQualifierValue::Approximately)
        }
        None => None,
    }
}

fn convert_browser_support(bs: &worker_schema::WorkerBrowserSupport) -> BrowserSupportValue {
    let version = |v: &Option<String>| v.as_ref().map(|s| BrowserVersionValue::Version(s.clone()));
    BrowserSupportValue {
        chrome: version(&bs.chrome),
        edge: version(&bs.edge),
        firefox: version(&bs.firefox),
        safari: version(&bs.safari),
    }
}
