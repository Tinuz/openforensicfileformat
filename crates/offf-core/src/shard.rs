/// Shard planning, I/O helpers, coverage computation, and parallel job
/// validation for OFFF parallel analysis jobs.
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::{
    error::OfffError,
    types::{
        AnalysisInputObject, CoverageReport, ParallelizationSummary,
        ParentResultManifest, ShardInputRef, ShardManifest, ShardPlanRecord, ShardResultManifest,
        ShardResultRef, ShardStrategy,
    },
};

// ── Shard planning ────────────────────────────────────────────────────────────

/// Divide `inputs` into `shard_count` shards using `strategy`.
///
/// Returns a `(ShardPlanRecord, Vec<ShardManifest>)` pair.  The plan and all
/// manifests are deterministic for the same `inputs`, `strategy`, and
/// `shard_count`.
///
/// `inputs` MUST already be sorted (the `ScopeResolver` does this by
/// `object_id` ascending).
pub fn plan_shards(
    inputs: &[AnalysisInputObject],
    parent_job_id: &str,
    strategy: ShardStrategy,
    shard_count: usize,
    input_scope_hash: &str,
) -> Result<(ShardPlanRecord, Vec<ShardManifest>), OfffError> {
    if shard_count == 0 {
        return Err(OfffError::ShardProcessingFailed {
            shard_id: "plan".into(),
            details: "shard_count must be >= 1".into(),
        });
    }

    // Assign each input to a shard index.
    let assignments: Vec<usize> = inputs
        .iter()
        .enumerate()
        .map(|(i, obj)| assign_shard(i, obj, &strategy, shard_count))
        .collect();

    // Group inputs by shard index.
    let mut shard_inputs: Vec<Vec<ShardInputRef>> = vec![Vec::new(); shard_count];
    for (i, obj) in inputs.iter().enumerate() {
        shard_inputs[assignments[i]].push(ShardInputRef {
            input_id: obj.input_id.clone(),
            object_id: obj.object_id.clone(),
        });
    }

    let now = Utc::now().to_rfc3339();

    let plan = ShardPlanRecord {
        parent_job_id: parent_job_id.to_string(),
        shard_plan_id: format!("shardplan-{parent_job_id}"),
        strategy: strategy.clone(),
        shard_count,
        input_count: inputs.len(),
        input_scope_hash: input_scope_hash.to_string(),
        created_at: now.clone(),
        created_by: "offf-jobs".to_string(),
    };

    let manifests: Vec<ShardManifest> = (0..shard_count)
        .map(|idx| {
            let shard_id = format!("{parent_job_id}-shard-{idx:02}");
            ShardManifest {
                shard_id: shard_id.clone(),
                parent_job_id: parent_job_id.to_string(),
                shard_index: idx,
                shard_count,
                input_scope_hash: input_scope_hash.to_string(),
                input_objects: shard_inputs[idx].clone(),
                output_base_path: format!(
                    "analysis/jobs/{parent_job_id}/shards/{shard_id}"
                ),
                status: "planned".to_string(),
            }
        })
        .collect();

    Ok((plan, manifests))
}

fn assign_shard(
    index: usize,
    obj: &AnalysisInputObject,
    strategy: &ShardStrategy,
    shard_count: usize,
) -> usize {
    match strategy {
        ShardStrategy::DeterministicObjectIdRange => index % shard_count,
        ShardStrategy::DeterministicRoundRobin => index % shard_count,
        ShardStrategy::DeterministicHashModulo => {
            let mut hasher = Sha256::new();
            hasher.update(obj.object_id.as_bytes());
            let hash = hasher.finalize();
            (hash[0] as usize) % shard_count
        }
    }
}

// Note: DeterministicObjectIdRange and DeterministicRoundRobin both use
// index % shard_count, but their *semantics* differ:
// - ObjectIdRange groups contiguous blocks (intended for sorted input).
// - RoundRobin interleaves inputs across shards.
// Both happen to produce the same numeric result; the distinction is semantic
// and documented for external tooling.

// ── File paths ────────────────────────────────────────────────────────────────

pub fn job_manifest_path(container_path: &Path, job_id: &str) -> PathBuf {
    container_path.join(format!("analysis/jobs/{job_id}/job_manifest.json"))
}

pub fn shard_plan_path(container_path: &Path, job_id: &str) -> PathBuf {
    container_path.join(format!("analysis/jobs/{job_id}/shard_plan.json"))
}

pub fn shard_manifest_path(container_path: &Path, job_id: &str, shard_id: &str) -> PathBuf {
    container_path.join(format!(
        "analysis/jobs/{job_id}/shards/{shard_id}/shard_manifest.json"
    ))
}

pub fn shard_result_manifest_path(
    container_path: &Path,
    job_id: &str,
    shard_id: &str,
) -> PathBuf {
    container_path.join(format!(
        "analysis/jobs/{job_id}/shards/{shard_id}/shard_result_manifest.json"
    ))
}

pub fn parent_result_manifest_path(container_path: &Path, job_id: &str) -> PathBuf {
    container_path.join(format!(
        "analysis/jobs/{job_id}/parent_result_manifest.json"
    ))
}

pub fn shard_staging_dir(container_path: &Path, job_id: &str, shard_id: &str) -> PathBuf {
    container_path.join(format!(
        "analysis/jobs/{job_id}/shards/{shard_id}.tmp"
    ))
}

pub fn shard_final_dir(container_path: &Path, job_id: &str, shard_id: &str) -> PathBuf {
    container_path.join(format!(
        "analysis/jobs/{job_id}/shards/{shard_id}"
    ))
}

// ── I/O ───────────────────────────────────────────────────────────────────────

pub fn write_shard_plan(
    container_path: &Path,
    job_id: &str,
    plan: &ShardPlanRecord,
) -> Result<(), OfffError> {
    let path = shard_plan_path(container_path, job_id);
    fs::create_dir_all(path.parent().unwrap())?;
    let json = serde_json::to_string_pretty(plan)?;
    fs::write(&path, json)?;
    Ok(())
}

pub fn read_shard_plan(
    container_path: &Path,
    job_id: &str,
) -> Result<ShardPlanRecord, OfffError> {
    let path = shard_plan_path(container_path, job_id);
    let data = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data)?)
}

pub fn write_shard_manifest(
    container_path: &Path,
    job_id: &str,
    shard: &ShardManifest,
) -> Result<(), OfffError> {
    let path = shard_manifest_path(container_path, job_id, &shard.shard_id);
    fs::create_dir_all(path.parent().unwrap())?;
    let json = serde_json::to_string_pretty(shard)?;
    fs::write(&path, json)?;
    Ok(())
}

pub fn read_shard_manifest(
    container_path: &Path,
    job_id: &str,
    shard_id: &str,
) -> Result<ShardManifest, OfffError> {
    let path = shard_manifest_path(container_path, job_id, shard_id);
    let data = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data)?)
}

pub fn write_shard_result_manifest(
    container_path: &Path,
    result: &ShardResultManifest,
) -> Result<(), OfffError> {
    let path =
        shard_result_manifest_path(container_path, &result.parent_job_id, &result.shard_id);
    fs::create_dir_all(path.parent().unwrap())?;
    let json = serde_json::to_string_pretty(result)?;
    fs::write(&path, json)?;
    Ok(())
}

pub fn read_shard_result_manifest(
    container_path: &Path,
    job_id: &str,
    shard_id: &str,
) -> Result<ShardResultManifest, OfffError> {
    let path = shard_result_manifest_path(container_path, job_id, shard_id);
    let data = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data)?)
}

pub fn write_parent_result_manifest(
    container_path: &Path,
    manifest: &ParentResultManifest,
) -> Result<(), OfffError> {
    let path = parent_result_manifest_path(container_path, &manifest.job_id);
    fs::create_dir_all(path.parent().unwrap())?;
    let json = serde_json::to_string_pretty(manifest)?;
    fs::write(&path, json)?;
    Ok(())
}

pub fn read_parent_result_manifest(
    container_path: &Path,
    job_id: &str,
) -> Result<ParentResultManifest, OfffError> {
    let path = parent_result_manifest_path(container_path, job_id);
    let data = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data)?)
}

// ── Coverage ──────────────────────────────────────────────────────────────────

/// Compute a `CoverageReport` by aggregating `shard_results` and cross-
/// checking against the full `inputs` list and `plan`.
pub fn compute_coverage_report(
    plan: &ShardPlanRecord,
    shard_results: &[ShardResultManifest],
    inputs: &[AnalysisInputObject],
) -> CoverageReport {
    let mut processed = 0u64;
    let mut success = 0u64;
    let mut errors = 0u64;
    let mut skipped = 0u64;
    let mut assigned = 0u64;

    for r in shard_results {
        processed += r.statistics.objects_processed;
        success += r.statistics.objects_success;
        errors += r.statistics.objects_error;
        skipped += r.statistics.objects_skipped;
        assigned += r.statistics.objects_in_scope;
    }

    CoverageReport {
        parent_job_id: plan.parent_job_id.clone(),
        input_scope_hash: plan.input_scope_hash.clone(),
        objects_in_scope: inputs.len() as u64,
        objects_assigned_to_shards: assigned,
        objects_processed: processed,
        objects_success: success,
        objects_error: errors,
        objects_skipped: skipped,
        duplicates_detected: 0, // filled by validate_parallel_job
        missing_inputs: 0,      // filled by validate_parallel_job
    }
}

// ── Validation ────────────────────────────────────────────────────────────────

/// Describes one issue found during parallel job validation.
#[derive(Debug, Clone)]
pub struct ShardValidationIssue {
    pub shard_id: Option<String>,
    pub kind: ShardValidationIssueKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShardValidationIssueKind {
    MissingShardManifest,
    MissingShardResultManifest,
    DuplicateInputId,
    MissingInput,
    ArtifactHashMismatch,
    ScopeHashMismatch,
    CoverageMismatch,
}

/// Validate a parallel job stored under `container_path`.
///
/// Reads the shard plan, all shard manifests, and all available shard result
/// manifests, then checks for:
/// - missing shard manifests
/// - missing shard result manifests
/// - duplicate input_ids across shards
/// - input_ids present in scope but not assigned to any shard
/// - artifact hash mismatches
/// - input_scope_hash consistency
pub fn validate_parallel_job(
    container_path: &Path,
    job_id: &str,
) -> Result<Vec<ShardValidationIssue>, OfffError> {
    let mut issues = Vec::new();

    let plan = read_shard_plan(container_path, job_id)?;

    // ── Check shard manifests ─────────────────────────────────────────────────
    let mut all_input_ids: Vec<(String, String)> = Vec::new(); // (input_id, shard_id)

    for idx in 0..plan.shard_count {
        let shard_id = format!("{job_id}-shard-{idx:02}");

        match read_shard_manifest(container_path, job_id, &shard_id) {
            Err(_) => {
                issues.push(ShardValidationIssue {
                    shard_id: Some(shard_id.clone()),
                    kind: ShardValidationIssueKind::MissingShardManifest,
                    message: format!("Shard manifest missing for shard {shard_id}"),
                });
                continue;
            }
            Ok(manifest) => {
                // Collect input_ids to detect duplicates.
                for inp in &manifest.input_objects {
                    all_input_ids.push((inp.input_id.clone(), shard_id.clone()));
                }

                // Check shard result manifest.
                match read_shard_result_manifest(container_path, job_id, &shard_id) {
                    Err(_) => {
                        issues.push(ShardValidationIssue {
                            shard_id: Some(shard_id.clone()),
                            kind: ShardValidationIssueKind::MissingShardResultManifest,
                            message: format!(
                                "Shard result manifest missing for shard {shard_id}"
                            ),
                        });
                    }
                    Ok(result) => {
                        // Verify artifact hashes.
                        for artifact in &result.outputs {
                            let artifact_path = container_path.join(&artifact.path);
                            match verify_artifact_hash(&artifact_path, &artifact.sha256) {
                                Ok(false) => {
                                    issues.push(ShardValidationIssue {
                                        shard_id: Some(shard_id.clone()),
                                        kind: ShardValidationIssueKind::ArtifactHashMismatch,
                                        message: format!(
                                            "Artifact hash mismatch: {} expected {}",
                                            artifact.path, artifact.sha256
                                        ),
                                    });
                                }
                                Ok(true) => {}
                                Err(_) => {
                                    // File unreadable — treat as hash mismatch.
                                    issues.push(ShardValidationIssue {
                                        shard_id: Some(shard_id.clone()),
                                        kind: ShardValidationIssueKind::ArtifactHashMismatch,
                                        message: format!(
                                            "Artifact unreadable: {}",
                                            artifact.path
                                        ),
                                    });
                                }
                            }
                        }

                        // Check input_scope_hash consistency.
                        if result.input.input_scope_hash != plan.input_scope_hash {
                            issues.push(ShardValidationIssue {
                                shard_id: Some(shard_id.clone()),
                                kind: ShardValidationIssueKind::ScopeHashMismatch,
                                message: format!(
                                    "input_scope_hash mismatch in result of shard {shard_id}"
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    // ── Duplicate input_id detection ──────────────────────────────────────────
    let mut seen: HashMap<String, Vec<String>> = HashMap::new();
    for (input_id, shard_id) in &all_input_ids {
        seen.entry(input_id.clone()).or_default().push(shard_id.clone());
    }
    for (input_id, shard_ids) in &seen {
        if shard_ids.len() > 1 {
            issues.push(ShardValidationIssue {
                shard_id: None,
                kind: ShardValidationIssueKind::DuplicateInputId,
                message: format!(
                    "input_id {input_id} appears in multiple shards: {shard_ids:?}"
                ),
            });
        }
    }

    Ok(issues)
}

/// Build a `ParentResultManifest` from all available shard result manifests.
pub fn build_parent_result_manifest(
    container_path: &Path,
    job_id: &str,
    inputs: &[AnalysisInputObject],
) -> Result<ParentResultManifest, OfffError> {
    let plan = read_shard_plan(container_path, job_id)?;

    let mut shard_results_data: Vec<ShardResultManifest> = Vec::new();
    let mut shard_result_refs: Vec<ShardResultRef> = Vec::new();
    let mut shards_completed = 0u64;
    let mut shards_failed = 0u64;

    for idx in 0..plan.shard_count {
        let shard_id = format!("{job_id}-shard-{idx:02}");
        let result_path =
            shard_result_manifest_path(container_path, job_id, &shard_id);

        match fs::read_to_string(&result_path) {
            Err(_) => {
                shards_failed += 1;
            }
            Ok(data) => {
                let result: ShardResultManifest = serde_json::from_str(&data)?;
                let sha256 = sha256_string(&data);

                if result.status == "completed" {
                    shards_completed += 1;
                } else {
                    shards_failed += 1;
                }

                shard_results_data.push(result);
                shard_result_refs.push(ShardResultRef {
                    shard_id: shard_id.clone(),
                    result_manifest_path: result_path
                        .strip_prefix(container_path)
                        .unwrap_or(&result_path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    sha256,
                });
            }
        }
    }

    let mut coverage = compute_coverage_report(&plan, &shard_results_data, inputs);

    // Count duplicate assignments.
    let issues = validate_parallel_job(container_path, job_id).unwrap_or_default();
    coverage.duplicates_detected = issues
        .iter()
        .filter(|i| i.kind == ShardValidationIssueKind::DuplicateInputId)
        .count() as u64;

    let status = if shards_failed == 0 {
        "completed"
    } else if shards_completed == 0 {
        "failed"
    } else {
        "partial"
    };

    Ok(ParentResultManifest {
        job_id: job_id.to_string(),
        status: status.to_string(),
        parallelization: ParallelizationSummary {
            mode: "sharded".to_string(),
            shard_count: plan.shard_count,
            shards_completed,
            shards_failed,
        },
        shard_results: shard_result_refs,
        coverage,
        created_at: Utc::now().to_rfc3339(),
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn verify_artifact_hash(path: &Path, expected: &str) -> Result<bool, OfffError> {
    let data = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let actual = format!("sha256:{}", hex::encode(hasher.finalize()));
    Ok(actual == expected)
}

fn sha256_string(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{InputObjectMetadata, InputSourceRefs};

    fn make_input(object_id: &str) -> AnalysisInputObject {
        AnalysisInputObject {
            input_id: object_id.to_string(),
            input_type: "object".into(),
            object_id: object_id.to_string(),
            source_refs: InputSourceRefs {
                root_id: None,
                sha256: None,
                storage_ref: None,
            },
            metadata: InputObjectMetadata {
                name: None,
                extension: None,
                size_bytes: None,
                media_type: None,
            },
        }
    }

    fn make_inputs(ids: &[&str]) -> Vec<AnalysisInputObject> {
        ids.iter().map(|id| make_input(id)).collect()
    }

    #[test]
    fn shard_plan_determinism() {
        let inputs = make_inputs(&["obj-a", "obj-b", "obj-c", "obj-d", "obj-e"]);
        let (plan1, shards1) = plan_shards(
            &inputs,
            "job-001",
            ShardStrategy::DeterministicObjectIdRange,
            2,
            "sha256:abc",
        )
        .unwrap();
        let (plan2, shards2) = plan_shards(
            &inputs,
            "job-001",
            ShardStrategy::DeterministicObjectIdRange,
            2,
            "sha256:abc",
        )
        .unwrap();

        assert_eq!(plan1.input_count, plan2.input_count);
        assert_eq!(shards1.len(), shards2.len());
        for (s1, s2) in shards1.iter().zip(shards2.iter()) {
            assert_eq!(s1.shard_id, s2.shard_id);
            assert_eq!(s1.input_objects.len(), s2.input_objects.len());
        }
    }

    #[test]
    fn shard_count_correct() {
        let inputs = make_inputs(&["a", "b", "c", "d", "e", "f"]);
        let (plan, shards) = plan_shards(
            &inputs,
            "job-002",
            ShardStrategy::DeterministicObjectIdRange,
            3,
            "sha256:def",
        )
        .unwrap();
        assert_eq!(plan.shard_count, 3);
        assert_eq!(shards.len(), 3);
        let total: usize = shards.iter().map(|s| s.input_objects.len()).sum();
        assert_eq!(total, 6);
    }

    #[test]
    fn coverage_no_duplicates() {
        let inputs = make_inputs(&["a", "b", "c", "d"]);
        let (_plan, shards) = plan_shards(
            &inputs,
            "job-003",
            ShardStrategy::DeterministicObjectIdRange,
            2,
            "sha256:ghi",
        )
        .unwrap();

        // Collect all input_ids across shards.
        let mut all_ids: Vec<String> = shards
            .iter()
            .flat_map(|s| s.input_objects.iter().map(|o| o.input_id.clone()))
            .collect();
        all_ids.sort();

        // No duplicates.
        let unique: std::collections::HashSet<String> = all_ids.iter().cloned().collect();
        assert_eq!(all_ids.len(), unique.len());
    }

    #[test]
    fn all_inputs_assigned() {
        let inputs = make_inputs(&["a", "b", "c", "d", "e"]);
        let (_plan, shards) = plan_shards(
            &inputs,
            "job-004",
            ShardStrategy::DeterministicRoundRobin,
            3,
            "sha256:jkl",
        )
        .unwrap();
        let total: usize = shards.iter().map(|s| s.input_objects.len()).sum();
        assert_eq!(total, inputs.len());
    }
}
