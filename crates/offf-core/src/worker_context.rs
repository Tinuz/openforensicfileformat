/// AnalysisWorkerContext — shard-aware worker helper.
///
/// A worker opens a `ShardManifest`, processes its inputs, records results /
/// errors / skips, and then calls `commit()`.  `commit()` uses a staged
/// atomic commit pattern:
///
/// 1. Write `results.jsonl`, `errors.jsonl`, `skipped.jsonl` to a `.tmp/`
///    staging directory.
/// 2. Compute SHA-256 of each artifact.
/// 3. Write `shard_result_manifest.json` into the staging directory.
/// 4. Rename the staging directory to the final shard directory.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::{
    error::OfffError,
    evidence::read_evidence_object,
    shard::{read_shard_manifest, shard_final_dir, shard_staging_dir, write_shard_result_manifest},
    types::{
        ArtifactRef, ShardInputRef, ShardInputSummary, ShardManifest, ShardResultManifest,
        ShardStatistics, WorkerErrorRow, WorkerIdentity, WorkerSkippedRow,
    },
};

pub struct AnalysisWorkerContext {
    container_path: PathBuf,
    shard_manifest: ShardManifest,
    staging_dir: PathBuf,
    result_rows: Vec<serde_json::Value>,
    error_rows: Vec<WorkerErrorRow>,
    skipped_rows: Vec<WorkerSkippedRow>,
}

impl AnalysisWorkerContext {
    /// Open a worker context from a shard manifest file.
    ///
    /// `container_path` is the root of the OFFF container.
    /// `shard_manifest_path` is the path to the `shard_manifest.json` for this
    /// worker's shard.
    pub fn open(container_path: &Path, shard_manifest_path: &Path) -> Result<Self, OfffError> {
        let data = fs::read_to_string(shard_manifest_path)?;
        let shard_manifest: ShardManifest = serde_json::from_str(&data)?;
        let staging_dir = shard_staging_dir(
            container_path,
            &shard_manifest.parent_job_id,
            &shard_manifest.shard_id,
        );
        Ok(Self {
            container_path: container_path.to_path_buf(),
            shard_manifest,
            staging_dir,
            result_rows: Vec::new(),
            error_rows: Vec::new(),
            skipped_rows: Vec::new(),
        })
    }

    /// Open a worker context by reading the shard manifest from the standard
    /// container path given a job_id and shard_id.
    pub fn open_by_ids(
        container_path: &Path,
        job_id: &str,
        shard_id: &str,
    ) -> Result<Self, OfffError> {
        let shard_manifest = read_shard_manifest(container_path, job_id, shard_id)?;
        let staging_dir = shard_staging_dir(container_path, job_id, shard_id);
        Ok(Self {
            container_path: container_path.to_path_buf(),
            shard_manifest,
            staging_dir,
            result_rows: Vec::new(),
            error_rows: Vec::new(),
            skipped_rows: Vec::new(),
        })
    }

    /// Returns the list of input objects assigned to this shard.
    pub fn inputs(&self) -> &[ShardInputRef] {
        &self.shard_manifest.input_objects
    }

    /// Read and verify the content of an input object.
    ///
    /// For `file_collection` containers, this reads the content-addressed
    /// evidence object using the `storage_ref` stored in the object index.
    /// Verification (SHA-256) is performed by `read_evidence_object`.
    ///
    /// Returns `Err` when the object cannot be read or its hash does not match.
    pub fn read_input_verified(
        &self,
        _input: &ShardInputRef,
        storage_ref: &str,
    ) -> Result<Vec<u8>, OfffError> {
        read_evidence_object(&self.container_path, storage_ref)
    }

    /// Append a generic JSON result row; will be written to `results.jsonl`.
    pub fn write_result_row(&mut self, row: serde_json::Value) {
        self.result_rows.push(row);
    }

    /// Record a worker error for one input object.
    pub fn record_error(&mut self, row: WorkerErrorRow) {
        self.error_rows.push(row);
    }

    /// Record a skipped input object.
    pub fn record_skipped(&mut self, row: WorkerSkippedRow) {
        self.skipped_rows.push(row);
    }

    /// Commit the shard output atomically.
    ///
    /// 1. Creates a `.tmp/` staging directory.
    /// 2. Writes `results.jsonl`, `errors.jsonl`, `skipped.jsonl`.
    /// 3. Computes SHA-256 for each artifact.
    /// 4. Writes `shard_result_manifest.json` into staging.
    /// 5. Renames `.tmp/` → final shard directory.
    ///
    /// Returns the `ShardResultManifest` that was written.
    pub fn commit(
        &mut self,
        worker: WorkerIdentity,
    ) -> Result<ShardResultManifest, OfffError> {
        let shard_id = &self.shard_manifest.shard_id;
        let parent_job_id = &self.shard_manifest.parent_job_id;
        let now = Utc::now().to_rfc3339();

        // Create staging directory.
        fs::create_dir_all(&self.staging_dir)?;

        let mut artifacts: Vec<ArtifactRef> = Vec::new();

        // Write results.jsonl.
        if !self.result_rows.is_empty() {
            let path = self.staging_dir.join("results.jsonl");
            let sha256 = write_jsonl(&path, &self.result_rows)?;
            let rel = rel_path(&self.container_path, &path);
            artifacts.push(ArtifactRef {
                path: rel,
                sha256,
                schema_ref: None,
            });
        }

        // Write errors.jsonl.
        if !self.error_rows.is_empty() {
            let path = self.staging_dir.join("errors.jsonl");
            let rows: Vec<serde_json::Value> = self
                .error_rows
                .iter()
                .map(|r| serde_json::to_value(r).unwrap())
                .collect();
            let sha256 = write_jsonl(&path, &rows)?;
            let rel = rel_path(&self.container_path, &path);
            artifacts.push(ArtifactRef {
                path: rel,
                sha256,
                schema_ref: Some("schema:offf-worker-error-0.1.0".into()),
            });
        }

        // Write skipped.jsonl.
        if !self.skipped_rows.is_empty() {
            let path = self.staging_dir.join("skipped.jsonl");
            let rows: Vec<serde_json::Value> = self
                .skipped_rows
                .iter()
                .map(|r| serde_json::to_value(r).unwrap())
                .collect();
            let sha256 = write_jsonl(&path, &rows)?;
            let rel = rel_path(&self.container_path, &path);
            artifacts.push(ArtifactRef {
                path: rel,
                sha256,
                schema_ref: Some("schema:offf-worker-skipped-0.1.0".into()),
            });
        }

        let stats = ShardStatistics {
            objects_in_scope: self.shard_manifest.input_objects.len() as u64,
            objects_processed: (self.result_rows.len()
                + self.error_rows.len()
                + self.skipped_rows.len()) as u64,
            objects_success: self.result_rows.len() as u64,
            objects_error: self.error_rows.len() as u64,
            objects_skipped: self.skipped_rows.len() as u64,
        };

        let result = ShardResultManifest {
            job_id: shard_id.clone(),
            parent_job_id: parent_job_id.clone(),
            shard_id: shard_id.clone(),
            status: "completed".to_string(),
            worker,
            input: ShardInputSummary {
                input_scope_hash: self.shard_manifest.input_scope_hash.clone(),
                objects_in_shard: self.shard_manifest.input_objects.len() as u64,
            },
            outputs: artifacts,
            statistics: stats,
            created_at: now.clone(),
            completed_at: Some(now),
        };

        // Write shard_result_manifest.json into staging.
        let manifest_path = self.staging_dir.join("shard_result_manifest.json");
        fs::write(&manifest_path, serde_json::to_string_pretty(&result)?)?;

        // Atomic rename: staging → final directory.
        let final_dir = shard_final_dir(
            &self.container_path,
            parent_job_id,
            shard_id,
        );
        if final_dir.exists() {
            fs::remove_dir_all(&final_dir)?;
        }
        fs::rename(&self.staging_dir, &final_dir)?;

        // Write the result manifest to the standard location via the helper so
        // validate_parallel_job can find it.
        write_shard_result_manifest(&self.container_path, &result)?;

        Ok(result)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn write_jsonl(path: &Path, rows: &[serde_json::Value]) -> Result<String, OfffError> {
    let mut file = fs::File::create(path)?;
    let mut hasher = Sha256::new();
    for row in rows {
        let line = serde_json::to_string(row)?;
        let bytes = format!("{line}\n");
        file.write_all(bytes.as_bytes())?;
        hasher.update(bytes.as_bytes());
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

fn rel_path(container_path: &Path, path: &Path) -> String {
    path.strip_prefix(container_path)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
