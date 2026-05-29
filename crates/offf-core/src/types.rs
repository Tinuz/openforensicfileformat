use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const OFFF_VERSION: &str = "0.1.0";
pub const OFFF_V2_VERSION: &str = "0.2.0";

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
    pub is_sparse: bool,
    pub is_compressed: bool,
    pub is_encrypted: bool,
    /// JSON array of alternate data stream names
    pub ads_streams: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_scope: Option<JobInputScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_contract: Option<JobOutputContract>,
    /// Reference to a `ScopeRecord` in `extensions/scopes/scopes.jsonl`.
    /// Workers must resolve this scope and enforce include/exclude rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_ref: Option<String>,
    /// Set IDs from `extensions/sets/` to restrict processing to.
    /// Only objects that are members of at least one listed set are processed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_sets: Vec<String>,
    /// External policy references (e.g. "policy:external:scope-001").
    /// Recorded for audit purposes; workers enforce via scope/set membership.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_refs: Vec<String>,
    /// Task-specific parameters (see per-task docs).
    pub parameters: serde_json::Value,
    /// Optional parallelisation config; absent means single-worker execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallelization: Option<ParallelizationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobScope {
    /// SHA-256 chunk IDs ("sha256:…") or `["*"]` for all chunks.
    pub chunks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInputScope {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_types: Vec<String>,
    #[serde(default)]
    pub selectors: JobInputSelectors,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<JobInputInclude>,
    #[serde(default)]
    pub exclude: JobInputExclude,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<JobInputLimits>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JobInputSelectors {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JobInputExclude {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobOutputContract {
    pub may_produce_results: bool,
    pub may_produce_objects: bool,
    pub may_materialize_objects: bool,
    pub may_produce_edges: bool,
    pub may_produce_derivations: bool,
}

// ── Object lineage rows (Phase 9) ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectSourceRef {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunk_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub root_chunk_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_object_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derivation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectStorageRef {
    pub storage_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredObjectRow {
    pub object_id: String,
    pub object_type: String,
    pub name: Option<String>,
    pub logical_path: Option<String>,
    pub media_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub sha256: Option<String>,
    pub source_layer: String,
    pub storage_ref: Option<String>,
    /// Legacy field: kept for backward compat. Prefer `root_id` for new containers.
    pub root_source_ref: Option<String>,
    /// Root collection object_id this evidence object belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
    /// Path relative to the collection root (e.g. "Documents/contract.docx").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_relative_path: Option<String>,
    pub created_by_job_id: Option<String>,
    pub parser_status: String,
    pub provenance_ref: Option<String>,
    pub schema_version: String,
    /// Original file timestamps preserved from the source filesystem.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_modified_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_accessed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectEdgeRow {
    pub edge_id: String,
    pub parent_object_id: String,
    pub child_object_id: String,
    pub relation_type: String,
    pub method: Option<String>,
    pub logical_path: Option<String>,
    pub sequence: Option<u64>,
    pub created_by_job_id: Option<String>,
    pub provenance_ref: Option<String>,
    pub schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivationRow {
    pub derivation_id: String,
    pub parent_object_id: String,
    pub child_object_id: String,
    pub job_id: String,
    pub method: String,
    pub tool_id: String,
    pub tool_name: String,
    pub tool_version: String,
    pub parameters_hash: Option<String>,
    pub input_sha256: Option<String>,
    pub output_sha256: Option<String>,
    pub storage_mode: String,
    pub provenance_ref: Option<String>,
    pub created_at: String,
    pub schema_version: String,
}

// ── Object event log (Sprint 19) ─────────────────────────────────────────────

/// Append-only event for object index state changes.
/// Stored in `indexes/objects/object_events.jsonl`.
///
/// Workers append events; the derived `object_index.jsonl` / Parquet index is
/// rebuilt deterministically via `offf-index objects --from-events`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectEvent {
    pub event_id: String,
    pub timestamp: String,
    /// "discovered" | "updated" | "removed"
    pub event_type: String,
    pub object_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    /// Full `DiscoveredObjectRow`-compatible payload for "discovered" / "updated".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    pub schema_version: String,
}

/// Append-only event for object edge state changes.
/// Stored in `indexes/objects/object_edge_events.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectEdgeEvent {
    pub event_id: String,
    pub timestamp: String,
    /// "discovered" | "removed"
    pub event_type: String,
    pub edge_id: String,
    pub source_object_id: String,
    pub target_object_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    pub schema_version: String,
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

// ── Generic extension types (Sprint 15) ──────────────────────────────────────

/// Generic target reference used in extension events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionTarget {
    /// "file" | "chunk" | "artifact" | "object" | "job" | "container" | "set" | "scope"
    #[serde(rename = "type")]
    pub target_type: String,
    pub id: String,
}

/// Append-only label event stored in `extensions/labels/labels.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelEvent {
    pub label_event_id: String,
    pub timestamp: String,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolInfo>,
    pub target: ExtensionTarget,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
}

/// Date-range filter used inside a scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

/// Inclusion filter for a scope.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScopeInclude {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunk_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_range: Option<DateRange>,
}

/// Exclusion filter for a scope.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScopeExclude {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<String>,
}

/// Scope record stored in `extensions/scopes/scopes.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeRecord {
    pub scope_id: String,
    pub created_at: String,
    pub created_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<ScopeInclude>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<ScopeExclude>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
}

/// Members of a set.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetMembers {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunk_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_ids: Vec<String>,
}

/// Set record stored in `extensions/sets/{working,release,exclusion}_sets.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRecord {
    pub set_id: String,
    /// "working_set" | "release_set" | "exclusion_set"
    pub set_type: String,
    pub created_at: String,
    pub created_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_ref: Option<String>,
    #[serde(default)]
    pub members: SetMembers,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
}

/// Actor reference in a decision record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionActor {
    /// "user" | "tool" | "system"
    #[serde(rename = "type")]
    pub actor_type: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Decision record stored in `extensions/decisions/decisions.jsonl`.
///
/// Generic decision types: `release`, `exclude`, `restrict`, `unrestrict`,
/// `review_required`, `review_completed`, `export_approved`, `export_denied`,
/// `processing_allowed`, `processing_denied`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub decision_id: String,
    pub timestamp: String,
    pub actor: DecisionActor,
    pub decision_type: String,
    pub target: ExtensionTarget,
    /// "approved" | "denied" | "pending"
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
}

/// Policy reference stored in `extensions/policies/policy_refs.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRef {
    pub policy_ref: String,
    /// "external" | "internal"
    pub policy_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// `sha256:…` hash of the attached policy document, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
}

/// Access event stored in `extensions/access/access_events.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessEvent {
    pub access_event_id: String,
    pub timestamp: String,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolInfo>,
    pub action: String,
    pub target: ExtensionTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_refs: Vec<String>,
    /// "allowed"
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
}

/// Denied access event stored in `extensions/access/denied_access_events.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeniedAccessEvent {
    pub denied_event_id: String,
    pub timestamp: String,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolInfo>,
    pub action: String,
    pub target: ExtensionTarget,
    /// "denied"
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
}

/// Generic audit event stored in `extensions/audit/audit_events.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub audit_event_id: String,
    pub timestamp: String,
    pub actor: String,
    pub event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ExtensionTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
}

// ── Acquisition mode ──────────────────────────────────────────────────────────

/// How the evidence was acquired and what its source structure is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionMode {
    /// Full byte-stream image of a storage medium (raw/dd/E01).
    BlockImage,
    /// Collection of individual files or directories seized as evidence.
    FileCollection,
    /// Logical extraction from a device, app, cloud service, or mailbox.
    LogicalExtraction,
    /// Export received via an API (cloud, SaaS, etc.).
    ApiExport,
    /// Multiple evidence roots with different acquisition modes.
    Mixed,
}

impl AcquisitionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BlockImage => "block_image",
            Self::FileCollection => "file_collection",
            Self::LogicalExtraction => "logical_extraction",
            Self::ApiExport => "api_export",
            Self::Mixed => "mixed",
        }
    }
}

/// Describes a single evidence root within a container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRoot {
    /// Unique identifier for this root (also the object_id of the root collection object).
    pub root_id: String,
    /// "file_collection" | "block_image" | "logical_extraction" | ...
    pub root_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_count: Option<u64>,
    /// Deterministic hash of the collection manifest (sorted object metadata).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_hash: Option<String>,
}

/// Source context recorded for selective acquisitions (file_collection etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquisitionSourceContext {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_root_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_reason: Option<String>,
}

// ── Manifest ──────────────────────────────────────────────────────────────────

/// Free-form extension entries, keyed by `namespace:name` strings.
///
/// Tools may attach arbitrary JSON objects under their own namespace.
/// Keys MUST follow the `namespace:name` pattern to be conformance-valid.
/// Only present in OFFF v0.2.0+ containers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManifestExtensions {
    #[serde(flatten)]
    pub entries: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestJson {
    pub offf_version: String,
    pub container_id: String,
    pub created_at: DateTime<Utc>,
    pub created_by_tool: ToolInfo,
    /// How the evidence was acquired.  Absent in legacy containers (implies block_image).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acquisition_mode: Option<AcquisitionMode>,
    /// Present when acquisition_mode is block_image (legacy or explicit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
    /// Present when acquisition_mode is block_image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashes: Option<ManifestHashes>,
    /// Present when acquisition_mode is block_image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunking: Option<ChunkingInfo>,
    /// Evidence roots for non-block-image acquisitions (or multi-root).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_roots: Option<Vec<EvidenceRoot>>,
    /// Explicit limitations of this acquisition (mandatory for file_collection).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limitations: Option<Vec<String>>,
    pub indexes: ManifestIndexes,
    /// Optional extension namespace entries (OFFF v0.2.0+).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<ManifestExtensions>,
}

impl ManifestJson {
    /// Returns the effective acquisition mode (defaults to BlockImage for legacy containers).
    pub fn effective_mode(&self) -> AcquisitionMode {
        self.acquisition_mode
            .clone()
            .unwrap_or(AcquisitionMode::BlockImage)
    }
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
    /// Path to physical_to_chunk.parquet; present for block_image containers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_to_chunk: Option<String>,
    /// Path to object_index.jsonl / .parquet; present for file_collection containers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_index: Option<String>,
    /// Path to object_edges.jsonl / .parquet; present for file_collection containers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_edges: Option<String>,
}

// ── Acquisition ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquisitionJson {
    pub container_id: String,
    /// Acquisition identifier (e.g. "acq-000001").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acquisition_id: Option<String>,
    /// How the evidence was acquired.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acquisition_mode: Option<String>,
    pub acquired_at: DateTime<Utc>,
    /// Person or system that performed the acquisition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acquired_by: Option<String>,
    /// Acquisition method description (e.g. "selected_file_collection").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    pub tool: ToolInfo,
    /// Source metadata; optional for file_collection (no single source file).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<AcquisitionSource>,
    /// Human context for selective acquisitions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_context: Option<AcquisitionSourceContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_container: Option<SourceContainerInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_stream: Option<EvidenceStreamInfo>,
    /// Parameters used during acquisition; optional for file_collection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<AcquisitionParameters>,
    /// Explicit limitations that apply to this acquisition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limitations: Option<Vec<String>>,
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

// ── Parallel processing support ───────────────────────────────────────────────

/// Strategy used when dividing an input list into shards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShardStrategy {
    /// Slice the sorted input list into contiguous equal-size ranges.
    DeterministicObjectIdRange,
    /// Assign input[i] to shard i % shard_count.
    DeterministicRoundRobin,
    /// Assign by sha256(object_id)[0] % shard_count.
    DeterministicHashModulo,
}

impl ShardStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DeterministicObjectIdRange => "deterministic_object_id_range",
            Self::DeterministicRoundRobin => "deterministic_round_robin",
            Self::DeterministicHashModulo => "deterministic_hash_modulo",
        }
    }
}

impl std::str::FromStr for ShardStrategy {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "deterministic_object_id_range" => Ok(Self::DeterministicObjectIdRange),
            "deterministic_round_robin" => Ok(Self::DeterministicRoundRobin),
            "deterministic_hash_modulo" => Ok(Self::DeterministicHashModulo),
            other => Err(format!("unknown shard strategy: {other}")),
        }
    }
}

/// Parallelisation configuration embedded in `JobManifest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelizationConfig {
    pub enabled: bool,
    /// "sharded" (only supported mode for now).
    pub mode: String,
    pub shard_strategy: ShardStrategy,
    pub shard_count: usize,
}

/// Inclusion filter for `JobInputScope.include`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobInputInclude {
    /// Filter by `DiscoveredObjectRow.object_type`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_types: Vec<String>,
    /// Filter by file extension derived from `name` / `logical_path`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// Filter by `DiscoveredObjectRow.media_type`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_types: Vec<String>,
    /// Filter by `DiscoveredObjectRow.parser_status`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parser_statuses: Vec<String>,
}

/// Size and other quantitative limits for `JobInputScope`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobInputLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_object_size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_object_size_bytes: Option<u64>,
}

/// Source references for an `AnalysisInputObject`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSourceRefs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
    /// "sha256:{hex}"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Path to the content-addressed evidence object (file_collection).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_ref: Option<String>,
}

/// Lightweight metadata attached to an `AnalysisInputObject`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputObjectMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

/// A single processable input object resolved from a `JobManifest` scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisInputObject {
    /// Stable identifier within this job; format "input-{N:06}" padded.
    pub input_id: String,
    /// "object" | "chunk" | "file" | "evidence_file" | "derived_object" | "artifact"
    pub input_type: String,
    pub object_id: String,
    pub source_refs: InputSourceRefs,
    pub metadata: InputObjectMetadata,
}

/// Compact reference to one input object stored inside a `ShardManifest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardInputRef {
    pub input_id: String,
    pub object_id: String,
}

/// Describes how the full input list was divided into shards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardPlanRecord {
    pub parent_job_id: String,
    pub shard_plan_id: String,
    pub strategy: ShardStrategy,
    pub shard_count: usize,
    pub input_count: usize,
    /// SHA-256 of the newline-joined sorted object_ids of all inputs.
    pub input_scope_hash: String,
    pub created_at: String,
    pub created_by: String,
}

/// Describes the subset of input objects assigned to one worker/shard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardManifest {
    pub shard_id: String,
    pub parent_job_id: String,
    pub shard_index: usize,
    pub shard_count: usize,
    pub input_scope_hash: String,
    pub input_objects: Vec<ShardInputRef>,
    /// Base output path for this shard's artifacts.
    pub output_base_path: String,
    /// "planned" | "in_progress" | "completed" | "failed"
    pub status: String,
}

/// Summary of shard input used in `ShardResultManifest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardInputSummary {
    pub input_scope_hash: String,
    pub objects_in_shard: u64,
}

/// Worker identity written by the worker itself into `ShardResultManifest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerIdentity {
    pub tool_id: String,
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_sha256: Option<String>,
}

/// Reference to one output artifact produced by a worker shard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub path: String,
    /// "sha256:{hex}"
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<String>,
}

/// Per-shard processing statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardStatistics {
    pub objects_in_scope: u64,
    pub objects_processed: u64,
    pub objects_success: u64,
    pub objects_error: u64,
    pub objects_skipped: u64,
}

/// Final result manifest written by a worker upon shard completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardResultManifest {
    /// Equals `shard_id` for shard result manifests.
    pub job_id: String,
    pub parent_job_id: String,
    pub shard_id: String,
    /// "completed" | "failed" | "partial"
    pub status: String,
    pub worker: WorkerIdentity,
    pub input: ShardInputSummary,
    pub outputs: Vec<ArtifactRef>,
    pub statistics: ShardStatistics,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// Summary of the parallelisation used in `ParentResultManifest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelizationSummary {
    pub mode: String,
    pub shard_count: usize,
    pub shards_completed: u64,
    pub shards_failed: u64,
}

/// Reference to one shard result stored in `ParentResultManifest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardResultRef {
    pub shard_id: String,
    pub result_manifest_path: String,
    /// "sha256:{hex}" of the shard_result_manifest.json file.
    pub sha256: String,
}

/// Coverage statistics aggregated across all shards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    pub parent_job_id: String,
    pub input_scope_hash: String,
    pub objects_in_scope: u64,
    pub objects_assigned_to_shards: u64,
    pub objects_processed: u64,
    pub objects_success: u64,
    pub objects_error: u64,
    pub objects_skipped: u64,
    pub duplicates_detected: u64,
    pub missing_inputs: u64,
}

/// Final parent manifest written by a finalizer after all shards complete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParentResultManifest {
    pub job_id: String,
    /// "completed" | "failed" | "partial"
    pub status: String,
    pub parallelization: ParallelizationSummary,
    pub shard_results: Vec<ShardResultRef>,
    pub coverage: CoverageReport,
    pub created_at: String,
}

/// Generic target reference used in worker error / skipped rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTarget {
    #[serde(rename = "type")]
    pub target_type: String,
    pub id: String,
}

/// Error codes for `WorkerErrorRow`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkerErrorCode {
    InputNotFound,
    InputOutOfScope,
    InputTooLarge,
    InputUnreadable,
    ChunkVerificationFailed,
    ObjectHashMismatch,
    UnsupportedInputType,
    ToolTimeout,
    ToolParseFailed,
    ToolInternalError,
    OutputWriteFailed,
    OutputHashMismatch,
    ScopeResolutionFailed,
    AuthorizationDenied,
}

/// Skip reason codes for `WorkerSkippedRow`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SkipReasonCode {
    ExcludedByLabel,
    ExcludedBySet,
    OutOfScope,
    TooLarge,
    UnsupportedType,
    DuplicateInput,
    PolicyDenied,
}

/// An error row written to `errors.jsonl` by a worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerErrorRow {
    pub error_id: String,
    pub parent_job_id: String,
    pub shard_id: String,
    pub target: WorkerTarget,
    /// Always "error".
    pub status: String,
    pub error_code: WorkerErrorCode,
    pub message: String,
    pub recoverable: bool,
    pub created_at: String,
}

/// A skipped row written to `skipped.jsonl` by a worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSkippedRow {
    pub skipped_id: String,
    pub parent_job_id: String,
    pub shard_id: String,
    pub target: WorkerTarget,
    /// Always "skipped".
    pub status: String,
    pub reason_code: SkipReasonCode,
    pub message: String,
    pub created_at: String,
}
