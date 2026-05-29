use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use walkdir::WalkDir;

use offf_core::{
    evidence::write_evidence_object,
    parquet_io::{write_object_edges, write_object_index},
    provenance::ProvenanceWriter,
    types::{
        AcquisitionJson, AcquisitionSourceContext, AcquisitionMode, DiscoveredObjectRow,
        EvidenceRoot, ManifestIndexes, ManifestJson, ObjectEdgeRow, ToolInfo, OFFF_VERSION,
        TOOL_VERSION,
    },
};

const TOOL_NAME: &str = "offf-collect";

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "offf-collect",
    about = "Collect files or a directory tree into an OFFF file_collection container",
    version
)]
struct Args {
    /// One or more input paths (files or directories) to collect.
    #[arg(long, short, required = true, num_args = 1..)]
    input: Vec<PathBuf>,

    /// Output path for the new OFFF container directory.
    #[arg(long, short)]
    output: PathBuf,

    /// Case ID (used as container_id prefix). Defaults to a random UUID.
    #[arg(long)]
    case_id: Option<String>,

    /// Follow symbolic links during directory traversal.
    #[arg(long, default_value_t = false)]
    follow_symlinks: bool,

    /// Include hidden files and directories (those starting with '.').
    #[arg(long, default_value_t = false)]
    include_hidden: bool,

    /// Produce a deterministic container (use epoch 0 as timestamp).
    #[arg(long, default_value_t = false)]
    deterministic: bool,

    /// Optional human-readable description for this collection.
    #[arg(long)]
    description: Option<String>,

    /// Optional reason for collecting this evidence.
    #[arg(long)]
    collection_reason: Option<String>,
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args = Args::parse();
    let container_id = run(args)?;
    println!("Container created: {container_id}");
    Ok(())
}

fn run(args: Args) -> Result<String> {
    // ── Step 1: Validate inputs ────────────────────────────────────────────
    for input in &args.input {
        if !input.exists() {
            anyhow::bail!("input path does not exist: {}", input.display());
        }
    }

    // ── Step 2: Create temp output directory ──────────────────────────────
    let output = &args.output;
    if output.exists() {
        anyhow::bail!("output path already exists: {}", output.display());
    }

    let tmp_output = output.with_extension("offf.tmp");
    if tmp_output.exists() {
        fs::remove_dir_all(&tmp_output)
            .context("failed to remove existing temp output directory")?;
    }
    fs::create_dir_all(&tmp_output).context("failed to create temp output directory")?;

    let base = tmp_output.clone();

    let result = (|| -> Result<String> {
        let now = if args.deterministic {
            DateTime::<Utc>::from(SystemTime::UNIX_EPOCH)
        } else {
            Utc::now()
        };

        let case_id = args
            .case_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let container_id = format!("urn:offf:collection:{case_id}");

        // ── Step 3: Initialise provenance chain ────────────────────────────
        let prov_path = base.join("provenance").join("chain_of_custody.jsonl");
        fs::create_dir_all(prov_path.parent().unwrap())?;
        let mut prov = ProvenanceWriter::new(&prov_path)?;
        prov.record(
            "collection_started",
            TOOL_NAME,
            TOOL_VERSION,
            "system",
            serde_json::json!({
                "container_id": container_id,
                "inputs": args.input.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            }),
        )?;

        // ── Step 4: Create root collection object ─────────────────────────
        let root_obj_id = format!("obj-root-{}", &case_id[..8]);
        let mut object_rows: Vec<DiscoveredObjectRow> = Vec::new();
        let mut edge_rows: Vec<ObjectEdgeRow> = Vec::new();

        let root_description = args
            .description
            .clone()
            .unwrap_or_else(|| "File collection root".to_string());

        object_rows.push(DiscoveredObjectRow {
            object_id: root_obj_id.clone(),
            object_type: "collection_root".to_string(),
            name: Some(root_description.clone()),
            logical_path: None,
            media_type: None,
            size_bytes: None,
            sha256: None,
            source_layer: "collection".to_string(),
            storage_ref: None,
            content_ref: None,
            content_hash_status: None,
            root_source_ref: None,
            root_id: None,
            collection_relative_path: None,
            original_created_at: None,
            original_modified_at: None,
            original_accessed_at: None,
            created_by_job_id: None,
            parser_status: "ok".to_string(),
            provenance_ref: None,
            schema_version: "0.1.0".to_string(),
        });

        // ── Step 5 + 6: Enumerate, hash and store each file ───────────────
        let mut collection_hasher = Sha256::new();
        let mut file_count: u64 = 0;
        let mut total_bytes: u64 = 0;

        for input_path in &args.input {
            let input_root = if input_path.is_file() {
                input_path.parent().unwrap_or(Path::new(".")).to_path_buf()
            } else {
                input_path.clone()
            };

            let walker = WalkDir::new(input_path)
                .follow_links(args.follow_symlinks)
                .into_iter()
                .filter_entry(|e| {
                    if !args.include_hidden {
                        if let Some(name) = e.file_name().to_str() {
                            if name.starts_with('.') && e.depth() > 0 {
                                return false;
                            }
                        }
                    }
                    true
                });

            for entry in walker {
                let entry = entry.context("failed to read directory entry")?;
                let path = entry.path();

                // Only process regular files.
                if !path.is_file() {
                    continue;
                }

                // Compute relative path from the input root.
                let rel = path
                    .strip_prefix(&input_root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");

                // Read file content, hash and store.
                let content = fs::read(path)
                    .with_context(|| format!("failed to read file: {}", path.display()))?;
                let file_sha256 = write_evidence_object(&base, &content)
                    .with_context(|| format!("failed to store: {}", path.display()))?;

                // Accumulate into collection hash (sorted deterministically by path).
                collection_hasher.update(rel.as_bytes());
                collection_hasher.update(b"\n");
                collection_hasher.update(file_sha256.as_bytes());
                collection_hasher.update(b"\n");

                // Extract filesystem timestamps (best-effort).
                let meta = fs::metadata(path).ok();
                let created_at = meta
                    .as_ref()
                    .and_then(|m| m.created().ok())
                    .map(|t| DateTime::<Utc>::from(t).to_rfc3339());
                let modified_at = meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .map(|t| DateTime::<Utc>::from(t).to_rfc3339());
                let accessed_at = meta
                    .as_ref()
                    .and_then(|m| m.accessed().ok())
                    .map(|t| DateTime::<Utc>::from(t).to_rfc3339());

                let file_size = content.len() as u64;
                total_bytes += file_size;
                file_count += 1;

                // ── Step 7: Build object row ───────────────────────────────
                let obj_id = format!("obj-{}", &file_sha256[..16]);
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned());

                object_rows.push(DiscoveredObjectRow {
                    object_id: obj_id.clone(),
                    object_type: "file".to_string(),
                    name: file_name,
                    logical_path: Some(format!("{}/{}", input_root.display(), rel.clone())),
                    media_type: None,
                    size_bytes: Some(file_size),
                    sha256: Some(file_sha256.clone()),
                    source_layer: "collection".to_string(),
                    storage_ref: Some(file_sha256.clone()),
                    content_ref: None,
                    content_hash_status: Some("verified".to_string()),
                    root_source_ref: Some(root_obj_id.clone()),
                    root_id: Some(root_obj_id.clone()),
                    collection_relative_path: Some(rel.clone()),
                    original_created_at: created_at,
                    original_modified_at: modified_at,
                    original_accessed_at: accessed_at,
                    created_by_job_id: None,
                    parser_status: "ok".to_string(),
                    provenance_ref: None,
                    schema_version: "0.1.0".to_string(),
                });

                // ── Step 7b: Build root→file edge ─────────────────────────
                edge_rows.push(ObjectEdgeRow {
                    edge_id: format!("edge-{}-{}", &root_obj_id[4..], &obj_id[4..]),
                    parent_object_id: root_obj_id.clone(),
                    child_object_id: obj_id,
                    relation_type: "contains".to_string(),
                    method: None,
                    logical_path: Some(rel.clone()),
                    sequence: Some(file_count),
                    created_by_job_id: None,
                    provenance_ref: None,
                    schema_version: "0.1.0".to_string(),
                });

                prov.record(
                    "file_collected",
                    TOOL_NAME,
                    TOOL_VERSION,
                    "system",
                    serde_json::json!({
                        "path": rel.clone(),
                        "sha256": file_sha256,
                        "size_bytes": file_size,
                    }),
                )?;
            }
        }

        // ── Step 8: Collection root hash ──────────────────────────────────
        let collection_hash = format!("{:x}", collection_hasher.finalize());

        // Update the root object with collection stats.
        if let Some(root) = object_rows.first_mut() {
            root.sha256 = Some(collection_hash.clone());
            root.size_bytes = Some(total_bytes);
        }

        // ── Step 9: Write object_index and object_edges ────────────────────
        let idx_dir = base.join("indexes").join("objects");
        fs::create_dir_all(&idx_dir)?;
        write_object_index(&idx_dir.join("object_index.parquet"), &object_rows)
            .context("failed to write object_index.parquet")?;
        write_object_edges(&idx_dir.join("object_edges.parquet"), &edge_rows)
            .context("failed to write object_edges.parquet")?;

        // ── Step 10: acquisition.json ──────────────────────────────────────
        let acquisition = AcquisitionJson {
            container_id: container_id.clone(),
            acquisition_id: Some(format!("acq-{}", Uuid::new_v4())),
            acquisition_mode: Some("file_collection".to_string()),
            acquired_at: now,
            acquired_by: None,
            method: Some("directory_walk".to_string()),
            tool: ToolInfo {
                name: TOOL_NAME.to_string(),
                version: TOOL_VERSION.to_string(),
            },
            source: None,
            source_context: Some(AcquisitionSourceContext {
                description: root_description.clone(),
                original_root_path: args.input.first().map(|p| p.display().to_string()),
                collection_reason: args.collection_reason.clone(),
            }),
            source_container: None,
            evidence_stream: None,
            parameters: None,
            limitations: Some(vec![
                "collection does not guarantee completeness".to_string(),
                "no block-level imaging; deleted files not recovered".to_string(),
            ]),
        };
        let acq_json =
            serde_json::to_string_pretty(&acquisition).context("failed to serialise acquisition")?;
        fs::write(base.join("acquisition.json"), acq_json)
            .context("failed to write acquisition.json")?;

        // ── Step 11: manifest.json ─────────────────────────────────────────
        let manifest = ManifestJson {
            offf_version: OFFF_VERSION.to_string(),
            container_id: container_id.clone(),
            created_at: now,
            created_by_tool: ToolInfo {
                name: TOOL_NAME.to_string(),
                version: TOOL_VERSION.to_string(),
            },
            acquisition_mode: Some(AcquisitionMode::FileCollection),
            source: None,
            hashes: None,
            chunking: None,
            evidence_roots: Some(vec![EvidenceRoot {
                root_id: root_obj_id.clone(),
                root_type: "collection_root".to_string(),
                description: Some(root_description),
                object_count: Some(file_count),
                root_hash: Some(collection_hash.clone()),
            }]),
            limitations: Some(vec![
                "collection does not guarantee completeness".to_string(),
                "no block-level imaging; deleted files not recovered".to_string(),
            ]),
            indexes: ManifestIndexes {
                physical_to_chunk: None,
                object_index: Some("indexes/objects/object_index.parquet".to_string()),
                object_edges: Some("indexes/objects/object_edges.parquet".to_string()),
            },
            extensions: None,
        };
        let manifest_json =
            serde_json::to_string_pretty(&manifest).context("failed to serialise manifest")?;
        fs::write(base.join("manifest.json"), manifest_json)
            .context("failed to write manifest.json")?;

        prov.record(
            "collection_completed",
            TOOL_NAME,
            TOOL_VERSION,
            "system",
            serde_json::json!({
                "file_count": file_count,
                "total_bytes": total_bytes,
                "collection_hash": collection_hash,
            }),
        )?;

        Ok(container_id)
    })();

    let container_id = match result {
        Ok(id) => id,
        Err(e) => {
            let _ = fs::remove_dir_all(&tmp_output);
            return Err(e);
        }
    };

    // ── Step 12+13: Atomic rename ──────────────────────────────────────────
    fs::rename(&tmp_output, output).context("failed to rename temp directory to final output")?;

    Ok(container_id)
}
