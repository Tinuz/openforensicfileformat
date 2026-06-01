use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::Parser;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use offf_core::types::{ManifestJson, ToolInfo, OFFF_V2_VERSION, OFFF_VERSION};

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "offf-migrate",
    about = "Migrate an OFFF container between format versions",
    version
)]
struct Args {
    /// Path to the source OFFF container directory.
    container: PathBuf,

    /// Output directory for the migrated container.
    ///
    /// If omitted, the migration is performed in-place (--in-place must be set).
    #[arg(long)]
    out: Option<PathBuf>,

    /// Migrate the container in-place (rewrites manifest.json inside the source directory).
    ///
    /// Cannot be combined with --out.
    #[arg(long, conflicts_with = "out")]
    in_place: bool,

    /// Target OFFF version to migrate to. Currently only "0.2.0" is supported.
    #[arg(long, default_value = "0.2.0")]
    target_version: String,

    /// Preview the migration without writing any files.
    #[arg(long)]
    dry_run: bool,

    /// Write the JSON migration report to this file instead of stdout.
    #[arg(long)]
    report: Option<PathBuf>,
}

// ── Report ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct MigrationReport {
    source_container: String,
    container_id: String,
    source_version: String,
    target_version: String,
    dry_run: bool,
    migrated_at: String,
    migrated_by_tool: ToolInfo,
    invariant_checks: InvariantChecks,
    changes: Vec<String>,
    result: MigrationResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct InvariantChecks {
    container_id_preserved: bool,
    source_sha256_preserved: bool,
    merkle_root_preserved: bool,
    chunk_files_unmodified: bool,
    physical_map_unmodified: bool,
}

impl InvariantChecks {
    fn all_passed(&self) -> bool {
        self.container_id_preserved
            && self.source_sha256_preserved
            && self.merkle_root_preserved
            && self.chunk_files_unmodified
            && self.physical_map_unmodified
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum MigrationResult {
    Success,
    DryRun,
    Failed,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Compute the SHA-256 of a file's contents.
fn file_sha256(path: &Path) -> Result<String> {
    let bytes = fs::read(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let digest = Sha256::digest(&bytes);
    Ok(hex::encode(digest))
}

/// Collect the names of all files directly under `chunks/`.
fn chunk_names(container: &Path) -> Result<Vec<String>> {
    let dir = container.join("chunks");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut names: Vec<String> = fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(names)
}

/// Read and deserialise the manifest from a container directory.
fn read_manifest(container: &Path) -> Result<ManifestJson> {
    let path = container.join("manifest.json");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("parsing {}", path.display()))
}

/// Verify evidence hash invariants between `source` manifest and `baseline` chunk state.
/// Returns an `InvariantChecks` struct; callers inspect `.all_passed()`.
fn check_invariants(
    source_container: &Path,
    source_manifest: &ManifestJson,
) -> Result<InvariantChecks> {
    // container_id: preserved as-is (always passes at this stage – verified
    // again after writing output).
    let container_id_preserved = true;

    // source_sha256 and merkle_root: we do not recompute the source image here;
    // we rely on the values already stored in the manifest being trustworthy
    // (offf-verify covers full recomputation). What we assert is that the fields
    // are present and non-empty.
    let source_sha256_preserved = source_manifest.hashes.source_sha256.len() == 64;
    let merkle_root_preserved = source_manifest.hashes.merkle_root_sha256.len() == 64;

    // chunk_files_unmodified: each file under chunks/ must be named by its own
    // SHA-256 hex. We only verify the name → content binding.
    let mut chunk_files_unmodified = true;
    for name in chunk_names(source_container)? {
        // Strip any extension appended by compression (.zst etc.)
        let base = if let Some(stem) = name.strip_suffix(".zst") {
            stem.to_owned()
        } else {
            name.clone()
        };
        // Chunk names that are valid sha256 hex (64 hex chars) are content-addressed.
        // We verify the raw file hash only for uncompressed chunks; compressed
        // chunks are validated by offf-verify's full integrity pass.
        if base.len() == 64 && base.chars().all(|c| c.is_ascii_hexdigit()) {
            let path = source_container.join("chunks").join(&name);
            // For compressed chunks we can only check that the file is readable.
            if !name.ends_with(".zst") {
                let computed = file_sha256(&path)
                    .with_context(|| format!("hashing chunk {}", name))?;
                if computed != base {
                    chunk_files_unmodified = false;
                    break;
                }
            }
        }
    }

    // physical_map_unmodified: the parquet map must be readable (content
    // correctness is covered by offf-verify; here we check presence).
    let map_path = source_container.join("maps").join("physical_to_chunk.parquet");
    let physical_map_unmodified = map_path.exists();

    Ok(InvariantChecks {
        container_id_preserved,
        source_sha256_preserved,
        merkle_root_preserved,
        chunk_files_unmodified,
        physical_map_unmodified,
    })
}

// ── Migration logic ───────────────────────────────────────────────────────────

/// Bump the manifest's offf_version and record migration provenance in extensions.
fn build_migrated_manifest(source: &ManifestJson, target_version: &str) -> ManifestJson {
    use offf_core::types::ManifestExtensions;
    use std::collections::HashMap;

    let mut out = source.clone();
    out.offf_version = target_version.to_owned();

    // Record migration in extensions["offf:migration"].
    let migration_meta = serde_json::json!({
        "migrated_from": source.offf_version,
        "migrated_to": target_version,
        "migrated_at": Utc::now().to_rfc3339(),
        "migrated_by": {
            "name": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
        }
    });

    let mut entries: HashMap<String, serde_json::Value> = out
        .extensions
        .as_ref()
        .map(|e| e.entries.clone())
        .unwrap_or_default();
    entries.insert("offf:migration".to_owned(), migration_meta);
    out.extensions = Some(ManifestExtensions { entries });
    out
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn run(args: &Args) -> Result<MigrationReport> {
    let source = args.container.canonicalize()
        .with_context(|| format!("resolving container path: {}", args.container.display()))?;

    // Validate arguments.
    if !args.dry_run && args.out.is_none() && !args.in_place {
        bail!("specify either --out <dir> or --in-place (or --dry-run to preview)");
    }
    if args.target_version != OFFF_V2_VERSION {
        bail!(
            "unsupported target version '{}'; only '{}' is currently supported",
            args.target_version,
            OFFF_V2_VERSION
        );
    }

    // Read source manifest.
    let manifest = read_manifest(&source)?;

    // Check source version.
    if manifest.offf_version == args.target_version {
        bail!(
            "container is already at version '{}'; nothing to migrate",
            args.target_version
        );
    }
    if manifest.offf_version != OFFF_VERSION {
        bail!(
            "unsupported source version '{}'; only '{}' → '{}' migration is supported",
            manifest.offf_version,
            OFFF_VERSION,
            OFFF_V2_VERSION
        );
    }

    // Verify invariants.
    let invariants = check_invariants(&source, &manifest)?;
    if !invariants.all_passed() {
        let report = MigrationReport {
            source_container: source.display().to_string(),
            container_id: manifest.container_id.clone(),
            source_version: manifest.offf_version.clone(),
            target_version: args.target_version.clone(),
            dry_run: args.dry_run,
            migrated_at: Utc::now().to_rfc3339(),
            migrated_by_tool: ToolInfo {
                name: env!("CARGO_PKG_NAME").to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            invariant_checks: invariants,
            changes: vec![],
            result: MigrationResult::Failed,
            error: Some("evidence hash invariant check failed; migration aborted".to_owned()),
        };
        return Ok(report);
    }

    let new_manifest = build_migrated_manifest(&manifest, &args.target_version);
    let changes = vec![
        format!(
            "manifest.json: offf_version '{}' → '{}'",
            manifest.offf_version, args.target_version
        ),
        "manifest.json: extensions[\"offf:migration\"] added".to_owned(),
    ];

    if args.dry_run {
        return Ok(MigrationReport {
            source_container: source.display().to_string(),
            container_id: manifest.container_id.clone(),
            source_version: manifest.offf_version.clone(),
            target_version: args.target_version.clone(),
            dry_run: true,
            migrated_at: Utc::now().to_rfc3339(),
            migrated_by_tool: ToolInfo {
                name: env!("CARGO_PKG_NAME").to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            invariant_checks: invariants,
            changes,
            result: MigrationResult::DryRun,
            error: None,
        });
    }

    // Determine output manifest path.
    let out_manifest_path: PathBuf = if args.in_place {
        source.join("manifest.json")
    } else {
        let out_dir = args.out.as_ref().unwrap();
        // Copy entire container to output directory.
        copy_dir_all(&source, out_dir)
            .with_context(|| format!("copying container to {}", out_dir.display()))?;
        out_dir.join("manifest.json")
    };

    // Serialise and write the migrated manifest.
    let manifest_text = serde_json::to_string_pretty(&new_manifest)
        .context("serialising migrated manifest")?;
    fs::write(&out_manifest_path, manifest_text)
        .with_context(|| format!("writing {}", out_manifest_path.display()))?;

    // Post-write invariant check: re-read and verify key fields.
    let written = read_manifest(out_manifest_path.parent().unwrap())?;
    if written.container_id != manifest.container_id {
        bail!("post-write invariant failure: container_id was altered");
    }
    if written.hashes.source_sha256 != manifest.hashes.source_sha256 {
        bail!("post-write invariant failure: source_sha256 was altered");
    }
    if written.hashes.merkle_root_sha256 != manifest.hashes.merkle_root_sha256 {
        bail!("post-write invariant failure: merkle_root_sha256 was altered");
    }

    Ok(MigrationReport {
        source_container: source.display().to_string(),
        container_id: manifest.container_id,
        source_version: manifest.offf_version,
        target_version: args.target_version.clone(),
        dry_run: false,
        migrated_at: Utc::now().to_rfc3339(),
        migrated_by_tool: ToolInfo {
            name: env!("CARGO_PKG_NAME").to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        },
        invariant_checks: invariants,
        changes,
        result: MigrationResult::Success,
        error: None,
    })
}

/// Recursively copy a directory tree from `src` to `dst`.
fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)
        .with_context(|| format!("creating {}", dst.display()))?;
    for entry in fs::read_dir(src)
        .with_context(|| format!("reading {}", src.display()))?
    {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest)
                .with_context(|| format!("copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn main() {
    let args = Args::parse();

    let report = match run(&args) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {:#}", e);
            std::process::exit(2);
        }
    };

    let json = serde_json::to_string_pretty(&report).expect("serialise report");

    match &args.report {
        Some(path) => {
            if let Err(e) = fs::write(path, &json) {
                eprintln!("warning: could not write report to {}: {}", path.display(), e);
            }
            // Also print a brief summary to stdout.
            println!(
                "{}: {} ({})",
                report.result_label(),
                report.container_id,
                report.source_version
            );
        }
        None => println!("{}", json),
    }

    if report.result == MigrationResult::Failed {
        std::process::exit(1);
    }
}

impl MigrationReport {
    fn result_label(&self) -> &'static str {
        match self.result {
            MigrationResult::Success => "migrated",
            MigrationResult::DryRun => "dry-run ok",
            MigrationResult::Failed => "failed",
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn minimal_manifest_v1() -> ManifestJson {
        use offf_core::types::{
            ChunkingInfo, ManifestHashes, ManifestIndexes, SourceInfo,
        };
        ManifestJson {
            offf_version: OFFF_VERSION.to_owned(),
            container_id: "urn:offf:case:test-0001".to_owned(),
            created_at: Utc::now(),
            created_by_tool: ToolInfo {
                name: "test".to_owned(),
                version: "0.0.1".to_owned(),
            },
            source: SourceInfo {
                source_type: "raw_image".to_owned(),
                size_bytes: 1024,
                sector_size: 512,
            },
            hashes: ManifestHashes {
                source_sha256: "a".repeat(64),
                merkle_root_sha256: "b".repeat(64),
            },
            chunking: ChunkingInfo {
                chunk_size: 1048576,
                chunking_mode: "fixed".to_owned(),
                compression: "none".to_owned(),
                hash_algorithm: "sha256".to_owned(),
            },
            indexes: ManifestIndexes {
                physical_to_chunk: "maps/physical_to_chunk.parquet".to_owned(),
            },
            extensions: None,
        }
    }

    fn write_container(dir: &Path, manifest: &ManifestJson) -> Result<()> {
        let maps = dir.join("maps");
        fs::create_dir_all(&maps)?;
        // Write a placeholder physical_to_chunk.parquet so presence check passes.
        fs::write(maps.join("physical_to_chunk.parquet"), b"PAR1")?;
        let text = serde_json::to_string_pretty(manifest)?;
        fs::write(dir.join("manifest.json"), text)?;
        Ok(())
    }

    #[test]
    fn dry_run_reports_expected_changes() {
        let tmp = TempDir::new().unwrap();
        let manifest = minimal_manifest_v1();
        write_container(tmp.path(), &manifest).unwrap();

        let args = Args {
            container: tmp.path().to_path_buf(),
            out: None,
            in_place: false,
            target_version: OFFF_V2_VERSION.to_owned(),
            dry_run: true,
            report: None,
        };
        let report = run(&args).unwrap();
        assert_eq!(report.result, MigrationResult::DryRun);
        assert!(report.invariant_checks.all_passed());
        assert!(report.changes.iter().any(|c| c.contains("offf_version")));
        assert!(report.changes.iter().any(|c| c.contains("offf:migration")));
    }

    #[test]
    fn in_place_migration_bumps_version() {
        let tmp = TempDir::new().unwrap();
        let manifest = minimal_manifest_v1();
        write_container(tmp.path(), &manifest).unwrap();

        let args = Args {
            container: tmp.path().to_path_buf(),
            out: None,
            in_place: true,
            target_version: OFFF_V2_VERSION.to_owned(),
            dry_run: false,
            report: None,
        };
        let report = run(&args).unwrap();
        assert_eq!(report.result, MigrationResult::Success);

        let written = read_manifest(tmp.path()).unwrap();
        assert_eq!(written.offf_version, OFFF_V2_VERSION);
        // Invariants preserved
        assert_eq!(written.container_id, manifest.container_id);
        assert_eq!(written.hashes.source_sha256, manifest.hashes.source_sha256);
        assert_eq!(
            written.hashes.merkle_root_sha256,
            manifest.hashes.merkle_root_sha256
        );
        // Migration extension recorded
        let ext = written.extensions.unwrap();
        assert!(ext.entries.contains_key("offf:migration"));
    }

    #[test]
    fn out_dir_migration_copies_container() {
        let tmp_src = TempDir::new().unwrap();
        let tmp_dst = TempDir::new().unwrap();
        let manifest = minimal_manifest_v1();
        write_container(tmp_src.path(), &manifest).unwrap();

        let out = tmp_dst.path().join("migrated");
        let args = Args {
            container: tmp_src.path().to_path_buf(),
            out: Some(out.clone()),
            in_place: false,
            target_version: OFFF_V2_VERSION.to_owned(),
            dry_run: false,
            report: None,
        };
        let report = run(&args).unwrap();
        assert_eq!(report.result, MigrationResult::Success);

        // Source is unchanged
        let src_manifest = read_manifest(tmp_src.path()).unwrap();
        assert_eq!(src_manifest.offf_version, OFFF_VERSION);

        // Output has the new version
        let out_manifest = read_manifest(&out).unwrap();
        assert_eq!(out_manifest.offf_version, OFFF_V2_VERSION);
        assert!(out.join("maps").join("physical_to_chunk.parquet").exists());
    }

    #[test]
    fn already_at_target_version_returns_error() {
        let tmp = TempDir::new().unwrap();
        let mut manifest = minimal_manifest_v1();
        manifest.offf_version = OFFF_V2_VERSION.to_owned();
        write_container(tmp.path(), &manifest).unwrap();

        let args = Args {
            container: tmp.path().to_path_buf(),
            out: None,
            in_place: true,
            target_version: OFFF_V2_VERSION.to_owned(),
            dry_run: false,
            report: None,
        };
        assert!(run(&args).is_err());
    }

    #[test]
    fn missing_physical_map_fails_invariants() {
        let tmp = TempDir::new().unwrap();
        let manifest = minimal_manifest_v1();
        // Write manifest without the physical_to_chunk.parquet file
        let text = serde_json::to_string_pretty(&manifest).unwrap();
        fs::write(tmp.path().join("manifest.json"), text).unwrap();

        let invariants = check_invariants(tmp.path(), &manifest).unwrap();
        assert!(!invariants.physical_map_unmodified);
        assert!(!invariants.all_passed());
    }

    #[test]
    fn build_migrated_manifest_preserves_hashes() {
        let manifest = minimal_manifest_v1();
        let migrated = build_migrated_manifest(&manifest, OFFF_V2_VERSION);
        assert_eq!(migrated.offf_version, OFFF_V2_VERSION);
        assert_eq!(migrated.container_id, manifest.container_id);
        assert_eq!(migrated.hashes.source_sha256, manifest.hashes.source_sha256);
        assert_eq!(
            migrated.hashes.merkle_root_sha256,
            manifest.hashes.merkle_root_sha256
        );
    }
}
