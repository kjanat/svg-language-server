//! Verifies that every vendored source file matches the integrity facts
//! recorded in its `PROVENANCE.toml`.
//!
//! Each `data/sources/**/PROVENANCE.toml` records, per `[[inputs]]`, the
//! `sha256` (always) and often the `bytes` length of the file as extracted
//! verbatim from its pinned upstream. Those facts are the contract behind the
//! reproducibility claim — but recording them is worthless if nothing checks
//! the bytes still hash to what was written down. A vendored file can be
//! hand-edited, reformatted, or corrupted while the provenance keeps claiming
//! pristine upstream bytes; this test is the gate that catches exactly that.
//!
//! `sha256` is the load-bearing check: an identical SHA-256 means byte-identical
//! content (finding a collision is infeasible), so it subsumes any reformat or
//! edit. `git_blob` (present only where the upstream is a git object) is an
//! upstream-*traceability* locator — `git cat-file blob <git_blob>` resolves the
//! exact source revision — not a second integrity check; the bytes it names are
//! already locked down by `sha256`. So `git_blob` is format-validated here (a
//! 40-char lowercase hex git object id) rather than recomputed, which would pull
//! a SHA-1 dependency in solely to re-prove what `sha256` already proves.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Provenance {
    #[serde(default)]
    inputs: Vec<Input>,
}

#[derive(Deserialize)]
struct Input {
    /// File path relative to the `PROVENANCE.toml`'s own directory.
    path: String,
    /// Expected SHA-256 of the file bytes, lowercase hex.
    sha256: String,
    /// Expected byte length, when recorded.
    #[serde(default)]
    bytes: Option<u64>,
    /// Upstream git object id, when the source is a git blob.
    #[serde(default)]
    git_blob: Option<String>,
}

fn sources_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/sources")
}

/// Every `PROVENANCE.toml` anywhere under `data/sources`, found by walking the
/// tree so a newly vendored source directory is covered without editing a list.
fn provenance_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        panic!("sources dir not readable: {}", root.display());
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            found.extend(provenance_files(&path));
        } else if path
            .file_name()
            .is_some_and(|name| name == "PROVENANCE.toml")
        {
            found.push(path);
        }
    }
    found
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::new_with_prefix(bytes).finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn is_git_object_id(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[test]
fn vendored_bytes_match_recorded_provenance() {
    let sources = sources_dir();
    let files = provenance_files(&sources);
    assert!(
        !files.is_empty(),
        "no PROVENANCE.toml found under {} — the verifier would silently pass",
        sources.display()
    );

    let mut failures = Vec::new();
    let mut verified_inputs = 0usize;

    for provenance_path in &files {
        let relative = provenance_path
            .strip_prefix(&sources)
            .unwrap_or(provenance_path)
            .display();
        let base = provenance_path.parent().unwrap_or_else(|| Path::new("."));

        let text = match fs::read_to_string(provenance_path) {
            Ok(text) => text,
            Err(error) => {
                failures.push(format!("{relative}: unreadable ({error})"));
                continue;
            }
        };
        let provenance: Provenance = match toml::from_str(&text) {
            Ok(provenance) => provenance,
            Err(error) => {
                failures.push(format!("{relative}: invalid TOML ({error})"));
                continue;
            }
        };
        if provenance.inputs.is_empty() {
            failures.push(format!("{relative}: no [[inputs]] to verify"));
            continue;
        }

        for input in &provenance.inputs {
            let file_path = base.join(&input.path);
            let Ok(bytes) = fs::read(&file_path) else {
                failures.push(format!(
                    "{relative}: input `{}` is missing at {}",
                    input.path,
                    file_path.display()
                ));
                continue;
            };

            let actual_sha = sha256_hex(&bytes);
            if actual_sha != input.sha256 {
                failures.push(format!(
                    "{relative}: input `{}` sha256 mismatch\n      recorded: {}\n      actual:   {}",
                    input.path, input.sha256, actual_sha
                ));
            }
            if let Some(recorded_len) = input.bytes {
                let actual_len = bytes.len() as u64;
                if actual_len != recorded_len {
                    failures.push(format!(
                        "{relative}: input `{}` byte length mismatch (recorded {recorded_len}, actual {actual_len})",
                        input.path
                    ));
                }
            }
            if let Some(git_blob) = &input.git_blob
                && !is_git_object_id(git_blob)
            {
                failures.push(format!(
                    "{relative}: input `{}` git_blob is not a 40-char lowercase-hex object id: {git_blob:?}",
                    input.path
                ));
            }
            verified_inputs += 1;
        }
    }

    assert!(
        failures.is_empty(),
        "vendored source bytes diverge from recorded provenance ({} input(s) checked):\n\n{}",
        verified_inputs,
        failures.join("\n")
    );
}
