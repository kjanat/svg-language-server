//! Typed W3C edition index and freshness primitives.
//!
//! The [W3C specification API] publishes *bibliographic* metadata for every
//! `/TR/` version of a spec: its publication `date`, maturity `status`, the
//! dated `uri`, whether it is on the Recommendation track, the rolling
//! editor's-draft URL, and the `shortlink` (the undated `/TR/SVG2/` pointer).
//! It carries **no** technical content — no elements, attributes, properties,
//! or value grammars. This module models that metadata as ADTs and bakes an
//! [`EDITION_INDEX`] from the vendored API responses at build time, so the
//! crate stays hermetic (no network at build time).
//!
//! The actual live freshness check (hitting `api.w3.org` or the `svgwg` git
//! repo) is the LSP runtime's job. Here we provide the *index* plus the *pure*
//! comparison logic — [`classify_freshness`] — that the runtime layers a
//! network fetch on top of.
//!
//! [W3C specification API]: https://w3c.github.io/w3c-api/

use std::borrow::Cow;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::types::SpecSnapshotId;

/// One of the three SVG specification *series* tracked by the W3C API.
///
/// Each series has its own version history under a distinct API shortname.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Series {
    /// SVG 1.0 — API shortname `SVG`.
    Svg10,
    /// SVG 1.1 (First + Second Edition) — API shortname `SVG11`.
    Svg11,
    /// SVG 2 — API shortname `SVG2`.
    Svg2,
}

impl Series {
    /// Every tracked series, in chronological order of first publication.
    pub const ALL: [Self; 3] = [Self::Svg10, Self::Svg11, Self::Svg2];

    /// The W3C API shortname used to address this series' version history.
    #[must_use]
    pub const fn shortname(self) -> &'static str {
        match self {
            Self::Svg10 => "SVG",
            Self::Svg11 => "SVG11",
            Self::Svg2 => "SVG2",
        }
    }
}

/// Publication maturity status, mapped from the W3C API `status` strings.
///
/// The API reports the *Process 2020* document-status vocabulary. Each variant
/// renames to its exact API spelling so a [`PublishedVersion`] round-trips
/// through `serde` without a stringly-typed intermediate.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum Status {
    /// `Working Draft` — early, unstable.
    #[serde(rename = "Working Draft")]
    WorkingDraft,
    /// `Last Call Working Draft` — WD frozen for last-call review (legacy
    /// Process term; still present in the SVG 1.1 history).
    #[serde(rename = "Last Call Working Draft")]
    LastCallWorkingDraft,
    /// `Candidate Recommendation Snapshot` — dated, stable, seeking
    /// implementation feedback.
    #[serde(rename = "Candidate Recommendation Snapshot")]
    CandidateRecommendation,
    /// `Proposed Recommendation` — submitted for Advisory Committee review.
    #[serde(rename = "Proposed Recommendation")]
    ProposedRecommendation,
    /// `Recommendation` — final, ratified.
    Recommendation,
}

impl Status {
    /// Whether this status denotes a *frozen*, dated edition.
    ///
    /// REC, PR, and CR-Snapshot are immutable dated publications. WD and
    /// Last-Call WD are likewise dated `/TR/` snapshots, so they too are
    /// frozen *publications* — only the rolling editor's draft is unfrozen,
    /// and that has no published status at all. Every published version in the
    /// index is therefore frozen; this method exists so callers can reason
    /// about maturity without re-deriving the rule.
    #[must_use]
    pub const fn is_frozen(self) -> bool {
        // All published `/TR/` statuses are dated, immutable artifacts.
        match self {
            Self::WorkingDraft
            | Self::LastCallWorkingDraft
            | Self::CandidateRecommendation
            | Self::ProposedRecommendation
            | Self::Recommendation => true,
        }
    }

    /// Maturity ordering rank, ascending toward Recommendation.
    ///
    /// Used to break ties when two published versions share a date (they do
    /// not in practice, but the rule keeps `latest_published` total).
    #[must_use]
    pub const fn maturity_rank(self) -> u8 {
        match self {
            Self::WorkingDraft => 0,
            Self::LastCallWorkingDraft => 1,
            Self::CandidateRecommendation => 2,
            Self::ProposedRecommendation => 3,
            Self::Recommendation => 4,
        }
    }
}

/// A single published `/TR/` version, distilled from the W3C API
/// `version-history` records.
///
/// Owns its strings via [`Cow`] so the same type serves two roles: `serde`
/// deserialization from the vendored JSON (yielding owned data) and the baked
/// [`EDITION_INDEX`] table (borrowing `'static` string literals).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PublishedVersion {
    /// Series this version belongs to. Filled in from the file's shortname at
    /// parse time (the per-record API field is `null`), so it is `serde`
    /// `default` to tolerate the field's absence in raw API JSON.
    #[serde(default = "default_series")]
    pub series: Series,
    /// ISO-8601 publication date (`YYYY-MM-DD`).
    pub date: Cow<'static, str>,
    /// Publication maturity status.
    pub status: Status,
    /// The dated `/TR/` URL — the canonical citation for this version.
    pub uri: Cow<'static, str>,
    /// Whether this version is on the Recommendation track.
    #[serde(rename = "rec-track")]
    pub rec_track: bool,
    /// Rolling editor's-draft URL, when the series has one (SVG 2 only).
    #[serde(
        rename = "editor-draft",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub editor_draft: Option<Cow<'static, str>>,
    /// The undated `/TR/<series>/` pointer to the latest published version.
    pub shortlink: Cow<'static, str>,
}

const fn default_series() -> Series {
    Series::Svg2
}

/// HAL pagination envelope returned by the W3C API for a versions listing.
///
/// Only the inlined `_embedded.version-history` array is meaningful for the
/// index; pagination/link fields are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VersionsEnvelope {
    /// Inlined resources (present when the API is queried with `?embed=1`).
    #[serde(rename = "_embedded")]
    pub embedded: EmbeddedVersions,
}

/// The `_embedded` block of a [`VersionsEnvelope`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EmbeddedVersions {
    /// Every published version of the series, newest first.
    #[serde(rename = "version-history")]
    pub version_history: Vec<PublishedVersion>,
}

impl VersionsEnvelope {
    /// Parse a vendored W3C API versions response, stamping each record with
    /// its `series` (the per-record API `shortname` is `null`).
    ///
    /// # Errors
    ///
    /// Returns the underlying `serde_json` error if `json` is not a valid
    /// versions envelope.
    pub fn parse(series: Series, json: &str) -> Result<Vec<PublishedVersion>, serde_json::Error> {
        let mut envelope: Self = serde_json::from_str(json)?;
        for version in &mut envelope.embedded.version_history {
            version.series = series;
        }
        Ok(envelope.embedded.version_history)
    }
}

/// The set of SVG versions published across all three series, baked from the
/// vendored W3C API JSON at build time.
///
/// Ordering matches the vendored files: per series, newest publication first.
/// The backing array (`EDITION_INDEX_ENTRIES`) is generated by
/// `build/edition.rs` from the vendored API metadata.
pub static EDITION_INDEX: &[PublishedVersion] = EDITION_INDEX_ENTRIES;

include!(concat!(env!("OUT_DIR"), "/edition_index.rs"));

/// Git provenance pin for the rolling SVG 2 editor's-draft capture.
///
/// The editor's draft has no dated `/TR/` URL — it tracks `svgwg` git `master`.
/// This is the exact commit the baked SVG2-ED inventory was derived from, baked
/// from `data/specs/Svg2EditorsDraft/snapshot.json` at build time so the
/// runtime freshness check can compare it against live `svgwg` HEAD without the
/// data directory present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollingPin {
    /// The upstream git repository the draft is tracked from.
    pub repository: &'static str,
    /// The pinned `master` commit the baked data was derived from.
    pub commit: &'static str,
    /// The capture date recorded in the snapshot (`YYYY-MM-DD`).
    pub captured_date: &'static str,
}

/// The `svgwg` git pin the baked SVG 2 editor's-draft data was derived from.
///
/// Generated alongside [`EDITION_INDEX`] from the editor's-draft snapshot's
/// `pinned_sources[].pin` block.
pub static ROLLING_PIN: RollingPin = RollingPin {
    repository: ROLLING_PIN_REPOSITORY,
    commit: ROLLING_PIN_COMMIT,
    captured_date: ROLLING_PIN_CAPTURED_DATE,
};

/// Every published version of `series`, newest publication first.
#[must_use]
pub fn published_versions(series: Series) -> Vec<&'static PublishedVersion> {
    EDITION_INDEX
        .iter()
        .filter(move |version| version.series == series)
        .collect()
}

/// The most recent published version of `series`.
///
/// "Most recent" is the latest publication `date`, with [`Status::maturity_rank`]
/// breaking any same-date tie in favour of the more mature status. Returns
/// `None` only if the index has no entry for `series` (never the case for the
/// three SVG series).
#[must_use]
pub fn latest_published(series: Series) -> Option<&'static PublishedVersion> {
    EDITION_INDEX
        .iter()
        .filter(|version| version.series == series)
        .max_by(|a, b| {
            a.date
                .cmp(&b.date)
                .then_with(|| a.status.maturity_rank().cmp(&b.status.maturity_rank()))
        })
}

/// Live versions of `series` that are **not** in the baked [`EDITION_INDEX`].
///
/// This is the pure half of the *published-edition* freshness check: feed it the
/// `version-history` the W3C API returns *now* and it yields every entry whose
/// dated `/TR/` `uri` the crate has not yet vendored. A non-empty result means
/// W3C published something the baked catalog has not caught up to.
///
/// Matching is by `uri` — the dated `/TR/` URL is the stable identity of a
/// publication. Returns owned clones so callers can render them without holding
/// a borrow on `live`.
#[must_use]
pub fn unseen_versions(series: Series, live: &[PublishedVersion]) -> Vec<PublishedVersion> {
    live.iter()
        .filter(|candidate| {
            candidate.series == series
                && !EDITION_INDEX
                    .iter()
                    .any(|known| known.series == series && known.uri == candidate.uri)
        })
        .cloned()
        .collect()
}

/// Freshness classification of a captured edition against the live index.
///
/// This is the *pure* half of the freshness check. The network half — asking
/// `api.w3.org` whether a newer `/TR/` version exists, or comparing the rolling
/// editor's draft against `svgwg` git HEAD — belongs to the LSP runtime, which
/// supplies the comparison inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Freshness {
    /// The captured edition is a frozen, dated publication (REC/PR/CR/WD). It
    /// can never go stale: it *is* the historical record.
    Final {
        /// The matched index entry's dated `/TR/` URI.
        uri: Cow<'static, str>,
    },
    /// The captured edition is the rolling editor's draft and matches the
    /// latest known reference (commit/date). Up to date as of the comparison.
    RollingCurrent,
    /// The captured rolling editor's draft is behind the latest known
    /// reference — a newer draft exists upstream.
    RollingStale {
        /// The reference the capture was compared against (e.g. git HEAD).
        latest: Cow<'static, str>,
    },
}

/// Identity of a captured edition, as recorded in a snapshot's `snapshot.json`.
///
/// Frozen editions are pinned by their dated `/TR/` URL; the rolling editor's
/// draft is pinned by a git commit (and capture date) instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapturedEditionIdentity<'a> {
    /// A dated `/TR/` publication, identified by its `uri`.
    Dated {
        /// The dated `/TR/` URL the snapshot pinned.
        uri: &'a str,
    },
    /// The rolling editor's draft, identified by the captured git commit.
    Rolling {
        /// The git commit the snapshot captured.
        commit: &'a str,
    },
}

/// Classify a captured edition against the index and (for the rolling draft)
/// the latest known upstream reference.
///
/// Pure: no network access. The caller obtains `latest_rolling_ref` (e.g.
/// `svgwg` git HEAD) however it likes — offline it may pass `None`, in which
/// case a rolling capture is reported [`Freshness::RollingCurrent`] (no newer
/// reference is known).
///
/// A [`CapturedEditionIdentity::Dated`] capture is [`Freshness::Final`] when its `uri`
/// matches an index entry; an unmatched dated URI also classifies as `Final`
/// (it is still a frozen `/TR/` artifact, just one outside the tracked index).
#[must_use]
pub fn classify_freshness(
    captured: &CapturedEditionIdentity<'_>,
    latest_rolling_ref: Option<&str>,
) -> Freshness {
    match captured {
        CapturedEditionIdentity::Dated { uri } => {
            let matched = EDITION_INDEX.iter().find(|version| version.uri == *uri);
            let uri = matched.map_or_else(
                || Cow::Owned((*uri).to_string()),
                |version| version.uri.clone(),
            );
            Freshness::Final { uri }
        }
        CapturedEditionIdentity::Rolling { commit } => match latest_rolling_ref {
            None => Freshness::RollingCurrent,
            Some(latest) if latest == *commit => Freshness::RollingCurrent,
            Some(latest) => Freshness::RollingStale {
                latest: Cow::Owned(latest.to_string()),
            },
        },
    }
}

/// The index entry a [`SpecSnapshotId`] was captured from, when one exists.
///
/// Frozen snapshots map 1:1 to a dated `/TR/` version in [`EDITION_INDEX`].
/// The rolling [`SpecSnapshotId::Svg2EditorsDraft`] has no `/TR/` entry — it
/// tracks the editor's draft + `svgwg` git — so it returns `None`.
#[must_use]
pub fn index_entry_for_snapshot(snapshot: SpecSnapshotId) -> Option<&'static PublishedVersion> {
    let uri = match snapshot {
        SpecSnapshotId::Svg11Rec20030114 => "https://www.w3.org/TR/2003/REC-SVG11-20030114/",
        SpecSnapshotId::Svg11Rec20110816 => "https://www.w3.org/TR/2011/REC-SVG11-20110816/",
        SpecSnapshotId::Svg2Cr20181004 => "https://www.w3.org/TR/2018/CR-SVG2-20181004/",
        SpecSnapshotId::Svg2EditorsDraft => return None,
    };
    EDITION_INDEX.iter().find(|version| version.uri == uri)
}
