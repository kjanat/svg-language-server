//! Reproduction test: prove the deterministic `propidx.html` extractor
//! regenerates the hand-curated keyword enums in each SVG 1.1 snapshot's
//! `grammars.json`.
//!
//! For every `enum-*` grammar that is a pure keyword `Choice` (and whose
//! property is present in the property index as a keyword enum), the extractor's
//! keyword set must equal the committed keyword set. This is what lets the
//! curated transcription be replaced by the parser — and it is the regression
//! guard for the issue #9 transcription bugs (`pointer-events` had a bogus
//! `auto`; `text-decoration` dropped `blink`).
//!
//! Ordering-only differences are reported (not failed): the catalog flattens a
//! keyword enum to a *set*, and the committed data deliberately reorders one
//! enum (`pointer-events` hoists `none` to the front) relative to the spec's
//! source order. Set equality is the correctness contract; an order mismatch is
//! surfaced as a finding for the human to confirm, not a hard failure.

#[path = "../build/propidx.rs"]
mod propidx;
#[path = "../build/value_syntax.rs"]
mod value_syntax;

use std::collections::BTreeMap;

use svg_data::snapshot_schema::{GrammarFile, GrammarNode};

use propidx::parse_propidx;
use value_syntax::keyword_enum;

/// Vendored snapshots that ship a real SVG 1.1 `propidx.html`.
const SNAPSHOTS: &[Snapshot] = &[
    Snapshot {
        id: "Svg11Rec20030114",
        propidx: "data/sources/svg11-rec-20030114/propidx.html",
        grammars: "data/specs/Svg11Rec20030114/grammars.json",
    },
    Snapshot {
        id: "Svg11Rec20110816",
        propidx: "data/sources/svg11-rec-20110816/propidx.html",
        grammars: "data/specs/Svg11Rec20110816/grammars.json",
    },
];

struct Snapshot {
    id: &'static str,
    propidx: &'static str,
    grammars: &'static str,
}

/// Read a crate-relative data file.
fn read(path: &str) -> String {
    let full = format!("{}/{path}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("read {full}: {err}"))
}

/// Flatten a committed grammar `root` into its keyword set, in source order.
///
/// Mirrors the build's `collect_enum_keywords`: keywords, `Choice` branches and
/// `OneOrMore` wrappers (the `text-decoration` combinable shape) are keyword
/// enums; anything else (datatype/grammar ref, sequence, …) is not, returning
/// `None`.
fn committed_keywords(root: &GrammarNode) -> Option<Vec<String>> {
    fn walk(node: &GrammarNode, out: &mut Vec<String>) -> Option<()> {
        match node {
            GrammarNode::Keyword { value } => {
                out.push(value.clone());
                Some(())
            }
            GrammarNode::Choice { options } => {
                for option in options {
                    walk(option, out)?;
                }
                Some(())
            }
            GrammarNode::OneOrMore { item } => walk(item, out),
            _ => None,
        }
    }
    let GrammarNode::Choice { .. } = root else {
        return None;
    };
    let mut out = Vec::new();
    walk(root, &mut out).map(|()| out)
}

#[test]
fn propidx_reproduces_committed_keyword_enums() {
    let mut total_compared = 0usize;
    let mut order_diffs: Vec<String> = Vec::new();
    let mut set_diffs: Vec<String> = Vec::new();

    for snapshot in SNAPSHOTS {
        let rows = parse_propidx(&read(snapshot.propidx))
            .unwrap_or_else(|err| panic!("{}: {err}", snapshot.id));

        // property-name -> extracted keyword set (only pure keyword enums).
        let extracted: BTreeMap<String, Vec<String>> = rows
            .into_iter()
            .filter_map(|row| keyword_enum(&row.values).map(|kws| (row.name, kws)))
            .collect();

        let grammars: GrammarFile = serde_json::from_str(&read(snapshot.grammars))
            .unwrap_or_else(|err| panic!("{} grammars.json: {err}", snapshot.id));

        for grammar in &grammars.grammars {
            let Some(property) = grammar.id.strip_prefix("enum-") else {
                continue;
            };
            // Only enums whose value list lives in the property index. The
            // attribute/DTD-sourced enums (accumulate, additive, gradientUnits,
            // in, spreadMethod) are not properties and are out of scope here.
            let Some(extracted_kws) = extracted.get(property) else {
                continue;
            };
            let Some(committed_kws) = committed_keywords(&grammar.root) else {
                continue;
            };
            total_compared += 1;

            if extracted_kws == &committed_kws {
                continue;
            }

            let mut extracted_sorted = extracted_kws.clone();
            let mut committed_sorted = committed_kws.clone();
            extracted_sorted.sort();
            committed_sorted.sort();

            if extracted_sorted == committed_sorted {
                order_diffs.push(format!(
                    "{}/{}: order differs\n    propidx:   {:?}\n    committed: {:?}",
                    snapshot.id, grammar.id, extracted_kws, committed_kws
                ));
            } else {
                set_diffs.push(format!(
                    "{}/{}: SET differs\n    propidx:   {:?}\n    committed: {:?}",
                    snapshot.id, grammar.id, extracted_kws, committed_kws
                ));
            }
        }
    }

    assert!(
        total_compared > 0,
        "no enum grammars were compared — extractor or mapping is broken"
    );

    // Set mismatches are correctness failures: a real transcription bug or a
    // genuine spec-vs-data divergence the extractor must not paper over.
    assert!(
        set_diffs.is_empty(),
        "propidx keyword sets diverge from committed grammars:\n{}",
        set_diffs.join("\n")
    );

    // Ordering-only diffs are expected (pointer-events) and reported, not failed.
    if !order_diffs.is_empty() {
        eprintln!(
            "propidx repro: {} enum(s) match by set but differ in order (committed reorder):\n{}",
            order_diffs.len(),
            order_diffs.join("\n")
        );
    }

    eprintln!(
        "propidx repro: {total_compared} keyword enum(s) compared across {} snapshot(s); \
         {} set-equal, {} order-only diffs.",
        SNAPSHOTS.len(),
        total_compared - order_diffs.len(),
        order_diffs.len()
    );
}
