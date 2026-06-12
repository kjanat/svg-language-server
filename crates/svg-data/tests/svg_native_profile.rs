//! Faithfulness + oracle test for the SVG Native profile extractor.
//!
//! Two responsibilities:
//!
//! 1. **Reproduction** — re-run `build/svg_native.rs` over the vendored
//!    `index.bs` and assert the result equals the committed
//!    `data/profiles/svg-native.json` (modulo the editor-only `$schema` key).
//!    This proves the committed dataset is a faithful product of the
//!    deterministic extractor, not a hand-edited file.
//!
//! 2. **Oracle** — assert the extracted dataset contains the known
//!    constraints from the SVG Native spec prose (membership assertions), and
//!    that extracted element/attribute names are real SVG names (cross-checked
//!    against the generated catalog, with an explicit allowlist for the
//!    SVG 1.1-only font/glyph family that SVG 2's catalog does not carry).
//!
//! If the extractor ever misses an oracle item, the fix is in the parser
//! (`build/svg_native.rs`), never a hand-edit of the data file.

// The extractor references `crate::profile`; alias that to the crate's PUBLIC
// `svg_data::profile` so the included extractor type-checks against the same
// canonical types the test asserts on.
mod profile {
    pub use svg_data::profile::*;
}
#[path = "../build/svg_native.rs"]
mod svg_native;

use std::{collections::BTreeSet, path::PathBuf};

use profile::{ConstraintKind, ConstraintRule, ConstraintScope, ProvenancePin, SvgNative};
use serde::Deserialize;
use serde_json::Value;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[derive(Deserialize)]
struct Provenance {
    pin: Pin,
    profile: ProfileMeta,
}

#[derive(Deserialize)]
struct Pin {
    commit: String,
    date: String,
    repo: String,
}

#[derive(Deserialize)]
struct ProfileMeta {
    basis: String,
}

/// Run the extractor over the vendored source with the committed provenance.
fn extract() -> SvgNative {
    let source_dir = manifest_dir().join("data/sources/svg-native");
    let bikeshed = match std::fs::read_to_string(source_dir.join("index.bs")) {
        Ok(text) => text,
        Err(error) => panic!("index.bs not readable: {error}"),
    };
    let provenance_text = match std::fs::read_to_string(source_dir.join("PROVENANCE.toml")) {
        Ok(text) => text,
        Err(error) => panic!("PROVENANCE.toml not readable: {error}"),
    };
    let provenance: Provenance = match toml::from_str(&provenance_text) {
        Ok(value) => value,
        Err(error) => panic!("PROVENANCE.toml malformed: {error}"),
    };
    let pin = ProvenancePin {
        repository: provenance.pin.repo,
        commit: provenance.pin.commit,
        capture_date: provenance.pin.date,
        basis: provenance.profile.basis,
    };
    match svg_native::extract_svg_native(&bikeshed, pin) {
        Ok(profile) => profile,
        Err(error) => panic!("extractor failed: {error}"),
    }
}

/// The extractor output must reproduce the committed JSON byte-for-byte
/// (semantically), proving the dataset is derived and not hand-maintained.
#[test]
fn extractor_reproduces_committed_dataset() {
    let produced = match serde_json::to_value(extract()) {
        Ok(value) => value,
        Err(error) => panic!("profile does not serialize: {error}"),
    };

    let committed_path = manifest_dir().join("data/profiles/svg-native.json");
    let committed_text = match std::fs::read_to_string(&committed_path) {
        Ok(text) => text,
        Err(error) => panic!(
            "committed dataset not readable at {}: {error}",
            committed_path.display()
        ),
    };
    let mut committed: Value = match serde_json::from_str(&committed_text) {
        Ok(value) => value,
        Err(error) => panic!("committed dataset is not valid JSON: {error}"),
    };
    // Drop the editor-only `$schema` pointer; it is not extractor output.
    if let Some(object) = committed.as_object_mut() {
        object.remove("$schema");
    }

    assert_eq!(
        produced, committed,
        "extractor output diverges from committed data/profiles/svg-native.json — \
         regenerate with `cargo run -p svg-data --example generate_svg_native_profile`"
    );
}

/// Helper: is `(kind, name)` recorded as fully unsupported?
fn assert_unsupported(profile: &SvgNative, kind: ConstraintKind, name: &str) {
    assert!(
        profile.is_unsupported(kind, name),
        "expected {kind:?} `{name}` to be recorded Unsupported"
    );
}

#[test]
fn oracle_unsupported_elements() {
    let profile = extract();
    for name in [
        "text",
        "tspan",
        "textPath",
        "marker",
        "pattern",
        "symbol",
        "switch",
        "style",
        "script",
        "a",
        "view",
        "foreignObject",
    ] {
        assert_unsupported(&profile, ConstraintKind::Element, name);
    }
    // font / glyph family.
    for name in [
        "font",
        "glyph",
        "missingGlyph",
        "hkern",
        "vkern",
        "font-face",
        "font-face-src",
        "font-face-uri",
        "font-face-format",
        "font-face-name",
    ] {
        assert_unsupported(&profile, ConstraintKind::Element, name);
    }
}

#[test]
fn oracle_unsupported_properties() {
    let profile = extract();
    for name in [
        "display",
        "color",
        "pointer-events",
        "vector-effect",
        "paint-order",
        "color-interpolation",
    ] {
        assert_unsupported(&profile, ConstraintKind::Property, name);
    }
}

#[test]
fn oracle_lengths_and_values() {
    let profile = extract();
    // percentage + relative lengths unsupported (recorded as Features).
    assert_unsupported(&profile, ConstraintKind::Feature, "percentage-length");
    assert_unsupported(&profile, ConstraintKind::Feature, "relative-length");

    // calc() unsupported; env()/var() supported → must NOT appear as a
    // constraint at all.
    assert_unsupported(&profile, ConstraintKind::Value, "calc()");
    let value_names: BTreeSet<&str> = profile
        .of_kind(ConstraintKind::Value)
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert!(
        !value_names.contains("env") && !value_names.contains("env()"),
        "env() is supported and must not be recorded as a constraint"
    );
    assert!(
        !value_names.contains("var") && !value_names.contains("var()"),
        "var() is supported and must not be recorded as a constraint"
    );
}

#[test]
fn oracle_supported_only_allowlists() {
    let profile = extract();

    // Supported units == {(unitless), px, pt, pc, mm, cm, in}.
    match profile.supported_only(ConstraintKind::Feature, "length-unit") {
        Some(ConstraintScope::Units { names }) => {
            let got: BTreeSet<&str> = names.iter().map(String::as_str).collect();
            let want: BTreeSet<&str> = ["unitless", "px", "pt", "pc", "mm", "cm", "in"]
                .into_iter()
                .collect();
            assert_eq!(got, want, "length-unit allowlist mismatch");
        }
        other => panic!("expected length-unit Units allowlist, got {other:?}"),
    }

    // Supported image formats == {JPEG, PNG}.
    match profile.supported_only(ConstraintKind::Feature, "image-format") {
        Some(ConstraintScope::ImageFormats { names }) => {
            let got: BTreeSet<&str> = names.iter().map(String::as_str).collect();
            let want: BTreeSet<&str> = ["JPEG", "PNG"].into_iter().collect();
            assert_eq!(got, want, "image-format allowlist mismatch");
        }
        other => panic!("expected image-format ImageFormats allowlist, got {other:?}"),
    }

    // Only gradientUnits=userSpaceOnUse.
    match profile.supported_only(ConstraintKind::Attribute, "gradientUnits") {
        Some(ConstraintScope::Values { names }) => {
            assert_eq!(names, &vec!["userSpaceOnUse".to_string()]);
        }
        other => panic!("expected gradientUnits userSpaceOnUse allowlist, got {other:?}"),
    }

    // transform-bearing elements.
    match profile.supported_only(ConstraintKind::Property, "transform") {
        Some(ConstraintScope::Elements { names }) => {
            let got: BTreeSet<&str> = names.iter().map(String::as_str).collect();
            let want: BTreeSet<&str> = [
                "svg", "g", "defs", "use", "image", "path", "rect", "circle", "ellipse", "line",
                "polyline", "polygon", "clipPath",
            ]
            .into_iter()
            .collect();
            assert_eq!(got, want, "transform-bearing element allowlist mismatch");
        }
        other => panic!("expected transform Elements allowlist, got {other:?}"),
    }

    // viewBox only on svg.
    match profile.supported_only(ConstraintKind::Attribute, "viewBox") {
        Some(ConstraintScope::Elements { names }) => {
            assert_eq!(names, &vec!["svg".to_string()]);
        }
        other => panic!("expected viewBox svg-only allowlist, got {other:?}"),
    }

    // preserveAspectRatio only on {svg, image, pattern}.
    match profile.supported_only(ConstraintKind::Attribute, "preserveAspectRatio") {
        Some(ConstraintScope::Elements { names }) => {
            let got: BTreeSet<&str> = names.iter().map(String::as_str).collect();
            let want: BTreeSet<&str> = ["svg", "image", "pattern"].into_iter().collect();
            assert_eq!(got, want, "preserveAspectRatio allowlist mismatch");
        }
        other => panic!("expected preserveAspectRatio allowlist, got {other:?}"),
    }
}

#[test]
fn oracle_pathlength_unsupported() {
    let profile = extract();
    assert_unsupported(&profile, ConstraintKind::Attribute, "pathLength");
}

/// SVG 1.1 font/glyph elements legitimately absent from the SVG 2 catalog.
/// SVG Native is a subset of SVG 1.1, so it can name elements SVG 2 dropped.
const SVG11_ONLY_ELEMENTS: &[&str] = &[
    "font",
    "font-face",
    "font-face-format",
    "font-face-name",
    "font-face-src",
    "font-face-uri",
    "glyph",
    "hkern",
    "meta",
    "missingGlyph",
    "view",
    "vkern",
];

/// Every extracted element name is a real SVG element — present in the
/// generated catalog, or a known SVG 1.1-only name. Guards against the
/// heuristic parser inventing names from prose noise.
#[test]
fn extracted_element_names_are_real_svg() {
    let profile = extract();
    let catalog: BTreeSet<&str> = svg_data::elements().iter().map(|e| e.name).collect();

    let mut unknown = Vec::new();
    for constraint in profile.of_kind(ConstraintKind::Element) {
        let name = constraint.name.as_str();
        if !catalog.contains(name) && !SVG11_ONLY_ELEMENTS.contains(&name) {
            unknown.push(name.to_string());
        }
    }
    assert!(
        unknown.is_empty(),
        "extracted element names not in the SVG catalog or SVG 1.1 allowlist: {unknown:?}"
    );

    // Scope element allowlists (transform / viewBox / preserveAspectRatio) must
    // also reference only real catalog elements.
    for constraint in &profile.constraints {
        if let ConstraintRule::SupportedOnly {
            scope: ConstraintScope::Elements { names },
        } = &constraint.rule
        {
            for name in names {
                assert!(
                    catalog.contains(name.as_str()),
                    "scope `{}` references non-catalog element `{name}`",
                    constraint.name
                );
            }
        }
    }
}

/// Every extracted plain-attribute name (kind == Attribute) is either a real
/// catalog attribute or one of the geometry/structural attributes the spec
/// names that the catalog tracks. Presentation properties are checked
/// separately and may legitimately be CSS-only names.
/// Attributes the SVG Native spec names that are valid SVG but may not all sit
/// in the generated attribute catalog (it is SVG2-derived). These are confirmed
/// real SVG attribute names from the spec text.
const SPEC_ATTRS: &[&str] = &[
    "clip-path",
    "gradientUnits",
    "pathLength",
    "preserveAspectRatio",
    "style",
    "viewBox",
    "xml:space",
    "zoomAndPan",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "x",
    "y",
    "width",
    "height",
];

#[test]
fn extracted_attribute_names_are_real_svg() {
    let profile = extract();
    let catalog: BTreeSet<&str> = svg_data::attributes().iter().map(|a| a.name).collect();

    let mut unknown = Vec::new();
    for constraint in profile.of_kind(ConstraintKind::Attribute) {
        let name = constraint.name.as_str();
        if !catalog.contains(name) && !SPEC_ATTRS.contains(&name) {
            unknown.push(name.to_string());
        }
    }
    assert!(
        unknown.is_empty(),
        "extracted attribute names not in catalog or spec allowlist: {unknown:?}"
    );
}

/// Coverage gaps are recorded honestly: the prose-only group section is
/// flagged rather than silently dropped.
#[test]
fn coverage_gaps_are_recorded() {
    let profile = extract();
    assert!(
        profile
            .coverage_gaps
            .iter()
            .any(|g| g.section == "commonattributes"),
        "the common-attributes group section must be recorded as a coverage gap"
    );
}
