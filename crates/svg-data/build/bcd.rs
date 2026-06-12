use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use super::{
    BaselineQualifierValue, BaselineValue, BrowserFlagValue, BrowserSupportValue,
    BrowserVersionValue, CompatEntry, RawVersionAddedValue, ensure_cached, worker_schema,
};

const SVG_COMPAT_URL: &str = "https://svg-compat.kjanat.com/data.json";
const SVG_COMPAT_FILE_ENV: &str = "SVG_COMPAT_FILE";
const SVG_COMPAT_URL_ENV: &str = "SVG_COMPAT_URL";

/// Vendored compat slice, relative to the crate manifest dir. This is the
/// default, hermetic source for the build — no network, no Deno toolchain.
/// Stage 1 of the spec-derivation work checked this file in; the build reads
/// it directly. `SVG_COMPAT_FILE` / `SVG_COMPAT_URL` remain optional refresh
/// overrides only.
const VENDORED_COMPAT_PATH: &str = "data/sources/svg-compat-data.json";

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
    /// BCD package version the worker was built against, for
    /// reconciliation error messages. Falls back to `"unknown"` when
    /// the worker JSON didn't include source metadata.
    pub bcd_version: String,
}

/// Load pre-processed compat data for the build.
///
/// Default source is the vendored slice at [`VENDORED_COMPAT_PATH`]; a missing
/// or unparseable vendored slice is a HARD build error (Q-BCD-FALLBACK), since
/// the baked catalog cannot be produced without it. `SVG_COMPAT_FILE` and
/// `SVG_COMPAT_URL` are optional refresh overrides; of those, only the
/// network (`SVG_COMPAT_URL`) override may fail soft to empty maps.
pub fn fetch_compat_data(out_dir: &Path) -> CompatData {
    let offline = std::env::var("SVG_DATA_OFFLINE").is_ok();
    let cache_path = out_dir.join("svg-compat-data.json");

    let empty = CompatData {
        elements: HashMap::new(),
        attributes: HashMap::new(),
        bcd_version: "unknown".to_string(),
    };

    let raw = match load_compat_json(&cache_path, offline) {
        CompatLoad::Loaded(source) => source,
        CompatLoad::SoftEmpty(reason) => {
            println!("cargo::warning=compat: {reason}");
            return empty;
        }
        CompatLoad::Hard(reason) => {
            panic!(
                "svg-data build: vendored compat slice unusable: {reason}\n\
                 The build reads {VENDORED_COMPAT_PATH} (relative to the crate). \
                 Restore/refresh it (see data/sources/svg-compat-data.PROVENANCE.toml).",
            );
        }
    };

    let output: worker_schema::WorkerOutput = match serde_json::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(error) => {
            panic!(
                "svg-data build: failed to parse compat JSON ({VENDORED_COMPAT_PATH} \
                 or override): {error}",
            );
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
        "svg-data: loaded {} element entries from compat slice",
        elements.len()
    );
    println!(
        "svg-data: loaded {} attribute entries from compat slice",
        attributes.len()
    );

    // Parse the BCD package version out of the worker's `sources.bcd.resolved`
    // field. Plain JSON lookup — we don't want to extend `worker_schema.rs`
    // just for an error-message nicety.
    let bcd_version = serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| {
            v.get("sources")?
                .get("bcd")?
                .get("resolved")?
                .as_str()
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_string());

    CompatData {
        elements,
        attributes,
        bcd_version,
    }
}

/// Outcome of locating + reading the compat JSON for the build.
enum CompatLoad {
    /// Raw JSON text was read successfully.
    Loaded(String),
    /// Degrade-to-empty path (only the `SVG_COMPAT_URL` network override may
    /// take this when the fetch can't be satisfied).
    SoftEmpty(String),
    /// Unrecoverable: the build cannot bake the catalog. Surfaces as a panic
    /// (Q-BCD-FALLBACK — no silent degrade for the vendored default).
    Hard(String),
}

fn load_compat_json(cache_path: &Path, offline: bool) -> CompatLoad {
    // Explicit local-file override: must exist and be readable. Pointing the
    // override at a missing file is an operator error, not a soft degrade.
    if let Ok(raw_path) = std::env::var(SVG_COMPAT_FILE_ENV) {
        let resolved = resolve_relative(&raw_path);
        println!(
            "svg-data: using {SVG_COMPAT_FILE_ENV} {}",
            resolved.display()
        );
        return match read_json_file(&resolved, SVG_COMPAT_FILE_ENV) {
            Ok(raw) => CompatLoad::Loaded(raw),
            Err(error) => CompatLoad::Hard(error),
        };
    }

    // Network refresh override: allowed to fail soft to empty maps.
    if let Ok(url) = std::env::var(SVG_COMPAT_URL_ENV) {
        println!("svg-data: using {SVG_COMPAT_URL_ENV} {url}");
        return match ensure_cached(&url, cache_path, offline) {
            Ok(true) => match read_json_file(cache_path, "compat cache") {
                Ok(raw) => CompatLoad::Loaded(raw),
                Err(error) => CompatLoad::SoftEmpty(error),
            },
            Ok(false) => CompatLoad::SoftEmpty("no cached data and offline — skipping".to_string()),
            Err(error) => CompatLoad::SoftEmpty(format!("fetch failed: {error}")),
        };
    }

    // Default, hermetic path: the vendored slice. Missing/unreadable is fatal.
    let vendored = Path::new(env!("CARGO_MANIFEST_DIR")).join(VENDORED_COMPAT_PATH);
    println!(
        "svg-data: using vendored compat slice {}",
        vendored.display()
    );
    match read_json_file(&vendored, "vendored compat slice") {
        Ok(raw) => CompatLoad::Loaded(raw),
        Err(error) => CompatLoad::Hard(error),
    }
}

/// Resolve a user-supplied path against the workspace root (falling back to the
/// crate manifest dir), leaving absolute paths untouched.
fn resolve_relative(raw_path: &str) -> PathBuf {
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        return path;
    }
    workspace_root()
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf())
        .join(path)
}

fn workspace_root() -> Option<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
}

fn read_json_file(path: &Path, label: &str) -> Result<String, String> {
    fs::read_to_string(path)
        .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))
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
    BrowserSupportValue {
        chrome: bs.chrome.as_ref().map(convert_browser_version),
        edge: bs.edge.as_ref().map(convert_browser_version),
        firefox: bs.firefox.as_ref().map(convert_browser_version),
        safari: bs.safari.as_ref().map(convert_browser_version),
    }
}

fn convert_browser_version(v: &worker_schema::WorkerBrowserVersion) -> BrowserVersionValue {
    BrowserVersionValue {
        raw_value_added: convert_raw_version_added(&v.raw_value_added),
        version_added: v.version_added.clone(),
        version_qualifier: convert_version_qualifier(v.version_qualifier.as_deref()),
        supported: v.supported,
        version_removed: v.version_removed.clone(),
        version_removed_qualifier: convert_version_qualifier(
            v.version_removed_qualifier.as_deref(),
        ),
        partial_implementation: v.partial_implementation.unwrap_or(false),
        prefix: v.prefix.clone(),
        alternative_name: v.alternative_name.clone(),
        flags: v
            .flags
            .as_ref()
            .map(|list| list.iter().map(convert_browser_flag).collect())
            .unwrap_or_default(),
        notes: v.notes.clone().unwrap_or_default(),
    }
}

fn convert_raw_version_added(raw: &worker_schema::WorkerRawVersionAdded) -> RawVersionAddedValue {
    match raw {
        worker_schema::WorkerRawVersionAdded::Text(s) => RawVersionAddedValue::Text(s.clone()),
        worker_schema::WorkerRawVersionAdded::Flag(b) => RawVersionAddedValue::Flag(*b),
        worker_schema::WorkerRawVersionAdded::Null => RawVersionAddedValue::Null,
    }
}

fn convert_version_qualifier(raw: Option<&str>) -> Option<BaselineQualifierValue> {
    // Version qualifier uses the same semantic as baseline qualifier.
    convert_qualifier(raw)
}

fn convert_browser_flag(f: &worker_schema::WorkerBrowserFlag) -> BrowserFlagValue {
    BrowserFlagValue {
        flag_type: f.r#type.clone(),
        name: f.name.clone(),
        value_to_set: f.value_to_set.clone(),
    }
}
