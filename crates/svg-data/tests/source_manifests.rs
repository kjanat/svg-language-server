//! Regression coverage for checked-in source manifests.

use std::{fs, path::Path};

const MANIFESTS: &[(&str, &str)] = &[
    ("svg11-rec-20030114.toml", "[[inputs]]"),
    ("svg11-rec-20110816.toml", "[[inputs]]"),
    ("svg2-cr-20181004.toml", "[[inputs]]"),
    ("svg2-ed-20250914.toml", "[[inputs]]"),
    ("foreign-references.toml", "[[inputs]]"),
];

#[test]
fn source_manifests_cover_required_fields() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/sources");

    for &(file_name, repeated_section) in MANIFESTS {
        let manifest = fs::read_to_string(manifest_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));

        for required in [
            "schema_version",
            "manifest_id",
            "authority",
            "[checksum]",
            "strategy",
            "[fetch]",
            "policy",
            "pin",
            repeated_section,
        ] {
            assert!(
                manifest_contains_key(&manifest, required),
                "{file_name} missing required field or section: {required}"
            );
        }
    }
}

#[test]
fn snapshot_manifests_cover_all_tracked_snapshots() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/sources");

    for &(file_name, snapshot_id) in &[
        ("svg11-rec-20030114.toml", "Svg11Rec20030114"),
        ("svg11-rec-20110816.toml", "Svg11Rec20110816"),
        ("svg2-cr-20181004.toml", "Svg2Cr20181004"),
        ("svg2-ed-20250914.toml", "Svg2EditorsDraft20250914"),
    ] {
        let manifest = fs::read_to_string(manifest_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));

        assert!(
            manifest_contains_exact_value(&manifest, "snapshot", snapshot_id),
            "{file_name} missing snapshot id {snapshot_id}"
        );
    }
}

fn manifest_contains_key(manifest: &str, key: &str) -> bool {
    if key.starts_with('[') {
        return manifest.contains(key);
    }

    if key == "authority" {
        return manifest.contains("[authority]")
            || manifest
                .lines()
                .map(str::trim_start)
                .any(|line| line.starts_with("authority ") || line.starts_with("authority="));
    }

    if key == "pin" {
        return manifest.contains("[pin]")
            || manifest
                .lines()
                .map(str::trim_start)
                .any(|line| line.starts_with("pin ") || line.starts_with("pin="));
    }

    manifest
        .lines()
        .map(str::trim_start)
        .any(|line| line.starts_with(&format!("{key} ")) || line.starts_with(&format!("{key}=")))
}

fn manifest_contains_exact_value(manifest: &str, key: &str, expected: &str) -> bool {
    manifest.lines().map(str::trim).any(|line| {
        line.split_once('=')
            .is_some_and(|(candidate_key, candidate_value)| {
                candidate_key.trim() == key && candidate_value.trim() == format!("\"{expected}\"")
            })
    })
}
