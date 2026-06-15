//! Audit of spec-derived element **descriptions** against the committed
//! `Svg2EditorsDraft` snapshot.
//!
//! `build/spec.rs` derives a clean lead description for each SVG element from
//! the vendored svgwg chapter HTML. This test runs that extractor over the
//! snapshot's pinned vendored checkout and audits the result against the
//! `title` field each element carries in
//! `data/specs/Svg2EditorsDraft/elements.json` (that `title` is what
//! `build.rs` maps onto `UnionElement.description`).
//!
//! Descriptions are prose, so exact byte-reproduction is not achievable for
//! every element. The audit therefore asserts two complementary properties:
//!
//! - **Robust faithfulness** (every element the extractor can derive): the
//!   description is non-empty, whitespace-normalized, and free of leftover HTML
//!   markup or entities — i.e. it is genuine clean lead prose, not table chrome
//!   or a stray fragment.
//! - **Exact normalized reproduction** (the subset that clearly transcribes the
//!   chapter lead `<p>`): the extracted description equals the snapshot `title`
//!   after whitespace/quote normalization. This subset is pinned so a
//!   regression in the extractor (or a snapshot edit that silently diverges
//!   from the spec prose) fails the build's test gate.
//!
//! Elements whose chapter prose is **not** in the vendored set — the filter
//! primitives (`filters.html` is not vendored), the animation elements
//! (`Overview.html` is not vendored), and a handful of others (`clipPath`,
//! `mask`, `mpath`, `style`, `tspan`) — are recorded as **gaps**: the extractor
//! must *not* fabricate a description for them. Pinning the gap set documents
//! the honest fidelity ceiling and flags the moment a future vendor capture
//! closes one of these gaps.

#[path = "../build/spec.rs"]
mod spec;

use std::{collections::BTreeMap, path::PathBuf};

use serde::Deserialize;

/// The vendored `master/` directory for the `Svg2EditorsDraft` snapshot,
/// resolved from the `git_commit` pin in `snapshot.json` (dir is
/// `svgwg-<commit[..8]>`). Reading the pin keeps the audit aligned with the
/// snapshot under test across re-vendors — no hardcoded commit to update — even
/// though other vendored captures (e.g. the `spec_scan` source) coexist under
/// `data/sources/`.
fn vendored_ed_master() -> PathBuf {
    let path = manifest_dir().join("data/specs/Svg2EditorsDraft/snapshot.json");
    let raw = std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    let snapshot: serde_json::Value = serde_json::from_slice(&raw)
        .unwrap_or_else(|err| panic!("parse {}: {err}", path.display()));
    let commit = snapshot
        .get("pinned_sources")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|source| source.get("pin"))
        .find(|pin| pin.get("kind").and_then(serde_json::Value::as_str) == Some("git_commit"))
        .and_then(|pin| pin.get("commit"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("no git_commit pin in {}", path.display()));
    let prefix = commit
        .get(..8)
        .unwrap_or_else(|| panic!("git_commit pin too short: {commit:?}"));
    manifest_dir().join(format!("data/sources/svgwg-{prefix}/master"))
}

/// A single element record from the snapshot `elements.json`. Only the fields
/// the audit needs are deserialized.
#[derive(Debug, Deserialize)]
struct ElementRecord {
    name: String,
    title: String,
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Whitespace/quote normalization, mirroring the extractor's own
/// `normalize_text`, so both sides of the audit are compared on equal footing.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(['\u{2018}', '\u{2019}'], "'")
}

/// Load the snapshot's element `name → title` map.
fn snapshot_titles() -> BTreeMap<String, String> {
    let path = manifest_dir().join("data/specs/Svg2EditorsDraft/elements.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    let records: Vec<ElementRecord> =
        serde_json::from_str(&raw).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()));
    records
        .into_iter()
        .map(|record| (record.name, normalize(&record.title)))
        .collect()
}

/// Run the extractor over the pinned vendored ED chapter HTML.
fn extracted_descriptions() -> BTreeMap<String, String> {
    spec::extract_chapter_descriptions(&vendored_ed_master())
}

/// Elements whose extracted lead description reproduces the snapshot `title`
/// **exactly** after normalization. These clearly transcribe the chapter lead
/// paragraph, so the extractor must keep reproducing them byte-for-byte.
const EXACT_REPRODUCTION: &[&str] = &[
    "circle",
    "defs",
    "ellipse",
    "line",
    "linearGradient",
    "marker",
    "metadata",
    "polygon",
    "polyline",
    "radialGradient",
    "rect",
    "script",
    "stop",
    "symbol",
    "text",
    "use",
    "view",
];

/// Elements with **no** locatable chapter prose in the vendored ED set. The
/// extractor must not invent descriptions for them. Closing a gap (e.g. by
/// vendoring `filters.html`) is a deliberate data refresh that should update
/// this list — the test failing on a newly-derivable element is the intended
/// signal, not a false alarm.
const KNOWN_GAPS: &[&str] = &[
    // Animation elements — `specs/animations/.../Overview.html` not vendored.
    "animate",
    "animateMotion",
    "animateTransform",
    "set",
    "mpath",
    // Filter primitives + `filter` — `filters.html` not vendored.
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDistantLight",
    "feDropShadow",
    "feFlood",
    "feFuncA",
    "feFuncB",
    "feFuncG",
    "feFuncR",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feOffset",
    "fePointLight",
    "feSpecularLighting",
    "feSpotLight",
    "feTile",
    "feTurbulence",
    "filter",
    // Masking — `masking.html` carries no element-definition anchors.
    "clipPath",
    "mask",
    // Misc — no lead-paragraph anchor in the vendored chapters.
    "style",
    "tspan",
];

#[test]
fn exact_reproduction_subset_matches_snapshot() {
    let extracted = extracted_descriptions();
    let titles = snapshot_titles();

    let mut failures = Vec::new();
    for &name in EXACT_REPRODUCTION {
        let Some(snapshot) = titles.get(name) else {
            failures.push(format!("{name}: not present in snapshot elements.json"));
            continue;
        };
        match extracted.get(name) {
            None => failures.push(format!("{name}: extractor produced no description")),
            Some(derived) if derived == snapshot => {}
            Some(derived) => failures.push(format!(
                "{name}: extracted != snapshot\n    extracted: {derived}\n    snapshot:  {snapshot}"
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "exact-reproduction audit failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_derived_description_is_clean_prose() {
    // Robust faithfulness: whatever the extractor derives must be clean,
    // non-empty, normalized lead prose — never table chrome or markup debris.
    let extracted = extracted_descriptions();
    assert!(
        !extracted.is_empty(),
        "extractor derived zero descriptions — vendored chapters missing?"
    );

    let mut failures = Vec::new();
    for (name, desc) in &extracted {
        if desc.trim().is_empty() {
            failures.push(format!("{name}: empty description"));
        }
        if desc.chars().count() < 20 {
            failures.push(format!("{name}: implausibly short: {desc:?}"));
        }
        if desc.contains('<') || desc.contains('>') {
            failures.push(format!("{name}: contains leftover HTML markup: {desc:?}"));
        }
        if desc.contains("&lt;") || desc.contains("&gt;") || desc.contains("&amp;") {
            failures.push(format!(
                "{name}: contains unescaped HTML entities: {desc:?}"
            ));
        }
        // Normalization invariant: no double spaces or leading/trailing space.
        if desc != &normalize(desc) {
            failures.push(format!("{name}: not whitespace-normalized: {desc:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "clean-prose audit failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn derived_descriptions_are_anchored_to_their_element() {
    // Faithfulness: a derived description should describe the element it is
    // attributed to. We require either the element's own name or a sibling
    // reference to appear — except for the small, explicitly-listed set whose
    // lead paragraph genuinely opens without naming the element (it names the
    // element's attributes instead). Listing the exceptions keeps the property
    // meaningful while staying honest about the prose that legitimately differs.
    const NAME_ABSENT_OK: &[&str] = &["textPath"];

    let extracted = extracted_descriptions();
    let mut failures = Vec::new();
    for (name, desc) in &extracted {
        if NAME_ABSENT_OK.contains(&name.as_str()) {
            continue;
        }
        let names_element = desc.contains(name.as_str()) || desc.contains(&format!("'{name}'"));
        if !names_element {
            failures.push(format!(
                "{name}: description never names the element: {desc:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "anchoring audit failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn known_gaps_are_not_fabricated() {
    let extracted = extracted_descriptions();
    let mut newly_derivable = Vec::new();
    for &name in KNOWN_GAPS {
        if let Some(desc) = extracted.get(name) {
            newly_derivable.push(format!("{name}: now derivable -> {desc:?}"));
        }
    }
    assert!(
        newly_derivable.is_empty(),
        "elements listed as gaps now have derived prose — update KNOWN_GAPS \
         (and audit the new descriptions):\n{}",
        newly_derivable.join("\n")
    );
}

#[test]
fn exact_subset_and_gaps_are_disjoint_and_known() {
    // Guard the bookkeeping: the exact-reproduction set and the gap set must not
    // overlap, and every snapshot element must be accounted for as either
    // extracted (exact / near / divergent) or a known gap. This keeps the audit
    // honest if the snapshot gains or loses elements.
    let titles = snapshot_titles();
    let extracted = extracted_descriptions();

    for &name in EXACT_REPRODUCTION {
        assert!(
            !KNOWN_GAPS.contains(&name),
            "{name} is in both EXACT_REPRODUCTION and KNOWN_GAPS"
        );
    }
    for &name in KNOWN_GAPS {
        assert!(
            !extracted.contains_key(name),
            "{name} is a KNOWN_GAP but was extracted"
        );
        assert!(
            titles.contains_key(name),
            "{name} is a KNOWN_GAP but is not a snapshot element"
        );
    }
    // Every snapshot element is either derived or a known gap — no silent
    // unaccounted elements.
    for name in titles.keys() {
        let accounted = extracted.contains_key(name) || KNOWN_GAPS.contains(&name.as_str());
        assert!(
            accounted,
            "snapshot element {name} is neither derived nor a known gap"
        );
    }
}
