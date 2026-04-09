//! Regenerate checked-in union membership and adjacent snapshot overlays.

use std::{error::Error, fs, path::Path};

use svg_data::{
    derived::{
        AttributeMembershipFile, DerivedMembershipArtifacts, ElementMembershipFile,
        ReviewedSnapshotMembershipInput, SnapshotOverlayFile, build_membership_artifacts,
    },
    snapshot_schema::{ReviewFile, SnapshotAttributeRecord, SnapshotElementRecord},
    spec_snapshots,
};

fn main() -> Result<(), Box<dyn Error>> {
    let inputs = spec_snapshots()
        .iter()
        .copied()
        .map(|snapshot| {
            let root = specs_root().join(snapshot.as_str());
            Ok(OwnedSnapshotMembershipInput {
                snapshot,
                elements: read_json(&root.join("elements.json"))?,
                attributes: read_json(&root.join("attributes.json"))?,
                review: read_json(&root.join("review.json"))?,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;

    let borrowed_inputs: Vec<_> = inputs
        .iter()
        .map(|input| ReviewedSnapshotMembershipInput {
            snapshot: input.snapshot,
            elements: &input.elements,
            attributes: &input.attributes,
            review: &input.review,
        })
        .collect();
    let artifacts = build_membership_artifacts(&borrowed_inputs)?;
    write_artifacts(&artifacts)?;

    println!("{}", derived_root().display());
    Ok(())
}

#[derive(Debug)]
struct OwnedSnapshotMembershipInput {
    snapshot: svg_data::types::SpecSnapshotId,
    elements: Vec<SnapshotElementRecord>,
    attributes: Vec<SnapshotAttributeRecord>,
    review: ReviewFile,
}

fn specs_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/specs")
}

fn derived_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/derived")
}

fn write_artifacts(artifacts: &DerivedMembershipArtifacts) -> Result<(), Box<dyn Error>> {
    let root = derived_root();
    write_json(&root.join("union/elements.json"), &artifacts.elements)?;
    write_json(&root.join("union/attributes.json"), &artifacts.attributes)?;
    for overlay in &artifacts.overlays {
        write_json(&root.join(overlay_file_name(overlay)), overlay)?;
    }
    Ok(())
}

fn overlay_file_name(overlay: &SnapshotOverlayFile) -> String {
    format!(
        "overlays/{}__{}.json",
        overlay.from_snapshot.as_str(),
        overlay.to_snapshot.as_str()
    )
}

fn read_json<T>(path: &Path) -> Result<T, Box<dyn Error>>
where
    T: serde::de::DeserializeOwned,
{
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

fn write_json<T>(path: &Path, value: &T) -> Result<(), Box<dyn Error>>
where
    T: serde::Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    fs::write(path, text)?;
    Ok(())
}

#[allow(dead_code)]
const fn _assert_schema_types(_: &ElementMembershipFile, _: &AttributeMembershipFile) {}
