//! Build-time generation of the W3C edition index.
//!
//! Parses the **vendored** W3C specification-API version histories
//! (`data/sources/w3c-api/*.versions.json`) and emits a generated
//! `EDITION_INDEX_ENTRIES` static that backs the baked
//! [`crate::edition::EDITION_INDEX`].
//!
//! Hermetic by construction: the only inputs are checked-in JSON files. The
//! live W3C API is never contacted at build time — that belongs to the LSP
//! runtime's freshness check.

use std::{fmt::Write as _, path::Path};

use serde::Deserialize;

/// Vendored API file paired with the series it describes.
///
/// `series_variant` is the `crate::edition::Series` variant name emitted into
/// generated code; `shortname` is only used for diagnostics.
struct SeriesSource {
    series_variant: &'static str,
    shortname: &'static str,
    file: &'static str,
}

const SERIES_SOURCES: &[SeriesSource] = &[
    SeriesSource {
        series_variant: "Svg10",
        shortname: "SVG",
        file: "data/sources/w3c-api/svg.versions.json",
    },
    SeriesSource {
        series_variant: "Svg11",
        shortname: "SVG11",
        file: "data/sources/w3c-api/svg11.versions.json",
    },
    SeriesSource {
        series_variant: "Svg2",
        shortname: "SVG2",
        file: "data/sources/w3c-api/svg2.versions.json",
    },
];

/// HAL pagination envelope (only `_embedded` is read).
#[derive(Deserialize)]
struct RawEnvelope {
    #[serde(rename = "_embedded")]
    embedded: RawEmbedded,
}

#[derive(Deserialize)]
struct RawEmbedded {
    #[serde(rename = "version-history")]
    version_history: Vec<RawVersion>,
}

/// One raw `version-history[]` record as published by the W3C API.
#[derive(Deserialize)]
struct RawVersion {
    date: String,
    status: String,
    uri: String,
    #[serde(rename = "rec-track")]
    rec_track: bool,
    #[serde(rename = "editor-draft")]
    editor_draft: Option<String>,
    shortlink: String,
}

/// Minimal view of an editor's-draft `snapshot.json`, read only to recover the
/// rolling git pin baked into `ROLLING_PIN`.
#[derive(Deserialize)]
struct SnapshotPinFile {
    date: String,
    pinned_sources: Vec<SnapshotPinnedSource>,
}

#[derive(Deserialize)]
struct SnapshotPinnedSource {
    pin: SnapshotPin,
}

/// A `pinned_sources[].pin` entry. Only `git_commit` pins carry the fields the
/// rolling freshness check needs; other pin kinds are skipped.
#[derive(Deserialize)]
struct SnapshotPin {
    kind: String,
    repository: Option<String>,
    commit: Option<String>,
}

/// Emit the `ROLLING_PIN_*` string consts that back `crate::edition::ROLLING_PIN`.
///
/// Reads the editor's-draft snapshot, takes the first `git_commit` pin (all
/// inputs in that snapshot share one commit), and bakes its repository/commit
/// plus the snapshot capture date.
///
/// # Errors
///
/// Returns an error if the snapshot is missing/unreadable, is not valid JSON, or
/// carries no `git_commit` pin with both `repository` and `commit`.
fn generate_rolling_pin(manifest_dir: &Path) -> Result<String, String> {
    let path = manifest_dir.join("data/specs/Svg2EditorsDraft/snapshot.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("edition: read {}: {e}", path.display()))?;
    let snapshot: SnapshotPinFile = serde_json::from_str(&raw)
        .map_err(|e| format!("edition: parse {}: {e}", path.display()))?;

    let pin = snapshot
        .pinned_sources
        .iter()
        .map(|source| &source.pin)
        .find(|pin| pin.kind == "git_commit")
        .ok_or_else(|| format!("edition: {} has no git_commit pin", path.display()))?;
    let repository = pin.repository.as_deref().ok_or_else(|| {
        format!(
            "edition: {} git_commit pin lacks repository",
            path.display()
        )
    })?;
    let commit = pin
        .commit
        .as_deref()
        .ok_or_else(|| format!("edition: {} git_commit pin lacks commit", path.display()))?;

    let mut body = String::with_capacity(512);
    writeln!(
        body,
        "/// svgwg repository the baked editor's-draft data was derived from.\n\
         static ROLLING_PIN_REPOSITORY: &str = \"{}\";\n\
         /// svgwg `master` commit the baked editor's-draft data was derived from.\n\
         static ROLLING_PIN_COMMIT: &str = \"{}\";\n\
         /// Capture date recorded in the editor's-draft snapshot.\n\
         static ROLLING_PIN_CAPTURED_DATE: &str = \"{}\";",
        escape(repository),
        escape(commit),
        escape(&snapshot.date),
    )
    .map_err(|e| format!("edition: format rolling pin: {e}"))?;
    Ok(body)
}

/// Map an API `status` string to a `crate::edition::Status` variant name.
///
/// Returns `Err` for an unrecognised status so a future API vocabulary change
/// fails the build loudly instead of silently dropping a version.
fn status_variant(status: &str) -> Result<&'static str, String> {
    match status {
        "Working Draft" => Ok("WorkingDraft"),
        "Last Call Working Draft" => Ok("LastCallWorkingDraft"),
        "Candidate Recommendation Snapshot" => Ok("CandidateRecommendation"),
        "Proposed Recommendation" => Ok("ProposedRecommendation"),
        "Recommendation" => Ok("Recommendation"),
        other => Err(format!("unknown W3C API status: {other:?}")),
    }
}

fn escape(s: &str) -> String {
    s.chars().flat_map(char::escape_default).collect()
}

fn cow_lit(value: &str) -> String {
    format!("std::borrow::Cow::Borrowed(\"{}\")", escape(value))
}

/// Read, parse, and emit the generated `EDITION_INDEX_ENTRIES` source.
///
/// # Errors
///
/// Returns an error if any vendored file is missing/unreadable, is not valid
/// JSON, or carries an unrecognised status string.
pub fn generate(manifest_dir: &Path) -> Result<String, String> {
    let mut body = String::with_capacity(8 * 1024);
    body.push_str("// @generated by build/edition.rs -- do not edit\n\n");
    body.push_str("/// Baked W3C edition index entries, parsed from vendored API metadata.\n");
    body.push_str("static EDITION_INDEX_ENTRIES: &[PublishedVersion] = &[\n");

    for source in SERIES_SOURCES {
        let path = manifest_dir.join(source.file);
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| format!("edition: read {}: {e}", path.display()))?;
        let envelope: RawEnvelope = serde_json::from_str(&raw).map_err(|e| {
            format!(
                "edition: parse {} ({}): {e}",
                path.display(),
                source.shortname
            )
        })?;

        for version in &envelope.embedded.version_history {
            let status = status_variant(&version.status)
                .map_err(|e| format!("edition: {} {}: {e}", source.shortname, version.uri))?;
            let editor_draft = version.editor_draft.as_deref().map_or_else(
                || "None".to_string(),
                |url| format!("Some({})", cow_lit(url)),
            );
            writeln!(
                body,
                concat!(
                    "        PublishedVersion {{\n",
                    "            series: Series::{},\n",
                    "            date: {},\n",
                    "            status: Status::{},\n",
                    "            uri: {},\n",
                    "            rec_track: {},\n",
                    "            editor_draft: {},\n",
                    "            shortlink: {},\n",
                    "        }},",
                ),
                source.series_variant,
                cow_lit(&version.date),
                status,
                cow_lit(&version.uri),
                version.rec_track,
                editor_draft,
                cow_lit(&version.shortlink),
            )
            .map_err(|e| format!("edition: format: {e}"))?;
        }
    }

    body.push_str("];\n\n");
    body.push_str(&generate_rolling_pin(manifest_dir)?);
    body.push('\n');
    Ok(body)
}
