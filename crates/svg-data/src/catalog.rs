use crate::types::{
    AttributeDef, AttributeValues, BaselineQualifier, BaselineStatus, BrowserSupport,
    BrowserVersion, CompatVerdict, ContentModel, ElementCategory, ElementDef, RawVersionAdded,
    SpecLifecycle, SpecSnapshotId, VerdictReason, VerdictRecommendation,
};

include!(concat!(env!("OUT_DIR"), "/catalog.rs"));
