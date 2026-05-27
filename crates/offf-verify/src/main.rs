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
    hash::{generate_merkle_proof, parse_and_validate_merkle_tree, verify_merkle_proof},
    lineage::ObjectLineageValidator,
    parquet_io::{
        read_derivations, read_leaves, read_object_edges, read_object_index,
        read_physical_to_chunk, read_physical_to_chunk_bytes,
    },
    storage::{derived_object_path, read_chunk_verified, ContainerRef},
    types::{AcquisitionJson, ManifestJson, OFFF_V2_VERSION, OFFF_VERSION},
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
    let all_chunks = match container.local_path(&manifest.indexes.physical_to_chunk) {
        Some(p) => read_physical_to_chunk(&p),
        None => {
            let data = container.read_bytes(&manifest.indexes.physical_to_chunk);
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

                    if tree.root == manifest.hashes.merkle_root_sha256 {
                        report.ok(format!("Merkle root: {}", &tree.root[..16]));
                    } else {
                        report.fail(
                            "Merkle root",
                            format!(
                                "tree root ({}) differs from manifest ({})",
                                &tree.root[..16],
                                &manifest.hashes.merkle_root_sha256[..16]
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
                                &manifest.hashes.merkle_root_sha256,
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
            if computed == manifest.hashes.source_sha256 {
                report.ok(format!(
                    "Source SHA-256: {}",
                    &manifest.hashes.source_sha256[..16]
                ));
            } else {
                report.fail(
                    "Source SHA-256",
                    format!(
                        "computed {}, manifest has {}",
                        &computed[..16],
                        &manifest.hashes.source_sha256[..16]
                    ),
                );
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

// ── Object lineage verification ───────────────────────────────────────────────

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
