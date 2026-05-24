use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use offf_core::{
    ntfs::index_ntfs,
    parquet_io::{read_physical_to_chunk, write_file_index},
    partition::{detect_and_parse, detect_volume_type},
    provenance::ProvenanceWriter,
    types::ManifestJson,
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
