//! Canonical-upstream fetching.
//!
//! Resolves the repository's default branch (or an explicit ref) on GitHub and
//! pulls raw files at that exact commit. The local, gitignored `svgwg/` clone
//! is never read - the only source is canonical upstream over the network, so a
//! regeneration is reproducible from nothing but the repo slug and a ref.

use std::time::Duration;

use serde_json::Value;
use ureq::config::IpFamily;

use crate::util::boxed;

/// User agent GitHub requires on API requests.
const USER_AGENT: &str = "svg-data-regen (+https://github.com/kjanat/svg)";
/// Maximum body size accepted from a single fetch. Generous: the largest spec
/// pages are a few megabytes; this only guards against a runaway response.
const BODY_LIMIT: u64 = 64 * 1024 * 1024;
/// Bound each upstream request so one slow spec host cannot hang regeneration.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// A resolved commit: its SHA and committer date.
///
/// The committer date is what the SVG publication tooling uses as the
/// document's date while a spec is an Editor's Draft (no explicit
/// `publication-date`), so it is the deterministic, wall-clock-free date the
/// catalog stamps editions with.
pub struct Head {
    /// The full 40-hex commit SHA.
    pub sha: String,
    /// The committer date, ISO-8601 (e.g. `2026-06-04T12:00:00Z`).
    pub committed_date: String,
}

/// Fetch the body of `url` as text, sending the headers GitHub expects.
fn get_text(url: &str, accept: &str) -> Fallible<String> {
    let config = ureq::Agent::config_builder()
        .ip_family(IpFamily::Ipv4Only)
        .timeout_global(Some(REQUEST_TIMEOUT))
        .timeout_per_call(Some(REQUEST_TIMEOUT))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", accept)
        .call()?;
    let body = response
        .body_mut()
        .with_config()
        .limit(BODY_LIMIT)
        .read_to_string()?;
    Ok(body)
}

/// Fetch an arbitrary URL as text.
pub fn url_text(url: &str, accept: &str) -> Fallible<String> {
    get_text(url, accept)
}

/// The repository's default branch name, resolved via the API.
///
/// Never hardcode `main`/`master`: the default branch is authoritative and has
/// changed before, so it is always resolved fresh.
pub fn default_branch(slug: &str) -> Fallible<String> {
    let url = format!("https://api.github.com/repos/{slug}");
    let json: Value = serde_json::from_str(&get_text(&url, "application/vnd.github+json")?)?;
    let branch = json
        .get("default_branch")
        .and_then(Value::as_str)
        .ok_or_else(|| boxed("GitHub repository response missing `default_branch`"))?;
    Ok(branch.to_owned())
}

/// Resolve a ref (branch name or commit SHA) to its commit SHA and date.
pub fn resolve_head(slug: &str, reference: &str) -> Fallible<Head> {
    let url = format!("https://api.github.com/repos/{slug}/commits/{reference}");
    let json: Value = serde_json::from_str(&get_text(&url, "application/vnd.github+json")?)?;
    let sha = json
        .get("sha")
        .and_then(Value::as_str)
        .ok_or_else(|| boxed("commit response missing `sha`"))?;
    let committed_date = json
        .pointer("/commit/committer/date")
        .and_then(Value::as_str)
        .ok_or_else(|| boxed("commit response missing `commit.committer.date`"))?;
    Ok(Head {
        sha: sha.to_owned(),
        committed_date: committed_date.to_owned(),
    })
}

/// Fetch a raw file from the repository at an exact commit.
pub fn raw_file(slug: &str, sha: &str, path: &str) -> Fallible<String> {
    let url = format!("https://raw.githubusercontent.com/{slug}/{sha}/{path}");
    get_text(&url, "text/plain")
}

/// Resolve an href relative to a repository directory, collapsing `.` and `..`
/// segments so module hrefs like `../specs/animations/master/definitions.xml`
/// resolve to a real repo path.
pub fn resolve_repo_path(base_dir: &str, href: &str) -> Fallible<String> {
    let mut segments: Vec<&str> = base_dir.split('/').filter(|seg| !seg.is_empty()).collect();
    for part in href.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(boxed(format!(
                        "href `{href}` escapes repository root from `{base_dir}`"
                    )));
                }
            }
            other => segments.push(other),
        }
    }
    Ok(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_repo_paths_without_leaving_the_repo() -> Fallible<()> {
        assert_eq!(
            resolve_repo_path("master", "../specs/animations/master/definitions.xml")?,
            "specs/animations/master/definitions.xml"
        );
        Ok(())
    }

    #[test]
    fn rejects_repo_path_underflow() {
        assert!(resolve_repo_path("", "../outside.xml").is_err());
    }
}
