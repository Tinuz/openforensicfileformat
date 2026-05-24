use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use serde::Serialize;

use offf_core::{
    chunk::hex_sha256,
    parquet_io::{read_physical_to_chunk, read_physical_to_chunk_bytes, write_yara_hits},
    provenance::ProvenanceWriter,
    storage::{read_chunk_verified, ContainerRef},
    types::{JobManifest, ManifestJson, ToolInfo, YaraHitRow},
};

const TOOL_NAME: &str = "offf-yara-worker";
const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
const YARA_HIT_SCHEMA: &str = "offf-yara-hit-row-0.1.0";

#[derive(Debug, Serialize)]
struct ResultManifest {
    job_id: String,
    task: String,
    created_at: String,
    tool: ToolInfo,
    input: ResultManifestInput,
    outputs: Vec<ResultManifestOutput>,
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
struct ResultManifestOutput {
    path: String,
    sha256: String,
    schema: String,
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

    for chunk in &scoped_chunks {
        let plaintext = read_chunk_verified(&container, chunk)
            .with_context(|| format!("failed to read chunk {}", chunk.chunk_id))?;

        let results = scanner
            .scan(plaintext.as_slice())
            .context("YARA scan failed")?;

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
    let result_manifest = ResultManifest {
        job_id: job.job_id.clone(),
        task: job.task.clone(),
        created_at: Utc::now().to_rfc3339(),
        tool: ToolInfo {
            name: TOOL_NAME.to_string(),
            version: TOOL_VERSION.to_string(),
        },
        input: ResultManifestInput {
            container_id: manifest.container_id,
            source_sha256: manifest.hashes.source_sha256,
            merkle_root_sha256: manifest.hashes.merkle_root_sha256,
            scope_ref: None,
            chunk_count: scoped_chunks.len(),
        },
        outputs: vec![ResultManifestOutput {
            path: rel_output.clone(),
            sha256: output_sha256,
            schema: YARA_HIT_SCHEMA.to_string(),
        }],
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
