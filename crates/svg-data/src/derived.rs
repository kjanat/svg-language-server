//! Derived union membership and adjacent snapshot overlay artifacts.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::{
    snapshot_schema::{ReviewFile, SnapshotAttributeRecord, SnapshotElementRecord},
    spec_snapshots,
    types::SpecSnapshotId,
};

/// Current checked-in schema version for derived union and overlay artifacts.
pub const DERIVED_SCHEMA_VERSION: u32 = 1;

/// Borrowed reviewed snapshot facts used to derive union membership artifacts.
#[derive(Debug, Clone, Copy)]
pub struct ReviewedSnapshotMembershipInput<'a> {
    /// Canonical snapshot id.
    pub snapshot: SpecSnapshotId,
    /// Checked-in element facts for the snapshot.
    pub elements: &'a [SnapshotElementRecord],
    /// Checked-in attribute facts for the snapshot.
    pub attributes: &'a [SnapshotAttributeRecord],
    /// Checked-in review report for the snapshot.
    pub review: &'a ReviewFile,
}

/// Generated derived artifacts written under `data/derived/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedMembershipArtifacts {
    /// Canonical union element membership.
    pub elements: ElementMembershipFile,
    /// Canonical union attribute membership.
    pub attributes: AttributeMembershipFile,
    /// Adjacent version diffs for element and attribute membership.
    pub overlays: Vec<SnapshotOverlayFile>,
}

/// Typed payload for `data/derived/union/elements.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementMembershipFile {
    /// Schema version for this derived payload.
    pub schema_version: u32,
    /// Canonical snapshot order used to derive membership.
    pub snapshots: Vec<SpecSnapshotId>,
    /// Union element names and the snapshots where they exist.
    pub elements: Vec<FeatureMembershipRecord>,
}

/// Typed payload for `data/derived/union/attributes.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeMembershipFile {
    /// Schema version for this derived payload.
    pub schema_version: u32,
    /// Canonical snapshot order used to derive membership.
    pub snapshots: Vec<SpecSnapshotId>,
    /// Union attribute names and the snapshots where they exist.
    pub attributes: Vec<FeatureMembershipRecord>,
}

/// Canonical union membership for one named feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureMembershipRecord {
    /// Element or attribute name.
    pub name: String,
    /// Snapshots where the feature exists.
    pub present_in: Vec<SpecSnapshotId>,
}

/// Typed payload for `data/derived/overlays/<from>__<to>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotOverlayFile {
    /// Schema version for this derived payload.
    pub schema_version: u32,
    /// Snapshot id the diff starts from.
    pub from_snapshot: SpecSnapshotId,
    /// Snapshot id the diff targets.
    pub to_snapshot: SpecSnapshotId,
    /// Element membership changes between snapshots.
    pub elements: MembershipDelta,
    /// Attribute membership changes between snapshots.
    pub attributes: MembershipDelta,
}

/// Added and removed names between two adjacent snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipDelta {
    /// Names present in the target snapshot but not the source snapshot.
    pub added: Vec<String>,
    /// Names present in the source snapshot but not the target snapshot.
    pub removed: Vec<String>,
}

/// Errors raised while deriving artifacts from reviewed snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeriveError {
    /// One or more canonical snapshots were missing from the input set.
    MissingSnapshots(Vec<SpecSnapshotId>),
    /// A snapshot review still contains unresolved issues.
    SnapshotReviewNotClean {
        /// Snapshot that failed the review gate.
        snapshot: SpecSnapshotId,
        /// Number of unresolved review issues.
        unresolved: usize,
    },
}

impl std::fmt::Display for DeriveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSnapshots(missing) => write!(
                f,
                "missing reviewed snapshots: {}",
                missing
                    .iter()
                    .map(|snapshot| snapshot.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::SnapshotReviewNotClean {
                snapshot,
                unresolved,
            } => write!(
                f,
                "snapshot {} still has {} unresolved review issues",
                snapshot.as_str(),
                unresolved
            ),
        }
    }
}

impl std::error::Error for DeriveError {}

/// Derive canonical union membership and adjacent snapshot overlays.
///
/// # Errors
/// Returns an error if any canonical snapshot is missing or a review gate is not clean.
pub fn build_membership_artifacts(
    inputs: &[ReviewedSnapshotMembershipInput<'_>],
) -> Result<DerivedMembershipArtifacts, DeriveError> {
    let canonical_snapshots = spec_snapshots();
    let input_by_snapshot: HashMap<SpecSnapshotId, ReviewedSnapshotMembershipInput<'_>> = inputs
        .iter()
        .map(|input| (input.snapshot, *input))
        .collect();

    let missing_snapshots: Vec<SpecSnapshotId> = canonical_snapshots
        .iter()
        .copied()
        .filter(|snapshot| !input_by_snapshot.contains_key(snapshot))
        .collect();
    if !missing_snapshots.is_empty() {
        return Err(DeriveError::MissingSnapshots(missing_snapshots));
    }

    let ordered_inputs: Vec<ReviewedSnapshotMembershipInput<'_>> = canonical_snapshots
        .iter()
        .copied()
        .map(|snapshot| input_by_snapshot[&snapshot])
        .collect();

    for input in &ordered_inputs {
        if !input.review.unresolved.is_empty() {
            return Err(DeriveError::SnapshotReviewNotClean {
                snapshot: input.snapshot,
                unresolved: input.review.unresolved.len(),
            });
        }
    }

    let snapshots = canonical_snapshots.to_vec();
    let elements = ElementMembershipFile {
        schema_version: DERIVED_SCHEMA_VERSION,
        snapshots: snapshots.clone(),
        elements: membership_records(ordered_inputs.iter().map(|input| {
            (
                input.snapshot,
                input.elements.iter().map(|record| record.name.as_str()),
            )
        })),
    };
    let attributes = AttributeMembershipFile {
        schema_version: DERIVED_SCHEMA_VERSION,
        snapshots,
        attributes: membership_records(ordered_inputs.iter().map(|input| {
            (
                input.snapshot,
                input.attributes.iter().map(|record| record.name.as_str()),
            )
        })),
    };
    let overlays = ordered_inputs
        .windows(2)
        .map(|pair| {
            let [from, to] = pair else {
                unreachable!("adjacent windows always have length two")
            };
            SnapshotOverlayFile {
                schema_version: DERIVED_SCHEMA_VERSION,
                from_snapshot: from.snapshot,
                to_snapshot: to.snapshot,
                elements: membership_delta(
                    from.elements.iter().map(|record| record.name.as_str()),
                    to.elements.iter().map(|record| record.name.as_str()),
                ),
                attributes: membership_delta(
                    from.attributes.iter().map(|record| record.name.as_str()),
                    to.attributes.iter().map(|record| record.name.as_str()),
                ),
            }
        })
        .collect();

    Ok(DerivedMembershipArtifacts {
        elements,
        attributes,
        overlays,
    })
}

/// Reconstruct the element set for one snapshot from the union membership file.
#[must_use]
pub fn element_set_for_snapshot(
    file: &ElementMembershipFile,
    snapshot: SpecSnapshotId,
) -> BTreeSet<&str> {
    membership_set(file.elements.iter(), snapshot)
}

/// Reconstruct the attribute set for one snapshot from the union membership file.
#[must_use]
pub fn attribute_set_for_snapshot(
    file: &AttributeMembershipFile,
    snapshot: SpecSnapshotId,
) -> BTreeSet<&str> {
    membership_set(file.attributes.iter(), snapshot)
}

fn membership_records<'a>(
    snapshots: impl Iterator<Item = (SpecSnapshotId, impl Iterator<Item = &'a str>)>,
) -> Vec<FeatureMembershipRecord> {
    let mut by_name: BTreeMap<String, Vec<SpecSnapshotId>> = BTreeMap::new();

    for (snapshot, names) in snapshots {
        for name in names {
            let present_in = by_name.entry(name.to_string()).or_default();
            if !present_in.contains(&snapshot) {
                present_in.push(snapshot);
            }
        }
    }

    by_name
        .into_iter()
        .map(|(name, present_in)| FeatureMembershipRecord { name, present_in })
        .collect()
}

fn membership_delta<'a>(
    from: impl Iterator<Item = &'a str>,
    to: impl Iterator<Item = &'a str>,
) -> MembershipDelta {
    let from_set: BTreeSet<&str> = from.collect();
    let to_set: BTreeSet<&str> = to.collect();

    MembershipDelta {
        added: to_set
            .difference(&from_set)
            .copied()
            .map(str::to_string)
            .collect(),
        removed: from_set
            .difference(&to_set)
            .copied()
            .map(str::to_string)
            .collect(),
    }
}

fn membership_set<'a>(
    records: impl Iterator<Item = &'a FeatureMembershipRecord>,
    snapshot: SpecSnapshotId,
) -> BTreeSet<&'a str> {
    records
        .filter(|record| record.present_in.contains(&snapshot))
        .map(|record| record.name.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        review::{ReviewInput, build_review},
        snapshot_schema::{
            CategoriesFile, ElementAttributeMatrixFile, ExceptionsFile, GrammarFile, ReviewFile,
            SnapshotAttributeRecord, SnapshotElementRecord,
        },
    };

    fn clean_review() -> ReviewFile {
        build_review(ReviewInput {
            elements: &[],
            attributes: &[],
            grammars: &GrammarFile {
                schema_version: 1,
                grammars: Vec::new(),
            },
            categories: &CategoriesFile {
                schema_version: 1,
                element_categories: Vec::new(),
                attribute_categories: Vec::new(),
            },
            element_attribute_matrix: &ElementAttributeMatrixFile {
                schema_version: 1,
                edges: Vec::new(),
            },
            exceptions: &ExceptionsFile {
                schema_version: 1,
                exceptions: Vec::new(),
            },
            manual_notes: &[],
        })
    }

    #[test]
    fn derivation_requires_all_canonical_snapshots() {
        let review = clean_review();
        let inputs = [ReviewedSnapshotMembershipInput {
            snapshot: SpecSnapshotId::Svg11Rec20030114,
            elements: &[] as &[SnapshotElementRecord],
            attributes: &[] as &[SnapshotAttributeRecord],
            review: &review,
        }];

        let Err(DeriveError::MissingSnapshots(missing)) = build_membership_artifacts(&inputs)
        else {
            panic!("expected missing snapshot error");
        };

        assert_eq!(missing.len(), spec_snapshots().len() - 1);
    }

    #[test]
    fn derivation_requires_clean_reviews() {
        let mut dirty_review = clean_review();
        dirty_review
            .unresolved
            .push(crate::snapshot_schema::ReviewIssue {
                id: String::from("still-dirty"),
                severity: crate::snapshot_schema::ReviewSeverity::Error,
                summary: String::from("still dirty"),
            });
        let clean_review = clean_review();
        let inputs: Vec<_> = spec_snapshots()
            .iter()
            .copied()
            .map(|snapshot| ReviewedSnapshotMembershipInput {
                snapshot,
                elements: &[] as &[SnapshotElementRecord],
                attributes: &[] as &[SnapshotAttributeRecord],
                review: if snapshot == SpecSnapshotId::Svg11Rec20030114 {
                    &dirty_review
                } else {
                    &clean_review
                },
            })
            .collect();

        let Err(DeriveError::SnapshotReviewNotClean {
            snapshot,
            unresolved,
        }) = build_membership_artifacts(&inputs)
        else {
            panic!("expected dirty review error");
        };

        assert_eq!(snapshot, SpecSnapshotId::Svg11Rec20030114);
        assert_eq!(unresolved, 1);
    }
}
