//! CI sentinel for baked SVG spec and compat-source freshness.

use std::{process::ExitCode, time::Duration};

use serde_json::{Value, json};
use svg_data::edition::{ROLLING_PIN, Series, VersionsEnvelope, unseen_versions};
use ureq::config::IpFamily;

const SVGWG_REPO_API: &str = "https://api.github.com/repos/w3c/svgwg";
const TRACKED_SPEC_PATH_PREFIX: &str = "master/";
const USER_AGENT: &str = "svg-data-spec-freshness (+https://github.com/kjanat/svg)";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const BODY_LIMIT: u64 = 64 * 1024 * 1024;
const COMPAT_CATALOG: &str = include_str!("../../data/catalog.compat.json");

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("spec-freshness: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Fallible<ExitCode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json_output = args.iter().any(|arg| arg == "--json");
    let compat_drift = args.iter().any(|arg| arg == "--compat-drift");
    if args
        .iter()
        .any(|arg| !matches!(arg.as_str(), "--json" | "--compat-drift"))
    {
        return Err(boxed("usage: spec-freshness [--json] [--compat-drift]"));
    }

    if compat_drift {
        let report = compat_report()?;
        print_report(json_output, &report);
        return Ok(exit_for_drift(
            report["drift"]
                .as_array()
                .is_some_and(|items| !items.is_empty()),
        ));
    }

    let report = spec_report()?;
    print_report(json_output, &report);
    let published_drift = report["published_drift"]
        .as_array()
        .is_some_and(|items| !items.is_empty());
    let rolling_drift = report
        .pointer("/rolling/tracked_input_drift")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty());
    Ok(exit_for_drift(published_drift || rolling_drift))
}

fn print_report(json_output: bool, report: &Value) {
    if json_output {
        println!("{report}");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(report).unwrap_or_else(|_| report.to_string())
        );
    }
}

fn exit_for_drift(drift: bool) -> ExitCode {
    if drift {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn spec_report() -> Fallible<Value> {
    let agent = agent();
    let head = svgwg_head(&agent)?;
    let tracked_input_drift = if ROLLING_PIN.commit.is_empty() || ROLLING_PIN.commit == head {
        Vec::new()
    } else {
        changed_files(&agent, ROLLING_PIN.commit, &head)?
            .into_iter()
            .filter(|file| file.starts_with(TRACKED_SPEC_PATH_PREFIX))
            .collect()
    };

    let mut published_drift = Vec::new();
    let mut published_errors = Vec::new();
    for series in Series::ALL {
        let live = match w3c_versions(&agent, series) {
            Ok(live) => live,
            Err(error) => {
                published_errors.push(json!({
                    "series": series.shortname(),
                    "error": error.to_string(),
                }));
                continue;
            }
        };
        for version in unseen_versions(series, &live) {
            published_drift.push(json!({
                "series": series.shortname(),
                "uri": version.uri,
            }));
        }
    }

    Ok(json!({
        "published_errors": published_errors,
        "published_drift": published_drift,
        "rolling": {
            "head_commit": head,
            "pinned_commit": ROLLING_PIN.commit,
            "tracked_input_drift": tracked_input_drift,
        },
    }))
}

fn compat_report() -> Fallible<Value> {
    let agent = agent();
    let catalog: Value = serde_json::from_str(COMPAT_CATALOG)?;
    let packages = [
        (
            "@mdn/browser-compat-data",
            catalog.pointer("/browser_compat_data/version"),
        ),
        ("web-features", catalog.pointer("/web_features/version")),
    ];
    let mut drift = Vec::new();
    for (name, baked) in packages {
        let baked = baked
            .and_then(Value::as_str)
            .ok_or_else(|| boxed(format!("compat catalog missing version for `{name}`")))?;
        let latest = npm_latest_version(&agent, name)?;
        if latest != baked {
            drift.push(json!({
                "name": name,
                "baked": baked,
                "latest": latest,
            }));
        }
    }

    Ok(json!({ "drift": drift }))
}

fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .ip_family(IpFamily::Ipv4Only)
        .timeout_global(Some(REQUEST_TIMEOUT))
        .timeout_per_call(Some(REQUEST_TIMEOUT))
        .build();
    config.into()
}

fn svgwg_head(agent: &ureq::Agent) -> Fallible<String> {
    let repo: Value =
        serde_json::from_str(&fetch_text(agent, SVGWG_REPO_API, "application/json")?)?;
    let branch = repo
        .get("default_branch")
        .and_then(Value::as_str)
        .ok_or_else(|| boxed("GitHub repository response missing `default_branch`"))?;
    let commit: Value = serde_json::from_str(&fetch_text(
        agent,
        &format!("{SVGWG_REPO_API}/commits/{branch}"),
        "application/json",
    )?)?;
    let head = commit
        .get("sha")
        .and_then(Value::as_str)
        .ok_or_else(|| boxed("GitHub commit response missing `sha`"))?;
    Ok(head.to_owned())
}

fn changed_files(agent: &ureq::Agent, base: &str, head: &str) -> Fallible<Vec<String>> {
    let compare: Value = serde_json::from_str(&fetch_text(
        agent,
        &format!("{SVGWG_REPO_API}/compare/{base}...{head}"),
        "application/json",
    )?)?;
    let files = compare
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| boxed("GitHub compare response missing `files`"))?
        .iter()
        .filter_map(|file| file.get("filename").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    Ok(files)
}

fn w3c_versions(agent: &ureq::Agent, series: Series) -> Fallible<VersionsEnvelope> {
    let url = format!(
        "https://api.w3.org/specifications/{}/versions?embed=1&items=100",
        series.shortname()
    );
    VersionsEnvelope::parse(series, &fetch_text(agent, &url, "application/json")?)
        .map_err(Box::<dyn std::error::Error>::from)
}

fn npm_latest_version(agent: &ureq::Agent, package: &str) -> Fallible<String> {
    let package = package.replace('/', "%2F");
    let url = format!("https://registry.npmjs.org/{package}/latest");
    let json: Value = serde_json::from_str(&fetch_text(agent, &url, "application/json")?)?;
    let version = json
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| boxed("npm response missing `version`"))?;
    Ok(version.to_owned())
}

fn fetch_text(agent: &ureq::Agent, url: &str, accept: &str) -> Fallible<String> {
    let mut request = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", accept);
    if url.starts_with("https://api.github.com/")
        && let Ok(token) = std::env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        request = request.header("Authorization", &format!("Bearer {token}"));
    }
    let mut response = request.call()?;
    Ok(response
        .body_mut()
        .with_config()
        .limit(BODY_LIMIT)
        .read_to_string()?)
}

fn boxed(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::<dyn std::error::Error>::from(message.into())
}
