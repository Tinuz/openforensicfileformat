pub mod case;
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
pub use case::{
    append_case_provenance_event, attach_root, build_case_global_indexes, create_case,
    detach_root, list_roots, read_case_manifest, read_case_object_verified,
    read_case_cross_root_relations, read_case_provenance_events, read_case_verify_report,
    read_evidence_roots_registry,
    resolve_root_ref, verify_case, write_case_manifest, write_case_verify_report,
    write_evidence_roots_registry, CaseIndexBuildResult, CrossRootRelation,
    CASE_CROSS_ROOT_RELATIONS_FILE,
    CASE_DERIVATIONS_FILE, CASE_MANIFEST_FILE, CASE_OBJECT_EDGES_FILE, CASE_OBJECT_INDEX_FILE,
    CASE_PROVENANCE_FILE, CASE_ROOT_SUMMARY_FILE, CASE_ROOTS_REGISTRY_FILE,
    CASE_VERIFY_REPORT_FILE,
};
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
    AccessEvent, AuditEvent, CaseEventToolInfo, CaseGlobalIndexes, CaseManifest,
    CaseObjectSummary, CaseProvenanceEvent, CaseVerifyReport, DateRange, DecisionActor,
    DecisionRecord, DerivationRow, DeniedAccessEvent, DiscoveredObjectRow,
    EvidenceRootsRegistry, ExtensionTarget, LabelEvent, ManifestExtensions, ObjectContentRef,
    ObjectEdgeEvent, ObjectEdgeRow, ObjectEvent, PolicyRef, RootAvailability,
    RootDescriptor, RootRef, RootRefType, RootRegistryStatus, RootSummary,
    RootVerifyStatus, ScopeExclude, ScopeInclude, ScopeRecord, SetMembers, SetRecord,
    ToolActorInfo,
    // Parallel processing types
    AnalysisInputObject, ArtifactRef, CoverageReport, InputObjectMetadata, InputSourceRefs,
    JobInputInclude, JobInputLimits, ParallelizationConfig, ParallelizationSummary,
    ParentResultManifest, ShardInputRef, ShardInputSummary, ShardManifest, ShardPlanRecord,
    ShardResultManifest, ShardResultRef, ShardStatistics, ShardStrategy,
    SkipReasonCode, WorkerErrorCode, WorkerErrorRow, WorkerIdentity, WorkerSkippedRow,
    WorkerTarget, OFFF_CASE_SCHEMA_VERSION,
    OFFF_V2_VERSION,
};
