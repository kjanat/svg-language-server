use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::{
    BaselineQualifierValue, BaselineValue, BrowserFlagValue, BrowserSupportValue,
    BrowserVersionValue, CompatEntry, RawVersionAddedValue, ensure_cached, worker_schema,
};

const SVG_COMPAT_URL: &str = "https://svg-compat.kjanat.com/data.json";
const SVG_COMPAT_FILE_ENV: &str = "SVG_COMPAT_FILE";
const SVG_COMPAT_URL_ENV: &str = "SVG_COMPAT_URL";

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
    let cache_path = out_dir.join("svg-compat-data.json");

    let empty = CompatData {
        elements: HashMap::new(),
        attributes: HashMap::new(),
    };

    let raw = match load_worker_json(&cache_path, offline) {
        Ok(Some(source)) => source,
        Ok(None) => return empty,
        Err(error) => {
            println!("cargo::warning=compat: {error}");
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

fn load_worker_json(cache_path: &Path, offline: bool) -> Result<Option<String>, String> {
    if let Some(file_path) = compat_file_path()? {
        println!("svg-data: using local compat file {}", file_path.display());
        return read_json_file(&file_path, "local compat file").map(Some);
    }

    if std::env::var(SVG_COMPAT_URL_ENV).is_err()
        && !offline
        && let Some(worker_dir) = local_worker_dir()
    {
        match run_local_worker_cli(&worker_dir, cache_path) {
            Ok(()) => {
                println!(
                    "svg-data: using local svg-compat CLI {}",
                    worker_dir.display()
                );
                return read_json_file(cache_path, "local svg-compat CLI output").map(Some);
            }
            Err(error) => {
                println!(
                    "cargo::warning=compat: local svg-compat CLI failed: {error}; falling back to remote cache"
                );
            }
        }
    }

    let url = std::env::var(SVG_COMPAT_URL_ENV).unwrap_or_else(|_| SVG_COMPAT_URL.to_string());
    match ensure_cached(&url, cache_path, offline) {
        Ok(true) => read_json_file(cache_path, "compat cache").map(Some),
        Ok(false) => {
            println!("cargo::warning=compat: no cached data and offline — skipping");
            Ok(None)
        }
        Err(error) => Err(format!("fetch failed: {error}")),
    }
}

fn compat_file_path() -> Result<Option<PathBuf>, String> {
    let Ok(raw_path) = std::env::var(SVG_COMPAT_FILE_ENV) else {
        return Ok(None);
    };

    let path = PathBuf::from(raw_path);
    let resolved = if path.is_absolute() {
        path
    } else {
        workspace_root()
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf())
            .join(path)
    };

    if resolved.exists() {
        Ok(Some(resolved))
    } else {
        Err(format!(
            "{} points to missing file {}",
            SVG_COMPAT_FILE_ENV,
            resolved.display()
        ))
    }
}

fn local_worker_dir() -> Option<PathBuf> {
    let worker_dir = workspace_root()?.join("workers/svg-compat");
    let cli_path = worker_dir.join("src/cli.ts");
    cli_path.exists().then_some(worker_dir)
}

fn workspace_root() -> Option<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
}

fn run_local_worker_cli(worker_dir: &Path, cache_path: &Path) -> Result<(), String> {
    let output = Command::new("deno")
        .arg("run")
        .arg("-A")
        .arg("src/cli.ts")
        .arg("emit")
        .arg("data")
        .arg("--out")
        .arg(cache_path)
        .current_dir(worker_dir)
        .output()
        .map_err(|error| format!("spawn deno in {}: {error}", worker_dir.display()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let details = stderr.trim();
    if !details.is_empty() {
        return Err(format!("exit {}: {details}", output.status));
    }

    let details = stdout.trim();
    if !details.is_empty() {
        return Err(format!("exit {}: {details}", output.status));
    }

    Err(format!("exit {}", output.status))
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
