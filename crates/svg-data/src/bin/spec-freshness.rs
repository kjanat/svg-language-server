//! Live spec-freshness sentinel.
//!
//! Compares the crate's **baked** spec catalog against what W3C, `svgwg`, and
//! the upstream compat-data registries publish *right now*, and reports any
//! drift:
//!
//! * **Published editions** — for each SVG series, fetch the W3C specification
//!   API `version-history` and flag any dated `/TR/` version the baked
//!   [`EDITION_INDEX`](svg_data::edition::EDITION_INDEX) has not vendored yet
//!   (via the pure [`unseen_versions`](svg_data::edition::unseen_versions)).
//! * **Rolling editor's draft** — fetch `svgwg`'s default-branch HEAD and compare it
//!   against the baked [`ROLLING_PIN`](svg_data::edition::ROLLING_PIN) commit
//!   (via the pure [`classify_freshness`](svg_data::edition::classify_freshness)).
//!   Reported in two *distinct* signals so a path-irrelevant default-branch
//!   commit does not masquerade as real drift: bare HEAD movement, and
//!   **tracked-input** drift — whether any vendored `master/*` input's git blob
//!   has actually changed at the upstream branch versus the blob SHA recorded in
//!   the vendored `PROVENANCE.toml`.
//! * **Compat data** (`--compat-drift`) — compare the vendored
//!   `@mdn/browser-compat-data` and `web-features` versions baked into
//!   `data/sources/svg-compat-data.json` against the latest versions the npm
//!   registry publishes, and report any lag.
//!
//! The W3C/svgwg decision logic lives in `svg_data::edition` and is unit-tested
//! offline; this binary is only the network shell + reporting around it.
//!
//! Exit codes: `0` = up to date, `1` = drift detected (a refresh is due), `2` =
//! an operational error (network/parse) prevented the check. CI keys an issue
//! off exit `1`; exit `2` should fail the job loudly instead.
//!
//! Usage: `spec-freshness [--json] [--compat-drift]`. With `--json` the report
//! is emitted as a single JSON object on stdout (for use as an issue body);
//! otherwise a human-readable summary is printed. `--compat-drift` runs the
//! compat-data registry check instead of the W3C/svgwg edition checks.

use std::{path::PathBuf, process::ExitCode, time::Duration};

use serde::Serialize;
use svg_data::edition::{
    CapturedEditionIdentity, Freshness, PublishedVersion, ROLLING_PIN, Series, VersionsEnvelope,
    classify_freshness, unseen_versions,
};

/// A single newly-published edition the baked catalog has not caught up to.
#[derive(Debug, Serialize)]
struct PublishedDrift {
    series: Series,
    date: String,
    status: String,
    uri: String,
}

/// One vendored input whose upstream git blob has moved past the recorded pin.
#[derive(Debug, Serialize)]
struct TrackedInputDrift {
    /// Provenance input id (e.g. `definitions`).
    id: String,
    /// Path within the upstream repo this input was vendored from.
    upstream_path: String,
    /// The blob SHA recorded in the vendored `PROVENANCE.toml`.
    pinned_blob: String,
    /// The blob SHA the upstream default branch carries now.
    head_blob: String,
}

/// Rolling editor's-draft comparison result.
///
/// `state` is the bare HEAD-movement signal (whether the default branch tip
/// advanced past the pin); it is reported **separately** from
/// `tracked_input_drift`, which is the path-relevant signal (whether any
/// vendored input's blob actually changed). A default branch can move for
/// reasons that never touch the files we vendor, so only `tracked_input_drift`
/// (or an inability to evaluate it) should be treated as a true refresh
/// trigger.
#[derive(Debug, Serialize)]
struct RollingReport {
    repository: String,
    /// The repository's default branch, resolved at runtime (not hardcoded) so
    /// the check follows upstream even if it renames `master` → `main` etc.
    branch: String,
    pinned_commit: String,
    head_commit: String,
    /// `"current"` when HEAD matches the pin, `"stale"` when it has advanced.
    /// Bare HEAD movement only — see `tracked_input_drift` for the path-relevant
    /// signal.
    state: &'static str,
    /// Vendored inputs whose upstream blob changed at `branch` HEAD versus the
    /// `PROVENANCE.toml` pin. Empty when every tracked input is byte-identical
    /// upstream (so a moved HEAD is path-irrelevant). [`None`] when the
    /// tracked-input comparison could not be performed (no provenance, or every
    /// blob lookup failed) — distinct from "evaluated, no drift".
    tracked_input_drift: Option<Vec<TrackedInputDrift>>,
}

/// The full freshness verdict, serialised to stdout under `--json`.
#[derive(Debug, Serialize)]
struct FreshnessReport {
    fresh: bool,
    published_drift: Vec<PublishedDrift>,
    rolling: RollingReport,
}

/// One compat-data dependency's vendored-vs-latest version comparison.
#[derive(Debug, Serialize)]
struct CompatDependencyDrift {
    /// npm package name, e.g. `@mdn/browser-compat-data`.
    package: String,
    /// The version baked into `data/sources/svg-compat-data.json`.
    vendored: String,
    /// The latest version the npm registry publishes.
    latest: String,
}

/// The compat-data freshness verdict, serialised to stdout under
/// `--compat-drift --json`.
#[derive(Debug, Serialize)]
struct CompatDriftReport {
    fresh: bool,
    /// Per-dependency drift; empty when every vendored version matches latest.
    drift: Vec<CompatDependencyDrift>,
}

/// Build the shared HTTP agent with a 30s global timeout, mirroring the LSP
/// crate's runtime freshness agent (`crates/svg-language-server/src/freshness.rs`)
/// so a hung endpoint cannot stall the sentinel indefinitely.
fn agent() -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build(),
    )
}

/// W3C specification API endpoint for a series' full version history.
fn w3c_versions_url(series: Series) -> String {
    format!(
        "https://api.w3.org/specifications/{}/versions?embed=1&items=100",
        series.shortname()
    )
}

/// GET `url`, returning the response body as a string.
///
/// Sends a `User-Agent` (GitHub rejects requests without one) and an optional
/// bearer token from `GITHUB_TOKEN` so CI runs use the authenticated rate limit.
fn fetch(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let mut request = agent
        .get(url)
        .header("User-Agent", "svg-language-server-spec-freshness");
    if url.contains("api.github.com")
        && let Ok(token) = std::env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        request = request.header("Authorization", &format!("Bearer {token}"));
    }
    let mut response = request.call().map_err(|e| format!("fetch {url}: {e}"))?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("read body {url}: {e}"))
}

/// Collect every published edition newer than the baked index, across all series.
fn check_published(agent: &ureq::Agent) -> Result<Vec<PublishedDrift>, String> {
    let mut drift = Vec::new();
    for series in Series::ALL {
        let json = fetch(agent, &w3c_versions_url(series))?;
        let live = VersionsEnvelope::parse(series, &json)
            .map_err(|e| format!("parse {} versions: {e}", series.shortname()))?;
        for version in unseen_versions(series, &live) {
            drift.push(published_drift(series, &version));
        }
    }
    Ok(drift)
}

fn published_drift(series: Series, version: &PublishedVersion) -> PublishedDrift {
    PublishedDrift {
        series,
        date: version.date.to_string(),
        status: format!("{:?}", version.status),
        uri: version.uri.to_string(),
    }
}

/// GitHub REST base for the svgwg repository.
const SVGWG_REPO_API: &str = "https://api.github.com/repos/w3c/svgwg";

/// Resolve svgwg's default branch at runtime rather than hardcoding `master`
/// (which is a stale legacy branch) or `main`.
fn svgwg_default_branch(agent: &ureq::Agent) -> Result<String, String> {
    let json = fetch(agent, SVGWG_REPO_API)?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("parse svgwg repo metadata: {e}"))?;
    value
        .get("default_branch")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "svgwg repo response had no `default_branch` field".to_string())
}

/// One vendored input, distilled from a `PROVENANCE.toml` `[[inputs]]` entry.
struct ProvenanceInput {
    id: String,
    /// The path within the upstream repo (`upstream = "…"`), used to address the
    /// file via the GitHub contents API.
    upstream_path: String,
    /// The recorded git blob SHA (`git_blob = "…"`).
    git_blob: String,
}

/// Path (relative to the crate manifest) of the svgwg provenance manifest the
/// rolling pin's tracked inputs are recorded in.
const SVGWG_PROVENANCE: &str = "data/sources/svgwg-19482daf/PROVENANCE.toml";

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// Read the tracked inputs from the vendored svgwg `PROVENANCE.toml`.
///
/// Returns the inputs that carry both an `upstream` path and a `git_blob` SHA —
/// the pair needed to compare a vendored file against its upstream blob. Returns
/// an empty vec (not an error) when the manifest is absent or carries no usable
/// inputs, so a missing manifest degrades the tracked-input signal to
/// "unevaluated" rather than failing the whole check.
fn read_tracked_inputs() -> Vec<ProvenanceInput> {
    let Ok(raw) = std::fs::read_to_string(manifest_path(SVGWG_PROVENANCE)) else {
        return Vec::new();
    };
    let Ok(value) = toml::from_str::<toml::Value>(&raw) else {
        return Vec::new();
    };
    let Some(inputs) = value.get("inputs").and_then(toml::Value::as_array) else {
        return Vec::new();
    };
    inputs
        .iter()
        .filter_map(|input| {
            let id = input.get("id")?.as_str()?.to_owned();
            let upstream_path = input.get("upstream")?.as_str()?.to_owned();
            let git_blob = input.get("git_blob")?.as_str()?.to_owned();
            Some(ProvenanceInput {
                id,
                upstream_path,
                git_blob,
            })
        })
        .collect()
}

/// Look up the current git blob SHA of `upstream_path` at `branch` HEAD via the
/// GitHub contents API.
fn upstream_blob_sha(
    agent: &ureq::Agent,
    upstream_path: &str,
    branch: &str,
) -> Result<String, String> {
    let url = format!("{SVGWG_REPO_API}/contents/{upstream_path}?ref={branch}");
    let json = fetch(agent, &url)?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("parse contents {upstream_path}: {e}"))?;
    value
        .get("sha")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("contents response for {upstream_path} had no `sha` field"))
}

/// Compare every tracked input's vendored blob against its upstream blob at
/// `branch` HEAD.
///
/// Returns:
/// - `Some(drift)` — the comparison ran; `drift` lists inputs whose blob
///   actually changed (empty when none did, i.e. HEAD movement is
///   path-irrelevant);
/// - `None` — the comparison could not be performed at all (no provenance
///   inputs, or every per-file blob lookup failed), so the caller must not read
///   an empty list as "no drift".
fn check_tracked_inputs(agent: &ureq::Agent, branch: &str) -> Option<Vec<TrackedInputDrift>> {
    let inputs = read_tracked_inputs();
    if inputs.is_empty() {
        return None;
    }
    let mut drift = Vec::new();
    let mut any_resolved = false;
    for input in inputs {
        match upstream_blob_sha(agent, &input.upstream_path, branch) {
            Ok(head_blob) => {
                any_resolved = true;
                if head_blob != input.git_blob {
                    drift.push(TrackedInputDrift {
                        id: input.id,
                        upstream_path: input.upstream_path,
                        pinned_blob: input.git_blob,
                        head_blob,
                    });
                }
            }
            Err(error) => {
                eprintln!("spec-freshness: tracked-input blob lookup failed: {error}");
            }
        }
    }
    // If not a single blob resolved, we have no signal — report "unevaluated".
    any_resolved.then_some(drift)
}

/// Compare the baked rolling pin against live `svgwg` default-branch HEAD, and
/// (separately) the vendored tracked inputs against their upstream blobs.
fn check_rolling(agent: &ureq::Agent) -> Result<RollingReport, String> {
    let branch = svgwg_default_branch(agent)?;
    let json = fetch(agent, &format!("{SVGWG_REPO_API}/commits/{branch}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("parse svgwg HEAD: {e}"))?;
    let head = value
        .get("sha")
        .and_then(serde_json::Value::as_str)
        .ok_or("svgwg HEAD response had no `sha` field")?;

    let captured = CapturedEditionIdentity::Rolling {
        commit: ROLLING_PIN.commit,
    };
    let state = match classify_freshness(&captured, Some(head)) {
        Freshness::RollingCurrent | Freshness::Final { .. } => "current",
        Freshness::RollingStale { .. } => "stale",
    };

    // Only bother with the (per-file, N-request) tracked-input comparison when
    // bare HEAD has actually moved; if HEAD matches the pin, no blob can differ.
    let tracked_input_drift = if state == "stale" {
        check_tracked_inputs(agent, &branch)
    } else {
        Some(Vec::new())
    };

    Ok(RollingReport {
        repository: ROLLING_PIN.repository.to_string(),
        branch,
        pinned_commit: ROLLING_PIN.commit.to_string(),
        head_commit: head.to_string(),
        state,
        tracked_input_drift,
    })
}

/// Whether the rolling report denotes a real refresh trigger.
///
/// A moved HEAD alone is **not** drift: it counts only when a tracked input's
/// blob actually changed, or when the tracked-input comparison could not be
/// performed (so the path-relevant signal is unknown and we fail safe toward
/// "investigate").
fn rolling_is_stale(rolling: &RollingReport) -> bool {
    rolling
        .tracked_input_drift
        .as_ref()
        .map_or_else(|| rolling.state == "stale", |drift| !drift.is_empty())
}

/// Run both checks and assemble the verdict.
fn run(agent: &ureq::Agent) -> Result<FreshnessReport, String> {
    let published_drift = check_published(agent)?;
    let rolling = check_rolling(agent)?;
    let fresh = published_drift.is_empty() && !rolling_is_stale(&rolling);
    Ok(FreshnessReport {
        fresh,
        published_drift,
        rolling,
    })
}

/// Path (relative to the crate manifest) of the vendored compat data, whose
/// `sources.*.resolved` versions the compat-drift check compares against latest.
const COMPAT_DATA: &str = "data/sources/svg-compat-data.json";

/// Read a vendored compat dependency's package name + resolved version from the
/// `sources.<key>` block of `svg-compat-data.json`.
fn vendored_compat_source(
    value: &serde_json::Value,
    key: &str,
) -> Result<(String, String), String> {
    let source = value
        .get("sources")
        .and_then(|s| s.get(key))
        .ok_or_else(|| format!("{COMPAT_DATA} missing sources.{key}"))?;
    let package = source
        .get("package")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("sources.{key} missing `package`"))?;
    let resolved = source
        .get("resolved")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("sources.{key} missing `resolved`"))?;
    Ok((package.to_owned(), resolved.to_owned()))
}

/// The latest published version of an npm `package`, via the registry's
/// `/<package>/latest` dist-tag endpoint.
fn latest_npm_version(agent: &ureq::Agent, package: &str) -> Result<String, String> {
    let url = format!("https://registry.npmjs.org/{package}/latest");
    let json = fetch(agent, &url)?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("parse registry {package}: {e}"))?;
    value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("registry response for {package} had no `version` field"))
}

/// Compare each vendored compat dependency against the latest npm release.
fn check_compat_drift(agent: &ureq::Agent) -> Result<CompatDriftReport, String> {
    let raw = std::fs::read_to_string(manifest_path(COMPAT_DATA))
        .map_err(|e| format!("read {COMPAT_DATA}: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse {COMPAT_DATA}: {e}"))?;

    let mut drift = Vec::new();
    for key in ["bcd", "web_features"] {
        let (package, vendored) = vendored_compat_source(&value, key)?;
        let latest = latest_npm_version(agent, &package)?;
        if latest != vendored {
            drift.push(CompatDependencyDrift {
                package,
                vendored,
                latest,
            });
        }
    }
    Ok(CompatDriftReport {
        fresh: drift.is_empty(),
        drift,
    })
}

/// Render a human-readable summary to stdout.
fn print_human(report: &FreshnessReport) {
    if report.fresh {
        println!("✅ spec data is up to date");
    } else {
        println!("⚠️  spec data is STALE — a refresh is due");
    }

    println!("\nPublished editions (W3C API vs baked EDITION_INDEX):");
    if report.published_drift.is_empty() {
        println!("  · no new /TR/ publications");
    } else {
        for drift in &report.published_drift {
            println!(
                "  · NEW {:?} {} [{}] {}",
                drift.series, drift.date, drift.status, drift.uri
            );
        }
    }

    println!(
        "\nRolling editor's draft (svgwg {} HEAD vs baked pin):",
        report.rolling.branch
    );
    println!("  · pinned: {}", report.rolling.pinned_commit);
    println!("  · head:   {}", report.rolling.head_commit);
    println!(
        "  · HEAD movement: {}",
        if report.rolling.state == "stale" {
            "advanced past the pin"
        } else {
            "matches the pin"
        }
    );
    match &report.rolling.tracked_input_drift {
        None => println!("  · tracked inputs: UNEVALUATED (no provenance or all lookups failed)"),
        Some(drift) if drift.is_empty() => {
            println!("  · tracked inputs: unchanged (HEAD movement is path-irrelevant)");
        }
        Some(drift) => {
            println!(
                "  · tracked inputs: DRIFTED — {} input(s) changed upstream:",
                drift.len()
            );
            for entry in drift {
                println!(
                    "      - {} ({}): {} → {}",
                    entry.id, entry.upstream_path, entry.pinned_blob, entry.head_blob
                );
            }
        }
    }
}

/// Render the compat-drift summary to stdout.
fn print_compat_human(report: &CompatDriftReport) {
    if report.fresh {
        println!("✅ vendored compat data matches the latest npm releases");
    } else {
        println!("⚠️  vendored compat data is BEHIND the latest npm releases");
    }
    println!("\nCompat dependencies (svg-compat-data.json vs npm registry):");
    if report.drift.is_empty() {
        println!("  · all dependencies at latest");
    } else {
        for entry in &report.drift {
            println!(
                "  · {} vendored {} < latest {}",
                entry.package, entry.vendored, entry.latest
            );
        }
    }
}

/// Serialise a report to pretty JSON on stdout, mapping errors to exit code 2.
fn emit_json<T: Serialize>(report: &T) -> Result<(), ExitCode> {
    match serde_json::to_string_pretty(report) {
        Ok(json) => {
            println!("{json}");
            Ok(())
        }
        Err(error) => {
            eprintln!("spec-freshness: serialise report: {error}");
            Err(ExitCode::from(2))
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let json_mode = args.iter().any(|arg| arg == "--json");
    let compat_mode = args.iter().any(|arg| arg == "--compat-drift");

    let agent = agent();

    if compat_mode {
        let report = match check_compat_drift(&agent) {
            Ok(report) => report,
            Err(error) => {
                eprintln!("spec-freshness: {error}");
                return ExitCode::from(2);
            }
        };
        if json_mode {
            if let Err(code) = emit_json(&report) {
                return code;
            }
        } else {
            print_compat_human(&report);
        }
        return if report.fresh {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    }

    let report = match run(&agent) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("spec-freshness: {error}");
            return ExitCode::from(2);
        }
    };

    if json_mode {
        if let Err(code) = emit_json(&report) {
            return code;
        }
    } else {
        print_human(&report);
    }

    if report.fresh {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
