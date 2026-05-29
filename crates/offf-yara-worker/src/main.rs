use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use serde::Serialize;

use offf_core::{
    chunk::hex_sha256,
    parquet_io::{
        read_file_index_for_resolution, read_physical_to_chunk, read_physical_to_chunk_bytes,
        write_yara_hits,
    },
    provenance::ProvenanceWriter,
    storage::{read_chunk_verified, ContainerRef},
    types::{JobManifest, ManifestJson, ToolInfo, YaraHitRow},
};

const TOOL_NAME: &str = "offf-yara-worker";
const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
const YARA_HIT_SCHEMA: &str = "offf-yara-hit-row-0.1.0";
const ERROR_SCHEMA: &str = "offf-analysis-error-0.2.0";

#[derive(Debug, Serialize)]
struct ResultManifest {
    job_id: String,
    task: String,
    status: String,
    created_at: String,
    tool: ToolInfo,
    input: ResultManifestInput,
    outputs: ResultManifestOutputs,
    statistics: ResultManifestStats,
}

#[derive(Debug, Serialize)]
struct ResultManifestInput {
    container_id: String,
    source_sha256: String,
    merkle_root_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_ref: Option<String>,
    chunk_count: usize,
}

#[derive(Debug, Serialize)]
struct ResultManifestOutputs {
    analysis_results: Vec<ResultManifestArtifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<ResultManifestArtifact>,
}

#[derive(Debug, Serialize)]
struct ResultManifestArtifact {
    path: String,
    sha256: String,
    schema: String,
}

#[derive(Debug, Serialize)]
struct ResultManifestStats {
    chunks_in_scope: usize,
    chunks_scanned: usize,
    results_written: usize,
    errors: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ScanErrorEntry {
    error_id: String,
    chunk_id: String,
    timestamp: String,
    message: String,
    schema_version: String,
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "offf-yara-worker",
    about = "Scan OFFF container chunks with YARA rules",
    version
)]
struct Cli {
    /// OFFF container path or URI (local path or s3://bucket/prefix)
    #[arg(long)]
    case: String,
    /// Path to the job manifest JSON
    #[arg(long)]
    job: PathBuf,
    /// Worker identifier (used in provenance)
    #[arg(long, default_value = "worker-0")]
    worker_id: String,
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();
    let container = ContainerRef::parse(&cli.case)?;

    // ── Load manifests ────────────────────────────────────────────────────
    let job_raw = fs::read_to_string(&cli.job)
        .with_context(|| format!("cannot read job manifest: {}", cli.job.display()))?;
    let job: JobManifest = serde_json::from_str(&job_raw).context("invalid job manifest")?;

    if job.task != "yara_scan" {
        anyhow::bail!("job task is '{}', expected 'yara_scan'", job.task);
    }

    let manifest_raw = container
        .read_text("manifest.json")
        .context("manifest.json not found – is this an OFFF container?")?;
    let manifest: ManifestJson = serde_json::from_str(&manifest_raw)?;

    // ── Acquisition mode guard ────────────────────────────────────────────
    use offf_core::types::AcquisitionMode;
    if matches!(manifest.effective_mode(), AcquisitionMode::FileCollection) {
        anyhow::bail!(
            "yara_scan is not supported for file_collection containers; \
             use a file-collection–aware worker instead"
        );
    }

    // ── Parse parameters ──────────────────────────────────────────────────
    let rules_text = job.parameters["rules_inline"]
        .as_str()
        .context("missing 'rules_inline' in job parameters")?;
    let ruleset_hash = job.parameters["rules_hash"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    // ── Compile YARA rules ────────────────────────────────────────────────
    let mut compiler = yara_x::Compiler::new();
    compiler
        .add_source(rules_text)
        .context("failed to compile YARA rules")?;
    let rules = compiler.build();

    // ── Load chunks ───────────────────────────────────────────────────────
    let all_chunks = match container.local_path("maps/physical_to_chunk.parquet") {
        Some(p) => read_physical_to_chunk(&p),
        None => {
            let bytes = container.read_bytes("maps/physical_to_chunk.parquet");
            bytes.and_then(|b| read_physical_to_chunk_bytes(&b))
        }
    }
    .context("failed to read physical_to_chunk.parquet")?;

    let scoped_chunks: Vec<_> = if job.scope.chunks == vec!["*"] || job.scope.chunks.is_empty() {
        all_chunks.iter().collect()
    } else {
        let scope_set: std::collections::HashSet<&str> =
            job.scope.chunks.iter().map(|s| s.as_str()).collect();
        all_chunks
            .iter()
            .filter(|c| scope_set.contains(c.chunk_id.as_str()))
            .collect()
    };

    println!("Job:         {}", job.job_id);
    println!("Ruleset:     {ruleset_hash}");
    println!("Chunks:      {}", scoped_chunks.len());
    println!();

    // ── Sequential chunk scan (Scanner is not Send) ───────────────────────
    let mut scanner = yara_x::Scanner::new(&rules);
    let mut all_hits: Vec<YaraHitRow> = Vec::new();
    let mut all_errors: Vec<ScanErrorEntry> = Vec::new();

    for chunk in &scoped_chunks {
        let plaintext = match read_chunk_verified(&container, chunk) {
            Ok(p) => p,
            Err(e) => {
                all_errors.push(ScanErrorEntry {
                    error_id: format!("err-{}", chunk.chunk_id),
                    chunk_id: chunk.chunk_id.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    message: e.to_string(),
                    schema_version: "0.2.0".to_string(),
                });
                continue;
            }
        };

        let results = match scanner.scan(plaintext.as_slice()) {
            Ok(r) => r,
            Err(e) => {
                all_errors.push(ScanErrorEntry {
                    error_id: format!("err-scan-{}", chunk.chunk_id),
                    chunk_id: chunk.chunk_id.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    message: format!("YARA scan failed: {e}"),
                    schema_version: "0.2.0".to_string(),
                });
                continue;
            }
        };

        for matching_rule in results.matching_rules() {
            let rule_name = matching_rule.identifier().to_string();
            for pattern in matching_rule.patterns() {
                for match_ in pattern.matches() {
                    let within_chunk = match_.range().start as u64;
                    let match_length = match_.range().len() as u64;
                    let physical_offset = chunk.source_offset + within_chunk;

                    all_hits.push(YaraHitRow {
                        hit_id: String::new(), // filled after sort
                        job_id: job.job_id.clone(),
                        rule_name: rule_name.clone(),
                        ruleset_hash: ruleset_hash.clone(),
                        chunk_id: chunk.chunk_id.clone(),
                        physical_offset,
                        match_length,
                        file_id: String::new(),
                        worker_id: cli.worker_id.clone(),
                        timestamp: Utc::now().to_rfc3339(),
                    });
                }
            }
        }
    }

    // Sort and assign IDs
    all_hits.sort_by_key(|h| (h.physical_offset, h.rule_name.clone()));
    for (i, hit) in all_hits.iter_mut().enumerate() {
        hit.hit_id = format!("yara-hit-{i:08}");
    }

    // ── file_id resolution (non-fatal) ────────────────────────────────────
    let chunk_to_file: std::collections::HashMap<String, Vec<u64>> = {
        let mut map: std::collections::HashMap<String, Vec<u64>> =
            std::collections::HashMap::new();
        if let Some(fs_base) = container.local_path("indexes/filesystems") {
            if let Ok(rd) = fs::read_dir(&fs_base) {
                for entry in rd.flatten() {
                    let p = entry.path().join("file_index.parquet");
                    if let Ok(rows) = read_file_index_for_resolution(&p) {
                        for (fid, chunk_refs_json) in rows {
                            if let Ok(chunk_ids) =
                                serde_json::from_str::<Vec<String>>(&chunk_refs_json)
                            {
                                for cid in chunk_ids {
                                    map.entry(cid).or_default().push(fid);
                                }
                            }
                        }
                    }
                }
            }
        }
        map
    };
    if !chunk_to_file.is_empty() {
        for hit in &mut all_hits {
            if let Some(fids) = chunk_to_file.get(&hit.chunk_id) {
                if fids.len() == 1 {
                    hit.file_id = fids[0].to_string();
                }
            }
        }
    }

    println!("Hits found:  {}", all_hits.len());

    // ── Write Parquet ─────────────────────────────────────────────────────
    let job_dir = analysis_job_dir(&job.job_id)?;
    let rel_output = format!("{job_dir}/yara_hits.parquet");
    let rel_result_manifest = format!("{job_dir}/result_manifest.json");

    if container.exists(&rel_output)? {
        anyhow::bail!("refusing to overwrite existing analysis output: {rel_output}");
    }
    if container.exists(&rel_result_manifest)? {
        anyhow::bail!("refusing to overwrite existing result manifest: {rel_result_manifest}");
    }

    let output_bytes = match container.local_path(&rel_output) {
        Some(path) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            write_yara_hits(&path, &all_hits).context("failed to write yara_hits.parquet")?;
            fs::read(&path).context("failed to re-read yara_hits.parquet for hashing")?
        }
        None => {
            let tmp = tempfile::NamedTempFile::new()?;
            write_yara_hits(tmp.path(), &all_hits).context("failed to write yara_hits.parquet")?;
            let bytes = fs::read(tmp.path())?;
            container.write_bytes(&rel_output, &bytes)?;
            bytes
        }
    };
    println!("Written:     {rel_output}");

    let output_sha256 = format!("sha256:{}", hex_sha256(&output_bytes));

    // ── Write errors.jsonl (if any) ───────────────────────────────────────
    let errors_artifact = if !all_errors.is_empty() {
        let rel_errors = format!("{job_dir}/errors.jsonl");
        let mut lines = String::new();
        for (i, entry) in all_errors.iter().enumerate() {
            let mut e = entry.clone();
            e.error_id = format!("err-{i:06}");
            lines.push_str(&serde_json::to_string(&e)?);
            lines.push('\n');
        }
        let errors_bytes = lines.into_bytes();
        let errors_sha256 = format!("sha256:{}", hex_sha256(&errors_bytes));
        container
            .write_bytes(&rel_errors, &errors_bytes)
            .context("failed to write errors.jsonl")?;
        println!("Written:     {rel_errors}");
        Some(ResultManifestArtifact {
            path: rel_errors,
            sha256: errors_sha256,
            schema: ERROR_SCHEMA.to_string(),
        })
    } else {
        None
    };

    let status = if all_errors.is_empty() {
        "completed"
    } else {
        "partial"
    };
    let result_manifest = ResultManifest {
        job_id: job.job_id.clone(),
        task: job.task.clone(),
        status: status.to_string(),
        created_at: Utc::now().to_rfc3339(),
        tool: ToolInfo {
            name: TOOL_NAME.to_string(),
            version: TOOL_VERSION.to_string(),
        },
        input: ResultManifestInput {
            container_id: manifest.container_id,
            source_sha256: manifest.hashes.as_ref().map(|h| h.source_sha256.clone()).unwrap_or_default(),
            merkle_root_sha256: manifest.hashes.as_ref().map(|h| h.merkle_root_sha256.clone()).unwrap_or_default(),
            scope_ref: job.scope_ref.clone(),
            chunk_count: scoped_chunks.len(),
        },
        outputs: ResultManifestOutputs {
            analysis_results: vec![ResultManifestArtifact {
                path: rel_output.clone(),
                sha256: output_sha256,
                schema: YARA_HIT_SCHEMA.to_string(),
            }],
            errors: errors_artifact,
        },
        statistics: ResultManifestStats {
            chunks_in_scope: scoped_chunks.len(),
            chunks_scanned: scoped_chunks.len() - all_errors.len(),
            results_written: all_hits.len(),
            errors: all_errors.len(),
        },
    };
    let result_manifest_json = serde_json::to_vec_pretty(&result_manifest)
        .context("failed to serialize result_manifest.json")?;
    container
        .write_bytes(&rel_result_manifest, &result_manifest_json)
        .context("failed to write result_manifest.json")?;
    println!("Written:     {rel_result_manifest}");

    // ── Provenance ────────────────────────────────────────────────────────
    append_provenance(
        &container,
        "yara_scan_complete",
        TOOL_NAME,
        TOOL_VERSION,
        &cli.worker_id,
        serde_json::json!({
            "job_id": job.job_id,
            "ruleset_hash": ruleset_hash,
            "chunks_scanned": scoped_chunks.len(),
            "hits_found": all_hits.len(),
            "output": rel_output,
            "result_manifest": rel_result_manifest,
        }),
    )?;

    // ── Scope-evaluated audit event ───────────────────────────────────────
    if job.scope_ref.is_some() || !job.include_sets.is_empty() || !job.policy_refs.is_empty() {
        let audit_event = serde_json::json!({
            "event_id": format!("scope-eval-{}", job.job_id),
            "timestamp": Utc::now().to_rfc3339(),
            "actor": cli.worker_id,
            "action": "scope_evaluated",
            "target": {
                "kind": "job",
                "id": job.job_id
            },
            "detail": {
                "scope_ref": job.scope_ref,
                "include_sets": job.include_sets,
                "policy_refs": job.policy_refs,
                "chunks_in_scope": scoped_chunks.len(),
                "tool": TOOL_NAME,
            }
        });
        let line = serde_json::to_string(&audit_event)?;
        // Non-fatal: audit write failure does not abort the job.
        let _ = container.append_jsonl_line("extensions/audit/audit_events.jsonl", &line);
    }

    Ok(())
}

fn append_provenance(
    container: &ContainerRef,
    action: &str,
    tool_name: &str,
    tool_version: &str,
    actor: &str,
    details: serde_json::Value,
) -> Result<()> {
    let rel = "provenance/chain_of_custody.jsonl";
    match container {
        ContainerRef::Local(base) => {
            let mut prov = ProvenanceWriter::new(&base.join(rel))?;
            prov.record(action, tool_name, tool_version, actor, details)?;
            Ok(())
        }
        ContainerRef::S3 { .. } => {
            let counter = if container.exists(rel)? {
                container
                    .read_text(rel)?
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .count() as u64
            } else {
                0
            };
            let event = serde_json::json!({
                "event_id": format!("evt-{counter:06}"),
                "timestamp": Utc::now().to_rfc3339(),
                "actor": actor,
                "action": action,
                "tool": {
                    "name": tool_name,
                    "version": tool_version,
                },
                "details": details,
            });
            let line = serde_json::to_string(&event)?;
            container.append_jsonl_line(rel, &line)?;
            Ok(())
        }
    }
}

fn analysis_job_dir(job_id: &str) -> Result<String> {
    let job_id = job_id.trim();
    if job_id.is_empty() {
        anyhow::bail!("job_id is empty");
    }
    if job_id.contains('/') || job_id.contains('\\') || job_id.contains("..") {
        anyhow::bail!("job_id contains invalid path characters");
    }
    Ok(format!("analysis/jobs/{job_id}"))
}
