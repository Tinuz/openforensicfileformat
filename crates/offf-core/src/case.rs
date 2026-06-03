use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    chunk::hex_sha256,
    error::OfffError,
    lineage::ObjectLineageValidator,
    parquet_io::{
        read_derivations, read_object_edges, read_object_index, write_derivations,
        write_object_edges, write_object_index,
    },
    storage::{read_object_verified, ContainerRef},
    types::{
        CaseEventToolInfo, CaseManifest, CaseObjectSummary, CaseProvenanceEvent,
        CaseVerifyReport, DerivationRow, DiscoveredObjectRow, EvidenceRootsRegistry,
        ManifestJson, ObjectEdgeRow, RootAvailability, RootDescriptor, RootRef,
        RootRefType, RootRegistryStatus, RootSummary, ToolActorInfo, OFFF_VERSION,
    },
};

pub const CASE_MANIFEST_FILE: &str = "case_manifest.json";
pub const CASE_ROOTS_REGISTRY_FILE: &str = "evidence_roots.json";
pub const CASE_PROVENANCE_FILE: &str = "provenance/case_provenance.jsonl";
pub const CASE_VERIFY_REPORT_FILE: &str = "reports/verify/case_verify_report.json";
pub const CASE_OBJECT_INDEX_FILE: &str = "indexes/objects/object_index.parquet";
pub const CASE_OBJECT_EDGES_FILE: &str = "indexes/objects/object_edges.parquet";
pub const CASE_DERIVATIONS_FILE: &str = "indexes/objects/derivations.parquet";
pub const CASE_ROOT_SUMMARY_FILE: &str = "indexes/objects/root_summary.json";
pub const CASE_CROSS_ROOT_RELATIONS_FILE: &str = "indexes/objects/cross_root_relations.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrossRootRelation {
    pub relation_kind: String,
    pub relation_id: String,
    pub relation_type: String,
    pub parent_object_id: String,
    pub child_object_id: String,
    pub parent_root_id: String,
    pub child_root_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseIndexBuildResult {
    pub included_roots: Vec<String>,
    pub skipped_roots: Vec<String>,
    pub object_count: usize,
    pub edge_count: usize,
    pub derivation_count: usize,
    pub cross_root_relation_count: usize,
    pub root_summary_path: String,
    pub cross_root_relations_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RootIndexSummaryRow {
    root_id: String,
    status: String,
    object_count: usize,
    edge_count: usize,
    derivation_count: usize,
    detail: Option<String>,
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<(), OfffError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let data = serde_json::to_vec_pretty(value)?;
    fs::write(&tmp, data)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, OfffError> {
    let data = fs::read(path)?;
    Ok(serde_json::from_slice(&data)?)
}

fn manifest_path(case_root: &Path) -> PathBuf {
    case_root.join(CASE_MANIFEST_FILE)
}

fn provenance_path(case_root: &Path) -> PathBuf {
    case_root.join(CASE_PROVENANCE_FILE)
}

fn verify_report_path(case_root: &Path) -> PathBuf {
    case_root.join(CASE_VERIFY_REPORT_FILE)
}

pub fn read_case_manifest(case_root: &Path) -> Result<CaseManifest, OfffError> {
    read_json(&manifest_path(case_root))
}

pub fn write_case_manifest(case_root: &Path, manifest: &CaseManifest) -> Result<(), OfffError> {
    write_json_pretty(&manifest_path(case_root), manifest)
}

pub fn read_evidence_roots_registry(case_root: &Path) -> Result<EvidenceRootsRegistry, OfffError> {
    let manifest = read_case_manifest(case_root)?;
    read_json(&case_root.join(&manifest.roots_registry_path))
}

pub fn write_evidence_roots_registry(
    case_root: &Path,
    registry: &EvidenceRootsRegistry,
) -> Result<(), OfffError> {
    let manifest = read_case_manifest(case_root)?;
    write_json_pretty(&case_root.join(&manifest.roots_registry_path), registry)
}

pub fn read_case_verify_report(case_root: &Path) -> Result<CaseVerifyReport, OfffError> {
    read_json(&verify_report_path(case_root))
}

pub fn write_case_verify_report(
    case_root: &Path,
    report: &CaseVerifyReport,
) -> Result<(), OfffError> {
    write_json_pretty(&verify_report_path(case_root), report)
}

pub fn read_case_cross_root_relations(case_root: &Path) -> Result<Vec<CrossRootRelation>, OfffError> {
    let path = case_root.join(CASE_CROSS_ROOT_RELATIONS_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    read_json(&path)
}

pub fn read_case_provenance_events(case_root: &Path) -> Result<Vec<CaseProvenanceEvent>, OfffError> {
    let path = provenance_path(case_root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<Vec<_>, _>>()
        .map_err(OfffError::Json)
}

pub fn append_case_provenance_event(
    case_root: &Path,
    event: &CaseProvenanceEvent,
) -> Result<(), OfffError> {
    let path = provenance_path(case_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut content = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };
    content.push_str(&serde_json::to_string(event)?);
    content.push('\n');
    fs::write(path, content)?;
    Ok(())
}

fn next_case_event_id(case_root: &Path) -> Result<String, OfffError> {
    Ok(format!(
        "case-evt-{:06}",
        read_case_provenance_events(case_root)?.len()
    ))
}

fn sorted_roots(mut roots: Vec<RootDescriptor>) -> Vec<RootDescriptor> {
    roots.sort_by(|left, right| left.root_id.cmp(&right.root_id));
    roots
}

fn validate_registry(manifest: &CaseManifest, registry: &EvidenceRootsRegistry) -> Result<(), OfffError> {
    if manifest.case_id != registry.case_id {
        return Err(OfffError::InvalidContainer(format!(
            "case manifest case_id '{}' does not match roots registry case_id '{}'",
            manifest.case_id, registry.case_id
        )));
    }
    if manifest.root_count != registry.roots.len() as u64 {
        return Err(OfffError::InvalidContainer(format!(
            "case manifest root_count {} does not match roots registry count {}",
            manifest.root_count,
            registry.roots.len()
        )));
    }
    let mut seen = HashSet::new();
    for root in &registry.roots {
        if !seen.insert(root.root_id.as_str()) {
            return Err(OfffError::InvalidContainer(format!(
                "duplicate root_id '{}' in evidence roots registry",
                root.root_id
            )));
        }
    }
    Ok(())
}

pub fn create_case(
    case_root: &Path,
    manifest: &CaseManifest,
    registry: &EvidenceRootsRegistry,
) -> Result<(), OfffError> {
    validate_registry(manifest, registry)?;
    fs::create_dir_all(case_root)?;
    fs::create_dir_all(case_root.join("indexes/objects"))?;
    fs::create_dir_all(case_root.join("provenance"))?;
    fs::create_dir_all(case_root.join("reports/verify"))?;
    write_case_manifest(case_root, manifest)?;
    write_json_pretty(&case_root.join(&manifest.roots_registry_path), registry)?;
    Ok(())
}

pub fn list_roots(case_root: &Path) -> Result<Vec<RootDescriptor>, OfffError> {
    Ok(sorted_roots(read_evidence_roots_registry(case_root)?.roots))
}

pub fn attach_root(
    case_root: &Path,
    root: RootDescriptor,
    actor: &ToolActorInfo,
) -> Result<(), OfffError> {
    let mut manifest = read_case_manifest(case_root)?;
    let mut registry = read_evidence_roots_registry(case_root)?;
    if registry.roots.iter().any(|existing| existing.root_id == root.root_id) {
        return Err(OfffError::InvalidContainer(format!(
            "root_id '{}' already exists in evidence roots registry",
            root.root_id
        )));
    }
    registry.roots.push(root.clone());
    registry.roots = sorted_roots(registry.roots);
    manifest.root_count = registry.roots.len() as u64;
    validate_registry(&manifest, &registry)?;
    write_case_manifest(case_root, &manifest)?;
    write_json_pretty(&case_root.join(&manifest.roots_registry_path), &registry)?;
    append_case_provenance_event(
        case_root,
        &CaseProvenanceEvent {
            event_id: next_case_event_id(case_root)?,
            case_id: manifest.case_id,
            root_id: Some(root.root_id.clone()),
            timestamp: Utc::now(),
            actor: actor.actor.clone(),
            tool: CaseEventToolInfo {
                tool_id: actor.tool_id.clone(),
                tool_version: actor.tool_version.clone(),
            },
            action: "root_attached".to_string(),
            result: "success".to_string(),
            details: serde_json::json!({
                "root_id": root.root_id,
                "root_type": root.root_type,
                "status": "attached",
            }),
        },
    )
}

pub fn detach_root(
    case_root: &Path,
    root_id: &str,
    actor: &ToolActorInfo,
) -> Result<(), OfffError> {
    let manifest = read_case_manifest(case_root)?;
    let mut registry = read_evidence_roots_registry(case_root)?;
    let root = registry
        .roots
        .iter_mut()
        .find(|root| root.root_id == root_id)
        .ok_or_else(|| {
            OfffError::InvalidContainer(format!(
                "root_id '{}' not found in evidence roots registry",
                root_id
            ))
        })?;
    root.status = RootRegistryStatus::Detached;
    write_json_pretty(&case_root.join(&manifest.roots_registry_path), &registry)?;
    append_case_provenance_event(
        case_root,
        &CaseProvenanceEvent {
            event_id: next_case_event_id(case_root)?,
            case_id: manifest.case_id,
            root_id: Some(root_id.to_string()),
            timestamp: Utc::now(),
            actor: actor.actor.clone(),
            tool: CaseEventToolInfo {
                tool_id: actor.tool_id.clone(),
                tool_version: actor.tool_version.clone(),
            },
            action: "root_detached".to_string(),
            result: "success".to_string(),
            details: serde_json::json!({
                "root_id": root_id,
                "status": "detached",
            }),
        },
    )
}

pub fn resolve_root_ref(case_root: &Path, root_ref: &RootRef) -> Result<PathBuf, OfffError> {
    let path = match root_ref.ref_type {
        RootRefType::Embedded | RootRefType::RelativePath => case_root.join(&root_ref.ref_value),
        RootRefType::AbsolutePath => PathBuf::from(&root_ref.ref_value),
        RootRefType::Uri => match ContainerRef::parse(&root_ref.ref_value)? {
            ContainerRef::Local(path) => path,
            ContainerRef::S3 { .. } => {
                return Err(OfffError::InvalidContainer(
                    "case helpers only support local root URIs for global index build and verified reads"
                        .to_string(),
                ))
            }
        },
    };
    Ok(path)
}

fn infer_root_id(manifest: &ManifestJson) -> Option<String> {
    manifest
        .evidence_roots
        .as_ref()
        .and_then(|roots| roots.first())
        .map(|root| root.root_id.clone())
}

fn infer_root_type(manifest: &ManifestJson) -> String {
    manifest
        .evidence_roots
        .as_ref()
        .and_then(|roots| roots.first())
        .map(|root| root.root_type.clone())
        .unwrap_or_else(|| manifest.effective_mode().as_str().to_string())
}

fn validate_root_descriptor(
    case_root: &Path,
    root: &RootDescriptor,
) -> Result<(PathBuf, ManifestJson), OfffError> {
    let root_path = resolve_root_ref(case_root, &root.root_ref)?;
    let manifest_bytes = fs::read(root_path.join("manifest.json")).map_err(|err| {
        OfffError::InvalidContainer(format!(
            "failed to read manifest.json for root '{}' at '{}': {}",
            root.root_id,
            root_path.display(),
            err
        ))
    })?;
    let actual_manifest_hash = format!("sha256:{}", hex_sha256(&manifest_bytes));
    if root.manifest_hash != actual_manifest_hash {
        return Err(OfffError::InvalidContainer(format!(
            "root '{}' manifest hash mismatch: descriptor {}, actual {}",
            root.root_id, root.manifest_hash, actual_manifest_hash
        )));
    }
    if let Some(expected) = root.root_ref.expected_manifest_hash.as_deref() {
        if expected != actual_manifest_hash {
            return Err(OfffError::InvalidContainer(format!(
                "root '{}' expected manifest hash {}, got {}",
                root.root_id, expected, actual_manifest_hash
            )));
        }
    }

    let manifest: ManifestJson = serde_json::from_slice(&manifest_bytes)?;
    let actual_root_id = infer_root_id(&manifest);
    if let Some(inferred) = actual_root_id.as_deref() {
        if inferred != root.root_id {
            return Err(OfffError::InvalidContainer(format!(
                "root '{}' manifest root_id '{}' does not match descriptor root_id",
                root.root_id, inferred
            )));
        }
    }
    if let Some(expected) = root.root_ref.expected_root_id.as_deref() {
        if let Some(inferred) = actual_root_id.as_deref() {
            if inferred != expected {
                return Err(OfffError::InvalidContainer(format!(
                    "root '{}' expected root_id '{}', got '{}'",
                    root.root_id, expected, inferred
                )));
            }
        }
    }

    let actual_root_type = infer_root_type(&manifest);
    if actual_root_type != root.root_type {
        return Err(OfffError::InvalidContainer(format!(
            "root '{}' manifest type '{}' does not match descriptor type '{}'",
            root.root_id, actual_root_type, root.root_type
        )));
    }
    if let Some(expected) = root.root_ref.expected_root_type.as_deref() {
        if actual_root_type != expected {
            return Err(OfffError::InvalidContainer(format!(
                "root '{}' expected type '{}', got '{}'",
                root.root_id, expected, actual_root_type
            )));
        }
    }

    Ok((root_path, manifest))
}

fn object_index_path(root_path: &Path, manifest: &ManifestJson) -> PathBuf {
    root_path.join(
        manifest
            .indexes
            .object_index
            .as_deref()
            .unwrap_or(CASE_OBJECT_INDEX_FILE),
    )
}

fn object_edges_path(root_path: &Path, manifest: &ManifestJson) -> PathBuf {
    root_path.join(
        manifest
            .indexes
            .object_edges
            .as_deref()
            .unwrap_or(CASE_OBJECT_EDGES_FILE),
    )
}

fn derivations_path(root_path: &Path) -> PathBuf {
    root_path.join(CASE_DERIVATIONS_FILE)
}

fn mark_object_root(row: &mut DiscoveredObjectRow, root_id: &str) {
    if row.root_id.is_none() {
        row.root_id = Some(root_id.to_string());
    }
    if let Some(content_ref) = row.content_ref.as_mut() {
        if content_ref.root_id.is_none() {
            content_ref.root_id = Some(root_id.to_string());
        }
    }
}

fn mark_edge_root(row: &mut ObjectEdgeRow, root_id: &str) {
    if row.parent_root_id.is_none() {
        row.parent_root_id = Some(root_id.to_string());
    }
    if row.child_root_id.is_none() {
        row.child_root_id = Some(root_id.to_string());
    }
}

fn mark_derivation_root(row: &mut DerivationRow, root_id: &str) {
    if row.parent_root_id.is_none() {
        row.parent_root_id = Some(root_id.to_string());
    }
    if row.child_root_id.is_none() {
        row.child_root_id = Some(root_id.to_string());
    }
}

fn collect_cross_root_relations(
    edges: &[ObjectEdgeRow],
    derivations: &[DerivationRow],
) -> Vec<CrossRootRelation> {
    let mut relations = Vec::new();

    for edge in edges {
        if let (Some(parent_root_id), Some(child_root_id)) =
            (edge.parent_root_id.as_deref(), edge.child_root_id.as_deref())
        {
            if parent_root_id != child_root_id {
                relations.push(CrossRootRelation {
                    relation_kind: "edge".to_string(),
                    relation_id: edge.edge_id.clone(),
                    relation_type: edge.relation_type.clone(),
                    parent_object_id: edge.parent_object_id.clone(),
                    child_object_id: edge.child_object_id.clone(),
                    parent_root_id: parent_root_id.to_string(),
                    child_root_id: child_root_id.to_string(),
                });
            }
        }
    }

    for row in derivations {
        if let (Some(parent_root_id), Some(child_root_id)) =
            (row.parent_root_id.as_deref(), row.child_root_id.as_deref())
        {
            if parent_root_id != child_root_id {
                relations.push(CrossRootRelation {
                    relation_kind: "derivation".to_string(),
                    relation_id: row.derivation_id.clone(),
                    relation_type: row.method.clone(),
                    parent_object_id: row.parent_object_id.clone(),
                    child_object_id: row.child_object_id.clone(),
                    parent_root_id: parent_root_id.to_string(),
                    child_root_id: child_root_id.to_string(),
                });
            }
        }
    }

    relations.sort_by(|left, right| {
        left.relation_kind
            .cmp(&right.relation_kind)
            .then(left.relation_id.cmp(&right.relation_id))
    });
    relations
}

pub fn build_case_global_indexes(case_root: &Path) -> Result<CaseIndexBuildResult, OfffError> {
    let mut manifest = read_case_manifest(case_root)?;
    let registry = read_evidence_roots_registry(case_root)?;
    validate_registry(&manifest, &registry)?;

    let mut objects = Vec::new();
    let mut edges = Vec::new();
    let mut derivations = Vec::new();
    let mut included_roots = Vec::new();
    let mut skipped_roots = Vec::new();
    let mut root_summary = Vec::new();

    for root in sorted_roots(registry.roots) {
        if !matches!(root.status, RootRegistryStatus::Active) {
            skipped_roots.push(root.root_id.clone());
            root_summary.push(RootIndexSummaryRow {
                root_id: root.root_id,
                status: "skipped_inactive".to_string(),
                object_count: 0,
                edge_count: 0,
                derivation_count: 0,
                detail: None,
            });
            continue;
        }

        if matches!(
            root.root_ref.availability,
            RootAvailability::Offline | RootAvailability::Missing | RootAvailability::Unknown
        ) {
            skipped_roots.push(root.root_id.clone());
            root_summary.push(RootIndexSummaryRow {
                root_id: root.root_id,
                status: "skipped_unavailable".to_string(),
                object_count: 0,
                edge_count: 0,
                derivation_count: 0,
                detail: None,
            });
            continue;
        }

        let (root_path, root_manifest) = validate_root_descriptor(case_root, &root)?;
        let object_index = object_index_path(&root_path, &root_manifest);
        if !object_index.exists() {
            skipped_roots.push(root.root_id.clone());
            root_summary.push(RootIndexSummaryRow {
                root_id: root.root_id,
                status: "skipped_missing_object_index".to_string(),
                object_count: 0,
                edge_count: 0,
                derivation_count: 0,
                detail: Some(object_index.display().to_string().replace('\\', "/")),
            });
            continue;
        }

        let mut root_objects = read_object_index(&object_index)?;
        let mut root_edges = if object_edges_path(&root_path, &root_manifest).exists() {
            read_object_edges(&object_edges_path(&root_path, &root_manifest))?
        } else {
            Vec::new()
        };
        let mut root_derivations = if derivations_path(&root_path).exists() {
            read_derivations(&derivations_path(&root_path))?
        } else {
            Vec::new()
        };

        for row in &mut root_objects {
            mark_object_root(row, &root.root_id);
        }
        for row in &mut root_edges {
            mark_edge_root(row, &root.root_id);
        }
        for row in &mut root_derivations {
            mark_derivation_root(row, &root.root_id);
        }

        root_summary.push(RootIndexSummaryRow {
            root_id: root.root_id.clone(),
            status: "included".to_string(),
            object_count: root_objects.len(),
            edge_count: root_edges.len(),
            derivation_count: root_derivations.len(),
            detail: None,
        });
        included_roots.push(root.root_id);
        objects.extend(root_objects);
        edges.extend(root_edges);
        derivations.extend(root_derivations);
    }

    objects.sort_by(|left, right| {
        left.root_id
            .cmp(&right.root_id)
            .then(left.object_id.cmp(&right.object_id))
    });
    edges.sort_by(|left, right| {
        left.parent_root_id
            .cmp(&right.parent_root_id)
            .then(left.child_root_id.cmp(&right.child_root_id))
            .then(left.edge_id.cmp(&right.edge_id))
    });
    derivations.sort_by(|left, right| {
        left.parent_root_id
            .cmp(&right.parent_root_id)
            .then(left.child_root_id.cmp(&right.child_root_id))
            .then(left.derivation_id.cmp(&right.derivation_id))
    });

    let cross_root_relations = collect_cross_root_relations(&edges, &derivations);

    write_object_index(&case_root.join(CASE_OBJECT_INDEX_FILE), &objects)?;
    write_object_edges(&case_root.join(CASE_OBJECT_EDGES_FILE), &edges)?;
    write_derivations(&case_root.join(CASE_DERIVATIONS_FILE), &derivations)?;
    write_json_pretty(&case_root.join(CASE_ROOT_SUMMARY_FILE), &root_summary)?;
    write_json_pretty(
        &case_root.join(CASE_CROSS_ROOT_RELATIONS_FILE),
        &cross_root_relations,
    )?;

    manifest.global_indexes.object_index = CASE_OBJECT_INDEX_FILE.to_string();
    manifest.global_indexes.object_edges = CASE_OBJECT_EDGES_FILE.to_string();
    manifest.global_indexes.derivations = CASE_DERIVATIONS_FILE.to_string();
    manifest.global_indexes.root_summary = Some(CASE_ROOT_SUMMARY_FILE.to_string());
    manifest.global_indexes.cross_root_relations =
        Some(CASE_CROSS_ROOT_RELATIONS_FILE.to_string());
    write_case_manifest(case_root, &manifest)?;

    Ok(CaseIndexBuildResult {
        included_roots,
        skipped_roots,
        object_count: objects.len(),
        edge_count: edges.len(),
        derivation_count: derivations.len(),
        cross_root_relation_count: cross_root_relations.len(),
        root_summary_path: CASE_ROOT_SUMMARY_FILE.to_string(),
        cross_root_relations_path: CASE_CROSS_ROOT_RELATIONS_FILE.to_string(),
    })
}

pub fn read_case_object_verified(case_root: &Path, object_id: &str) -> Result<Vec<u8>, OfffError> {
    let manifest = read_case_manifest(case_root)?;
    let registry = read_evidence_roots_registry(case_root)?;
    let object_index_path = case_root.join(&manifest.global_indexes.object_index);
    let objects = read_object_index(&object_index_path)?;
    let matches: Vec<&DiscoveredObjectRow> = objects
        .iter()
        .filter(|row| row.object_id == object_id)
        .collect();
    if matches.is_empty() {
        return Err(OfffError::InvalidContainer(format!(
            "object_id '{}' not found in case object index",
            object_id
        )));
    }
    if matches.len() > 1 {
        return Err(OfffError::InvalidContainer(format!(
            "object_id '{}' is ambiguous in case object index; root-aware IDs are required",
            object_id
        )));
    }

    let row = matches[0];
    let root_id = row
        .root_id
        .clone()
        .or_else(|| row.content_ref.as_ref().and_then(|content_ref| content_ref.root_id.clone()))
        .ok_or_else(|| {
            OfffError::InvalidContainer(format!(
                "object_id '{}' has no root_id in case object index",
                object_id
            ))
        })?;
    let root = registry
        .roots
        .iter()
        .find(|root| root.root_id == root_id)
        .ok_or_else(|| {
            OfffError::InvalidContainer(format!(
                "root_id '{}' for object_id '{}' not found in evidence roots registry",
                root_id, object_id
            ))
        })?;
    if !matches!(root.status, RootRegistryStatus::Active) {
        return Err(OfffError::InvalidContainer(format!(
            "root_id '{}' for object_id '{}' is not active",
            root_id, object_id
        )));
    }
    let (root_path, _) = validate_root_descriptor(case_root, root)?;
    read_object_verified(&root_path, object_id)
}

fn lineage_errors(report: crate::lineage::ObjectLineageValidationReport) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    if !report.missing_edge_parents.is_empty() {
        out.push(serde_json::json!({
            "check": "missing_edge_parents",
            "items": report.missing_edge_parents,
        }));
    }
    if !report.missing_edge_children.is_empty() {
        out.push(serde_json::json!({
            "check": "missing_edge_children",
            "items": report.missing_edge_children,
        }));
    }
    if !report.missing_derivation_parents.is_empty() {
        out.push(serde_json::json!({
            "check": "missing_derivation_parents",
            "items": report.missing_derivation_parents,
        }));
    }
    if !report.missing_derivation_children.is_empty() {
        out.push(serde_json::json!({
            "check": "missing_derivation_children",
            "items": report.missing_derivation_children,
        }));
    }
    if !report.invalid_derivation_links.is_empty() {
        out.push(serde_json::json!({
            "check": "invalid_derivation_links",
            "items": report.invalid_derivation_links,
        }));
    }
    if !report.cycles.is_empty() {
        out.push(serde_json::json!({
            "check": "cycles",
            "items": report.cycles,
        }));
    }
    out
}

pub fn verify_case(
    case_root: &Path,
    profile: &str,
    sample_read_limit: usize,
) -> Result<CaseVerifyReport, OfffError> {
    let mut manifest = read_case_manifest(case_root)?;
    let registry = read_evidence_roots_registry(case_root)?;
    validate_registry(&manifest, &registry)?;

    let mut checks = Vec::new();
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    let mut root_summary = RootSummary {
        total: registry.roots.len() as u64,
        valid: 0,
        warning: 0,
        invalid: 0,
        missing: 0,
        offline: 0,
    };

    for root in sorted_roots(registry.roots.clone()) {
        match root.status {
            RootRegistryStatus::Detached | RootRegistryStatus::Archived => {
                root_summary.warning += 1;
                warnings.push(serde_json::json!({
                    "check": "root_inactive",
                    "root_id": root.root_id,
                    "status": format!("{:?}", root.status).to_lowercase(),
                }));
                continue;
            }
            RootRegistryStatus::Missing => {
                root_summary.missing += 1;
                warnings.push(serde_json::json!({
                    "check": "root_missing",
                    "root_id": root.root_id,
                }));
                continue;
            }
            RootRegistryStatus::Active => {}
        }

        match root.root_ref.availability {
            RootAvailability::Offline => {
                root_summary.offline += 1;
                warnings.push(serde_json::json!({
                    "check": "root_offline",
                    "root_id": root.root_id,
                }));
                continue;
            }
            RootAvailability::Missing => {
                root_summary.missing += 1;
                warnings.push(serde_json::json!({
                    "check": "root_unavailable",
                    "root_id": root.root_id,
                    "availability": "missing",
                }));
                continue;
            }
            RootAvailability::Unknown => {
                root_summary.warning += 1;
                warnings.push(serde_json::json!({
                    "check": "root_unavailable",
                    "root_id": root.root_id,
                    "availability": "unknown",
                }));
                continue;
            }
            RootAvailability::Online => {}
        }

        match validate_root_descriptor(case_root, &root) {
            Ok(_) => {
                root_summary.valid += 1;
                checks.push(serde_json::json!({
                    "check": "root_descriptor_valid",
                    "root_id": root.root_id,
                }));
            }
            Err(err) => {
                root_summary.invalid += 1;
                errors.push(serde_json::json!({
                    "check": "root_descriptor_invalid",
                    "root_id": root.root_id,
                    "detail": err.to_string(),
                }));
            }
        }
    }

    let mut object_summary = CaseObjectSummary {
        total_objects: 0,
        roots_referenced: 0,
        orphan_objects: 0,
        cross_root_relations: 0,
    };

    if profile != "registry" {
        let object_index_path = case_root.join(&manifest.global_indexes.object_index);
        if object_index_path.exists() {
            let objects = read_object_index(&object_index_path)?;
            let edges = if case_root.join(&manifest.global_indexes.object_edges).exists() {
                read_object_edges(&case_root.join(&manifest.global_indexes.object_edges))?
            } else {
                Vec::new()
            };
            let derivations = if case_root.join(&manifest.global_indexes.derivations).exists() {
                read_derivations(&case_root.join(&manifest.global_indexes.derivations))?
            } else {
                Vec::new()
            };

            object_summary.total_objects = objects.len() as u64;
            object_summary.roots_referenced = objects
                .iter()
                .filter_map(|row| row.root_id.clone())
                .collect::<BTreeSet<_>>()
                .len() as u64;
            object_summary.orphan_objects =
                objects.iter().filter(|row| row.root_id.is_none()).count() as u64;
            object_summary.cross_root_relations = edges
                .iter()
                .filter(|edge| edge.parent_root_id != edge.child_root_id)
                .count() as u64
                + derivations
                    .iter()
                    .filter(|row| row.parent_root_id != row.child_root_id)
                    .count() as u64;

            let mut duplicates = HashMap::new();
            for row in &objects {
                *duplicates.entry(row.object_id.clone()).or_insert(0usize) += 1;
            }
            let duplicate_object_ids: Vec<String> = duplicates
                .into_iter()
                .filter(|(_, count)| *count > 1)
                .map(|(object_id, _)| object_id)
                .collect();
            if !duplicate_object_ids.is_empty() {
                errors.push(serde_json::json!({
                    "check": "duplicate_object_ids",
                    "items": duplicate_object_ids,
                }));
            }

            if registry.roots.len() > 1 && object_summary.orphan_objects > 0 {
                errors.push(serde_json::json!({
                    "check": "orphan_objects",
                    "count": object_summary.orphan_objects,
                }));
            }

            errors.extend(lineage_errors(ObjectLineageValidator::validate(
                &objects,
                &edges,
                &derivations,
            )));

            if profile == "full" && sample_read_limit > 0 {
                for row in objects
                    .iter()
                    .filter(|row| {
                        row.root_id.is_some()
                            && (row.content_ref.is_some() || row.storage_ref.is_some())
                    })
                    .take(sample_read_limit)
                {
                    match read_case_object_verified(case_root, &row.object_id) {
                        Ok(_) => checks.push(serde_json::json!({
                            "check": "sample_verified_read",
                            "object_id": row.object_id,
                        })),
                        Err(err) => errors.push(serde_json::json!({
                            "check": "sample_verified_read_failed",
                            "object_id": row.object_id,
                            "detail": err.to_string(),
                        })),
                    }
                }
            }
        } else {
            warnings.push(serde_json::json!({
                "check": "missing_case_object_index",
                "path": manifest.global_indexes.object_index,
            }));
        }
    }

    let status = if !errors.is_empty() {
        "invalid"
    } else if !warnings.is_empty() {
        "warning"
    } else {
        "valid"
    }
    .to_string();

    let report = CaseVerifyReport {
        case_id: manifest.case_id.clone(),
        profile: profile.to_string(),
        status: status.clone(),
        root_summary,
        object_summary,
        checks,
        warnings,
        errors,
    };

    write_case_verify_report(case_root, &report)?;
    if !manifest.verify_reports.iter().any(|path| path == CASE_VERIFY_REPORT_FILE) {
        manifest.verify_reports.push(CASE_VERIFY_REPORT_FILE.to_string());
        write_case_manifest(case_root, &manifest)?;
    }
    append_case_provenance_event(
        case_root,
        &CaseProvenanceEvent {
            event_id: next_case_event_id(case_root)?,
            case_id: report.case_id.clone(),
            root_id: None,
            timestamp: Utc::now(),
            actor: "system".to_string(),
            tool: CaseEventToolInfo {
                tool_id: "offf-core".to_string(),
                tool_version: OFFF_VERSION.to_string(),
            },
            action: if status == "invalid" {
                "case_verify_failed".to_string()
            } else {
                "case_verified".to_string()
            },
            result: if status == "invalid" {
                "failure".to_string()
            } else if status == "warning" {
                "warning".to_string()
            } else {
                "success".to_string()
            },
            details: serde_json::json!({
                "profile": profile,
                "status": status,
            }),
        },
    )?;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        evidence::write_evidence_object,
        parquet_io::{read_object_index, write_derivations, write_object_edges, write_object_index},
        types::{
            AcquisitionMode, CaseGlobalIndexes, EvidenceRoot, ManifestIndexes,
            ObjectContentRef, RootRegistryStatus, RootVerifyStatus, ToolInfo,
            OFFF_CASE_SCHEMA_VERSION, OFFF_V2_VERSION,
        },
    };
    use tempfile::tempdir;

    fn actor() -> ToolActorInfo {
        ToolActorInfo {
            tool_id: "offf-core-test".to_string(),
            tool_version: "0.1.0".to_string(),
            actor: "tester".to_string(),
        }
    }

    fn base_case_manifest(case_id: &str) -> CaseManifest {
        CaseManifest {
            schema_version: OFFF_CASE_SCHEMA_VERSION.to_string(),
            offf_version: OFFF_VERSION.to_string(),
            case_id: case_id.to_string(),
            title: Some("Test case".to_string()),
            created_at: Utc::now(),
            created_by: actor(),
            root_count: 0,
            roots_registry_path: CASE_ROOTS_REGISTRY_FILE.to_string(),
            global_indexes: CaseGlobalIndexes {
                object_index: CASE_OBJECT_INDEX_FILE.to_string(),
                object_edges: CASE_OBJECT_EDGES_FILE.to_string(),
                derivations: CASE_DERIVATIONS_FILE.to_string(),
                root_summary: Some(CASE_ROOT_SUMMARY_FILE.to_string()),
                cross_root_relations: Some(CASE_CROSS_ROOT_RELATIONS_FILE.to_string()),
            },
            provenance_paths: vec![CASE_PROVENANCE_FILE.to_string()],
            verify_reports: Vec::new(),
            limitations: Vec::new(),
        }
    }

    fn empty_registry(case_id: &str) -> EvidenceRootsRegistry {
        EvidenceRootsRegistry {
            schema_version: OFFF_CASE_SCHEMA_VERSION.to_string(),
            case_id: case_id.to_string(),
            roots: Vec::new(),
        }
    }

    fn write_root_container(root_path: &Path, root_id: &str, payload: &[u8]) -> (String, String) {
        fs::create_dir_all(root_path.join("indexes/objects")).unwrap();
        let hex = write_evidence_object(root_path, payload).unwrap();
        let sha = format!("sha256:{hex}");
        let object_id = format!("obj-{root_id}-file");
        let collection_root_id = root_id.to_string();
        let objects = vec![
            DiscoveredObjectRow {
                object_id: collection_root_id.clone(),
                object_type: "collection_root".to_string(),
                name: Some(collection_root_id.clone()),
                logical_path: Some("/".to_string()),
                media_type: None,
                size_bytes: None,
                sha256: None,
                source_layer: "collection".to_string(),
                storage_ref: None,
                content_ref: None,
                content_hash_status: None,
                root_source_ref: None,
                root_id: Some(collection_root_id.clone()),
                collection_relative_path: None,
                created_by_job_id: None,
                parser_status: "ok".to_string(),
                provenance_ref: None,
                schema_version: OFFF_V2_VERSION.to_string(),
                original_created_at: None,
                original_modified_at: None,
                original_accessed_at: None,
            },
            DiscoveredObjectRow {
                object_id: object_id.clone(),
                object_type: "file".to_string(),
                name: Some(format!("{root_id}.bin")),
                logical_path: Some(format!("/{root_id}.bin")),
                media_type: Some("application/octet-stream".to_string()),
                size_bytes: Some(payload.len() as u64),
                sha256: Some(sha.clone()),
                source_layer: "collection".to_string(),
                storage_ref: Some(sha.clone()),
                content_ref: Some(ObjectContentRef {
                    ref_type: "evidence_object_store".to_string(),
                    root_id: None,
                    filesystem_id: None,
                    file_id: None,
                    file_index_path: None,
                    storage_ref: Some(sha.clone()),
                }),
                content_hash_status: Some("verified".to_string()),
                root_source_ref: Some(collection_root_id.clone()),
                root_id: None,
                collection_relative_path: Some(format!("{root_id}.bin")),
                created_by_job_id: None,
                parser_status: "ok".to_string(),
                provenance_ref: None,
                schema_version: OFFF_V2_VERSION.to_string(),
                original_created_at: None,
                original_modified_at: None,
                original_accessed_at: None,
            },
        ];
        let edges = vec![ObjectEdgeRow {
            edge_id: format!("edge-{root_id}-1"),
            parent_object_id: collection_root_id.clone(),
            child_object_id: object_id.clone(),
            parent_root_id: None,
            child_root_id: None,
            relation_type: "contains".to_string(),
            method: Some("collection_capture".to_string()),
            logical_path: Some(format!("{root_id}.bin")),
            sequence: Some(1),
            created_by_job_id: None,
            provenance_ref: None,
            schema_version: OFFF_V2_VERSION.to_string(),
        }];
        let derivations = vec![DerivationRow {
            derivation_id: format!("deriv-{root_id}-1"),
            parent_object_id: collection_root_id.clone(),
            child_object_id: object_id.clone(),
            parent_root_id: None,
            child_root_id: None,
            job_id: format!("job-{root_id}"),
            method: "collection_capture".to_string(),
            tool_id: "offf-core-test".to_string(),
            tool_name: "OFFF Core Test".to_string(),
            tool_version: "0.1.0".to_string(),
            parameters_hash: None,
            input_sha256: None,
            output_sha256: Some(sha.clone()),
            storage_mode: "referenced_only".to_string(),
            provenance_ref: None,
            created_at: Utc::now().to_rfc3339(),
            schema_version: OFFF_V2_VERSION.to_string(),
        }];

        write_object_index(&root_path.join(CASE_OBJECT_INDEX_FILE), &objects).unwrap();
        write_object_edges(&root_path.join(CASE_OBJECT_EDGES_FILE), &edges).unwrap();
        write_derivations(&root_path.join(CASE_DERIVATIONS_FILE), &derivations).unwrap();

        let manifest = ManifestJson {
            offf_version: OFFF_VERSION.to_string(),
            container_id: format!("urn:offf:root:{root_id}"),
            created_at: Utc::now(),
            created_by_tool: ToolInfo {
                name: "offf-core-test".to_string(),
                version: "0.1.0".to_string(),
            },
            acquisition_mode: Some(AcquisitionMode::FileCollection),
            source: None,
            hashes: None,
            chunking: None,
            evidence_roots: Some(vec![EvidenceRoot {
                root_id: root_id.to_string(),
                root_type: "file_collection".to_string(),
                description: Some(format!("root {root_id}")),
                object_count: Some(objects.len() as u64),
                root_hash: None,
            }]),
            limitations: None,
            indexes: ManifestIndexes {
                physical_to_chunk: None,
                object_index: Some(CASE_OBJECT_INDEX_FILE.to_string()),
                object_edges: Some(CASE_OBJECT_EDGES_FILE.to_string()),
            },
            extensions: None,
        };
        let manifest_path = root_path.join("manifest.json");
        write_json_pretty(&manifest_path, &manifest).unwrap();
        let manifest_hash = format!("sha256:{}", hex_sha256(&fs::read(manifest_path).unwrap()));
        (object_id, manifest_hash)
    }

    fn make_descriptor(root_id: &str, root_path: &Path, manifest_hash: &str) -> RootDescriptor {
        RootDescriptor {
            root_id: root_id.to_string(),
            root_type: "file_collection".to_string(),
            description: format!("root {root_id}"),
            root_ref: RootRef {
                ref_type: RootRefType::AbsolutePath,
                ref_value: root_path.display().to_string(),
                expected_manifest_hash: Some(manifest_hash.to_string()),
                expected_root_id: Some(root_id.to_string()),
                expected_root_type: Some("file_collection".to_string()),
                availability: RootAvailability::Online,
            },
            manifest_hash: manifest_hash.to_string(),
            verify_status: RootVerifyStatus::Valid,
            attached_at: Utc::now(),
            attached_by: actor(),
            status: RootRegistryStatus::Active,
        }
    }

    #[test]
    fn case_roundtrip_attach_and_detach_root() {
        let dir = tempdir().unwrap();
        let case_root = dir.path().join("case");
        create_case(&case_root, &base_case_manifest("case-1"), &empty_registry("case-1")).unwrap();

        let root_dir = dir.path().join("root-a");
        fs::create_dir_all(&root_dir).unwrap();
        let (_, manifest_hash) = write_root_container(&root_dir, "root-a", b"alpha");
        attach_root(
            &case_root,
            make_descriptor("root-a", &root_dir, &manifest_hash),
            &actor(),
        )
        .unwrap();

        let manifest = read_case_manifest(&case_root).unwrap();
        assert_eq!(manifest.root_count, 1);
        assert_eq!(list_roots(&case_root).unwrap().len(), 1);

        detach_root(&case_root, "root-a", &actor()).unwrap();
        let roots = list_roots(&case_root).unwrap();
        assert!(matches!(roots[0].status, RootRegistryStatus::Detached));
        let events = read_case_provenance_events(&case_root).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].action, "root_attached");
        assert_eq!(events[1].action, "root_detached");
    }

    #[test]
    fn build_case_indexes_and_verify_reads() {
        let dir = tempdir().unwrap();
        let case_root = dir.path().join("case");
        create_case(&case_root, &base_case_manifest("case-2"), &empty_registry("case-2")).unwrap();

        let root_a = dir.path().join("root-a");
        let root_b = dir.path().join("root-b");
        fs::create_dir_all(&root_a).unwrap();
        fs::create_dir_all(&root_b).unwrap();
        let (object_a, hash_a) = write_root_container(&root_a, "root-a", b"alpha bytes");
        let (_object_b, hash_b) = write_root_container(&root_b, "root-b", b"beta bytes");

        attach_root(&case_root, make_descriptor("root-a", &root_a, &hash_a), &actor()).unwrap();
        attach_root(&case_root, make_descriptor("root-b", &root_b, &hash_b), &actor()).unwrap();

        let build = build_case_global_indexes(&case_root).unwrap();
        assert_eq!(build.included_roots, vec!["root-a".to_string(), "root-b".to_string()]);
        assert_eq!(build.object_count, 4);

        let rows = read_object_index(&case_root.join(CASE_OBJECT_INDEX_FILE)).unwrap();
        assert!(rows.iter().all(|row| row.root_id.is_some()));
        assert_eq!(read_case_object_verified(&case_root, &object_a).unwrap(), b"alpha bytes");

        let report = verify_case(&case_root, "full", 1).unwrap();
        assert_eq!(report.status, "valid");
        assert_eq!(report.root_summary.valid, 2);
        assert_eq!(report.object_summary.total_objects, 4);
        let persisted = read_case_verify_report(&case_root).unwrap();
        assert_eq!(persisted.status, "valid");
    }
}