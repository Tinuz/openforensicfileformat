pub mod chunk;
pub mod error;
pub mod evidence;
pub mod extensions;
pub mod hash;
pub mod lineage;
pub mod ntfs;
pub mod packed;
pub mod parquet_io;
pub mod partition;
pub mod provenance;
pub mod scope;
pub mod shard;
pub mod storage;
pub mod types;
pub mod worker_context;

pub use error::OfffError;
pub use extensions::{
    append_access_event, append_audit_event, append_decision, append_denied_access_event,
    append_label_event, append_object_edge_event, append_object_event, append_policy_ref,
    append_scope, append_set, object_edge_events_path, object_events_path,
    read_access_events, read_audit_events, read_decisions, read_denied_access_events,
    read_label_events, read_object_edge_events, read_object_events, read_policy_refs,
    read_scopes, read_sets, rebuild_object_index_from_events, validate_extension_files,
};
pub use lineage::{
    compute_lineage_stats, export_dot, export_lineage_json, LineageStats,
    ObjectLineageValidationReport, ObjectLineageValidator,
};
pub use parquet_io::{
    for_each_derivation_batch, for_each_edge_batch, for_each_object_batch,
    write_derivations_batched, write_object_edges_batched, write_object_index_batched,
};
pub use types::{
    AccessEvent, AuditEvent, DateRange, DecisionActor, DecisionRecord, DerivationRow,
    DeniedAccessEvent, DiscoveredObjectRow, ExtensionTarget, LabelEvent, ManifestExtensions,
    ObjectContentRef,
    ObjectEdgeEvent, ObjectEdgeRow, ObjectEvent, PolicyRef, ScopeExclude, ScopeInclude,
    ScopeRecord, SetMembers, SetRecord,
    // Parallel processing types
    AnalysisInputObject, ArtifactRef, CoverageReport, InputObjectMetadata, InputSourceRefs,
    JobInputInclude, JobInputLimits, ParallelizationConfig, ParallelizationSummary,
    ParentResultManifest, ShardInputRef, ShardInputSummary, ShardManifest, ShardPlanRecord,
    ShardResultManifest, ShardResultRef, ShardStatistics, ShardStrategy,
    SkipReasonCode, WorkerErrorCode, WorkerErrorRow, WorkerIdentity, WorkerSkippedRow,
    WorkerTarget,
    OFFF_V2_VERSION,
};
