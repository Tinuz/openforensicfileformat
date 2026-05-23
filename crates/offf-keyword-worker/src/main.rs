use std::{fs, path::PathBuf, sync::Mutex};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use rayon::prelude::*;

use offf_core::{
    parquet_io::{read_physical_to_chunk, read_physical_to_chunk_bytes, write_keyword_hits},
    provenance::ProvenanceWriter,
    storage::{read_chunk_verified, ContainerRef},
    types::{JobManifest, KeywordHitRow, ManifestJson},
};

const TOOL_NAME: &str = "offf-keyword-worker";
const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Bytes of context to capture before and after each hit.
const CONTEXT_BYTES: usize = 32;

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
    let _manifest: ManifestJson = serde_json::from_str(&manifest_raw)?;

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
        all_chunks.iter().filter(|c| scope_set.contains(c.chunk_id.as_str())).collect()
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
            encodings.iter().filter_map(|enc| {
                match enc.as_str() {
                    "utf-8" => {
                        Some((kw.clone(), "utf-8".to_string(), kw.as_bytes().to_vec()))
                    }
                    "utf-16le" => {
                        let bytes: Vec<u8> = kw
                            .encode_utf16()
                            .flat_map(|c| c.to_le_bytes())
                            .collect();
                        Some((kw.clone(), "utf-16le".to_string(), bytes))
                    }
                    _ => None,
                }
            })
        })
        .collect();

    // ── Parallel chunk scan ───────────────────────────────────────────────
    let hits: Mutex<Vec<KeywordHitRow>> = Mutex::new(Vec::new());

    scoped_chunks.par_iter().try_for_each(|chunk| -> Result<()> {
        let plaintext = read_chunk_verified(&container, chunk)
            .with_context(|| format!("failed to read chunk {}", chunk.chunk_id))?;

        let mut local_hits: Vec<KeywordHitRow> = Vec::new();

        for (keyword, encoding, pattern) in &patterns {
            if pattern.is_empty() {
                continue;
            }
            for offset in find_all(&plaintext, pattern) {
                let physical_offset = chunk.source_offset + offset as u64;
                let before_start = offset.saturating_sub(CONTEXT_BYTES);
                let after_end = (offset + pattern.len() + CONTEXT_BYTES).min(plaintext.len());
                let context_before =
                    hex::encode(&plaintext[before_start..offset]);
                let context_after =
                    hex::encode(&plaintext[(offset + pattern.len())..after_end]);

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
        Ok(())
    })?;

    let mut all_hits = hits.into_inner().unwrap();
    // Sort by physical offset for deterministic output
    all_hits.sort_by_key(|h| (h.physical_offset, h.encoding.clone(), h.keyword.clone()));
    // Assign hit IDs
    for (i, hit) in all_hits.iter_mut().enumerate() {
        hit.hit_id = format!("hit-{i:08}");
    }

    println!("Hits found: {}", all_hits.len());

    // ── Write Parquet ─────────────────────────────────────────────────────
    let rel_output = "analysis/keyword_hits.parquet";
    let output_display = match container.local_path(rel_output) {
        Some(path) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            write_keyword_hits(&path, &all_hits)
                .context("failed to write keyword_hits.parquet")?;
            path.display().to_string()
        }
        None => {
            let tmp = tempfile::NamedTempFile::new()?;
            write_keyword_hits(tmp.path(), &all_hits)
                .context("failed to write keyword_hits.parquet")?;
            let bytes = fs::read(tmp.path())?;
            container.write_bytes(rel_output, &bytes)?;
            format!("{}/{}", container.display(), rel_output)
        }
    };
    println!("Written:    {output_display}");

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
            "output": output_display,
        }),
    )?;

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
