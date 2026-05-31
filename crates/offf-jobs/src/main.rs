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
    scope::{compute_input_scope_hash, resolve_analysis_scope},
    shard::{
        build_parent_result_manifest, plan_shards, write_parent_result_manifest,
        write_shard_manifest, write_shard_plan,
    },
    types::{JobManifest, JobOutputContract, JobScope, ManifestJson, ShardStrategy, ToolInfo},
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
        /// Reference to a ScopeRecord ID in extensions/scopes/scopes.jsonl
        #[arg(long)]
        scope_ref: Option<String>,
        /// Set IDs to restrict processing to (repeatable: --include-set ws-001 --include-set ws-002)
        #[arg(long, value_name = "SET_ID")]
        include_set: Vec<String>,
        /// External policy references (repeatable)
        #[arg(long, value_name = "POLICY_REF")]
        policy_ref: Vec<String>,
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
        /// Reference to a ScopeRecord ID in extensions/scopes/scopes.jsonl
        #[arg(long)]
        scope_ref: Option<String>,
        /// Set IDs to restrict processing to (repeatable: --include-set ws-001 --include-set ws-002)
        #[arg(long, value_name = "SET_ID")]
        include_set: Vec<String>,
        /// External policy references (repeatable)
        #[arg(long, value_name = "POLICY_REF")]
        policy_ref: Vec<String>,
        /// Output path for the job manifest JSON
        #[arg(long, short)]
        output: PathBuf,
    },
    /// Create an object-producing worker job manifest
    CreateObjectWorker {
        /// Path to the OFFF container
        #[arg(long)]
        case: PathBuf,
        /// Task name for this job
        #[arg(long)]
        task: String,
        /// Tool name to record in the manifest
        #[arg(long)]
        tool_name: String,
        /// Tool version
        #[arg(long, default_value = "0.1.0")]
        tool_version: String,
        /// Chunks to scan: "all" or comma-separated "sha256:…" IDs
        #[arg(long, default_value = "all")]
        chunks: String,
        /// Allow this job to discover objects
        #[arg(long, default_value_t = false)]
        may_produce_objects: bool,
        /// Allow this job to produce object edges
        #[arg(long, default_value_t = false)]
        may_produce_edges: bool,
        /// Allow this job to produce derivations
        #[arg(long, default_value_t = false)]
        may_produce_derivations: bool,
        /// Allow this job to materialize objects
        #[arg(long, default_value_t = false)]
        may_materialize_objects: bool,
        /// Reference to a ScopeRecord ID in extensions/scopes/scopes.jsonl
        #[arg(long)]
        scope_ref: Option<String>,
        /// Set IDs to restrict processing to (repeatable)
        #[arg(long, value_name = "SET_ID")]
        include_set: Vec<String>,
        /// External policy references (repeatable)
        #[arg(long, value_name = "POLICY_REF")]
        policy_ref: Vec<String>,
        /// Output path for the job manifest JSON
        #[arg(long, short)]
        output: PathBuf,
    },
    /// List all job manifests stored in a container
    List {
        /// Path to the OFFF container
        container: PathBuf,
    },
    /// Plan shards for a parallel analysis job
    PlanShards {
        /// Path to the OFFF container
        #[arg(long)]
        case: PathBuf,
        /// Job ID of the parent job manifest (in jobs/{job_id}.json)
        #[arg(long)]
        job_id: String,
        /// Number of shards to create
        #[arg(long, default_value = "4")]
        shard_count: usize,
        /// Shard strategy: deterministic_object_id_range | deterministic_round_robin | deterministic_hash_modulo
        #[arg(long, default_value = "deterministic_object_id_range")]
        strategy: String,
    },
    /// Finalise a parallel job by writing the parent result manifest
    FinalizeJob {
        /// Path to the OFFF container
        #[arg(long)]
        case: PathBuf,
        /// Job ID of the parent job
        #[arg(long)]
        job_id: String,
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
            scope_ref,
            include_set,
            policy_ref,
            output,
        } => cmd_create_keyword(&case, &keywords, &encoding, &chunks, scope_ref, include_set, policy_ref, &output),
        Command::CreateYara {
            case,
            rules,
            chunks,
            scope_ref,
            include_set,
            policy_ref,
            output,
        } => cmd_create_yara(&case, &rules, &chunks, scope_ref, include_set, policy_ref, &output),
        Command::List { container } => cmd_list(&container),
        Command::PlanShards {
            case,
            job_id,
            shard_count,
            strategy,
        } => cmd_plan_shards(&case, &job_id, shard_count, &strategy),
        Command::FinalizeJob { case, job_id } => cmd_finalize_job(&case, &job_id),
        Command::Run {
            case,
            job,
            worker_id,
            max_retries,
            force,
        } => cmd_run(&case, &job, &worker_id, max_retries, force),
        Command::CreateObjectWorker {
            case,
            task,
            tool_name,
            tool_version,
            chunks,
            may_produce_objects,
            may_produce_edges,
            may_produce_derivations,
            may_materialize_objects,
            scope_ref,
            include_set,
            policy_ref,
            output,
        } => cmd_create_object_worker(
            &case,
            &task,
            &tool_name,
            &tool_version,
            &chunks,
            may_produce_objects,
            may_produce_edges,
            may_produce_derivations,
            may_materialize_objects,
            scope_ref,
            include_set,
            policy_ref,
            &output,
        ),
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

// ── offf-jobs create-object-worker ────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn cmd_create_object_worker(
    case: &Path,
    task: &str,
    tool_name: &str,
    tool_version: &str,
    chunks_arg: &str,
    may_produce_objects: bool,
    may_produce_edges: bool,
    may_produce_derivations: bool,
    may_materialize_objects: bool,
    scope_ref: Option<String>,
    include_sets: Vec<String>,
    policy_refs: Vec<String>,
    output: &Path,
) -> Result<()> {
    let (case_id, chunk_ids) = resolve_case(case, chunks_arg)?;

    let job_id = format!("job-{}", uuid::Uuid::new_v4());
    let manifest = JobManifest {
        job_id: job_id.clone(),
        created_at: Utc::now(),
        case_id,
        task: task.to_string(),
        scope: JobScope { chunks: chunk_ids },
        tool: ToolInfo {
            name: tool_name.to_string(),
            version: tool_version.to_string(),
        },
        input_scope: None,
        output_contract: Some(JobOutputContract {
            may_produce_results: true,
            may_produce_objects,
            may_produce_edges,
            may_produce_derivations,
            may_materialize_objects,
        }),
        scope_ref,
        include_sets,
        policy_refs,
        parameters: serde_json::Value::Object(serde_json::Map::new()),
        parallelization: None,
    };

    let json = serde_json::to_vec_pretty(&manifest).context("serialize manifest")?;
    fs::write(output, &json).with_context(|| format!("write {}", output.display()))?;
    println!("Job manifest written to {}", output.display());
    println!("  job_id  : {job_id}");
    println!("  task    : {task}");
    println!("  tool    : {tool_name} {tool_version}");
    Ok(())
}

// ── offf-jobs create-keyword ──────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn cmd_create_keyword(
    case: &Path,
    keywords_csv: &str,
    encoding_csv: &str,
    chunks_arg: &str,
    scope_ref: Option<String>,
    include_sets: Vec<String>,
    policy_refs: Vec<String>,
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
        input_scope: None,
        output_contract: None,
        scope_ref,
        include_sets,
        policy_refs,
        parameters: serde_json::json!({
            "keywords": keywords,
            "encoding": encodings,
        }),
        parallelization: None,
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

fn cmd_create_yara(
    case: &Path,
    rules_path: &Path,
    chunks_arg: &str,
    scope_ref: Option<String>,
    include_sets: Vec<String>,
    policy_refs: Vec<String>,
    output: &Path,
) -> Result<()> {
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
        input_scope: None,
        output_contract: None,
        scope_ref,
        include_sets,
        policy_refs,
        parameters: serde_json::json!({
            "rules_path": rules_path.display().to_string(),
            "rules_hash": format!("sha256:{rules_hash}"),
            "rules_inline": rules_text,
        }),
        parallelization: None,
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
    let output = match task {
        "keyword_scan" | "yara_scan" => {
            let package = if task == "keyword_scan" {
                "offf-keyword-worker"
            } else {
                "offf-yara-worker"
            };
            ProcessCommand::new("cargo")
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
                .with_context(|| format!("failed to spawn worker package {package}"))?
        }
        "build_object_graph_from_filesystem" | "object_graph_build" => {
            ProcessCommand::new("cargo")
                .arg("run")
                .arg("-p")
                .arg("offf-index")
                .arg("--")
                .arg("objects")
                .arg(case)
                .arg("--from-filesystem")
                .arg("--hash-content")
                .arg("deferred")
                .output()
                .context("failed to spawn offf-index for object graph build")?
        }
        other => anyhow::bail!("unsupported job task for runner: {other}"),
    };

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

// ── offf-jobs plan-shards ─────────────────────────────────────────────────────

fn cmd_plan_shards(
    case: &Path,
    job_id: &str,
    shard_count: usize,
    strategy_str: &str,
) -> Result<()> {
    let strategy: ShardStrategy = strategy_str
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    // Load the parent job manifest.
    let job_path = case.join("jobs").join(format!("{job_id}.json"));
    let job_raw = fs::read_to_string(&job_path)
        .with_context(|| format!("cannot read job manifest: {}", job_path.display()))?;
    let _job: JobManifest = serde_json::from_str(&job_raw).context("invalid job manifest")?;

    // Resolve scope.
    let inputs = resolve_analysis_scope(case, &_job)
        .context("failed to resolve analysis scope")?;

    let input_scope_hash = compute_input_scope_hash(&inputs);

    println!("Resolved {} input objects (scope hash: {input_scope_hash})", inputs.len());

    if inputs.is_empty() {
        println!("Warning: no inputs resolved — no object_index.parquet in container?");
    }

    // Plan shards.
    let (plan, manifests) =
        plan_shards(&inputs, job_id, strategy, shard_count, &input_scope_hash)
            .context("failed to plan shards")?;

    // Write shard plan.
    write_shard_plan(case, job_id, &plan)?;
    println!("Shard plan written: analysis/jobs/{job_id}/shard_plan.json");

    // Write shard manifests.
    for shard in &manifests {
        write_shard_manifest(case, job_id, shard)?;
        println!(
            "  Shard manifest: {} ({} inputs)",
            shard.shard_id,
            shard.input_objects.len()
        );
    }

    println!("Plan complete: {} shards, strategy: {}", shard_count, strategy_str);
    Ok(())
}

// ── offf-jobs finalize-job ────────────────────────────────────────────────────

fn cmd_finalize_job(case: &Path, job_id: &str) -> Result<()> {
    // Load the parent job to re-resolve scope (for coverage cross-check).
    let job_path = case.join("jobs").join(format!("{job_id}.json"));
    let job_raw = fs::read_to_string(&job_path)
        .with_context(|| format!("cannot read job manifest: {}", job_path.display()))?;
    let job: JobManifest = serde_json::from_str(&job_raw).context("invalid job manifest")?;

    let inputs = resolve_analysis_scope(case, &job)
        .context("failed to resolve analysis scope")?;

    let parent_manifest = build_parent_result_manifest(case, job_id, &inputs)
        .context("failed to build parent result manifest")?;

    write_parent_result_manifest(case, &parent_manifest)
        .context("failed to write parent result manifest")?;

    println!("Parent result manifest written: analysis/jobs/{job_id}/parent_result_manifest.json");
    println!("  Status:            {}", parent_manifest.status);
    println!("  Shards completed:  {}", parent_manifest.parallelization.shards_completed);
    println!("  Shards failed:     {}", parent_manifest.parallelization.shards_failed);
    println!("  Objects in scope:  {}", parent_manifest.coverage.objects_in_scope);
    println!("  Objects processed: {}", parent_manifest.coverage.objects_processed);
    println!("  Objects success:   {}", parent_manifest.coverage.objects_success);
    println!("  Objects error:     {}", parent_manifest.coverage.objects_error);
    println!("  Objects skipped:   {}", parent_manifest.coverage.objects_skipped);

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
        let map_path = case.join(
            manifest.indexes.physical_to_chunk.as_deref().unwrap_or("maps/physical_to_chunk.parquet")
        );
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
            input_scope: None,
            output_contract: None,
            scope_ref: None,
            include_sets: vec![],
            policy_refs: vec![],
            parameters: serde_json::json!({"k": ["x"]}),
            parallelization: None,
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

    #[test]
    fn run_writes_runtime_state_artifacts_for_failed_job_attempt() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let case_path = temp_dir.path();
        let job = JobManifest {
            job_id: "job-runtime-state-test".to_string(),
            created_at: Utc::now(),
            case_id: "urn:offf:case:test".to_string(),
            task: "unsupported_task".to_string(),
            scope: JobScope {
                chunks: vec!["*".to_string()],
            },
            tool: ToolInfo {
                name: "worker-test".to_string(),
                version: "0.1.0".to_string(),
            },
            input_scope: None,
            output_contract: None,
            scope_ref: None,
            include_sets: vec![],
            policy_refs: vec![],
            parameters: serde_json::json!({}),
            parallelization: None,
        };
        let job_path = case_path.join("job.json");
        fs::write(&job_path, serde_json::to_string_pretty(&job).unwrap()).unwrap();

        let result = cmd_run(case_path, &job_path, "worker-test", 0, false);
        assert!(result.is_err(), "unsupported task should fail");

        let runtime_dir = case_path.join("jobs/runtime");
        assert!(runtime_dir.join("job-runtime-state-test.state.json").exists());
        assert!(runtime_dir.join("assignment_audit.jsonl").exists());
        assert!(runtime_dir.join("worker_health.jsonl").exists());
    }
}
