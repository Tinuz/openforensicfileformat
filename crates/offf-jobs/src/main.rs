use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use offf_core::{
    parquet_io::read_physical_to_chunk,
    types::{JobManifest, JobScope, ManifestJson, ToolInfo},
};

const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "offf-jobs",
    about = "Create and manage OFFF analysis job manifests",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Create a keyword-scan job manifest
    CreateKeyword {
        /// Path to the OFFF container
        #[arg(long)]
        case: PathBuf,
        /// Comma-separated keywords to search for
        #[arg(long)]
        keywords: String,
        /// Comma-separated encodings: utf-8, utf-16le (default: both)
        #[arg(long, default_value = "utf-8,utf-16le")]
        encoding: String,
        /// Chunks to scan: "all" or comma-separated "sha256:…" IDs
        #[arg(long, default_value = "all")]
        chunks: String,
        /// Output path for the job manifest JSON
        #[arg(long, short)]
        output: PathBuf,
    },
    /// Create a YARA-scan job manifest
    CreateYara {
        /// Path to the OFFF container
        #[arg(long)]
        case: PathBuf,
        /// Path to YARA rules file (.yar / .yara)
        #[arg(long)]
        rules: PathBuf,
        /// Chunks to scan: "all" or comma-separated "sha256:…" IDs
        #[arg(long, default_value = "all")]
        chunks: String,
        /// Output path for the job manifest JSON
        #[arg(long, short)]
        output: PathBuf,
    },
    /// List all job manifests stored in a container
    List {
        /// Path to the OFFF container
        container: PathBuf,
    },
    /// Run a job with retry and runtime state tracking
    Run {
        /// Path to the OFFF container
        #[arg(long)]
        case: PathBuf,
        /// Path to the job manifest JSON
        #[arg(long)]
        job: PathBuf,
        /// Worker identifier used in audit/health logs
        #[arg(long, default_value = "worker-0")]
        worker_id: String,
        /// Maximum retries after the first attempt
        #[arg(long, default_value_t = 2)]
        max_retries: u32,
        /// Force replay even if a successful deterministic replay already exists
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::CreateKeyword {
            case,
            keywords,
            encoding,
            chunks,
            output,
        } => cmd_create_keyword(&case, &keywords, &encoding, &chunks, &output),
        Command::CreateYara {
            case,
            rules,
            chunks,
            output,
        } => cmd_create_yara(&case, &rules, &chunks, &output),
        Command::List { container } => cmd_list(&container),
        Command::Run {
            case,
            job,
            worker_id,
            max_retries,
            force,
        } => cmd_run(&case, &job, &worker_id, max_retries, force),
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum JobRuntimeStatus {
    Running,
    RetryScheduled,
    Succeeded,
    FailedTerminal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobRuntimeState {
    job_id: String,
    replay_id: String,
    worker_id: String,
    task: String,
    status: JobRuntimeStatus,
    attempt: u32,
    max_retries: u32,
    assigned_worker: String,
    updated_at: String,
    last_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct AssignmentAuditEvent {
    event_id: String,
    timestamp: String,
    job_id: String,
    replay_id: String,
    attempt: u32,
    worker_id: String,
    task: String,
}

#[derive(Debug, Serialize)]
struct WorkerHealthEvent {
    event_id: String,
    timestamp: String,
    worker_id: String,
    status: String,
    job_id: String,
    attempt: u32,
    detail: String,
}

// ── offf-jobs create-keyword ──────────────────────────────────────────────────

fn cmd_create_keyword(
    case: &Path,
    keywords_csv: &str,
    encoding_csv: &str,
    chunks_arg: &str,
    output: &Path,
) -> Result<()> {
    let (case_id, chunk_ids) = resolve_case(case, chunks_arg)?;

    let job_id = format!("job-{}", uuid::Uuid::new_v4());
    let keywords: Vec<String> = keywords_csv
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let encodings: Vec<String> = encoding_csv
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let manifest = JobManifest {
        job_id: job_id.clone(),
        created_at: Utc::now(),
        case_id,
        task: "keyword_scan".to_string(),
        scope: JobScope {
            chunks: chunk_ids.clone(),
        },
        tool: ToolInfo {
            name: "offf-keyword-worker".to_string(),
            version: TOOL_VERSION.to_string(),
        },
        parameters: serde_json::json!({
            "keywords": keywords,
            "encoding": encodings,
        }),
    };

    write_manifest(case, &manifest, output)?;

    println!("Job ID:    {job_id}");
    println!("Task:      keyword_scan");
    println!("Keywords:  {keywords_csv}");
    println!("Encodings: {encoding_csv}");
    println!("Chunks:    {} in scope", chunk_ids.len());
    println!("Written:   {}", output.display());

    Ok(())
}

// ── offf-jobs create-yara ─────────────────────────────────────────────────────

fn cmd_create_yara(case: &Path, rules_path: &Path, chunks_arg: &str, output: &Path) -> Result<()> {
    let (case_id, chunk_ids) = resolve_case(case, chunks_arg)?;

    // Read and hash the rules file (strip UTF-8 BOM if present)
    let rules_raw = fs::read(rules_path)
        .with_context(|| format!("cannot read rules file: {}", rules_path.display()))?;
    let rules_bytes = rules_raw
        .strip_prefix(b"\xEF\xBB\xBF")
        .unwrap_or(&rules_raw);
    let rules_text = std::str::from_utf8(rules_bytes).context("rules file is not valid UTF-8")?;
    let rules_hash = hex_sha256(rules_bytes);

    let job_id = format!("job-{}", uuid::Uuid::new_v4());

    let manifest = JobManifest {
        job_id: job_id.clone(),
        created_at: Utc::now(),
        case_id,
        task: "yara_scan".to_string(),
        scope: JobScope {
            chunks: chunk_ids.clone(),
        },
        tool: ToolInfo {
            name: "offf-yara-worker".to_string(),
            version: TOOL_VERSION.to_string(),
        },
        parameters: serde_json::json!({
            "rules_path": rules_path.display().to_string(),
            "rules_hash": format!("sha256:{rules_hash}"),
            "rules_inline": rules_text,
        }),
    };

    write_manifest(case, &manifest, output)?;

    println!("Job ID:      {job_id}");
    println!("Task:        yara_scan");
    println!("Rules file:  {}", rules_path.display());
    println!("Rules hash:  sha256:{rules_hash}");
    println!("Chunks:      {} in scope", chunk_ids.len());
    println!("Written:     {}", output.display());

    Ok(())
}

// ── offf-jobs list ────────────────────────────────────────────────────────────

fn cmd_list(container: &Path) -> Result<()> {
    let jobs_dir = container.join("jobs");
    if !jobs_dir.exists() {
        println!("No jobs directory found in container.");
        return Ok(());
    }

    let mut entries: Vec<_> = fs::read_dir(&jobs_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    if entries.is_empty() {
        println!("No job manifests found.");
        return Ok(());
    }

    println!("{:<40}  {:<16}  created_at", "job_id", "task");
    println!("{}", "-".repeat(80));
    for entry in entries {
        let raw = fs::read_to_string(entry.path())?;
        if let Ok(m) = serde_json::from_str::<JobManifest>(&raw) {
            println!(
                "{:<40}  {:<16}  {}",
                m.job_id,
                m.task,
                m.created_at.to_rfc3339()
            );
        }
    }

    Ok(())
}

fn cmd_run(
    case: &Path,
    job_path: &Path,
    worker_id: &str,
    max_retries: u32,
    force: bool,
) -> Result<()> {
    let job_raw = fs::read_to_string(job_path)
        .with_context(|| format!("cannot read job manifest: {}", job_path.display()))?;
    let job: JobManifest = serde_json::from_str(&job_raw).context("invalid job manifest")?;

    let runtime_dir = case.join("jobs").join("runtime");
    fs::create_dir_all(&runtime_dir)?;
    let state_path = runtime_dir.join(format!("{}.state.json", job.job_id));
    let assignment_path = runtime_dir.join("assignment_audit.jsonl");
    let health_path = runtime_dir.join("worker_health.jsonl");

    let replay_id = compute_replay_id(case, &job, &job_raw);
    let state = load_state_if_exists(&state_path)?;

    if let Some(existing) = &state {
        if existing.replay_id == replay_id
            && existing.status == JobRuntimeStatus::Succeeded
            && !force
        {
            println!(
                "Deterministic replay hit: job {} already succeeded with replay_id {}",
                job.job_id, replay_id
            );
            return Ok(());
        }
    }

    let initial_attempt = state.as_ref().map(|s| s.attempt).unwrap_or(0);
    let max_attempts = max_retries + 1;
    if initial_attempt >= max_attempts && !force {
        anyhow::bail!(
            "job reached terminal state at attempt {} (max retries {})",
            initial_attempt,
            max_retries
        );
    }

    let start_attempt = if force { 0 } else { initial_attempt };
    for attempt in (start_attempt + 1)..=max_attempts {
        let mut runtime = JobRuntimeState {
            job_id: job.job_id.clone(),
            replay_id: replay_id.clone(),
            worker_id: worker_id.to_string(),
            task: job.task.clone(),
            status: JobRuntimeStatus::Running,
            attempt,
            max_retries,
            assigned_worker: worker_id.to_string(),
            updated_at: Utc::now().to_rfc3339(),
            last_error: None,
        };
        save_runtime_state(&state_path, &runtime)?;

        append_jsonl(
            &assignment_path,
            &AssignmentAuditEvent {
                event_id: format!("assign-{}-{:03}", job.job_id, attempt),
                timestamp: Utc::now().to_rfc3339(),
                job_id: job.job_id.clone(),
                replay_id: replay_id.clone(),
                attempt,
                worker_id: worker_id.to_string(),
                task: job.task.clone(),
            },
        )?;

        append_jsonl(
            &health_path,
            &WorkerHealthEvent {
                event_id: format!("health-{}-{:03}-start", worker_id, attempt),
                timestamp: Utc::now().to_rfc3339(),
                worker_id: worker_id.to_string(),
                status: "busy".to_string(),
                job_id: job.job_id.clone(),
                attempt,
                detail: "job attempt started".to_string(),
            },
        )?;

        let run_outcome = run_worker_task(&job.task, case, job_path, worker_id)?;

        if run_outcome.success {
            runtime.status = JobRuntimeStatus::Succeeded;
            runtime.updated_at = Utc::now().to_rfc3339();
            runtime.last_error = None;
            save_runtime_state(&state_path, &runtime)?;

            append_jsonl(
                &health_path,
                &WorkerHealthEvent {
                    event_id: format!("health-{}-{:03}-ok", worker_id, attempt),
                    timestamp: Utc::now().to_rfc3339(),
                    worker_id: worker_id.to_string(),
                    status: "healthy".to_string(),
                    job_id: job.job_id.clone(),
                    attempt,
                    detail: "job attempt completed".to_string(),
                },
            )?;

            println!(
                "Job {} completed successfully (attempt {}/{})",
                job.job_id, attempt, max_attempts
            );
            return Ok(());
        }

        runtime.status = terminal_status_for_failure(attempt, max_retries);
        runtime.updated_at = Utc::now().to_rfc3339();
        runtime.last_error = Some(run_outcome.error_message.clone());
        save_runtime_state(&state_path, &runtime)?;

        append_jsonl(
            &health_path,
            &WorkerHealthEvent {
                event_id: format!("health-{}-{:03}-fail", worker_id, attempt),
                timestamp: Utc::now().to_rfc3339(),
                worker_id: worker_id.to_string(),
                status: "degraded".to_string(),
                job_id: job.job_id.clone(),
                attempt,
                detail: run_outcome.error_message.clone(),
            },
        )?;

        if runtime.status == JobRuntimeStatus::FailedTerminal {
            anyhow::bail!(
                "job {} failed terminally at attempt {}: {}",
                job.job_id,
                attempt,
                run_outcome.error_message
            );
        }
    }

    anyhow::bail!("job did not reach a terminal outcome")
}

struct RunOutcome {
    success: bool,
    error_message: String,
}

fn run_worker_task(
    task: &str,
    case: &Path,
    job_path: &Path,
    worker_id: &str,
) -> Result<RunOutcome> {
    let package = match task {
        "keyword_scan" => "offf-keyword-worker",
        "yara_scan" => "offf-yara-worker",
        other => anyhow::bail!("unsupported job task for runner: {other}"),
    };

    let output = ProcessCommand::new("cargo")
        .arg("run")
        .arg("-p")
        .arg(package)
        .arg("--")
        .arg("--case")
        .arg(case)
        .arg("--job")
        .arg(job_path)
        .arg("--worker-id")
        .arg(worker_id)
        .output()
        .with_context(|| format!("failed to spawn worker package {package}"))?;

    if output.status.success() {
        return Ok(RunOutcome {
            success: true,
            error_message: String::new(),
        });
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let msg = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("worker exited with status {}", output.status)
    };

    Ok(RunOutcome {
        success: false,
        error_message: msg,
    })
}

fn terminal_status_for_failure(attempt: u32, max_retries: u32) -> JobRuntimeStatus {
    if attempt <= max_retries {
        JobRuntimeStatus::RetryScheduled
    } else {
        JobRuntimeStatus::FailedTerminal
    }
}

fn compute_replay_id(case: &Path, job: &JobManifest, job_raw: &str) -> String {
    let case_norm = case.to_string_lossy();
    let payload = format!("{}|{}|{}|{}", case_norm, job.job_id, job.task, job_raw);
    format!("sha256:{}", hex_sha256(payload.as_bytes()))
}

fn load_state_if_exists(path: &Path) -> Result<Option<JobRuntimeState>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    let state: JobRuntimeState = serde_json::from_str(&raw)?;
    Ok(Some(state))
}

fn save_runtime_state(path: &Path, state: &JobRuntimeState) -> Result<()> {
    let json = serde_json::to_string_pretty(state)?;
    fs::write(path, json)?;
    Ok(())
}

fn append_jsonl<T: Serialize>(path: &Path, event: &T) -> Result<()> {
    let mut line = serde_json::to_string(event)?;
    line.push('\n');
    if !path.exists() {
        fs::write(path, line)?;
    } else {
        use std::io::Write;
        let mut f = fs::OpenOptions::new().append(true).open(path)?;
        f.write_all(line.as_bytes())?;
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Load case_id from manifest and resolve the chunk scope.
/// Returns (case_id, Vec<chunk_ids>).
fn resolve_case(case: &Path, chunks_arg: &str) -> Result<(String, Vec<String>)> {
    let manifest_raw = fs::read_to_string(case.join("manifest.json"))
        .context("manifest.json not found – is this an OFFF container?")?;
    let manifest: ManifestJson = serde_json::from_str(&manifest_raw)?;

    let chunk_ids = if chunks_arg == "all" {
        let map_path = case.join(&manifest.indexes.physical_to_chunk);
        let chunks = read_physical_to_chunk(&map_path)
            .context("failed to read physical_to_chunk.parquet")?;
        chunks.into_iter().map(|c| c.chunk_id).collect()
    } else {
        chunks_arg
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    };

    Ok((manifest.container_id, chunk_ids))
}

/// Write the job manifest both to the output path and into the container's
/// `jobs/` directory.
fn write_manifest(case: &Path, manifest: &JobManifest, output: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(manifest)?;

    // Write to requested output path
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(output, &json)?;

    // Also store inside the container
    let jobs_dir = case.join("jobs");
    fs::create_dir_all(&jobs_dir)?;
    let container_path = jobs_dir.join(format!("{}.json", manifest.job_id));
    fs::write(container_path, &json)?;

    Ok(())
}

fn hex_sha256(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_id_is_deterministic_for_same_input() {
        let case = PathBuf::from("tests/samples/4orensics.case2.offf");
        let job = JobManifest {
            job_id: "job-1".to_string(),
            created_at: Utc::now(),
            case_id: "urn:offf:case:test".to_string(),
            task: "keyword_scan".to_string(),
            scope: JobScope {
                chunks: vec!["*".to_string()],
            },
            tool: ToolInfo {
                name: "t".to_string(),
                version: "1".to_string(),
            },
            parameters: serde_json::json!({"k": ["x"]}),
        };
        let raw = serde_json::to_string(&job).unwrap();

        let a = compute_replay_id(&case, &job, &raw);
        let b = compute_replay_id(&case, &job, &raw);

        assert_eq!(a, b);
    }

    #[test]
    fn failure_transitions_to_terminal_after_last_attempt() {
        assert_eq!(
            terminal_status_for_failure(1, 2),
            JobRuntimeStatus::RetryScheduled
        );
        assert_eq!(
            terminal_status_for_failure(2, 2),
            JobRuntimeStatus::RetryScheduled
        );
        assert_eq!(
            terminal_status_for_failure(3, 2),
            JobRuntimeStatus::FailedTerminal
        );
    }
}
