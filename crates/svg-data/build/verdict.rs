//! Build-time computation of [`CompatVerdict`] values for the static catalog.
//!
//! This module runs inside `build.rs` (included via `#[path]`) and takes the
//! build-time shadow types (`CompatEntry`, `BrowserSupportValue`, etc.) plus
//! spec-membership facts, and emits a fully-reconciled verdict per entry per
//! snapshot. The resulting verdicts are baked into `catalog.rs` as static
//! `&[(SpecSnapshotId, CompatVerdict)]` slices; the runtime never recomputes.
//!
//! The verdict layer is the **single source of truth** for both hover
//! markdown and lint diagnostics — both read from `AttributeDef.verdicts` /
//! `ElementDef.verdicts` and render against the same reasons.
//!
//! Reason priority (highest tier wins):
//!
//! ```text
//! Safe < Caution < Avoid < Forbid
//! ```
//!
//! Multiple reasons at the same tier coexist; renderers surface them all.

use std::fmt::Write as _;

use super::{
    BaselineValue, BrowserSupportValue, BrowserVersionValue, CompatEntry, types::SpecLifecycle,
};

const BROWSERS: [&str; 4] = ["chrome", "edge", "firefox", "safari"];

/// The four recommendation tiers, in ascending severity.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Safe,
    Caution,
    Avoid,
    Forbid,
}

/// A single collected reason, mirroring `svg_data::VerdictReason`.
///
/// Owned at build time so the collection loop can extend a `Vec`; converted
/// to emitted static-slice syntax by [`format_reasons`].
#[derive(Clone)]
pub enum Reason {
    BcdDeprecated,
    BcdExperimental,
    ProfileObsolete {
        last_seen: String,
    },
    ProfileExperimental,
    BaselineLimited,
    BaselineNewly {
        since: u16,
    },
    PartialImplementationIn(&'static str),
    PrefixRequiredIn {
        browser: &'static str,
        prefix: String,
    },
    BehindFlagIn(&'static str),
    UnsupportedIn(&'static str),
    RemovedIn {
        browser: &'static str,
        version: String,
    },
}

impl Reason {
    const fn tier(&self) -> Tier {
        match self {
            Self::ProfileObsolete { .. } => Tier::Forbid,
            Self::BcdDeprecated | Self::BcdExperimental | Self::ProfileExperimental => Tier::Avoid,
            Self::BaselineLimited
            | Self::BaselineNewly { .. }
            | Self::PartialImplementationIn(_)
            | Self::PrefixRequiredIn { .. }
            | Self::BehindFlagIn(_)
            | Self::UnsupportedIn(_)
            | Self::RemovedIn { .. } => Tier::Caution,
        }
    }
}

/// A computed verdict for a single catalog entry × snapshot pair.
pub struct Verdict {
    pub recommendation: Tier,
    pub headline_template: &'static str,
    pub reasons: Vec<Reason>,
}

/// Extra spec-derived facts the verdict rules need but which live outside
/// `CompatEntry`. Populated by the caller from the union membership data.
pub struct SpecFacts {
    pub lifecycle: SpecLifecycle,
    /// When `lifecycle == Obsolete`, the most recent snapshot in which the
    /// feature was still defined. Used for `ProfileObsolete.last_seen`.
    pub last_seen: Option<String>,
}

/// Compute the verdict for a single entry × profile.
///
/// Input is already projected down to the build-time shadow types; the
/// function is pure and allocation-light (one `Vec` for reasons).
pub fn compute(compat: Option<&CompatEntry>, spec: SpecFacts) -> Verdict {
    let mut reasons: Vec<Reason> = Vec::new();

    // Rule 1: profile-obsolete (highest priority).
    if spec.lifecycle == SpecLifecycle::Obsolete {
        let last_seen = spec
            .last_seen
            .unwrap_or_else(|| "Svg11Rec20110816".to_string());
        reasons.push(Reason::ProfileObsolete { last_seen });
    }

    // Rule 2 (deferred): "every tracked browser unsupported" collapses to
    // Forbid after the per-browser scan, below. Marked here for clarity.
    let mut all_unsupported = true;
    let mut any_unsupported = false;

    if let Some(compat_entry) = compat {
        // Rule 3: BCD deprecated → Avoid.
        if compat_entry.deprecated {
            reasons.push(Reason::BcdDeprecated);
        }
        // Rule 4: BCD experimental → Avoid.
        if compat_entry.experimental {
            reasons.push(Reason::BcdExperimental);
        }

        // Rule 5-9: per-browser signals.
        if let Some(support) = &compat_entry.browser_support {
            for (browser, version) in iter_browsers(support) {
                match version {
                    None => {
                        // Missing data: not a reason by itself.
                        all_unsupported = false;
                    }
                    Some(v) => {
                        let mut this_browser_unsupported = false;
                        if v.supported == Some(false) {
                            reasons.push(Reason::UnsupportedIn(browser));
                            this_browser_unsupported = true;
                            any_unsupported = true;
                        } else if v.partial_implementation {
                            reasons.push(Reason::PartialImplementationIn(browser));
                        }
                        if let Some(prefix) = &v.prefix {
                            reasons.push(Reason::PrefixRequiredIn {
                                browser,
                                prefix: prefix.clone(),
                            });
                        }
                        if !v.flags.is_empty() {
                            reasons.push(Reason::BehindFlagIn(browser));
                        }
                        if let Some(removed) = &v.version_removed {
                            reasons.push(Reason::RemovedIn {
                                browser,
                                version: removed.clone(),
                            });
                        }
                        if !this_browser_unsupported {
                            all_unsupported = false;
                        }
                    }
                }
            }
        } else {
            // No browser data at all: we can't say "all unsupported".
            all_unsupported = false;
        }

        // Rule 7: baseline tier.
        match &compat_entry.baseline {
            Some(BaselineValue::Limited) => reasons.push(Reason::BaselineLimited),
            Some(BaselineValue::Newly { since, .. }) => {
                reasons.push(Reason::BaselineNewly { since: *since });
            }
            _ => {}
        }
    } else {
        // No BCD data at all: neither all-unsupported nor baseline applies.
        all_unsupported = false;
    }

    // Post-collection: if every tracked browser is explicitly unsupported,
    // promote to Forbid by synthesising a terminal marker. The per-browser
    // UnsupportedIn reasons are already in the list; we signal the tier
    // promotion purely via the recommendation computation below.
    let forbid_from_unsupported = all_unsupported && any_unsupported;

    // Compute final recommendation: max tier across reasons, with the
    // all-unsupported special case overriding up to Forbid.
    let mut recommendation = reasons.iter().map(Reason::tier).max().unwrap_or(Tier::Safe);
    if forbid_from_unsupported && recommendation < Tier::Forbid {
        recommendation = Tier::Forbid;
    }

    // Sort reasons by tier desc, then stable (preserving collection order)
    // for tie-breaking — renderer shows the most urgent first.
    reasons.sort_by_key(|reason| std::cmp::Reverse(reason.tier()));

    let headline_template = pick_headline_template(recommendation, &reasons);

    Verdict {
        recommendation,
        headline_template,
        reasons,
    }
}

fn iter_browsers(
    support: &BrowserSupportValue,
) -> Vec<(&'static str, Option<&BrowserVersionValue>)> {
    vec![
        ("chrome", support.chrome.as_ref()),
        ("edge", support.edge.as_ref()),
        ("firefox", support.firefox.as_ref()),
        ("safari", support.safari.as_ref()),
    ]
}

/// Choose a short headline fragment for the hover blockquote.
///
/// The template is a static string the renderer splices into the headline
/// like `"> ⊘ baseProfile — removed from SVG 2"`. Chosen by the first
/// (highest-tier) reason with a fallback on the recommendation tier.
const fn pick_headline_template(rec: Tier, reasons: &[Reason]) -> &'static str {
    match reasons.first() {
        Some(Reason::ProfileObsolete { .. }) => "removed from the current SVG profile",
        Some(Reason::BcdDeprecated) => "deprecated",
        Some(Reason::BcdExperimental) => "experimental",
        Some(Reason::ProfileExperimental) => "draft-only in the current profile",
        Some(Reason::BaselineLimited) => "limited availability",
        Some(Reason::BaselineNewly { .. }) => "newly available",
        Some(Reason::PartialImplementationIn(_)) => "partially implemented",
        Some(Reason::PrefixRequiredIn { .. }) => "requires a vendor prefix",
        Some(Reason::BehindFlagIn(_)) => "behind a flag",
        Some(Reason::UnsupportedIn(_)) => "not universally supported",
        Some(Reason::RemovedIn { .. }) => "removed in some browsers",
        None => match rec {
            Tier::Safe => "widely supported",
            _ => "",
        },
    }
}

/// Format a [`Verdict`] as the Rust `CompatVerdict` literal emitted into
/// `catalog.rs`.
///
/// Emits the struct inline for simplicity; reason slices are NOT yet
/// deduplicated across entries. Interning is a follow-up optimisation —
/// correctness first.
pub fn format_verdict(v: &Verdict) -> String {
    let mut out = String::from("CompatVerdict { recommendation: ");
    out.push_str(match v.recommendation {
        Tier::Safe => "VerdictRecommendation::Safe",
        Tier::Caution => "VerdictRecommendation::Caution",
        Tier::Avoid => "VerdictRecommendation::Avoid",
        Tier::Forbid => "VerdictRecommendation::Forbid",
    });
    out.push_str(", headline_template: \"");
    // headline_template is a `&'static str` literal we chose above — safe
    // to inline without escaping (no special chars in our templates).
    out.push_str(v.headline_template);
    out.push_str("\", reasons: &[");
    for (i, reason) in v.reasons.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "{}", format_reason(reason));
    }
    out.push_str("] }");
    out
}

fn format_reason(r: &Reason) -> String {
    match r {
        Reason::BcdDeprecated => "VerdictReason::BcdDeprecated".to_string(),
        Reason::BcdExperimental => "VerdictReason::BcdExperimental".to_string(),
        Reason::ProfileObsolete { last_seen } => {
            format!("VerdictReason::ProfileObsolete {{ last_seen: SpecSnapshotId::{last_seen} }}")
        }
        Reason::ProfileExperimental => "VerdictReason::ProfileExperimental".to_string(),
        Reason::BaselineLimited => "VerdictReason::BaselineLimited".to_string(),
        Reason::BaselineNewly { since } => {
            format!("VerdictReason::BaselineNewly {{ since: {since}, qualifier: None }}")
        }
        Reason::PartialImplementationIn(browser) => {
            format!("VerdictReason::PartialImplementationIn(\"{browser}\")")
        }
        Reason::PrefixRequiredIn { browser, prefix } => {
            format!(
                "VerdictReason::PrefixRequiredIn {{ browser: \"{browser}\", prefix: \"{}\" }}",
                escape(prefix)
            )
        }
        Reason::BehindFlagIn(browser) => {
            format!("VerdictReason::BehindFlagIn(\"{browser}\")")
        }
        Reason::UnsupportedIn(browser) => {
            format!("VerdictReason::UnsupportedIn(\"{browser}\")")
        }
        Reason::RemovedIn { browser, version } => {
            format!(
                "VerdictReason::RemovedIn {{ browser: \"{browser}\", version: \"{}\", qualifier: None }}",
                escape(version)
            )
        }
    }
}

fn escape(s: &str) -> String {
    s.chars().flat_map(char::escape_default).collect()
}

/// Render a `verdicts` field value as `&[(SpecSnapshotId, CompatVerdict), ...]`.
pub fn format_verdicts_slice(entries: &[(&'static str, Verdict)]) -> String {
    if entries.is_empty() {
        return "&[]".to_string();
    }
    let mut out = String::from("&[");
    for (i, (snapshot, verdict)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(
            out,
            "(SpecSnapshotId::{snapshot}, {})",
            format_verdict(verdict)
        );
    }
    out.push(']');
    out
}
