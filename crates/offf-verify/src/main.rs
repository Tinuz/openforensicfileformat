use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use serde::Serialize;
use sha2::{Digest, Sha256};

use offf_core::{
    chunk::verify_chunk,
    evidence::read_evidence_object,
    hash::{generate_merkle_proof, parse_and_validate_merkle_tree, verify_merkle_proof},
    lineage::ObjectLineageValidator,
    parquet_io::{
        read_derivations, read_leaves, read_object_edges, read_object_index,
        read_physical_to_chunk, read_physical_to_chunk_bytes,
    },
    scope::{compute_input_scope_hash, resolve_analysis_scope},
    shard::{read_shard_manifest, read_shard_plan, read_shard_result_manifest, validate_parallel_job, ShardValidationIssueKind},
    storage::{derived_object_path, read_chunk_verified, ContainerRef},
    types::{AcquisitionJson, AcquisitionMode, ManifestJson, OFFF_V2_VERSION, OFFF_VERSION},
};

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "offf-verify",
    about = "Verify the integrity of an OFFF container",
    version
)]
struct Args {
    /// OFFF container path or URI (local path or s3://bucket/prefix)
    container: String,
    /// Optional subset validation scope.
    ///
    /// Comma-separated chunk sequences and/or chunk IDs.
    /// Examples:
    ///   --chunks 0,1,2
    ///   --chunks sha256:abc...,sha256:def...
    #[arg(long)]
    chunks: Option<String>,

    /// Optional proof helper: generate and verify proof for a chunk sequence or chunk_id.
    /// Examples:
    ///   --proof-chunk 12
    ///   --proof-chunk sha256:abc...
    #[arg(long)]
    proof_chunk: Option<String>,

    /// Validation profile.
    #[arg(long, value_enum, default_value_t = VerifyProfile::Core)]
    profile: VerifyProfile,

    /// Optional machine-readable JSON report output path.
    #[arg(long)]
    report: Option<PathBuf>,

    /// Validate the lineage of a specific object (object_id).
    /// Requires the object indexes to be present.
    /// Use together with --lineage to enable full lineage chain validation.
    #[arg(long)]
    object: Option<String>,

    /// When combined with --object, perform full lineage chain validation:
    /// referential integrity, derivation hash checks, derived object hash verification.
    #[arg(long)]
    lineage: bool,

    /// Validate a parallel analysis job by job_id.
    /// Checks shard plan, all shard manifests, result manifests, artifact hashes, and coverage.
    #[arg(long)]
    analysis_job: Option<String>,

    /// Validate a single shard by shard_id (requires --analysis-job).
    #[arg(long)]
    shard: Option<String>,

    /// Print coverage report for a parallel analysis job (by job_id) and exit.
    #[arg(long)]
    coverage: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum VerifyProfile {
    Core,
    #[value(name = "core+schemas")]
    CoreSchemas,
    #[value(name = "core+extensions")]
    CoreExtensions,
    Conformance,
    /// Accept v0.1 analysis layouts; emit warnings for missing result_manifest.json
    /// and non-forensic-grade outputs. Use this to audit containers created by
    /// pre-v0.2 workers and plan migration to v0.2 output contracts.
    Legacy,
}

// ── Verification result ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct VerifyReport {
    container: String,
    profile: VerifyProfile,
    checks: Vec<CheckResult>,
}

#[derive(Debug, Serialize)]
struct CheckResult {
    label: String,
    status: CheckStatus,
    detail: Option<String>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum CheckStatus {
    Ok,
    Fail,
    Warn,
}

impl VerifyReport {
    fn ok(&mut self, label: impl Into<String>) {
        self.checks.push(CheckResult {
            label: label.into(),
            status: CheckStatus::Ok,
            detail: None,
        });
    }

    fn fail(&mut self, label: impl Into<String>, detail: impl Into<String>) {
        self.checks.push(CheckResult {
            label: label.into(),
            status: CheckStatus::Fail,
            detail: Some(detail.into()),
        });
    }

    fn warn(&mut self, label: impl Into<String>, detail: impl Into<String>) {
        self.checks.push(CheckResult {
            label: label.into(),
            status: CheckStatus::Warn,
            detail: Some(detail.into()),
        });
    }

    fn is_valid(&self) -> bool {
        !self.checks.iter().any(|c| c.status == CheckStatus::Fail)
    }

    fn status_counts(&self) -> HashMap<&'static str, usize> {
        let mut out = HashMap::from([("ok", 0usize), ("warn", 0usize), ("fail", 0usize)]);
        for c in &self.checks {
            match c.status {
                CheckStatus::Ok => *out.get_mut("ok").unwrap() += 1,
                CheckStatus::Warn => *out.get_mut("warn").unwrap() += 1,
                CheckStatus::Fail => *out.get_mut("fail").unwrap() += 1,
            }
        }
        out
    }

    fn print(&self) {
        println!("Container: {}", self.container);
        println!("Profile:   {:?}", self.profile);
        println!();
        for check in &self.checks {
            let marker = match check.status {
                CheckStatus::Ok => "OK  ",
                CheckStatus::Fail => "FAIL",
                CheckStatus::Warn => "WARN",
            };
            if let Some(detail) = &check.detail {
                println!("  [{marker}] {}: {}", check.label, detail);
            } else {
                println!("  [{marker}] {}", check.label);
            }
        }
        println!();
        if self.is_valid() {
            println!("Result: VALID");
        } else {
            println!("Result: INVALID");
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args = Args::parse();

    // ── Coverage-only mode ────────────────────────────────────────────────────
    if let Some(job_id) = &args.coverage {
        return cmd_print_coverage(&args.container, job_id);
    }

    // ── Parallel job verification ─────────────────────────────────────────────
    if let Some(job_id) = &args.analysis_job {
        let valid = cmd_verify_parallel_job(
            &args.container,
            job_id,
            args.shard.as_deref(),
            args.report.as_deref(),
        )?;
        if !valid {
            std::process::exit(1);
        }
        return Ok(());
    }

    // ── Object lineage verification ───────────────────────────────────────────
    if args.object.is_some() || args.lineage {
        let valid = verify_object_lineage(
            &args.container,
            args.object.as_deref(),
            args.lineage,
            args.report.as_deref(),
        )?;
        if !valid {
            std::process::exit(1);
        }
        return Ok(());
    }

    let valid = verify(
        &args.container,
        args.chunks.as_deref(),
        args.proof_chunk.as_deref(),
        args.profile,
        args.report.as_deref(),
    )?;
    if !valid {
        std::process::exit(1);
    }
    Ok(())
}

// ── Parallel job verification ─────────────────────────────────────────────────

fn cmd_verify_parallel_job(
    container_arg: &str,
    job_id: &str,
    shard_id_filter: Option<&str>,
    report_path: Option<&std::path::Path>,
) -> Result<bool> {
    let container_path = std::path::PathBuf::from(container_arg);
    let mut report = VerifyReport {
        container: container_arg.to_string(),
        profile: VerifyProfile::Core,
        checks: Vec::new(),
    };

    // Check 1: job_manifest.json exists.
    let job_manifest_path = container_path
        .join("jobs")
        .join(format!("{job_id}.json"));
    if !job_manifest_path.exists() {
        report.fail(
            "job_manifest",
            format!("job manifest not found: {}", job_manifest_path.display()),
        );
        report.print();
        return Ok(false);
    }
    report.ok("job_manifest");

    // Check 2: shard_plan.json exists and parses.
    let plan = match read_shard_plan(&container_path, job_id) {
        Err(e) => {
            report.fail("shard_plan", format!("cannot read shard_plan.json: {e}"));
            report.print();
            return Ok(false);
        }
        Ok(p) => {
            report.ok("shard_plan");
            p
        }
    };

    // Check 3: input_scope_hash in shard_plan matches re-computed hash.
    // Load job manifest and re-resolve scope.
    let job_raw = fs::read_to_string(&job_manifest_path)?;
    if let Ok(job) = serde_json::from_str::<offf_core::types::JobManifest>(&job_raw) {
        match resolve_analysis_scope(&container_path, &job) {
            Err(e) => {
                report.warn("scope_hash", format!("could not re-resolve scope: {e}"));
            }
            Ok(inputs) => {
                let computed_hash = compute_input_scope_hash(&inputs);
                if computed_hash == plan.input_scope_hash {
                    report.ok("scope_hash");
                } else {
                    report.fail(
                        "scope_hash",
                        format!(
                            "input_scope_hash mismatch: plan has {} but re-computed {}",
                            plan.input_scope_hash, computed_hash
                        ),
                    );
                }
            }
        }
    }

    // Check 4–7: Validate all shards (or single shard if filtered).
    let shard_ids_to_check: Vec<String> = if let Some(filter) = shard_id_filter {
        vec![filter.to_string()]
    } else {
        (0..plan.shard_count)
            .map(|i| format!("{job_id}-shard-{i:02}"))
            .collect()
    };

    for shard_id in &shard_ids_to_check {
        let label_prefix = format!("shard[{shard_id}]");

        // Check shard manifest.
        match read_shard_manifest(&container_path, job_id, shard_id) {
            Err(_) => {
                report.fail(
                    format!("{label_prefix}/manifest"),
                    format!("shard manifest missing for {shard_id}"),
                );
                continue;
            }
            Ok(_) => {
                report.ok(format!("{label_prefix}/manifest"));
            }
        }

        // Check shard result manifest.
        let result = match read_shard_result_manifest(&container_path, job_id, shard_id) {
            Err(_) => {
                report.fail(
                    format!("{label_prefix}/result_manifest"),
                    format!("shard result manifest missing for {shard_id}"),
                );
                continue;
            }
            Ok(r) => {
                report.ok(format!("{label_prefix}/result_manifest"));
                r
            }
        };

        // Check scope hash consistency in result.
        if result.input.input_scope_hash != plan.input_scope_hash {
            report.fail(
                format!("{label_prefix}/scope_hash"),
                format!(
                    "input_scope_hash mismatch: result has {} but plan has {}",
                    result.input.input_scope_hash, plan.input_scope_hash
                ),
            );
        } else {
            report.ok(format!("{label_prefix}/scope_hash"));
        }

        // Check artifact hashes.
        for artifact in &result.outputs {
            let artifact_path = container_path.join(&artifact.path);
            match verify_file_hash(&artifact_path, &artifact.sha256) {
                Ok(true) => {
                    report.ok(format!("{label_prefix}/artifact[{}]", artifact.path));
                }
                Ok(false) => {
                    report.fail(
                        format!("{label_prefix}/artifact[{}]", artifact.path),
                        format!("hash mismatch; expected {}", artifact.sha256),
                    );
                }
                Err(e) => {
                    report.fail(
                        format!("{label_prefix}/artifact[{}]", artifact.path),
                        format!("cannot read artifact: {e}"),
                    );
                }
            }
        }
    }

    // Check 8: Cross-shard duplicate detection (full job only).
    if shard_id_filter.is_none() {
        match validate_parallel_job(&container_path, job_id) {
            Err(e) => {
                report.warn("cross_shard_validation", format!("validation error: {e}"));
            }
            Ok(issues) => {
                let dups: Vec<_> = issues
                    .iter()
                    .filter(|i| i.kind == ShardValidationIssueKind::DuplicateInputId)
                    .collect();
                if dups.is_empty() {
                    report.ok("no_duplicate_inputs");
                } else {
                    for dup in &dups {
                        report.fail("duplicate_input", dup.message.clone());
                    }
                }
            }
        }
    }

    // Output report.
    report.print();
    if let Some(path) = report_path {
        fs::write(path, serde_json::to_string_pretty(&report)?)?;
    }

    Ok(report.is_valid())
}

fn cmd_print_coverage(container_arg: &str, job_id: &str) -> Result<()> {
    use offf_core::shard::build_parent_result_manifest;

    let container_path = std::path::PathBuf::from(container_arg);

    // Try to load the parent result manifest first; fall back to re-building it.
    let coverage = if let Ok(parent) =
        offf_core::shard::read_parent_result_manifest(&container_path, job_id)
    {
        parent.coverage
    } else {
        // Re-build on the fly without writing.
        let job_path = container_path.join("jobs").join(format!("{job_id}.json"));
        let job_raw = fs::read_to_string(&job_path)?;
        let job: offf_core::types::JobManifest = serde_json::from_str(&job_raw)?;
        let inputs = resolve_analysis_scope(&container_path, &job)?;
        let manifest = build_parent_result_manifest(&container_path, job_id, &inputs)?;
        manifest.coverage
    };

    println!("Coverage report for job: {job_id}");
    println!("  input_scope_hash:         {}", coverage.input_scope_hash);
    println!("  objects_in_scope:         {}", coverage.objects_in_scope);
    println!("  objects_assigned:         {}", coverage.objects_assigned_to_shards);
    println!("  objects_processed:        {}", coverage.objects_processed);
    println!("  objects_success:          {}", coverage.objects_success);
    println!("  objects_error:            {}", coverage.objects_error);
    println!("  objects_skipped:          {}", coverage.objects_skipped);
    println!("  duplicates_detected:      {}", coverage.duplicates_detected);
    println!("  missing_inputs:           {}", coverage.missing_inputs);

    Ok(())
}

fn verify_file_hash(path: &std::path::Path, expected: &str) -> Result<bool> {
    let data = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let actual = format!("sha256:{}", hex::encode(hasher.finalize()));
    Ok(actual == expected)
}

fn verify(
    container_arg: &str,
    subset_arg: Option<&str>,
    proof_chunk_arg: Option<&str>,
    profile: VerifyProfile,
    report_path: Option<&std::path::Path>,
) -> Result<bool> {
    let container = ContainerRef::parse(container_arg)?;
    let mut report = VerifyReport {
        container: container.display(),
        profile,
        checks: Vec::new(),
    };

    // ── Check 1: manifest exists and is parseable ──────────────────────────
    let manifest_raw = container.read_text("manifest.json").ok();

    let manifest_value: Option<serde_json::Value> = manifest_raw
        .as_ref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

    let manifest = match manifest_raw {
        Some(s) => match serde_json::from_str::<ManifestJson>(&s) {
            Ok(m) => {
                report.ok("Manifest present and valid");
                Some(m)
            }
            Err(e) => {
                report.fail("Manifest present and valid", e.to_string());
                None
            }
        },
        None => {
            report.fail("Manifest present and valid", "manifest.json not found");
            None
        }
    };

    let manifest = match manifest {
        Some(m) => m,
        None => {
            // Cannot continue without manifest
            report.print();
            return Ok(false);
        }
    };

    // ── Check 2: OFFF version ──────────────────────────────────────────────
    let is_v2 = manifest.offf_version == OFFF_V2_VERSION;
    if manifest.offf_version == OFFF_VERSION || is_v2 {
        report.ok(format!("OFFF version: {}", manifest.offf_version));
    } else {
        report.fail(
            "OFFF version",
            format!(
                "expected {} or {OFFF_V2_VERSION}, got {}",
                OFFF_VERSION, manifest.offf_version
            ),
        );
    }

    // ── Check 2a: extensions namespace format (v0.2.0+) ───────────────────
    if let Some(ext) = &manifest.extensions {
        if ext.entries.is_empty() {
            if is_v2 {
                report.ok("Extensions: present but empty");
            }
        } else if includes_schemas(profile) {
            // In core+schemas profile: enforce namespace:name key pattern
            let mut bad_keys: Vec<&String> = ext
                .entries
                .keys()
                .filter(|k| {
                    let mut parts = k.splitn(2, ':');
                    let ns = parts.next().unwrap_or("");
                    let name = parts.next().unwrap_or("");
                    ns.is_empty() || name.is_empty()
                })
                .collect();
            bad_keys.sort();
            if bad_keys.is_empty() {
                report.ok(format!(
                    "Extensions: {} namespace(s) valid",
                    ext.entries.len()
                ));
            } else {
                for k in bad_keys {
                    report.fail(
                        "Extension key format",
                        format!("key '{k}' must match 'namespace:name' pattern"),
                    );
                }
            }
        } else {
            report.ok(format!(
                "Extensions: {} namespace(s) present",
                ext.entries.len()
            ));
        }
    }

    if !is_v2 && manifest.extensions.is_some() {
        report.warn(
            "Extensions section",
            "extensions present in a v0.1.0 manifest; expected only in v0.2.0+",
        );
    }

    // ── Check 3-6: Load mapping table and verify chunk presence/hashes ─────
    // Branch on acquisition_mode: skip block-image checks for file_collection.
    let effective_mode = manifest.effective_mode();
    if matches!(effective_mode, AcquisitionMode::FileCollection) {
        verify_file_collection(&manifest, &container, &mut report);
        return Ok(report.is_valid());
    }

    let ptc_path = manifest
        .indexes
        .physical_to_chunk
        .as_deref()
        .unwrap_or("maps/physical_to_chunk.parquet");
    let all_chunks = match container.local_path(ptc_path) {
        Some(p) => read_physical_to_chunk(&p),
        None => {
            let data = container.read_bytes(ptc_path);
            data.and_then(|d| read_physical_to_chunk_bytes(&d))
        }
    };

    // ── Check 2b: acquisition parseability ─────────────────────────────────
    match container.read_text("acquisition.json") {
        Ok(raw) => match serde_json::from_str::<AcquisitionJson>(&raw) {
            Ok(_) => report.ok("Acquisition present and valid"),
            Err(e) => report.fail("Acquisition present and valid", e.to_string()),
        },
        Err(_) => report.fail(
            "Acquisition present and valid",
            "acquisition.json not found",
        ),
    }
    let chunks = match all_chunks {
        Ok(c) => {
            report.ok(format!("Mapping table: {} chunks", c.len()));
            c
        }
        Err(e) => {
            report.fail("Mapping table", e.to_string());
            report.print();
            return Ok(false);
        }
    };

    let subset = parse_subset(subset_arg);
    let is_subset = subset_arg.is_some();
    let chunks_to_verify: Vec<_> = if let Some((ids, seqs)) = subset {
        let filtered: Vec<_> = chunks
            .iter()
            .filter(|c| ids.contains(&c.chunk_id) || seqs.contains(&c.sequence))
            .cloned()
            .collect();
        report.ok(format!(
            "Subset mode: validating {} of {} chunks",
            filtered.len(),
            chunks.len()
        ));
        filtered
    } else {
        chunks.clone()
    };

    if chunks_to_verify.is_empty() {
        report.fail("Chunk selection", "no chunks selected for validation");
        report.print();
        return Ok(false);
    }

    let total = chunks_to_verify.len();
    let mut stored_ok = 0usize;
    let mut stored_fail = 0usize;

    for chunk in &chunks_to_verify {
        let res = match &container {
            ContainerRef::Local(base) => verify_chunk(base, chunk),
            ContainerRef::S3 { .. } => read_chunk_verified(&container, chunk).map(|_| ()),
        };
        match res {
            Ok(()) => stored_ok += 1,
            Err(e) => {
                stored_fail += 1;
                report.fail(format!("Chunk {} hash", chunk.sequence), e.to_string());
            }
        }
    }

    if stored_fail == 0 {
        report.ok(format!("Stored hash validation: {stored_ok}/{total} OK"));
        report.ok(format!("Plaintext hash validation: {stored_ok}/{total} OK"));
    } else {
        report.fail(
            "Stored/plaintext hash validation",
            format!("{stored_fail}/{total} chunks FAILED"),
        );
    }

    // ── Check 6b: leaves.parquet consistency ───────────────────────────────
    if is_subset {
        report.warn(
            "Leaves consistency",
            "skipped in subset mode (requires full chunk set)",
        );
    } else {
        let leaves_rows = match container.local_path("hashes/leaves.parquet") {
            Some(path) => read_leaves(&path),
            None => Err(offf_core::OfffError::InvalidContainer(
                "leaves.parquet validation for non-local container is not yet supported"
                    .to_string(),
            )),
        };

        match leaves_rows {
            Ok(leaves) => {
                let expected: Vec<(u64, String)> = chunks
                    .iter()
                    .map(|c| (c.sequence, c.plaintext_sha256.clone()))
                    .collect();

                if leaves.len() != expected.len() {
                    report.fail(
                        "Leaves consistency",
                        format!(
                            "leaves count {} differs from chunk count {}",
                            leaves.len(),
                            expected.len()
                        ),
                    );
                } else {
                    let mut mismatch: Option<String> = None;
                    for (i, ((l_seq, l_hash), (e_seq, e_hash))) in
                        leaves.iter().zip(expected.iter()).enumerate()
                    {
                        if l_seq != e_seq || l_hash != e_hash {
                            mismatch = Some(format!(
                                "row {i}: leaves=({l_seq},{}) expected=({e_seq},{})",
                                &l_hash[..16],
                                &e_hash[..16]
                            ));
                            break;
                        }
                    }
                    if let Some(msg) = mismatch {
                        report.fail("Leaves consistency", msg);
                    } else {
                        report.ok(format!(
                            "Leaves consistency: {}/{} rows OK",
                            leaves.len(),
                            expected.len()
                        ));
                    }
                }
            }
            Err(e) => report.fail("Leaves consistency", e.to_string()),
        }
    }

    // ── Check 7: Merkle tree structure + root consistency ─────────────────
    if is_subset {
        report.warn(
            "Merkle root",
            "skipped in subset mode (requires full leaf set)",
        );
    } else {
        match container.read_bytes("hashes/merkle_tree.bin") {
            Ok(blob) => match parse_and_validate_merkle_tree(&blob) {
                Ok(tree) => {
                    let map_leaves: Vec<String> =
                        chunks.iter().map(|c| c.plaintext_sha256.clone()).collect();

                    if tree.leaf_count as usize != map_leaves.len() {
                        report.fail(
                            "Merkle leaf count",
                            format!(
                                "tree leaf_count={} differs from map chunk_count={}",
                                tree.leaf_count,
                                map_leaves.len()
                            ),
                        );
                    } else {
                        report.ok(format!("Merkle leaf count: {}", tree.leaf_count));
                    }

                    if tree.leaves != map_leaves {
                        report.fail(
                            "Merkle leaves order/content",
                            "tree leaves differ from physical_to_chunk plaintext hashes",
                        );
                    } else {
                        report.ok("Merkle leaves match physical_to_chunk order");
                    }

                    if tree.root == manifest.hashes.as_ref().map(|h| h.merkle_root_sha256.as_str()).unwrap_or("") {
                        report.ok(format!("Merkle root: {}", &tree.root[..16]));
                    } else if manifest.hashes.is_none() {
                        report.warn("Merkle root", "skipped: no hashes section in manifest");
                    } else {
                        report.fail(
                            "Merkle root",
                            format!(
                                "tree root ({}) differs from manifest ({})",
                                &tree.root[..16],
                                &manifest.hashes.as_ref().unwrap().merkle_root_sha256[..16]
                            ),
                        );
                    }
                }
                Err(e) => report.fail("Merkle tree binary", e.to_string()),
            },
            Err(_) => report.fail("Merkle tree binary", "hashes/merkle_tree.bin not found"),
        }

        // Optional proof helper using full leaf set from map order.
        if let Some(proof_chunk_ref) = proof_chunk_arg {
            match resolve_chunk_ref(proof_chunk_ref, &chunks) {
                Some(chunk) => {
                    let leaves: Vec<String> =
                        chunks.iter().map(|c| c.plaintext_sha256.clone()).collect();
                    match generate_merkle_proof(&leaves, chunk.sequence) {
                        Ok(proof) => {
                            match verify_merkle_proof(
                                &chunk.plaintext_sha256,
                                chunk.sequence,
                                &proof,
                                manifest.hashes.as_ref().map(|h| h.merkle_root_sha256.as_str()).unwrap_or(""),
                            ) {
                                Ok(true) => {
                                    report.ok(format!(
                                        "Merkle proof verified for chunk {}",
                                        chunk.sequence
                                    ));
                                    if let Ok(json) = serde_json::to_string_pretty(&proof) {
                                        println!("\nMerkle proof:\n{json}\n");
                                    }
                                }
                                Ok(false) => report.fail(
                                    "Merkle proof verification",
                                    format!(
                                        "proof verification failed for chunk {}",
                                        chunk.sequence
                                    ),
                                ),
                                Err(e) => report.fail("Merkle proof verification", e.to_string()),
                            }
                        }
                        Err(e) => report.fail("Merkle proof generation", e.to_string()),
                    }
                }
                None => report.fail(
                    "Merkle proof chunk selection",
                    format!("chunk not found for reference: {proof_chunk_ref}"),
                ),
            }
        }
    }

    // ── Check 8: Source hash (stream all chunks in order) ─────────────────
    if is_subset {
        report.warn(
            "Source SHA-256",
            "skipped in subset mode (requires full chunk stream)",
        );
    } else {
        let mut source_hasher = Sha256::new();
        let mut hash_ok = true;

        for chunk in &chunks_to_verify {
            match read_chunk_verified(&container, chunk) {
                Ok(plaintext) => {
                    source_hasher.update(&plaintext);
                }
                Err(e) => {
                    report.fail("Source hash reconstruction", e.to_string());
                    hash_ok = false;
                    break;
                }
            }
        }

        if hash_ok {
            let computed = format!("{:x}", source_hasher.finalize());
            match manifest.hashes.as_ref() {
                Some(h) => {
                    if computed == h.source_sha256 {
                        report.ok(format!("Source SHA-256: {}", &h.source_sha256[..16]));
                    } else {
                        report.fail(
                            "Source SHA-256",
                            format!(
                                "computed {}, manifest has {}",
                                &computed[..16],
                                &h.source_sha256[..16]
                            ),
                        );
                    }
                }
                None => report.warn("Source SHA-256", "skipped: no hashes section in manifest"),
            }
        }
    }

    // ── Check 9: Container completeness ───────────────────────────────────
    let required_files = [
        "manifest.json",
        "acquisition.json",
        "maps/physical_to_chunk.parquet",
        "hashes/leaves.parquet",
        "hashes/merkle_tree.bin",
        "provenance/chain_of_custody.jsonl",
    ];
    let mut missing: Vec<&str> = required_files
        .iter()
        .filter(|f| !container.exists(f).unwrap_or(false))
        .copied()
        .collect();

    if missing.is_empty() {
        report.ok("Container complete (all required files present)");
    } else {
        missing.sort();
        report.fail(
            "Container complete",
            format!("missing: {}", missing.join(", ")),
        );
    }

    // ── Check 10: Provenance logs present and non-empty ───────────────────
    match container.read_text("provenance/chain_of_custody.jsonl") {
        Ok(content) if content.lines().any(|l| !l.trim().is_empty()) => {
            let count = content.lines().filter(|l| !l.trim().is_empty()).count();
            report.ok(format!("Provenance: {count} event(s)"));

            if includes_schemas(profile) {
                run_provenance_schema_checks(&content, &mut report);
            }
        }
        Ok(_) => report.fail("Provenance", "file exists but is empty"),
        Err(_) => report.fail("Provenance", "chain_of_custody.jsonl not found"),
    }

    if includes_extensions(profile) {
        run_extension_checks(manifest_value.as_ref(), &container, &mut report);
    }

    if profile == VerifyProfile::Legacy {
        run_legacy_checks(&container, &mut report);
    }

    if profile == VerifyProfile::Conformance && report_path.is_none() {
        report.warn(
            "Conformance report",
            "--report not provided; no machine-readable artifact written",
        );
    }

    let valid = report.is_valid();
    if let Some(path) = report_path {
        write_json_report(path, &report, valid)?;
    }
    report.print();
    Ok(valid)
}

fn includes_schemas(profile: VerifyProfile) -> bool {
    matches!(
        profile,
        VerifyProfile::CoreSchemas | VerifyProfile::CoreExtensions | VerifyProfile::Conformance
    )
}

// ── File-collection container verification ────────────────────────────────────

fn verify_file_collection(
    manifest: &ManifestJson,
    container: &ContainerRef,
    report: &mut VerifyReport,
) {
    // Check: evidence_roots present
    match &manifest.evidence_roots {
        Some(roots) if !roots.is_empty() => {
            report.ok(format!("evidence_roots: {} root(s) declared", roots.len()));
        }
        _ => {
            report.fail(
                "evidence_roots",
                "file_collection manifest must have at least one evidence_root",
            );
        }
    }

    // Check: limitations present
    match &manifest.limitations {
        Some(lims) if !lims.is_empty() => {
            report.ok(format!("limitations: {} item(s) declared", lims.len()));
        }
        _ => report.warn(
            "limitations",
            "file_collection should declare known limitations",
        ),
    }

    // Check: object_index path declared and readable
    let oi_path = match &manifest.indexes.object_index {
        Some(p) => p.clone(),
        None => {
            report.fail(
                "indexes.object_index",
                "file_collection manifest must declare indexes.object_index",
            );
            return;
        }
    };

    let object_rows = match container.local_path(&oi_path) {
        Some(p) => match read_object_index(&p) {
            Ok(rows) => {
                report.ok(format!("object_index: {} row(s)", rows.len()));
                rows
            }
            Err(e) => {
                report.fail("object_index readable", e.to_string());
                return;
            }
        },
        None => {
            // S3 path: just check existence
            match container.read_bytes(&oi_path) {
                Ok(_) => {
                    report.ok("object_index: present (S3, not parsed)");
                    return;
                }
                Err(e) => {
                    report.fail("object_index readable", e.to_string());
                    return;
                }
            }
        }
    };

    // Check: object_edges path declared and readable
    let oe_path = match &manifest.indexes.object_edges {
        Some(p) => p.clone(),
        None => {
            report.fail(
                "indexes.object_edges",
                "file_collection manifest must declare indexes.object_edges",
            );
            return;
        }
    };
    if let Some(p) = container.local_path(&oe_path) {
        match read_object_edges(&p) {
            Ok(edges) => report.ok(format!("object_edges: {} edge(s)", edges.len())),
            Err(e) => report.fail("object_edges readable", e.to_string()),
        }
    } else {
        match container.read_bytes(&oe_path) {
            Ok(_) => report.ok("object_edges: present (S3, not parsed)"),
            Err(e) => report.fail("object_edges readable", e.to_string()),
        }
    }

    // Check: collection_root object present in index
    let root_ids: std::collections::HashSet<&str> = manifest
        .evidence_roots
        .as_ref()
        .map(|r| r.iter().map(|e| e.root_id.as_str()).collect())
        .unwrap_or_default();

    let root_in_index = object_rows
        .iter()
        .any(|r| root_ids.contains(r.object_id.as_str()));
    if root_in_index {
        report.ok("collection_root object in object_index");
    } else {
        report.fail(
            "collection_root in object_index",
            "no row with object_type=collection_root matching evidence_roots root_id",
        );
    }

    // Check: storage_refs exist on disk and SHA-256 matches
    let base_path = match container {
        ContainerRef::Local(p) => Some(p.clone()),
        ContainerRef::S3 { .. } => None,
    };

    if let Some(base) = &base_path {
        let mut ok_count = 0usize;
        let mut fail_count = 0usize;
        for row in &object_rows {
            if row.object_type == "collection_root" {
                continue;
            }
            let sha256_ref = match row.storage_ref.as_deref() {
                Some(s) => s,
                None => {
                    fail_count += 1;
                    report.fail(
                        "storage_ref present",
                        format!("object '{}' has no storage_ref", row.object_id),
                    );
                    continue;
                }
            };
            match read_evidence_object(base, sha256_ref) {
                Ok(_) => ok_count += 1,
                Err(e) => {
                    fail_count += 1;
                    report.fail(
                        "evidence object integrity",
                        format!("object '{}': {e}", row.object_id),
                    );
                }
            }
        }
        if fail_count == 0 && ok_count > 0 {
            report.ok(format!("evidence objects: {ok_count} verified"));
        } else if ok_count == 0 && fail_count == 0 {
            report.warn("evidence objects", "no file objects found");
        }
    } else {
        report.warn("evidence objects", "S3 storage_ref verification not implemented");
    }
}



fn verify_object_lineage(
    container_arg: &str,
    object_id: Option<&str>,
    full_lineage: bool,
    report_path: Option<&std::path::Path>,
) -> Result<bool> {
    let container = ContainerRef::parse(container_arg)?;
    let mut report = VerifyReport {
        container: container.display(),
        profile: VerifyProfile::Core,
        checks: Vec::new(),
    };

    // ── Load object indexes ───────────────────────────────────────────────
    let idx_rel = "indexes/objects/object_index.parquet";
    let edges_rel = "indexes/objects/object_edges.parquet";
    let deriv_rel = "indexes/objects/derivations.parquet";

    macro_rules! load_parquet {
        ($rel:expr, $reader:ident, $label:expr) => {{
            match container.local_path($rel) {
                Some(p) if p.exists() => match $reader(&p) {
                    Ok(rows) => {
                        report.ok(format!("{}: {} row(s)", $label, rows.len()));
                        rows
                    }
                    Err(e) => {
                        report.fail($label, e.to_string());
                        vec![]
                    }
                },
                _ => {
                    report.warn($label, format!("{} not found; treating as empty", $rel));
                    vec![]
                }
            }
        }};
    }

    let objects = load_parquet!(idx_rel, read_object_index, "Object index");
    let edges = load_parquet!(edges_rel, read_object_edges, "Object edges");
    let derivations = load_parquet!(deriv_rel, read_derivations, "Derivations");

    // ── Validate target object exists ────────────────────────────────────
    if let Some(oid) = object_id {
        let found = objects.iter().any(|o| o.object_id == oid);
        if found {
            report.ok(format!("Object present: {oid}"));
        } else {
            report.fail(
                "Object present",
                format!("object_id '{oid}' not found in index"),
            );
        }
    }

    if !full_lineage {
        let valid = report.is_valid();
        if let Some(path) = report_path {
            write_json_report(path, &report, valid)?;
        }
        report.print();
        return Ok(valid);
    }

    // ── Full lineage: referential integrity ───────────────────────────────
    let lineage_report = ObjectLineageValidator::validate(&objects, &edges, &derivations);

    if lineage_report.missing_edge_parents.is_empty() {
        report.ok("Edge parent references valid");
    } else {
        report.fail(
            "Edge parent references",
            format!(
                "missing parent objects for edges: {}",
                lineage_report.missing_edge_parents.join(", ")
            ),
        );
    }

    if lineage_report.missing_edge_children.is_empty() {
        report.ok("Edge child references valid");
    } else {
        report.fail(
            "Edge child references",
            format!(
                "missing child objects for edges: {}",
                lineage_report.missing_edge_children.join(", ")
            ),
        );
    }

    if lineage_report.missing_derivation_parents.is_empty() {
        report.ok("Derivation parent references valid");
    } else {
        report.fail(
            "Derivation parent references",
            format!(
                "missing parent objects for derivations: {}",
                lineage_report.missing_derivation_parents.join(", ")
            ),
        );
    }

    if lineage_report.missing_derivation_children.is_empty() {
        report.ok("Derivation child references valid");
    } else {
        report.fail(
            "Derivation child references",
            format!(
                "missing child objects for derivations: {}",
                lineage_report.missing_derivation_children.join(", ")
            ),
        );
    }

    if lineage_report.invalid_derivation_links.is_empty() {
        report.ok("Derivation→edge links valid");
    } else {
        report.fail(
            "Derivation→edge links",
            format!(
                "derivations without corresponding edge: {}",
                lineage_report.invalid_derivation_links.join(", ")
            ),
        );
    }

    if lineage_report.cycles.is_empty() {
        report.ok("Object graph is acyclic");
    } else {
        for cycle in &lineage_report.cycles {
            report.fail(
                "Object graph cycle detected",
                format!("cycle: {}", cycle.join(" → ")),
            );
        }
    }

    // ── Derivation hash checks ─────────────────────────────────────────────
    let object_sha256: HashMap<&str, Option<&str>> = objects
        .iter()
        .map(|o| (o.object_id.as_str(), o.sha256.as_deref()))
        .collect();

    let mut hash_checks_ok = 0usize;
    let mut hash_checks_fail = 0usize;

    for drv in &derivations {
        if let Some(expected_input) = &drv.input_sha256 {
            let actual = object_sha256
                .get(drv.parent_object_id.as_str())
                .copied()
                .flatten();
            match actual {
                Some(h) if h == expected_input => {
                    hash_checks_ok += 1;
                }
                Some(h) => {
                    hash_checks_fail += 1;
                    report.fail(
                        "Derivation input hash",
                        format!(
                            "derivation {} parent {}: expected {} got {}",
                            drv.derivation_id,
                            drv.parent_object_id,
                            &expected_input[..16.min(expected_input.len())],
                            &h[..16.min(h.len())]
                        ),
                    );
                }
                None => {
                    report.warn(
                        "Derivation input hash",
                        format!(
                            "derivation {} parent {} has no sha256 in index",
                            drv.derivation_id, drv.parent_object_id
                        ),
                    );
                }
            }
        }

        if let Some(expected_output) = &drv.output_sha256 {
            let actual = object_sha256
                .get(drv.child_object_id.as_str())
                .copied()
                .flatten();
            match actual {
                Some(h) if h == expected_output => {
                    hash_checks_ok += 1;
                }
                Some(h) => {
                    hash_checks_fail += 1;
                    report.fail(
                        "Derivation output hash",
                        format!(
                            "derivation {} child {}: expected {} got {}",
                            drv.derivation_id,
                            drv.child_object_id,
                            &expected_output[..16.min(expected_output.len())],
                            &h[..16.min(h.len())]
                        ),
                    );
                }
                None => {
                    report.warn(
                        "Derivation output hash",
                        format!(
                            "derivation {} child {} has no sha256 in index",
                            drv.derivation_id, drv.child_object_id
                        ),
                    );
                }
            }
        }
    }

    if hash_checks_fail == 0 && hash_checks_ok > 0 {
        report.ok(format!("Derivation hash checks: {hash_checks_ok} passed"));
    } else if hash_checks_ok == 0 && derivations.is_empty() {
        report.ok("Derivation hash checks: no derivations to check");
    }

    // ── Derived object store integrity ────────────────────────────────────
    let derived_objects: Vec<_> = objects
        .iter()
        .filter(|o| o.source_layer == "derived" || o.storage_ref.is_some())
        .collect();

    let mut store_ok = 0usize;
    let mut store_fail = 0usize;

    for obj in &derived_objects {
        if let Some(sha256) = &obj.sha256 {
            if sha256.starts_with("sha256:") {
                let rel = derived_object_path(sha256);
                match container.exists(&rel) {
                    Ok(true) => {
                        // Verify stored hash by reading
                        match container.read_bytes(&rel) {
                            Ok(data) => {
                                let expected_hex =
                                    sha256.strip_prefix("sha256:").unwrap_or(sha256.as_str());
                                let actual_hex = format!("{:x}", Sha256::digest(&data));
                                if actual_hex == expected_hex {
                                    store_ok += 1;
                                } else {
                                    store_fail += 1;
                                    report.fail(
                                        "Derived object hash",
                                        format!(
                                            "object {} stored at {rel}: expected {} got {}",
                                            obj.object_id,
                                            &expected_hex[..16],
                                            &actual_hex[..16]
                                        ),
                                    );
                                }
                            }
                            Err(e) => {
                                store_fail += 1;
                                report.fail(
                                    "Derived object read",
                                    format!("object {} at {rel}: {e}", obj.object_id),
                                );
                            }
                        }
                    }
                    Ok(false) => {
                        store_fail += 1;
                        report.fail(
                            "Derived object missing",
                            format!(
                                "object {} references {} but file not found",
                                obj.object_id, rel
                            ),
                        );
                    }
                    Err(e) => {
                        store_fail += 1;
                        report.fail(
                            "Derived object exists check",
                            format!("object {}: {e}", obj.object_id),
                        );
                    }
                }
            }
        }
    }

    if store_fail == 0 {
        report.ok(format!(
            "Derived object store integrity: {store_ok} verified"
        ));
    }

    let valid = report.is_valid();
    if let Some(path) = report_path {
        write_json_report(path, &report, valid)?;
    }
    report.print();
    Ok(valid)
}

fn includes_extensions(profile: VerifyProfile) -> bool {
    matches!(
        profile,
        VerifyProfile::CoreExtensions | VerifyProfile::Conformance
    )
}

// ── Legacy compatibility checks ───────────────────────────────────────────────

/// Inspect the `analysis/` tree for v0.1-style layouts and emit warnings.
/// This does NOT fail the report — legacy outputs are accepted with caveats.
fn run_legacy_checks(container: &ContainerRef, report: &mut VerifyReport) {
    report.ok("Legacy profile: running v0.1 compatibility audit");

    let local_root = match container.local_path("") {
        Some(p) => p,
        None => {
            report.warn(
                "Legacy checks",
                "S3 containers do not support legacy layout inspection",
            );
            return;
        }
    };

    let analysis_dir = local_root.join("analysis");
    if !analysis_dir.exists() {
        report.ok("Legacy checks: no analysis/ directory found (no analysis outputs to audit)");
        return;
    }

    // ── Check for flat v0.1 direct files in analysis/ (non-job-scoped) ────
    let flat_files: Vec<String> = match fs::read_dir(&analysis_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => vec![],
    };
    if !flat_files.is_empty() {
        let names = flat_files.join(", ");
        report.warn(
            "Legacy layout: flat analysis files",
            format!(
                "analysis/ contains flat files ({names}) — v0.1 style, not job-scoped; \
                 mark as non-forensic-grade and migrate to analysis/jobs/<job_id>/ layout"
            ),
        );
    }

    // ── Check jobs/ subdirectories for missing result_manifest.json ───────
    let jobs_dir = analysis_dir.join("jobs");
    if !jobs_dir.exists() {
        report.ok("Legacy checks: analysis/jobs/ not present (no job-scoped outputs)");
        return;
    }

    let job_dirs: Vec<_> = match fs::read_dir(&jobs_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect(),
        Err(_) => {
            report.warn("Legacy checks", "could not read analysis/jobs/");
            return;
        }
    };

    if job_dirs.is_empty() {
        report.ok("Legacy checks: analysis/jobs/ is empty");
        return;
    }

    let mut legacy_count = 0usize;
    let mut compliant_count = 0usize;

    for entry in &job_dirs {
        let job_id = entry.file_name().to_string_lossy().into_owned();
        let result_manifest = entry.path().join("result_manifest.json");

        if !result_manifest.exists() {
            legacy_count += 1;
            report.warn(
                format!("Legacy job: {job_id}"),
                "missing result_manifest.json — non-forensic-grade v0.1 output; \
                 migrate to v0.2 output contract to attain forensic-grade status",
            );
        } else {
            // Check for v0.2 required fields in result_manifest.json
            match fs::read_to_string(&result_manifest) {
                Ok(raw) => {
                    let v: serde_json::Value =
                        serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
                    let has_job_id = v.get("job_id").and_then(|x| x.as_str()).is_some();
                    let has_status = v.get("status").and_then(|x| x.as_str()).is_some();
                    let has_tool = v.get("tool").is_some();
                    if has_job_id && has_status && has_tool {
                        compliant_count += 1;
                    } else {
                        legacy_count += 1;
                        let missing: Vec<&str> = [
                            (!has_job_id).then_some("job_id"),
                            (!has_status).then_some("status"),
                            (!has_tool).then_some("tool"),
                        ]
                        .into_iter()
                        .flatten()
                        .collect();
                        report.warn(
                            format!("Legacy job: {job_id}"),
                            format!(
                                "result_manifest.json missing fields: {} — \
                                 non-conformant with v0.2 output contract",
                                missing.join(", ")
                            ),
                        );
                    }
                }
                Err(e) => {
                    legacy_count += 1;
                    report.warn(
                        format!("Legacy job: {job_id}"),
                        format!("could not read result_manifest.json: {e}"),
                    );
                }
            }
        }
    }

    if legacy_count > 0 {
        report.warn(
            "Legacy migration guidance",
            format!(
                "{legacy_count} job(s) require migration to v0.2 output contract. \
                 Steps: (1) re-run with offf-keyword-worker / offf-yara-worker >= 0.2.0; \
                 (2) ensure result_manifest.json is written; \
                 (3) update scope field in job manifest to use scope_ref if applicable."
            ),
        );
    }
    if compliant_count > 0 {
        report.ok(format!(
            "Legacy checks: {compliant_count} job(s) already v0.2-compliant"
        ));
    }
}

fn run_provenance_schema_checks(content: &str, report: &mut VerifyReport) {
    let mut seen_ids = HashSet::new();
    let mut last_evt_num: Option<u64> = None;

    for (line_idx, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                report.fail(
                    "Provenance schema",
                    format!("invalid JSON at line {}: {e}", line_idx + 1),
                );
                return;
            }
        };

        let get_str = |k: &str| value.get(k).and_then(|v| v.as_str());
        let event_id = match get_str("event_id") {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                report.fail(
                    "Provenance schema",
                    format!("missing/empty event_id at line {}", line_idx + 1),
                );
                return;
            }
        };

        if !seen_ids.insert(event_id.to_string()) {
            report.fail(
                "Provenance schema",
                format!("duplicate event_id '{event_id}'"),
            );
            return;
        }

        for required in ["timestamp", "actor", "action"] {
            if get_str(required)
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                report.fail(
                    "Provenance schema",
                    format!("missing/empty {required} for event_id '{event_id}'"),
                );
                return;
            }
        }

        let tool = value.get("tool").and_then(|v| v.as_object());
        if tool
            .and_then(|t| t.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            report.fail(
                "Provenance schema",
                format!("missing tool.name for event_id '{event_id}'"),
            );
            return;
        }
        if tool
            .and_then(|t| t.get("version"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            report.fail(
                "Provenance schema",
                format!("missing tool.version for event_id '{event_id}'"),
            );
            return;
        }
        if value.get("details").is_none() {
            report.fail(
                "Provenance schema",
                format!("missing details for event_id '{event_id}'"),
            );
            return;
        }

        if let Some(num) = event_id
            .strip_prefix("evt-")
            .and_then(|s| s.parse::<u64>().ok())
        {
            if let Some(last) = last_evt_num {
                if num < last {
                    report.fail(
                        "Provenance ordering",
                        format!("event_id '{event_id}' is not monotonic"),
                    );
                    return;
                }
            }
            last_evt_num = Some(num);
        }
    }

    report.ok("Provenance schema checks passed");
}

fn run_extension_checks(
    manifest_value: Option<&serde_json::Value>,
    container: &ContainerRef,
    report: &mut VerifyReport,
) {
    let Some(manifest_obj) = manifest_value.and_then(|v| v.as_object()) else {
        report.warn(
            "Extensions",
            "manifest object unavailable for extension checks",
        );
        return;
    };

    let Some(ext) = manifest_obj.get("extensions") else {
        report.warn("Extensions", "manifest has no extensions block");
        return;
    };

    let mut paths = Vec::new();
    collect_string_paths(ext, &mut paths);
    if paths.is_empty() {
        report.warn(
            "Extensions",
            "extensions block present but no file paths found",
        );
        return;
    }

    // ── Check 1: referenced files exist ──────────────────────────────────
    let mut missing = Vec::new();
    for rel in &paths {
        match container.exists(rel) {
            Ok(true) => {}
            Ok(false) => missing.push(rel.clone()),
            Err(_) => missing.push(rel.clone()),
        }
    }

    if missing.is_empty() {
        report.ok(format!(
            "Extensions: {} referenced file(s) present",
            paths.len()
        ));
    } else {
        report.fail(
            "Extensions",
            format!("missing referenced extension files: {}", missing.join(", ")),
        );
    }

    // ── Check 2: validate known extension JSONL files ─────────────────────
    if let Some(local_root) = container.local_path("") {
        let results = offf_core::validate_extension_files(&local_root);
        let mut any_issue = false;
        for r in &results {
            if r.issues.is_empty() {
                report.ok(format!(
                    "Extension JSONL {}: {} record(s) valid",
                    r.rel_path, r.record_count
                ));
            } else {
                any_issue = true;
                for issue in &r.issues {
                    report.fail(
                        format!("Extension JSONL {}", r.rel_path),
                        issue.clone(),
                    );
                }
            }
        }
        if results.is_empty() {
            report.ok("Extension JSONL: no known extension files present (optional)");
        } else if !any_issue {
            report.ok(format!(
                "Extension JSONL: all {} file(s) structurally valid",
                results.len()
            ));
        }
    }
}

fn collect_string_paths(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_string_paths(v, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_string_paths(v, out);
            }
        }
        _ => {}
    }
}

fn write_json_report(path: &std::path::Path, report: &VerifyReport, valid: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let counts = report.status_counts();
    let payload = serde_json::json!({
        "container": report.container,
        "profile": report.profile,
        "valid": valid,
        "summary": {
            "ok": counts.get("ok").copied().unwrap_or(0),
            "warn": counts.get("warn").copied().unwrap_or(0),
            "fail": counts.get("fail").copied().unwrap_or(0),
        },
        "checks": report.checks,
    });
    fs::write(path, serde_json::to_string_pretty(&payload)?)?;
    Ok(())
}

fn parse_subset(arg: Option<&str>) -> Option<(HashSet<String>, HashSet<u64>)> {
    let text = arg?;
    let mut ids = HashSet::new();
    let mut seqs = HashSet::new();
    for token in text.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if token.starts_with("sha256:") {
            ids.insert(token.to_string());
        } else if let Ok(v) = token.parse::<u64>() {
            seqs.insert(v);
        }
    }
    Some((ids, seqs))
}

fn resolve_chunk_ref<'a>(
    chunk_ref: &str,
    chunks: &'a [offf_core::types::ChunkMetadata],
) -> Option<&'a offf_core::types::ChunkMetadata> {
    if chunk_ref.starts_with("sha256:") {
        chunks.iter().find(|c| c.chunk_id == chunk_ref)
    } else {
        chunk_ref
            .parse::<u64>()
            .ok()
            .and_then(|seq| chunks.iter().find(|c| c.sequence == seq))
    }
}
