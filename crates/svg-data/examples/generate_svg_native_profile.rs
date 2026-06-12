//! Generate the committed SVG Native profile constraint dataset.
//!
//! Re-extracts the constraints from the vendored Bikeshed source
//! (`data/sources/svg-native/index.bs`) and writes the result to
//! `data/profiles/svg-native.json`, with a `$schema` pointer for editor
//! tooling. The build's `audit_svg_native_profile` gate then verifies that the
//! committed file matches this extractor on every build.
//!
//! ```sh
//! cargo run -p svg-data --example generate_svg_native_profile
//! ```

// The extractor lives in the build tree (it is also compiled by `build.rs` and
// the reproduction test). Include it directly so the generator stays the single
// source of the committed JSON without duplicating parser logic in `src/`. The
// extractor references `crate::profile`; alias that to the crate's PUBLIC
// `svg_data::profile` so the included extractor type-checks against the same
// canonical types the rest of the crate uses (no duplicated type, no dead-code
// warnings on the public helper methods).
mod profile {
    pub use svg_data::profile::*;
}
#[path = "../build/svg_native.rs"]
mod svg_native;

use std::{error::Error, fs, path::Path};

use profile::ProvenancePin;
use serde::Deserialize;

// `SVG_SCHEMA_REF` is set by `build.rs`: the release tag on a tagged build,
// `master` otherwise. Keeps the committed `$schema` link pinned to the matching
// ref instead of always `master`.
const SCHEMA_URL: &str = concat!(
    "https://raw.githubusercontent.com/kjanat/svg-language-server/",
    env!("SVG_SCHEMA_REF"),
    "/crates/svg-data/data/schemas/svg-native-profile.schema.json"
);

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

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_dir = manifest_dir.join("data/sources/svg-native");

    let bikeshed = fs::read_to_string(source_dir.join("index.bs"))?;
    let provenance: Provenance =
        toml::from_str(&fs::read_to_string(source_dir.join("PROVENANCE.toml"))?)?;

    let pin = ProvenancePin {
        repository: provenance.pin.repo,
        commit: provenance.pin.commit,
        capture_date: provenance.pin.date,
        basis: provenance.profile.basis,
    };

    let profile = svg_native::extract_svg_native(&bikeshed, pin)?;

    // Serialize, then splice in a leading `$schema` key so the JSON validates
    // against the generated schema in editors. The build audit drops `$schema`
    // before comparing, so this pointer never trips the drift gate.
    let value = serde_json::to_value(&profile)?;
    let mut object = serde_json::Map::new();
    object.insert(
        "$schema".to_string(),
        serde_json::Value::String(SCHEMA_URL.to_string()),
    );
    if let serde_json::Value::Object(fields) = value {
        for (key, field) in fields {
            object.insert(key, field);
        }
    }

    let profiles_dir = manifest_dir.join("data/profiles");
    fs::create_dir_all(&profiles_dir)?;
    let out_path = profiles_dir.join("svg-native.json");
    let mut json = serde_json::to_string_pretty(&serde_json::Value::Object(object))?;
    json.push('\n');
    fs::write(&out_path, &json)?;
    println!("{}", out_path.display());

    println!(
        "constraints: {} | coverage_gaps: {}",
        profile.constraints.len(),
        profile.coverage_gaps.len()
    );
    Ok(())
}
