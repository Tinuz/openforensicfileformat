use std::{fs, path::PathBuf, sync::Mutex};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use rayon::prelude::*;
use serde::Serialize;

use offf_core::{
    chunk::hex_sha256,
    parquet_io::{
        read_file_index_for_resolution, read_physical_to_chunk, read_physical_to_chunk_bytes,
        write_keyword_hits,
    },
    provenance::ProvenanceWriter,
    storage::{read_chunk_verified, ContainerRef},
    types::{ChunkMetadata, JobManifest, KeywordHitRow, ManifestJson, ToolInfo},
};

const TOOL_NAME: &str = "offf-keyword-worker";
const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Bytes of context to capture before and after each hit.
const CONTEXT_BYTES: usize = 32;
const KEYWORD_HIT_SCHEMA: &str = "offf-keyword-hit-row-0.1.0";
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
    name = "offf-keyword-worker",
    about = "Scan OFFF container chunks for keywords",
    version
)]
struct Cli {
    /// OFFF container path or URI (local path or s3://bucket/prefix)
    #[arg(long)]
    case: String,
    /// Path to the job manifest JSON
    #[arg(long)]
    job: PathBuf,
    /// Worker identifier (used in provenance; defaults to hostname or "worker-0")
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

    if job.task != "keyword_scan" {
        anyhow::bail!("job task is '{}', expected 'keyword_scan'", job.task);
    }

    let manifest_raw = container
        .read_text("manifest.json")
        .context("manifest.json not found – is this an OFFF container?")?;
    let manifest: ManifestJson = serde_json::from_str(&manifest_raw)?;

    // ── Acquisition mode guard ────────────────────────────────────────────
    use offf_core::types::AcquisitionMode;
    if matches!(manifest.effective_mode(), AcquisitionMode::FileCollection) {
        anyhow::bail!(
            "keyword_scan is not supported for file_collection containers; \
             use a file-collection–aware worker instead"
        );
    }

    // ── Parse parameters ──────────────────────────────────────────────────
    let keywords: Vec<String> = job.parameters["keywords"]
        .as_array()
        .context("missing 'keywords' in parameters")?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    let encodings: Vec<String> = job.parameters["encoding"]
        .as_array()
        .context("missing 'encoding' in parameters")?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    if keywords.is_empty() {
        anyhow::bail!("no keywords specified in job");
    }

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

    println!("Job:      {}", job.job_id);
    println!("Keywords: {}", keywords.join(", "));
    println!("Chunks:   {}", scoped_chunks.len());
    println!();

    // ── Pre-compute search patterns ───────────────────────────────────────
    // For each keyword × encoding combination produce a byte pattern.
    let patterns: Vec<(String, String, Vec<u8>)> = keywords
        .iter()
        .flat_map(|kw| {
            encodings.iter().filter_map(|enc| match enc.as_str() {
                "utf-8" => Some((kw.clone(), "utf-8".to_string(), kw.as_bytes().to_vec())),
                "utf-16le" => {
                    let bytes: Vec<u8> = kw.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
                    Some((kw.clone(), "utf-16le".to_string(), bytes))
                }
                _ => None,
            })
        })
        .collect();

    // ── Parallel chunk scan ───────────────────────────────────────────────
    let hits: Mutex<Vec<KeywordHitRow>> = Mutex::new(Vec::new());
    let scan_errors: Mutex<Vec<ScanErrorEntry>> = Mutex::new(Vec::new());

    scoped_chunks.par_iter().for_each(|chunk| {
        let plaintext = match read_chunk_verified(&container, chunk) {
            Ok(p) => p,
            Err(e) => {
                scan_errors.lock().unwrap().push(ScanErrorEntry {
                    error_id: format!("err-{}", chunk.chunk_id),
                    chunk_id: chunk.chunk_id.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    message: e.to_string(),
                    schema_version: "0.2.0".to_string(),
                });
                return;
            }
        };

        let mut local_hits: Vec<KeywordHitRow> = Vec::new();

        for (keyword, encoding, pattern) in &patterns {
            if pattern.is_empty() {
                continue;
            }
            for offset in find_all(&plaintext, pattern) {
                let physical_offset = chunk.source_offset + offset as u64;
                let before_start = offset.saturating_sub(CONTEXT_BYTES);
                let after_end = (offset + pattern.len() + CONTEXT_BYTES).min(plaintext.len());
                let context_before = hex::encode(&plaintext[before_start..offset]);
                let context_after = hex::encode(&plaintext[(offset + pattern.len())..after_end]);

                local_hits.push(KeywordHitRow {
                    hit_id: String::new(), // filled in after collection
                    job_id: job.job_id.clone(),
                    keyword: keyword.clone(),
                    chunk_id: chunk.chunk_id.clone(),
                    physical_offset,
                    file_id: String::new(),
                    context_before,
                    context_after,
                    encoding: encoding.clone(),
                    worker_id: cli.worker_id.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                });
            }
        }

        if !local_hits.is_empty() {
            hits.lock().unwrap().extend(local_hits);
        }
    });

    let mut all_hits = hits.into_inner().unwrap();
    let all_errors = scan_errors.into_inner().unwrap();

    // ── Cross-chunk junction scanning ─────────────────────────────────────
    // For adjacent chunk pairs create a small junction buffer containing the
    // tail of chunk A and the head of chunk B, then search it for each pattern.
    let max_pattern_len = patterns.iter().map(|(_, _, p)| p.len()).max().unwrap_or(0);
    if max_pattern_len > 1 && scoped_chunks.len() > 1 {
        // Sort by physical offset so adjacency makes sense.
        let mut sorted_chunks: Vec<&ChunkMetadata> = scoped_chunks.clone();
        sorted_chunks.sort_by_key(|c| c.source_offset);
        for pair in sorted_chunks.windows(2) {
            let (a_meta, b_meta) = (&pair[0], &pair[1]);
            // Only scan truly adjacent chunks (no gap).
            if a_meta.source_offset + a_meta.source_length != b_meta.source_offset {
                continue;
            }
            let overlap = (max_pattern_len - 1).min(a_meta.source_length as usize);
            let a_data = match container.local_path(&format!("chunks/{}", a_meta.chunk_id)) {
                Some(p) => {
                    if let Ok(b) = fs::read(&p) { b } else { continue; }
                }
                None => match container.read_bytes(&format!("chunks/{}", a_meta.chunk_id)) {
                    Ok(b) => b,
                    Err(_) => continue,
                },
            };
            let b_data = match container.local_path(&format!("chunks/{}", b_meta.chunk_id)) {
                Some(p) => {
                    if let Ok(b) = fs::read(&p) { b } else { continue; }
                }
                None => match container.read_bytes(&format!("chunks/{}", b_meta.chunk_id)) {
                    Ok(b) => b,
                    Err(_) => continue,
                },
            };
            let head_a = &a_data[a_data.len().saturating_sub(overlap)..];
            let head_b_len = (max_pattern_len - 1).min(b_data.len());
            let head_b = &b_data[..head_b_len];
            let mut junc = Vec::with_capacity(head_a.len() + head_b.len());
            junc.extend_from_slice(head_a);
            junc.extend_from_slice(head_b);
                    let junc_start_offset = a_meta.source_offset + a_meta.source_length - head_a.len() as u64;

            for (keyword, encoding, pattern) in &patterns {
                for rel_off in find_all(&junc, pattern) {
                    let abs_off = rel_off as u64 + junc_start_offset;
                    // Skip if the hit starts inside chunk A (already found by per-chunk scan).
                    if abs_off < a_meta.source_offset + a_meta.source_length {
                        let ctx_start = rel_off.saturating_sub(CONTEXT_BYTES);
                        let ctx_end = (rel_off + pattern.len() + CONTEXT_BYTES).min(junc.len());
                        let context_before =
                            String::from_utf8_lossy(&junc[ctx_start..rel_off]).into_owned();
                        let context_after = String::from_utf8_lossy(
                            &junc[rel_off + pattern.len()..ctx_end],
                        )
                        .into_owned();
                        all_hits.push(KeywordHitRow {
                            hit_id: String::new(),
                            job_id: job.job_id.clone(),
                            keyword: keyword.clone(),
                            chunk_id: a_meta.chunk_id.clone(),
                            physical_offset: abs_off,
                            file_id: String::new(),
                            context_before,
                            context_after,
                            encoding: encoding.clone(),
                            worker_id: cli.worker_id.clone(),
                            timestamp: Utc::now().to_rfc3339(),
                        });
                    }
                }
            }
        }
    }

    // Sort by physical offset for deterministic output
    all_hits.sort_by_key(|h| (h.physical_offset, h.encoding.clone(), h.keyword.clone()));
    // Assign hit IDs
    for (i, hit) in all_hits.iter_mut().enumerate() {
        hit.hit_id = format!("hit-{i:08}");
    }

    // ── file_id resolution (non-fatal) ────────────────────────────────────
    // Build a map chunk_id → [file_id] from all file_index.parquet files under
    // indexes/filesystems/*/file_index.parquet.
    let chunk_to_file: std::collections::HashMap<String, Vec<u64>> = {
        let mut map: std::collections::HashMap<String, Vec<u64>> = std::collections::HashMap::new();
        let fs_glob = container.local_path("indexes/filesystems");
        if let Some(fs_base) = fs_glob {
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

    println!("Hits found: {}", all_hits.len());

    // ── Write Parquet ─────────────────────────────────────────────────────
    let job_dir = analysis_job_dir(&job.job_id)?;
    let rel_output = format!("{job_dir}/keyword_hits.parquet");
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
            write_keyword_hits(&path, &all_hits).context("failed to write keyword_hits.parquet")?;
            fs::read(&path).context("failed to re-read keyword_hits.parquet for hashing")?
        }
        None => {
            let tmp = tempfile::NamedTempFile::new()?;
            write_keyword_hits(tmp.path(), &all_hits)
                .context("failed to write keyword_hits.parquet")?;
            let bytes = fs::read(tmp.path())?;
            container.write_bytes(&rel_output, &bytes)?;
            bytes
        }
    };
    println!("Written:    {rel_output}");

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
        println!("Written:    {rel_errors}");
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
                schema: KEYWORD_HIT_SCHEMA.to_string(),
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
    println!("Written:    {rel_result_manifest}");

    // ── Provenance ────────────────────────────────────────────────────────
    append_provenance(
        &container,
        "keyword_scan_complete",
        TOOL_NAME,
        TOOL_VERSION,
        &cli.worker_id,
        serde_json::json!({
            "job_id": job.job_id,
            "keywords": keywords,
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

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Find all (non-overlapping) occurrences of `needle` in `haystack`.
/// Returns byte offsets.
fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    let n = needle.len();
    if n == 0 || haystack.len() < n {
        return vec![];
    }
    let mut offsets = Vec::new();
    let mut start = 0;
    while start + n <= haystack.len() {
        if let Some(pos) = haystack[start..].windows(n).position(|w| w == needle) {
            offsets.push(start + pos);
            start += pos + n;
        } else {
            break;
        }
    }
    offsets
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
