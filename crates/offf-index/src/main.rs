use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use offf_core::{
    lineage::{export_dot, export_lineage_json, ObjectLineageValidator},
    ntfs::index_ntfs,
    parquet_io::{
        for_each_derivation_batch, for_each_edge_batch, for_each_object_batch,
        read_derivations, read_object_edges, read_object_index, read_physical_to_chunk,
        write_derivations, write_derivations_batched, write_file_index, write_object_edges,
        write_object_edges_batched, write_object_index, write_object_index_batched,
    },
    partition::{detect_and_parse, detect_volume_type},
    provenance::ProvenanceWriter,
    rebuild_object_index_from_events,
    types::{DerivationRow, DiscoveredObjectRow, ManifestJson, ObjectEdgeRow},
};
const TOOL_NAME: &str = "offf-index";
const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "offf-index",
    about = "Build structural indexes for an OFFF container",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Detect and index the partition table (MBR or GPT)
    Partitions {
        /// Path to the OFFF container directory
        container: PathBuf,
    },
    /// Index the filesystem of a partition (currently: NTFS)
    Filesystem {
        /// Path to the OFFF container directory
        container: PathBuf,
        /// Partition ID to index (e.g. "gpt-2", "mbr-1", "volume-1").
        /// If omitted, auto-selects the partition when there is exactly one.
        #[arg(long)]
        partition: Option<String>,
    },
    /// Deterministically rebuild the object graph indexes from job deltas
    Objects {
        /// Path to the OFFF container directory
        container: PathBuf,
        /// When set, skip writing if indexes already exist (idempotent run)
        #[arg(long)]
        skip_existing: bool,
        /// Stream the existing index in batches of this many rows when merging,
        /// bounding peak heap to O(batch_size + delta_size) instead of
        /// O(total_index_size + delta_size).  Omit for the default eager mode.
        #[arg(long)]
        batch_size: Option<usize>,
        /// Rebuild object index from `indexes/objects/object_events.jsonl` (event-log mode)
        /// instead of scanning job delta files.
        #[arg(long)]
        from_events: bool,
    },
    /// Export the object lineage graph as a self-contained offline report
    ExportLineage {
        /// Path to the OFFF container directory
        container: PathBuf,
        /// Output format: `json` (default), `dot`, or `csv`
        #[arg(long, default_value = "json")]
        format: String,
        /// Write output to this file path (stdout if omitted)
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Partitions { container } => cmd_partitions(&container),
        Command::Filesystem {
            container,
            partition,
        } => cmd_filesystem(&container, partition),
        Command::Objects {
            container,
            skip_existing,
            batch_size,
            from_events,
        } => cmd_objects(&container, skip_existing, batch_size, from_events),
        Command::ExportLineage {
            container,
            format,
            out,
        } => cmd_export_lineage(&container, &format, out.as_deref()),
    }
}

// ── offf-index partitions ─────────────────────────────────────────────────────

fn cmd_partitions(base: &Path) -> Result<()> {
    let manifest_raw = fs::read_to_string(base.join("manifest.json"))
        .context("manifest.json not found – is this an OFFF container?")?;
    let manifest: ManifestJson =
        serde_json::from_str(&manifest_raw).context("invalid manifest.json")?;

    println!("Container: {}", base.display());
    println!("Container ID: {}", manifest.container_id);
    println!("Sector size: {} bytes", manifest.source.sector_size);
    println!();

    let map_path = base.join(&manifest.indexes.physical_to_chunk);
    let chunks =
        read_physical_to_chunk(&map_path).context("failed to read physical_to_chunk.parquet")?;

    if chunks.is_empty() {
        anyhow::bail!("no chunks found – cannot parse partition table");
    }

    println!("Loaded {} chunks. Detecting partition table…", chunks.len());

    let table = detect_and_parse(
        base,
        &chunks,
        manifest.source.sector_size,
        &manifest.container_id,
        TOOL_NAME,
    )
    .context("partition table detection failed")?;

    let out_path = base.join("indexes/partition_table.json");
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&table).context("serialisation failed")?;
    fs::write(&out_path, &json).with_context(|| format!("write failed: {}", out_path.display()))?;

    println!();
    println!("Partition table type: {}", table.partition_table_type);
    if let Some(guid) = &table.disk_guid {
        println!("Disk GUID:            {guid}");
    }
    println!("Partitions found:     {}", table.partitions.len());
    println!();

    for p in &table.partitions {
        println!(
            "  {:8}  {:>15} bytes  LBA {:>9}–{:>9}  {}",
            p.partition_id, p.length, p.first_lba, p.last_lba, p.partition_type
        );
        if let Some(name) = &p.name {
            print!("           Name: {name}");
        }
        if let Some(fs) = &p.filesystem_type {
            print!("  Filesystem: {fs}");
        }
        println!();
        println!(
            "           Offset: {} bytes  Chunks: {}",
            p.start_offset,
            p.chunk_refs.len()
        );
    }

    println!();
    println!("Written: {}", out_path.display());

    let prov_path = base.join("provenance/chain_of_custody.jsonl");
    let mut prov = ProvenanceWriter::new(&prov_path).context("provenance writer failed")?;
    prov.record(
        "indexed_partitions",
        TOOL_NAME,
        TOOL_VERSION,
        "system",
        serde_json::json!({
            "container": base.display().to_string(),
            "partition_table_type": table.partition_table_type,
            "partitions_found": table.partitions.len(),
            "output": out_path.display().to_string(),
        }),
    )
    .context("provenance write failed")?;

    Ok(())
}

// ── offf-index filesystem ─────────────────────────────────────────────────────

fn cmd_filesystem(base: &Path, partition_arg: Option<String>) -> Result<()> {
    let manifest_raw = fs::read_to_string(base.join("manifest.json"))
        .context("manifest.json not found – is this an OFFF container?")?;
    let manifest: ManifestJson =
        serde_json::from_str(&manifest_raw).context("invalid manifest.json")?;

    println!("Container:    {}", base.display());
    println!("Container ID: {}", manifest.container_id);
    println!();

    let map_path = base.join(&manifest.indexes.physical_to_chunk);
    let chunks =
        read_physical_to_chunk(&map_path).context("failed to read physical_to_chunk.parquet")?;

    if chunks.is_empty() {
        anyhow::bail!("no chunks found in container");
    }

    let (partition_id, volume_offset, volume_size, fs_type) =
        resolve_partition(base, &chunks, manifest.source.sector_size, &partition_arg)?;

    println!("Partition:    {partition_id}");
    println!("Filesystem:   {fs_type}");
    println!("Volume:       offset={volume_offset} bytes, size={volume_size} bytes");
    println!();

    if fs_type.to_uppercase() != "NTFS" {
        anyhow::bail!(
            "filesystem type '{fs_type}' is not supported yet; only NTFS is implemented in Phase 3"
        );
    }

    let filesystem_id = format!("ntfs-{}", partition_id);
    let rows = index_ntfs(
        base,
        &chunks,
        volume_offset,
        volume_size,
        &partition_id,
        &filesystem_id,
        TOOL_NAME,
    )
    .context("NTFS indexing failed")?;

    let total = rows.len();
    let files = rows
        .iter()
        .filter(|r| !r.is_directory && !r.is_deleted)
        .count();
    let dirs = rows
        .iter()
        .filter(|r| r.is_directory && !r.is_deleted)
        .count();
    let deleted = rows.iter().filter(|r| r.is_deleted).count();
    let partial = rows.iter().filter(|r| r.parser_status == "partial").count();
    let errors = rows.iter().filter(|r| r.parser_status == "error").count();

    println!("MFT entries indexed: {total}");
    println!("  Active files:      {files}");
    println!("  Active dirs:       {dirs}");
    println!("  Deleted entries:   {deleted}");
    println!("  Partial parses:    {partial}");
    println!("  Parse errors:      {errors}");
    println!();

    let out_dir = base.join(format!("indexes/filesystems/{partition_id}"));
    fs::create_dir_all(&out_dir)?;
    let parquet_path = out_dir.join("file_index.parquet");
    write_file_index(&parquet_path, &rows).context("failed to write file_index.parquet")?;
    println!("Written: {}", parquet_path.display());

    let prov_path = base.join("provenance/chain_of_custody.jsonl");
    let mut prov = ProvenanceWriter::new(&prov_path).context("provenance writer failed")?;
    prov.record(
        "indexed_filesystem",
        TOOL_NAME,
        TOOL_VERSION,
        "system",
        serde_json::json!({
            "container": base.display().to_string(),
            "partition_id": partition_id,
            "filesystem_id": filesystem_id,
            "filesystem_type": fs_type,
            "entries_indexed": total,
            "output": parquet_path.display().to_string(),
        }),
    )
    .context("provenance write failed")?;

    Ok(())
}

/// Determine the volume offset, size, partition ID, and filesystem type.
fn resolve_partition(
    base: &Path,
    chunks: &[offf_core::types::ChunkMetadata],
    sector_size: u32,
    partition_arg: &Option<String>,
) -> Result<(String, u64, u64, String)> {
    use offf_core::types::PartitionTableJson;

    let table_path = base.join("indexes/partition_table.json");
    let existing_table: Option<PartitionTableJson> = if table_path.exists() {
        let raw = fs::read_to_string(&table_path)?;
        serde_json::from_str(&raw).ok()
    } else {
        None
    };

    let table = match existing_table {
        Some(t) => t,
        None => detect_and_parse(base, chunks, sector_size, "auto", "offf-index")
            .context("partition table detection failed")?,
    };

    let target_id = match partition_arg {
        Some(id) => id.clone(),
        None => {
            if table.partitions.len() == 1 {
                table.partitions[0].partition_id.clone()
            } else {
                anyhow::bail!(
                    "container has {} partitions; specify one with --partition <id>",
                    table.partitions.len()
                );
            }
        }
    };

    let part = table
        .partitions
        .iter()
        .find(|p| p.partition_id == target_id)
        .with_context(|| {
            format!(
                "partition '{target_id}' not found (available: {})",
                table
                    .partitions
                    .iter()
                    .map(|p| p.partition_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    let fs_type = if let Some(ft) = &part.filesystem_type {
        ft.clone()
    } else {
        let volume_offset = part.start_offset;
        let sector = offf_core::partition::read_bytes_at(base, chunks, volume_offset, 512)
            .unwrap_or_default();
        detect_volume_type(&sector).unwrap_or_else(|| "unknown".to_string())
    };

    Ok((
        part.partition_id.clone(),
        part.start_offset,
        part.length,
        fs_type,
    ))
}

// ── offf-index objects ────────────────────────────────────────────────────────

/// Deterministically rebuild `indexes/objects/` from all job deltas found in
/// `analysis/jobs/*/objects_delta.jsonl`, `object_edges_delta.jsonl`, and
/// `derivations_delta.jsonl`.
///
/// Merge strategy: first-writer-wins per object_id / edge_id / derivation_id.
/// Job directories are sorted lexicographically to ensure reproducibility.
fn cmd_objects(
    base: &Path,
    skip_existing: bool,
    batch_size: Option<usize>,
    from_events: bool,
) -> Result<()> {
    let manifest_raw = fs::read_to_string(base.join("manifest.json"))
        .context("manifest.json not found – is this an OFFF container?")?;
    let _manifest: ManifestJson =
        serde_json::from_str(&manifest_raw).context("invalid manifest.json")?;

    let out_dir = base.join("indexes/objects");
    let idx_path = out_dir.join("object_index.parquet");
    let edges_path = out_dir.join("object_edges.parquet");
    let deriv_path = out_dir.join("derivations.parquet");

    if skip_existing && idx_path.exists() && edges_path.exists() && deriv_path.exists() {
        println!("Indexes already present and --skip-existing set; nothing to do.");
        return Ok(());
    }

    // ── Event-log rebuild path ────────────────────────────────────────────
    if from_events {
        let events_path = base.join("indexes/objects/object_events.jsonl");
        if !events_path.exists() {
            anyhow::bail!(
                "--from-events requires indexes/objects/object_events.jsonl which was not found"
            );
        }
        let objects =
            rebuild_object_index_from_events(base).context("event-log replay failed")?;
        fs::create_dir_all(&out_dir)
            .with_context(|| format!("failed to create {}", out_dir.display()))?;
        write_object_index(&idx_path, &objects)
            .context("failed to write object_index.parquet")?;
        println!("Container:        {}", base.display());
        println!("Mode:             event-log replay");
        println!("Objects indexed:  {}", objects.len());
        println!("Written:          {}", idx_path.display());
        let prov_path = base.join("provenance/chain_of_custody.jsonl");
        let mut prov = ProvenanceWriter::new(&prov_path).context("provenance writer failed")?;
        prov.record(
            "rebuilt_object_index_from_events",
            TOOL_NAME,
            TOOL_VERSION,
            "system",
            serde_json::json!({
                "container": base.display().to_string(),
                "objects_indexed": objects.len(),
                "output": idx_path.display().to_string(),
            }),
        )
        .context("provenance write failed")?;
        return Ok(());
    }

    let jobs_dir = base.join("analysis/jobs");

    // Collect sorted job directories
    let job_dirs: Vec<PathBuf> = if jobs_dir.exists() {
        let mut dirs = Vec::new();
        for entry in fs::read_dir(&jobs_dir).context("failed to read analysis/jobs/")? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                dirs.push(entry.path());
            }
        }
        dirs.sort();
        dirs
    } else {
        Vec::new()
    };

    // ── Merge: objects (first-writer-wins by object_id) ───────────────────
    let mut object_map: HashMap<String, DiscoveredObjectRow> = HashMap::new();
    let mut edge_map: HashMap<String, ObjectEdgeRow> = HashMap::new();
    let mut deriv_map: HashMap<String, DerivationRow> = HashMap::new();

    let mut delta_files_read = 0usize;

    for job_dir in &job_dirs {
        let job_id = job_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        // objects_delta.jsonl
        let obj_delta = job_dir.join("objects_delta.jsonl");
        if obj_delta.exists() {
            let content = fs::read_to_string(&obj_delta)
                .with_context(|| format!("read failed: {}", obj_delta.display()))?;
            for (line_no, line) in content.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let row: DiscoveredObjectRow = serde_json::from_str(line).with_context(|| {
                    format!(
                        "invalid object row at {}:{}: {line}",
                        obj_delta.display(),
                        line_no + 1
                    )
                })?;
                object_map.entry(row.object_id.clone()).or_insert(row);
            }
            delta_files_read += 1;
        }

        // object_edges_delta.jsonl
        let edges_delta = job_dir.join("object_edges_delta.jsonl");
        if edges_delta.exists() {
            let content = fs::read_to_string(&edges_delta)
                .with_context(|| format!("read failed: {}", edges_delta.display()))?;
            for (line_no, line) in content.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let row: ObjectEdgeRow = serde_json::from_str(line).with_context(|| {
                    format!(
                        "invalid edge row at {}:{}: {line}",
                        edges_delta.display(),
                        line_no + 1
                    )
                })?;
                edge_map.entry(row.edge_id.clone()).or_insert(row);
            }
            delta_files_read += 1;
        }

        // derivations_delta.jsonl
        let deriv_delta = job_dir.join("derivations_delta.jsonl");
        if deriv_delta.exists() {
            let content = fs::read_to_string(&deriv_delta)
                .with_context(|| format!("read failed: {}", deriv_delta.display()))?;
            for (line_no, line) in content.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let row: DerivationRow = serde_json::from_str(line).with_context(|| {
                    format!(
                        "invalid derivation row at {}:{}: {line}",
                        deriv_delta.display(),
                        line_no + 1
                    )
                })?;
                deriv_map.entry(row.derivation_id.clone()).or_insert(row);
            }
            delta_files_read += 1;
        }

        let _ = job_id; // suppress unused-variable warning when no deltas present
    }

    // ── Merge with existing indexes (idempotent incremental rebuild) ──────
    if let Some(bs) = batch_size {
        // Streaming mode: merge existing Parquet `bs` rows at a time to bound
        // peak memory at O(bs + delta_size) rather than O(total + delta_size).
        if idx_path.exists() {
            merge_object_index_streaming(&idx_path, &deriv_path, &mut object_map, bs)?;
        }
        if edges_path.exists() {
            merge_edge_index_streaming(&edges_path, &mut edge_map, bs)?;
        }
        if deriv_path.exists() {
            merge_derivation_index_streaming(&deriv_path, &mut deriv_map, bs)?;
        }
    } else {
        // Eager mode (original behaviour).
        if idx_path.exists() {
            let existing = read_object_index(&idx_path)
                .context("failed to read existing object_index.parquet")?;
            for row in existing {
                object_map.entry(row.object_id.clone()).or_insert(row);
            }
        }
        if edges_path.exists() {
            let existing = read_object_edges(&edges_path)
                .context("failed to read existing object_edges.parquet")?;
            for row in existing {
                edge_map.entry(row.edge_id.clone()).or_insert(row);
            }
        }
        if deriv_path.exists() {
            let existing = read_derivations(&deriv_path)
                .context("failed to read existing derivations.parquet")?;
            for row in existing {
                deriv_map.entry(row.derivation_id.clone()).or_insert(row);
            }
        }
    }

    // ── Sort deterministically and write ──────────────────────────────────
    let mut objects: Vec<DiscoveredObjectRow> = object_map.into_values().collect();
    objects.sort_by(|a, b| a.object_id.cmp(&b.object_id));

    let mut edges: Vec<ObjectEdgeRow> = edge_map.into_values().collect();
    edges.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));

    let mut derivations: Vec<DerivationRow> = deriv_map.into_values().collect();
    derivations.sort_by(|a, b| a.derivation_id.cmp(&b.derivation_id));

    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    write_object_index(&idx_path, &objects).context("failed to write object_index.parquet")?;
    write_object_edges(&edges_path, &edges).context("failed to write object_edges.parquet")?;
    write_derivations(&deriv_path, &derivations).context("failed to write derivations.parquet")?;

    println!("Container: {}", base.display());
    println!("Job directories scanned: {}", job_dirs.len());
    println!("Delta files read:        {delta_files_read}");
    println!("Objects indexed:         {}", objects.len());
    println!("Edges indexed:           {}", edges.len());
    println!("Derivations indexed:     {}", derivations.len());
    println!();
    println!("Written:");
    println!("  {}", idx_path.display());
    println!("  {}", edges_path.display());
    println!("  {}", deriv_path.display());

    let prov_path = base.join("provenance/chain_of_custody.jsonl");
    let mut prov = ProvenanceWriter::new(&prov_path).context("provenance writer failed")?;
    prov.record(
        "rebuilt_object_index",
        TOOL_NAME,
        TOOL_VERSION,
        "system",
        serde_json::json!({
            "container": base.display().to_string(),
            "job_dirs_scanned": job_dirs.len(),
            "delta_files_read": delta_files_read,
            "objects_indexed": objects.len(),
            "edges_indexed": edges.len(),
            "derivations_indexed": derivations.len(),
        }),
    )
    .context("provenance write failed")?;

    Ok(())
}

// ── Streaming merge helpers ───────────────────────────────────────────────────

/// Merge existing `object_index.parquet` into `map` using streaming batch
/// reads so that at most `batch_size` existing rows are in memory at once.
fn merge_object_index_streaming(
    path: &Path,
    _deriv_path: &Path,
    map: &mut HashMap<String, DiscoveredObjectRow>,
    batch_size: usize,
) -> Result<()> {
    for_each_object_batch(path, batch_size, |batch| {
        for row in batch {
            map.entry(row.object_id.clone()).or_insert_with(|| row.clone());
        }
        Ok(())
    })
    .with_context(|| format!("streaming read failed: {}", path.display()))
}

/// Merge existing `object_edges.parquet` into `map` using streaming batch reads.
fn merge_edge_index_streaming(
    path: &Path,
    map: &mut HashMap<String, ObjectEdgeRow>,
    batch_size: usize,
) -> Result<()> {
    for_each_edge_batch(path, batch_size, |batch| {
        for row in batch {
            map.entry(row.edge_id.clone()).or_insert_with(|| row.clone());
        }
        Ok(())
    })
    .with_context(|| format!("streaming read failed: {}", path.display()))
}

/// Merge existing `derivations.parquet` into `map` using streaming batch reads.
fn merge_derivation_index_streaming(
    path: &Path,
    map: &mut HashMap<String, DerivationRow>,
    batch_size: usize,
) -> Result<()> {
    for_each_derivation_batch(path, batch_size, |batch| {
        for row in batch {
            map.entry(row.derivation_id.clone()).or_insert_with(|| row.clone());
        }
        Ok(())
    })
    .with_context(|| format!("streaming read failed: {}", path.display()))
}

// ── offf-index export-lineage ─────────────────────────────────────────────────

fn cmd_export_lineage(base: &Path, format: &str, out: Option<&Path>) -> Result<()> {
    let manifest_raw = fs::read_to_string(base.join("manifest.json"))
        .context("manifest.json not found – is this an OFFF container?")?;
    let manifest: ManifestJson =
        serde_json::from_str(&manifest_raw).context("invalid manifest.json")?;

    let out_dir = base.join("indexes/objects");
    let idx_path = out_dir.join("object_index.parquet");
    let edges_path = out_dir.join("object_edges.parquet");
    let deriv_path = out_dir.join("derivations.parquet");

    let objects = if idx_path.exists() {
        read_object_index(&idx_path).context("failed to read object_index.parquet")?
    } else {
        Vec::new()
    };
    let edges = if edges_path.exists() {
        read_object_edges(&edges_path).context("failed to read object_edges.parquet")?
    } else {
        Vec::new()
    };
    let derivations = if deriv_path.exists() {
        read_derivations(&deriv_path).context("failed to read derivations.parquet")?
    } else {
        Vec::new()
    };

    let mut output: Box<dyn std::io::Write> = match out {
        Some(p) => {
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create output dir: {}", parent.display()))?;
            }
            Box::new(
                fs::File::create(p)
                    .with_context(|| format!("cannot create output file: {}", p.display()))?,
            )
        }
        None => Box::new(std::io::stdout()),
    };

    match format {
        "dot" => {
            export_dot(&objects, &edges, &mut output)
                .context("DOT export failed")?;
        }
        "csv" => {
            // Header
            writeln!(output, "object_id,object_type,name,parent_object_ids")?;
            // Build parent map: child → comma-separated parent IDs
            let mut parents: HashMap<&str, Vec<&str>> = HashMap::new();
            for edge in &edges {
                parents
                    .entry(edge.child_object_id.as_str())
                    .or_default()
                    .push(edge.parent_object_id.as_str());
            }
            for obj in &objects {
                let parent_list = parents
                    .get(obj.object_id.as_str())
                    .map(|v| v.join("|"))
                    .unwrap_or_default();
                writeln!(
                    output,
                    "{},{},{},{}",
                    csv_escape(&obj.object_id),
                    csv_escape(&obj.object_type),
                    csv_escape(obj.name.as_deref().unwrap_or("")),
                    csv_escape(&parent_list),
                )?;
            }
        }
        _ => {
            // Default: JSON
            let value = export_lineage_json(
                &manifest.container_id,
                &objects,
                &edges,
                &derivations,
            );
            serde_json::to_writer_pretty(&mut output, &value)
                .context("JSON serialisation failed")?;
            writeln!(output)?;
        }
    }

    if out.is_some() {
        eprintln!(
            "Lineage exported ({} objects, {} edges, {} derivations) → {}",
            objects.len(),
            edges.len(),
            derivations.len(),
            out.unwrap().display()
        );
    }
    Ok(())
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// Bring streaming helpers into scope (re-exported from offf_core via pub use,
// but we need the un-prefixed names used above).
use std::io::Write;
