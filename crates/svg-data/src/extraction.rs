//! Shared extraction helpers for checked-in per-snapshot SVG data.

use std::{
    error::Error as StdError,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    snapshot_schema::{
        CategoriesFile, ElementAttributeMatrixFile, ExceptionsFile, ExtractionConfidence,
        FactProvenance, GrammarFile, IngestionMetadata, ProvenanceSourceKind, ReviewFile,
        SNAPSHOT_SCHEMA_VERSION, SnapshotAttributeRecord, SnapshotElementRecord,
        SnapshotMetadataFile, SnapshotSourceRef, SnapshotStatus, SourceAuthority, SourceLocator,
        SourcePin,
    },
    types::SpecSnapshotId,
};

/// Current schema version for checked-in source manifests.
pub const SOURCE_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Shared result type for extraction helpers.
pub type Result<T> = std::result::Result<T, Error>;

/// Manifest-driven extraction error.
#[derive(Debug)]
pub enum Error {
    /// Failed file IO.
    Io {
        /// Path involved in the operation.
        path: PathBuf,
        /// Source IO failure.
        source: io::Error,
    },
    /// Failed TOML parsing.
    ManifestParse {
        /// Manifest path.
        path: PathBuf,
        /// Parse failure.
        source: toml::de::Error,
    },
    /// Failed JSON serialization.
    Json {
        /// Output path.
        path: PathBuf,
        /// Serialization failure.
        source: serde_json::Error,
    },
    /// Snapshot-only operation was attempted on a manifest without a snapshot id.
    MissingSnapshot {
        /// Manifest id.
        manifest_id: String,
    },
    /// Input id was not present in the manifest.
    MissingInput {
        /// Manifest id.
        manifest_id: String,
        /// Missing input id.
        input_id: String,
    },
    /// Manifest status cannot map to a tracked snapshot status.
    UnsupportedSnapshotStatus {
        /// Manifest id.
        manifest_id: String,
        /// Raw status value.
        status: SourceManifestStatus,
    },
    /// Offline mode prevented a required fetch.
    OfflineCacheMiss {
        /// Missing cache path.
        path: PathBuf,
    },
    /// The provided cache path must be relative.
    AbsoluteCachePath {
        /// Invalid path.
        path: PathBuf,
    },
    /// A caller-provided fetcher failed.
    Fetch {
        /// Logical cache key.
        cache_key: PathBuf,
        /// Failure detail.
        message: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "io error at {}: {source}", path.display())
            }
            Self::ManifestParse { path, source } => {
                write!(
                    formatter,
                    "failed to parse manifest {}: {source}",
                    path.display()
                )
            }
            Self::Json { path, source } => {
                write!(
                    formatter,
                    "failed to write json {}: {source}",
                    path.display()
                )
            }
            Self::MissingSnapshot { manifest_id } => {
                write!(formatter, "manifest {manifest_id} has no snapshot id")
            }
            Self::MissingInput {
                manifest_id,
                input_id,
            } => {
                write!(formatter, "manifest {manifest_id} missing input {input_id}")
            }
            Self::UnsupportedSnapshotStatus {
                manifest_id,
                status,
            } => write!(
                formatter,
                "manifest {manifest_id} status {status} is not a tracked snapshot status"
            ),
            Self::OfflineCacheMiss { path } => {
                write!(formatter, "offline cache miss for {}", path.display())
            }
            Self::AbsoluteCachePath { path } => {
                write!(formatter, "cache key must be relative: {}", path.display())
            }
            Self::Fetch { cache_key, message } => {
                write!(
                    formatter,
                    "fetch failed for {}: {message}",
                    cache_key.display()
                )
            }
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::ManifestParse { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            Self::MissingSnapshot { .. }
            | Self::MissingInput { .. }
            | Self::UnsupportedSnapshotStatus { .. }
            | Self::OfflineCacheMiss { .. }
            | Self::AbsoluteCachePath { .. }
            | Self::Fetch { .. } => None,
        }
    }
}

/// Checked-in manifest describing one snapshot or external pin set.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SourceManifest {
    /// Schema version for the manifest file.
    pub schema_version: u32,
    /// Stable manifest id.
    pub manifest_id: String,
    /// Snapshot id if this manifest drives a checked-in snapshot dataset.
    #[serde(default)]
    pub snapshot: Option<SpecSnapshotId>,
    /// Human-readable title.
    pub title: String,
    /// Source status classification.
    pub status: SourceManifestStatus,
    /// Authority precedence strategy.
    pub authority_policy: SourceAuthorityPolicy,
    /// Top-level authority descriptor when one exists.
    #[serde(default)]
    pub authority: Option<ManifestAuthority>,
    /// Top-level pin when one exists.
    #[serde(default)]
    pub pin: Option<ManifestPin>,
    /// Checksum policy.
    pub checksum: ManifestChecksum,
    /// Fetch policy.
    pub fetch: ManifestFetch,
    /// Inputs consumed by the extractor.
    pub inputs: Vec<SourceManifestInput>,
}

impl SourceManifest {
    /// Read and parse one checked-in source manifest.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed as a source manifest.
    pub fn read(path: &Path) -> Result<Self> {
        let manifest_text = fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&manifest_text).map_err(|source| Error::ManifestParse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Resolve one declared input by id.
    #[must_use]
    pub fn input(&self, input_id: &str) -> Option<&SourceManifestInput> {
        self.inputs.iter().find(|input| input.id() == input_id)
    }

    /// Build the pinned source reference stored in `snapshot.json`.
    ///
    /// # Errors
    /// Returns an error if `input_id` is not declared by the manifest.
    pub fn source_ref(&self, input_id: &str) -> Result<SnapshotSourceRef> {
        let input = self.input(input_id).ok_or_else(|| Error::MissingInput {
            manifest_id: self.manifest_id.clone(),
            input_id: input_id.into(),
        })?;

        Ok(SnapshotSourceRef {
            manifest_id: self.manifest_id.clone(),
            input_id: input.id().into(),
            authority: input.authority_classification(),
            pin: input.source_pin(self.pin.as_ref(), self.authority.as_ref()),
        })
    }

    /// Build the canonical `snapshot.json` payload from a snapshot manifest.
    ///
    /// # Errors
    /// Returns an error if the manifest is not snapshot-backed or any source ref cannot be resolved.
    pub fn snapshot_metadata(
        &self,
        extractor_version: impl Into<String>,
        normalized_at: impl Into<String>,
    ) -> Result<SnapshotMetadataFile> {
        let snapshot = self.snapshot.ok_or_else(|| Error::MissingSnapshot {
            manifest_id: self.manifest_id.clone(),
        })?;

        Ok(SnapshotMetadataFile {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            snapshot,
            title: self.title.clone(),
            date: self.pin_date().unwrap_or_default().into(),
            status: self.snapshot_status()?,
            pinned_sources: self
                .inputs
                .iter()
                .map(|input| self.source_ref(input.id()))
                .collect::<Result<Vec<_>>>()?,
            ingestion: IngestionMetadata {
                extractor_version: extractor_version.into(),
                normalized_at: normalized_at.into(),
            },
        })
    }

    /// Build reusable fact provenance for one manifest input.
    ///
    /// # Errors
    /// Returns an error if `input_id` is not declared by the manifest.
    pub fn fact_provenance(
        &self,
        input_id: &str,
        source_kind: ProvenanceSourceKind,
        locator: SourceLocator,
        confidence: ExtractionConfidence,
    ) -> Result<FactProvenance> {
        let input = self.input(input_id).ok_or_else(|| Error::MissingInput {
            manifest_id: self.manifest_id.clone(),
            input_id: input_id.into(),
        })?;

        Ok(FactProvenance {
            source_id: input.id().into(),
            source_kind,
            pin: input.source_pin(self.pin.as_ref(), self.authority.as_ref()),
            locator,
            confidence,
        })
    }

    fn pin_date(&self) -> Option<&str> {
        self.pin.as_ref().map(|pin| pin.date.as_str())
    }

    fn snapshot_status(&self) -> Result<SnapshotStatus> {
        match self.status {
            SourceManifestStatus::Recommendation => Ok(SnapshotStatus::Recommendation),
            SourceManifestStatus::CandidateRecommendation => {
                Ok(SnapshotStatus::CandidateRecommendation)
            }
            SourceManifestStatus::EditorsDraft => Ok(SnapshotStatus::EditorsDraft),
            SourceManifestStatus::Mixed => Err(Error::UnsupportedSnapshotStatus {
                manifest_id: self.manifest_id.clone(),
                status: self.status,
            }),
        }
    }
}

/// Top-level source status classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum SourceManifestStatus {
    /// Recommendation-class snapshot.
    #[serde(rename = "REC")]
    Recommendation,
    /// Candidate recommendation snapshot.
    #[serde(rename = "CR")]
    CandidateRecommendation,
    /// Editor's draft snapshot.
    #[serde(rename = "ED")]
    EditorsDraft,
    /// Mixed-status foreign pin set.
    #[serde(rename = "mixed")]
    Mixed,
}

impl fmt::Display for SourceManifestStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Recommendation => formatter.write_str("REC"),
            Self::CandidateRecommendation => formatter.write_str("CR"),
            Self::EditorsDraft => formatter.write_str("ED"),
            Self::Mixed => formatter.write_str("mixed"),
        }
    }
}

/// Authority precedence policy for a source manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum SourceAuthorityPolicy {
    /// Prefer the dated W3C TR snapshot.
    #[serde(rename = "tr-first")]
    TrFirst,
    /// Prefer a pinned repository commit.
    #[serde(rename = "git-first")]
    GitFirst,
    /// Typed foreign reference set.
    #[serde(rename = "typed-foreign-refs")]
    TypedForeignRefs,
}

/// Top-level authority metadata for a manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ManifestAuthority {
    /// Authority kind.
    pub kind: String,
    /// Human-readable authority title.
    pub title: String,
    /// Canonical authority URL.
    pub url: String,
}

/// Top-level manifest pin.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ManifestPin {
    /// Pin kind.
    pub kind: ManifestPinKind,
    /// Exact pinned value.
    pub value: String,
    /// Snapshot or pin date.
    pub date: String,
}

/// Manifest pin kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ManifestPinKind {
    /// Pinned absolute URL.
    #[serde(rename = "dated-url")]
    DatedUrl,
    /// Pinned repository commit hash.
    #[serde(rename = "git-commit")]
    GitCommit,
}

/// Checksum policy declared by a manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ManifestChecksum {
    /// Checksum strategy name.
    pub strategy: String,
    /// Human note about the checksum policy.
    pub note: String,
}

/// Fetch policy declared by a manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ManifestFetch {
    /// Fetch policy name.
    pub policy: String,
    /// Relative cache subdirectory.
    pub cache_subdir: PathBuf,
    /// Whether live refresh is allowed.
    pub allow_live_refresh: bool,
}

/// One manifest input, either snapshot-native or foreign-reference-only.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum SourceManifestInput {
    /// Snapshot-native input.
    Snapshot(SnapshotManifestInput),
    /// Foreign-reference-only input.
    Foreign(ForeignManifestInput),
}

impl SourceManifestInput {
    /// Stable input id.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Snapshot(input) => &input.id,
            Self::Foreign(input) => &input.id,
        }
    }

    const fn authority_classification(&self) -> SourceAuthority {
        match self {
            Self::Snapshot(input) => input.authority.into_source_authority(),
            Self::Foreign(_) => SourceAuthority::ForeignReference,
        }
    }

    fn source_pin(
        &self,
        manifest_pin: Option<&ManifestPin>,
        authority: Option<&ManifestAuthority>,
    ) -> SourcePin {
        match self {
            Self::Snapshot(input) => manifest_pin.map_or_else(
                || SourcePin::Url { url: String::new() },
                |pin| match pin.kind {
                    ManifestPinKind::DatedUrl => SourcePin::Url {
                        url: pin.value.clone(),
                    },
                    ManifestPinKind::GitCommit => SourcePin::GitCommit {
                        repository: authority.map_or_else(String::new, |manifest_authority| {
                            manifest_authority.url.clone()
                        }),
                        commit: pin.value.clone(),
                        path: input.path.clone(),
                    },
                },
            ),
            Self::Foreign(input) => SourcePin::Url {
                url: input.pin.clone(),
            },
        }
    }
}

/// Snapshot-native manifest input.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SnapshotManifestInput {
    /// Stable input id.
    pub id: String,
    /// Raw source kind.
    pub kind: String,
    /// Fetch locator.
    pub locator: String,
    /// Human-readable role.
    pub role: String,
    /// Canonical authority classification — drives provenance gating rather
    /// than parsing the free-form `role` string.
    pub authority: SnapshotInputAuthority,
    /// Optional repo-relative path for git-backed manifests, so per-input
    /// `SourcePin::GitCommit { path }` stays distinguishable when several
    /// inputs share the same commit.
    #[serde(default)]
    pub path: Option<String>,
}

/// Authority classification for a snapshot-native manifest input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotInputAuthority {
    /// Normative or canonical-inventory source.
    Primary,
    /// Assistive or cross-check source — does not override authority.
    Supporting,
}

impl SnapshotInputAuthority {
    const fn into_source_authority(self) -> SourceAuthority {
        match self {
            Self::Primary => SourceAuthority::Primary,
            Self::Supporting => SourceAuthority::Supporting,
        }
    }
}

/// Foreign pinned reference declared as an input.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ForeignManifestInput {
    /// Stable input id.
    pub id: String,
    /// Human-readable authority.
    pub authority: String,
    /// Raw foreign source kind.
    pub kind: String,
    /// Scope summary.
    pub scope: String,
    /// Pinned external target.
    pub pin: String,
    /// Human-readable role.
    pub role: String,
}

/// Shared cache manager for extracted source inputs.
#[derive(Debug, Clone)]
pub struct Cache {
    root: PathBuf,
    offline: bool,
}

impl Cache {
    /// Create a cache rooted at `root`.
    #[must_use]
    pub const fn new(root: PathBuf, offline: bool) -> Self {
        Self { root, offline }
    }

    /// Read cached bytes or fetch and persist them when missing.
    ///
    /// # Errors
    /// Returns an error if the cache key is absolute, cache reads/writes fail,
    /// offline mode blocks a missing fetch, or the caller fetcher fails.
    pub fn load_or_fetch_bytes<F>(
        &self,
        cache_key: &Path,
        force_refresh: bool,
        fetcher: F,
    ) -> Result<CachedInput>
    where
        F: FnOnce() -> std::result::Result<Vec<u8>, String>,
    {
        if cache_key.is_absolute() {
            return Err(Error::AbsoluteCachePath {
                path: cache_key.to_path_buf(),
            });
        }

        let cache_path = self.root.join(cache_key);
        let checksum_path = checksum_path_for(&cache_path);

        if !force_refresh && cache_path.exists() {
            let bytes = fs::read(&cache_path).map_err(|source| Error::Io {
                path: cache_path.clone(),
                source,
            })?;
            let checksum = sha256_hex(&bytes);
            if !checksum_path.exists() {
                write_bytes_with_parents(&checksum_path, checksum.as_bytes())?;
            }

            return Ok(CachedInput {
                cache_path,
                checksum_path,
                bytes,
                checksum_sha256: checksum,
                cache_hit: true,
            });
        }

        if self.offline {
            return Err(Error::OfflineCacheMiss { path: cache_path });
        }

        let bytes = fetcher().map_err(|message| Error::Fetch {
            cache_key: cache_key.to_path_buf(),
            message,
        })?;
        let checksum = sha256_hex(&bytes);
        write_bytes_with_parents(&cache_path, &bytes)?;
        write_bytes_with_parents(&checksum_path, checksum.as_bytes())?;

        Ok(CachedInput {
            cache_path,
            checksum_path,
            bytes,
            checksum_sha256: checksum,
            cache_hit: false,
        })
    }

    /// Read cached UTF-8 text or fetch and persist it when missing.
    ///
    /// # Errors
    /// Returns an error if byte caching fails or the cached body is not valid UTF-8.
    pub fn load_or_fetch_text<F>(
        &self,
        cache_key: &Path,
        force_refresh: bool,
        fetcher: F,
    ) -> Result<CachedText>
    where
        F: FnOnce() -> std::result::Result<String, String>,
    {
        let cached = self.load_or_fetch_bytes(cache_key, force_refresh, || {
            fetcher().map(String::into_bytes)
        })?;

        let text = String::from_utf8(cached.bytes).map_err(|error| Error::Fetch {
            cache_key: cache_key.to_path_buf(),
            message: error.to_string(),
        })?;

        Ok(CachedText {
            cache_path: cached.cache_path,
            checksum_path: cached.checksum_path,
            text,
            checksum_sha256: cached.checksum_sha256,
            cache_hit: cached.cache_hit,
        })
    }
}

/// Cached binary payload with checksum metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedInput {
    /// Cache file path.
    pub cache_path: PathBuf,
    /// Sidecar checksum path.
    pub checksum_path: PathBuf,
    /// Cached bytes.
    pub bytes: Vec<u8>,
    /// Hex SHA-256 digest string.
    pub checksum_sha256: String,
    /// Whether the cache was reused.
    pub cache_hit: bool,
}

/// Cached text payload with checksum metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedText {
    /// Cache file path.
    pub cache_path: PathBuf,
    /// Sidecar checksum path.
    pub checksum_path: PathBuf,
    /// Cached UTF-8 text.
    pub text: String,
    /// Hex digest string.
    pub checksum_sha256: String,
    /// Whether the cache was reused.
    pub cache_hit: bool,
}

/// Full typed payload for one checked-in snapshot dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotDataset {
    /// `snapshot.json` payload.
    pub metadata: SnapshotMetadataFile,
    /// `elements.json` payload.
    pub elements: Vec<SnapshotElementRecord>,
    /// `attributes.json` payload.
    pub attributes: Vec<SnapshotAttributeRecord>,
    /// `grammars.json` payload.
    pub grammars: GrammarFile,
    /// `categories.json` payload.
    pub categories: CategoriesFile,
    /// `element_attribute_matrix.json` payload.
    pub element_attribute_matrix: ElementAttributeMatrixFile,
    /// `exceptions.json` payload.
    pub exceptions: ExceptionsFile,
    /// `review.json` payload.
    pub review: ReviewFile,
}

/// Deterministic writer for checked-in snapshot artifacts.
#[derive(Debug, Clone)]
pub struct SnapshotDatasetWriter {
    root: PathBuf,
}

impl SnapshotDatasetWriter {
    /// Create a writer rooted at the `data/specs` directory.
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Write one snapshot dataset to `root/<snapshot-id>/` with stable JSON formatting.
    ///
    /// # Errors
    /// Returns an error if the output directory cannot be created or any JSON file cannot be serialized or written.
    pub fn write(&self, dataset: &SnapshotDataset) -> Result<Vec<PathBuf>> {
        let snapshot_root = self.root.join(dataset.metadata.snapshot.as_str());
        fs::create_dir_all(&snapshot_root).map_err(|source| Error::Io {
            path: snapshot_root.clone(),
            source,
        })?;

        let writes = [
            write_json_file(snapshot_root.join("snapshot.json"), &dataset.metadata),
            write_json_file(snapshot_root.join("elements.json"), &dataset.elements),
            write_json_file(snapshot_root.join("attributes.json"), &dataset.attributes),
            write_json_file(snapshot_root.join("grammars.json"), &dataset.grammars),
            write_json_file(snapshot_root.join("categories.json"), &dataset.categories),
            write_json_file(
                snapshot_root.join("element_attribute_matrix.json"),
                &dataset.element_attribute_matrix,
            ),
            write_json_file(snapshot_root.join("exceptions.json"), &dataset.exceptions),
            write_json_file(snapshot_root.join("review.json"), &dataset.review),
        ];

        writes.into_iter().collect()
    }
}

fn write_json_file<T>(path: PathBuf, value: &T) -> Result<PathBuf>
where
    T: Serialize,
{
    let mut json = serde_json::to_string_pretty(value).map_err(|source| Error::Json {
        path: path.clone(),
        source,
    })?;
    json.push('\n');
    write_bytes_with_parents(&path, json.as_bytes())?;
    Ok(path)
}

fn write_bytes_with_parents(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    fs::write(path, bytes).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn checksum_path_for(cache_path: &Path) -> PathBuf {
    let extension = cache_path.extension().map_or_else(
        || String::from("sha256"),
        |ext| format!("{}.sha256", ext.to_string_lossy()),
    );
    cache_path.with_extension(extension)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    let bytes = digest.finalize();

    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = fmt::Write::write_fmt(&mut hex, format_args!("{byte:02x}"));
    }
    hex
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::snapshot_schema::{
        AnimationBehavior, ApplicabilityCoverage, AttributeDefaultValue, AttributeRequirement,
        ElementContentModel, ExceptionInventory, ProvenanceCoverage, ProvenanceCoverageCount,
        ReviewCounts, ValueSyntax,
    };

    fn temp_dir_path() -> Result<tempfile::TempDir> {
        tempdir().map_err(|source| Error::Io {
            path: PathBuf::from("tempdir"),
            source,
        })
    }

    fn sample_snapshot_element(manifest: &SourceManifest) -> Result<SnapshotElementRecord> {
        Ok(SnapshotElementRecord {
            name: String::from("svg"),
            title: String::from("SVG root element"),
            categories: vec![String::from("container")],
            content_model: ElementContentModel::AnySvg,
            attributes: vec![String::from("width")],
            provenance: vec![manifest.fact_provenance(
                "tr-root",
                ProvenanceSourceKind::Html,
                SourceLocator::Fragment {
                    anchor: String::from("SVGElement"),
                },
                ExtractionConfidence::Exact,
            )?],
        })
    }

    fn sample_snapshot_attribute(manifest: &SourceManifest) -> Result<SnapshotAttributeRecord> {
        Ok(SnapshotAttributeRecord {
            name: String::from("width"),
            title: String::from("Width attribute"),
            value_syntax: ValueSyntax::Opaque {
                display: String::from("<length>"),
                reason: String::from("grammar not normalized yet"),
            },
            default_value: AttributeDefaultValue::None,
            animatable: AnimationBehavior::Unspecified,
            provenance: vec![manifest.fact_provenance(
                "attribute-index",
                ProvenanceSourceKind::Index,
                SourceLocator::Fragment {
                    anchor: String::from("width"),
                },
                ExtractionConfidence::Derived,
            )?],
        })
    }

    fn sample_review_file() -> ReviewFile {
        ReviewFile {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            counts: ReviewCounts {
                elements: 1,
                attributes: 1,
                grammars: 0,
                applicability_edges: 1,
                exceptions: 0,
            },
            applicability: ApplicabilityCoverage {
                elements_requiring_matrix_entries: 1,
                elements_with_matrix_entries: 1,
                elements_missing_matrix_entries: Vec::new(),
            },
            provenance: ProvenanceCoverage {
                elements: ProvenanceCoverageCount {
                    total: 1,
                    covered: 1,
                    missing: 0,
                },
                attributes: ProvenanceCoverageCount {
                    total: 1,
                    covered: 1,
                    missing: 0,
                },
                grammars: ProvenanceCoverageCount {
                    total: 0,
                    covered: 0,
                    missing: 0,
                },
                element_categories: ProvenanceCoverageCount {
                    total: 0,
                    covered: 0,
                    missing: 0,
                },
                attribute_categories: ProvenanceCoverageCount {
                    total: 0,
                    covered: 0,
                    missing: 0,
                },
                applicability_edges: ProvenanceCoverageCount {
                    total: 1,
                    covered: 0,
                    missing: 1,
                },
                exceptions: ProvenanceCoverageCount {
                    total: 0,
                    covered: 0,
                    missing: 0,
                },
            },
            exception_inventory: ExceptionInventory {
                total: 0,
                corrected: 0,
                deferred: 0,
                snapshot_scoped: 0,
                element_scoped: 0,
                attribute_scoped: 0,
                element_attribute_scoped: 0,
                grammar_scoped: 0,
                ids: Vec::new(),
            },
            unresolved: Vec::new(),
            manual_notes: Vec::new(),
        }
    }

    fn sample_snapshot_dataset(manifest: &SourceManifest) -> Result<SnapshotDataset> {
        Ok(SnapshotDataset {
            metadata: manifest.snapshot_metadata("extractor-v1", "2026-04-09")?,
            elements: vec![sample_snapshot_element(manifest)?],
            attributes: vec![sample_snapshot_attribute(manifest)?],
            grammars: GrammarFile {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                grammars: Vec::new(),
            },
            categories: CategoriesFile {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                element_categories: Vec::new(),
                attribute_categories: Vec::new(),
            },
            element_attribute_matrix: ElementAttributeMatrixFile {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                edges: vec![crate::snapshot_schema::ElementAttributeEdge {
                    element: String::from("svg"),
                    attribute: String::from("width"),
                    requirement: AttributeRequirement::Optional,
                    provenance: Vec::new(),
                }],
            },
            exceptions: ExceptionsFile {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                exceptions: Vec::new(),
            },
            review: sample_review_file(),
        })
    }

    #[test]
    fn parses_checked_in_snapshot_manifest_and_builds_metadata() -> Result<()> {
        let manifest = SourceManifest::read(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("data/sources/svg2-ed-20250914.toml"),
        )?;

        assert_eq!(manifest.schema_version, SOURCE_MANIFEST_SCHEMA_VERSION);
        assert_eq!(
            manifest.snapshot,
            Some(SpecSnapshotId::Svg2EditorsDraft20250914)
        );

        let metadata = manifest.snapshot_metadata("extractor-v1", "2026-04-09")?;
        assert_eq!(metadata.snapshot, SpecSnapshotId::Svg2EditorsDraft20250914);
        assert_eq!(metadata.pinned_sources.len(), manifest.inputs.len());
        assert_eq!(metadata.status, SnapshotStatus::EditorsDraft);
        Ok(())
    }

    #[test]
    fn git_backed_snapshot_pins_preserve_per_input_path_and_authority() -> Result<()> {
        let manifest = SourceManifest::read(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("data/sources/svg2-ed-20250914.toml"),
        )?;

        // `definitions` is the primary structured-data input; its pin must
        // point at `master/definitions.xml` within the pinned commit so
        // snapshot.json can distinguish it from `publish.xml` and friends.
        let definitions = manifest.source_ref("definitions")?;
        assert_eq!(definitions.authority, SourceAuthority::Primary);
        match &definitions.pin {
            SourcePin::GitCommit {
                path: Some(path), ..
            } => assert_eq!(path, "master/definitions.xml"),
            other => panic!("expected GitCommit pin with path, got {other:?}"),
        }

        // `chapter-html` is a prose cross-check — no per-input file path,
        // and must classify as Supporting (not Primary) despite the role
        // string containing no "normative" substring.
        let chapter_html = manifest.source_ref("chapter-html")?;
        assert_eq!(chapter_html.authority, SourceAuthority::Supporting);
        match &chapter_html.pin {
            SourcePin::GitCommit { path, .. } => assert_eq!(path, &None),
            other @ SourcePin::Url { .. } => panic!("expected GitCommit pin, got {other:?}"),
        }

        // Every git-pinned definitions input round-trips a distinct path.
        let mut paths: Vec<String> = Vec::new();
        for id in ["publish-conf", "definitions", "definitions-filters"] {
            match manifest.source_ref(id)?.pin {
                SourcePin::GitCommit {
                    path: Some(path), ..
                } => paths.push(path),
                other => panic!("expected GitCommit pin with path for {id}, got {other:?}"),
            }
        }
        let distinct: std::collections::HashSet<_> = paths.iter().collect();
        assert_eq!(distinct.len(), 3, "per-input paths must be distinct");
        Ok(())
    }

    #[test]
    fn cache_reuses_existing_text_in_offline_mode() -> Result<()> {
        let temp_dir = temp_dir_path()?;
        let online_cache = Cache::new(temp_dir.path().join("cache"), false);
        let cache_key = Path::new("spec-sources/svg11-rec-20030114/eltindex.html");

        let first =
            online_cache.load_or_fetch_text(cache_key, false, || Ok(String::from("<html/>")))?;
        assert!(!first.cache_hit);

        let offline_cache = Cache::new(temp_dir.path().join("cache"), true);
        let second = offline_cache.load_or_fetch_text(cache_key, false, || {
            Err(String::from("should not fetch when cache exists"))
        })?;

        assert!(second.cache_hit);
        assert_eq!(second.text, "<html/>");
        assert!(second.checksum_path.exists());
        Ok(())
    }

    #[test]
    fn writes_snapshot_dataset_files_with_stable_names() -> Result<()> {
        let temp_dir = temp_dir_path()?;
        let writer = SnapshotDatasetWriter::new(temp_dir.path().join("specs"));
        let manifest = SourceManifest::read(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("data/sources/svg11-rec-20030114.toml"),
        )?;
        let dataset = sample_snapshot_dataset(&manifest)?;

        let paths = writer.write(&dataset)?;
        assert_eq!(paths.len(), 8);
        for file_name in [
            "snapshot.json",
            "elements.json",
            "attributes.json",
            "grammars.json",
            "categories.json",
            "element_attribute_matrix.json",
            "exceptions.json",
            "review.json",
        ] {
            assert!(
                temp_dir
                    .path()
                    .join("specs/Svg11Rec20030114")
                    .join(file_name)
                    .exists()
            );
        }

        Ok(())
    }
}
