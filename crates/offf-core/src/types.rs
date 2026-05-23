use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const OFFF_VERSION: &str = "0.1.0";

// ── Partition table ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionTableJson {
    pub generated_at: DateTime<Utc>,
    pub generated_by_tool: ToolInfo,
    pub container_id: String,
    pub sector_size: u32,
    pub partition_table_type: String,
    /// GPT disk GUID (absent for MBR)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_guid: Option<String>,
    pub partitions: Vec<PartitionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionEntry {
    pub partition_id: String,
    /// Human-readable name (GPT only; None for MBR)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Human-readable type description
    pub partition_type: String,
    /// Partition type GUID (GPT only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_guid: Option<String>,
    /// Per-partition unique GUID (GPT only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_guid: Option<String>,
    /// Byte offset from the start of the disk image
    pub start_offset: u64,
    /// Length in bytes
    pub length: u64,
    pub first_lba: u64,
    pub last_lba: u64,
    /// GPT attribute flags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<u64>,
    /// MBR bootable flag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootable: Option<bool>,
    /// chunk_ids whose byte range overlaps this partition
    pub chunk_refs: Vec<String>,
    /// Detected filesystem type ("NTFS", "FAT32", "exFAT", …)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem_type: Option<String>,
}

// ── File index ────────────────────────────────────────────────────────────────

/// One row in `indexes/filesystems/<id>/file_index.parquet`.
#[derive(Debug, Clone)]
pub struct FileIndexRow {
    pub file_id: u64,
    pub filesystem_id: String,
    pub partition_id: String,
    /// Full logical path including filename
    pub path: String,
    pub filename: String,
    pub extension: String,
    pub size_bytes: u64,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
    pub accessed_at: Option<DateTime<Utc>>,
    pub changed_at: Option<DateTime<Utc>>,
    /// JSON: [{"offset": …, "length": …}, …]
    pub physical_extents: String,
    /// JSON: ["sha256:…", …]
    pub chunk_refs: String,
    pub is_directory: bool,
    pub is_deleted: bool,
    pub parser: String,
    pub parser_version: String,
    /// "ok" | "partial" | "error"
    pub parser_status: String,
    pub parser_error: String,
}
pub const TOOL_NAME: &str = env!("CARGO_PKG_NAME");
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

// ── Job manifest ──────────────────────────────────────────────────────────────

/// Serialised to `jobs/<job_id>.json` inside the OFFF container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobManifest {
    pub job_id: String,
    pub created_at: DateTime<Utc>,
    /// The container_id this job operates on.
    pub case_id: String,
    /// "keyword_scan" | "yara_scan"
    pub task: String,
    pub scope: JobScope,
    pub tool: ToolInfo,
    /// Task-specific parameters (see per-task docs).
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobScope {
    /// SHA-256 chunk IDs ("sha256:…") or `["*"]` for all chunks.
    pub chunks: Vec<String>,
}

// ── Analysis hit rows ─────────────────────────────────────────────────────────

/// One row in `analysis/keyword_hits.parquet`.
#[derive(Debug, Clone)]
pub struct KeywordHitRow {
    pub hit_id: String,
    pub job_id: String,
    pub keyword: String,
    pub chunk_id: String,
    /// Absolute byte offset from the start of the disk image.
    pub physical_offset: u64,
    /// file_id from file_index if resolved; empty string otherwise.
    pub file_id: String,
    /// Up to 32 bytes before the hit, hex-encoded.
    pub context_before: String,
    /// Up to 32 bytes after the hit, hex-encoded.
    pub context_after: String,
    /// "utf-8" | "utf-16le"
    pub encoding: String,
    pub worker_id: String,
    pub timestamp: String,
}

/// One row in `analysis/yara_hits.parquet`.
#[derive(Debug, Clone)]
pub struct YaraHitRow {
    pub hit_id: String,
    pub job_id: String,
    pub rule_name: String,
    /// SHA-256 of the ruleset source text.
    pub ruleset_hash: String,
    pub chunk_id: String,
    /// Absolute byte offset from the start of the disk image.
    pub physical_offset: u64,
    pub match_length: u64,
    /// file_id from file_index if resolved; empty string otherwise.
    pub file_id: String,
    pub worker_id: String,
    pub timestamp: String,
}

// ── Annotation layer (Phase 6) ───────────────────────────────────────────────

/// Logical target for an annotation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationTarget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
}

/// Append-only annotation event stored in `analysis/annotations.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationEvent {
    pub annotation_id: String,
    pub timestamp: String,
    pub actor: String,
    /// "human" | "ai"
    pub origin: String,
    /// e.g. "relevance_label", "classification", "correction"
    pub annotation_type: String,
    pub target: AnnotationTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classification: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correction_of: Option<String>,
}

// ── Manifest ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestJson {
    pub offf_version: String,
    pub container_id: String,
    pub created_at: DateTime<Utc>,
    pub created_by_tool: ToolInfo,
    pub source: SourceInfo,
    pub hashes: ManifestHashes,
    pub chunking: ChunkingInfo,
    pub indexes: ManifestIndexes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    #[serde(rename = "type")]
    pub source_type: String,
    pub size_bytes: u64,
    pub sector_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestHashes {
    pub source_sha256: String,
    pub merkle_root_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingInfo {
    pub chunk_size: u64,
    pub chunking_mode: String,
    pub compression: String,
    pub hash_algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestIndexes {
    pub physical_to_chunk: String,
}

// ── Acquisition ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquisitionJson {
    pub container_id: String,
    pub acquired_at: DateTime<Utc>,
    pub tool: ToolInfo,
    pub source: AcquisitionSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_container: Option<SourceContainerInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_stream: Option<EvidenceStreamInfo>,
    pub parameters: AcquisitionParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquisitionSource {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquisitionParameters {
    pub chunk_size: u64,
    pub sector_size: u32,
    pub compression: String,
    pub hash_algorithm: String,
    pub deterministic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceContainerInfo {
    #[serde(rename = "type")]
    pub container_type: String,
    pub container_sha256: String,
    pub tool_used: String,
    pub conversion_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceStreamInfo {
    pub stream_sha256: String,
}

// ── Chunk metadata ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub sequence: u64,
    pub chunk_id: String,
    pub source_offset: u64,
    pub source_length: u64,
    pub stored_length: u64,
    pub compression: String,
    pub plaintext_sha256: String,
    pub stored_sha256: String,
    pub read_errors: Vec<ReadError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadError {
    pub source_offset: u64,
    pub length: u64,
    pub error: String,
    pub fill_policy: String,
    pub device_reported_error: Option<String>,
}

// ── Compression ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compression {
    None,
    Zstd,
}

impl Compression {
    pub fn as_str(&self) -> &'static str {
        match self {
            Compression::None => "none",
            Compression::Zstd => "zstd",
        }
    }
}

impl std::str::FromStr for Compression {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Compression::None),
            "zstd" => Ok(Compression::Zstd),
            other => Err(format!("unknown compression: {other}")),
        }
    }
}
