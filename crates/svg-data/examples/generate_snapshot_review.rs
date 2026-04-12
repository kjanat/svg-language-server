//! Regenerate checked-in snapshot review reports from normalized snapshot facts.

use std::{error::Error, fs, path::Path};

use svg_data::{
    review::{Input, build_report},
    snapshot_schema::{
        CategoriesFile, ElementAttributeMatrixFile, ExceptionsFile, GrammarFile,
        SnapshotAttributeRecord, SnapshotElementRecord,
    },
    spec_snapshots,
    types::SpecSnapshotId,
};

fn main() -> Result<(), Box<dyn Error>> {
    let snapshot_arg = std::env::args().nth(1);
    let snapshots = match snapshot_arg.as_deref() {
        None | Some("all") => spec_snapshots().to_vec(),
        Some(snapshot) => vec![parse_snapshot_id(snapshot)?],
    };

    for snapshot in snapshots {
        let root = specs_root().join(snapshot.as_str());
        let elements: Vec<SnapshotElementRecord> = read_json(&root.join("elements.json"))?;
        let attributes: Vec<SnapshotAttributeRecord> = read_json(&root.join("attributes.json"))?;
        let grammars: GrammarFile = read_json(&root.join("grammars.json"))?;
        let categories: CategoriesFile = read_json(&root.join("categories.json"))?;
        let element_attribute_matrix: ElementAttributeMatrixFile =
            read_json(&root.join("element_attribute_matrix.json"))?;
        let exceptions: ExceptionsFile = read_json(&root.join("exceptions.json"))?;
        let existing_review: ExistingReviewNotes = read_json(&root.join("review.json"))?;
        let review = build_report(Input {
            elements: &elements,
            attributes: &attributes,
            grammars: &grammars,
            categories: &categories,
            element_attribute_matrix: &element_attribute_matrix,
            exceptions: &exceptions,
            manual_notes: &existing_review.manual_notes,
        });
        write_json(&root.join("review.json"), &review)?;
        println!("{}", root.join("review.json").display());
    }

    Ok(())
}

fn specs_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/specs")
}

#[derive(serde::Deserialize)]
struct ExistingReviewNotes {
    manual_notes: Vec<String>,
}

fn parse_snapshot_id(value: &str) -> Result<SpecSnapshotId, Box<dyn Error>> {
    match value {
        "Svg11Rec20030114" => Ok(SpecSnapshotId::Svg11Rec20030114),
        "Svg11Rec20110816" => Ok(SpecSnapshotId::Svg11Rec20110816),
        "Svg2Cr20181004" => Ok(SpecSnapshotId::Svg2Cr20181004),
        "Svg2EditorsDraft20250914" => Ok(SpecSnapshotId::Svg2EditorsDraft20250914),
        _ => Err(format!("snapshot review generator does not support {value}").into()),
    }
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
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    fs::write(path, text)?;
    Ok(())
}
