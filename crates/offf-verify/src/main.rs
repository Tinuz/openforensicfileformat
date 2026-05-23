use std::collections::HashSet;

use anyhow::Result;
use clap::Parser;
use sha2::{Digest, Sha256};

use offf_core::{
    chunk::verify_chunk,
    hash::{deserialize_merkle_root, merkle_root},
    parquet_io::{read_physical_to_chunk, read_physical_to_chunk_bytes},
    storage::{read_chunk_verified, ContainerRef},
    types::{ManifestJson, OFFF_VERSION},
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
}

// ── Verification result ───────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct VerifyReport {
    container: String,
    checks: Vec<CheckResult>,
}

#[derive(Debug)]
struct CheckResult {
    label: String,
    status: CheckStatus,
    detail: Option<String>,
}

#[derive(Debug, PartialEq)]
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

    fn print(&self) {
        println!("Container: {}", self.container);
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
    let valid = verify(&args.container, args.chunks.as_deref())?;
    if !valid {
        std::process::exit(1);
    }
    Ok(())
}

fn verify(container_arg: &str, subset_arg: Option<&str>) -> Result<bool> {
    let container = ContainerRef::parse(container_arg)?;
    let mut report = VerifyReport {
        container: container.display(),
        ..Default::default()
    };

    // ── Check 1: manifest exists and is parseable ──────────────────────────
    let manifest = match container.read_text("manifest.json") {
        Ok(s) => match serde_json::from_str::<ManifestJson>(&s) {
            Ok(m) => {
                report.ok("Manifest present and valid");
                Some(m)
            }
            Err(e) => {
                report.fail("Manifest present and valid", e.to_string());
                None
            }
        },
        Err(_) => {
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
    if manifest.offf_version == OFFF_VERSION {
        report.ok(format!("OFFF version: {}", manifest.offf_version));
    } else {
        report.fail(
            "OFFF version",
            format!(
                "expected {OFFF_VERSION}, got {}",
                manifest.offf_version
            ),
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
                report.fail(
                    format!("Chunk {} hash", chunk.sequence),
                    e.to_string(),
                );
            }
        }
    }

    if stored_fail == 0 {
        report.ok(format!(
            "Stored hash validation: {stored_ok}/{total} OK"
        ));
        report.ok(format!(
            "Plaintext hash validation: {stored_ok}/{total} OK"
        ));
    } else {
        report.fail(
            "Stored/plaintext hash validation",
            format!("{stored_fail}/{total} chunks FAILED"),
        );
    }

    // ── Check 7: Merkle root ───────────────────────────────────────────────
    let is_subset = subset_arg.is_some();
    if is_subset {
        report.warn(
            "Merkle root",
            "skipped in subset mode (requires full leaf set)",
        );
    } else {
        let merkle_valid = match container.read_bytes("hashes/merkle_tree.bin") {
            Ok(blob) => match deserialize_merkle_root(&blob) {
                Ok(stored_root) => {
                    let leaf_hashes: Vec<String> =
                        chunks.iter().map(|c| c.plaintext_sha256.clone()).collect();
                    match merkle_root(&leaf_hashes) {
                        Ok(computed) => {
                            if computed == stored_root
                                && stored_root == manifest.hashes.merkle_root_sha256
                            {
                                report.ok(format!("Merkle root: {}", &stored_root[..16]));
                                true
                            } else if stored_root != manifest.hashes.merkle_root_sha256 {
                                report.fail(
                                    "Merkle root",
                                    format!(
                                        "bin file root ({}) differs from manifest ({})",
                                        &stored_root[..16],
                                        &manifest.hashes.merkle_root_sha256[..16]
                                    ),
                                );
                                false
                            } else {
                                report.fail(
                                    "Merkle root",
                                    format!(
                                        "recomputed ({}) differs from stored ({})",
                                        &computed[..16],
                                        &stored_root[..16]
                                    ),
                                );
                                false
                            }
                        }
                        Err(e) => {
                            report.fail("Merkle root recomputation", e.to_string());
                            false
                        }
                    }
                }
                Err(e) => {
                    report.fail("Merkle tree binary", e.to_string());
                    false
                }
            },
            Err(_) => {
                report.fail("Merkle tree binary", "hashes/merkle_tree.bin not found");
                false
            }
        };
        let _ = merkle_valid;
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
        }
        Ok(_) => report.fail("Provenance", "file exists but is empty"),
        Err(_) => report.fail("Provenance", "chain_of_custody.jsonl not found"),
    }

    let valid = report.is_valid();
    report.print();
    Ok(valid)
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
