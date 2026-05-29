use std::{
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::Parser;
use sha2::{Digest, Sha256};

use offf_core::{
    chunk::read_chunk,
    packed::{pack_directory, read_index, unpack_to_directory},
    parquet_io::read_physical_to_chunk,
    types::ManifestJson,
};

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "offf-export",
    about = "Export raw images and manage OFFF packed containers",
    version
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Reconstruct a raw/dd image from an exploded OFFF directory
    Export {
        /// Path to the OFFF container directory
        container: PathBuf,
        /// Output file path
        #[arg(long, short)]
        output: PathBuf,
    },

    /// Pack an exploded OFFF directory into one .offfpack file
    Pack {
        /// Input OFFF directory
        input: PathBuf,
        /// Output packed file
        #[arg(long, short)]
        output: PathBuf,
    },

    /// List entries inside a packed OFFF container
    List {
        /// Input packed file (.offfpack)
        input: PathBuf,
    },

    /// Unpack a .offfpack file into a directory
    Unpack {
        /// Input packed file (.offfpack)
        input: PathBuf,
        /// Output directory
        #[arg(long, short)]
        output: PathBuf,
    },
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args = Args::parse();
    match args.command {
        Command::Export { container, output } => export(&container, &output),
        Command::Pack { input, output } => {
            let index = pack_directory(&input, &output)
                .with_context(|| format!("failed to pack {}", input.display()))?;
            println!(
                "Packed {} entries into {}",
                index.entries.len(),
                output.display()
            );
            Ok(())
        }
        Command::List { input } => {
            let index = read_index(&input)
                .with_context(|| format!("failed to read packed index from {}", input.display()))?;
            println!("format: {}", index.format);
            println!("version: {}", index.version);
            println!("entries: {}", index.entries.len());
            println!();
            for e in index.entries {
                println!("{}\t{}\t{}", e.path, e.length, e.sha256);
            }
            Ok(())
        }
        Command::Unpack { input, output } => {
            let index = unpack_to_directory(&input, &output)
                .with_context(|| format!("failed to unpack {}", input.display()))?;
            println!(
                "Unpacked {} entries to {}",
                index.entries.len(),
                output.display()
            );
            Ok(())
        }
    }
}

fn export(base: &Path, output: &Path) -> Result<()> {
    // ── Load manifest ──────────────────────────────────────────────────────
    let manifest_raw =
        fs::read_to_string(base.join("manifest.json")).context("manifest.json not found")?;
    let manifest: ManifestJson =
        serde_json::from_str(&manifest_raw).context("invalid manifest.json")?;

    // ── Load chunk map ─────────────────────────────────────────────────────
    let map_path = base.join(
        manifest.indexes.physical_to_chunk.as_deref().unwrap_or("maps/physical_to_chunk.parquet")
    );
    let chunks =
        read_physical_to_chunk(&map_path).context("failed to read physical_to_chunk.parquet")?;

    if chunks.is_empty() {
        anyhow::bail!("no chunks found in mapping table");
    }

    println!("Container: {}", base.display());
    println!("Chunks:    {}", chunks.len());
    println!("Output:    {}", output.display());
    println!();
    println!("Reconstructing…");

    // ── Write output ───────────────────────────────────────────────────────
    let out_file = fs::File::create(output)
        .with_context(|| format!("cannot create output file: {}", output.display()))?;
    let mut writer = BufWriter::new(out_file);
    let mut source_hasher = Sha256::new();

    for (i, chunk) in chunks.iter().enumerate() {
        let plaintext = read_chunk(base, chunk)
            .with_context(|| format!("failed to read chunk {}", chunk.sequence))?;

        source_hasher.update(&plaintext);
        writer
            .write_all(&plaintext)
            .with_context(|| format!("write failed at chunk {}", chunk.sequence))?;

        if (i + 1) % 100 == 0 || i + 1 == chunks.len() {
            println!("  chunk {:>6} / {} written", i + 1, chunks.len());
        }
    }

    writer.flush().context("flush failed")?;

    // ── Verify output hash ─────────────────────────────────────────────────
    let computed = format!("{:x}", source_hasher.finalize());
    println!();
    let expected_sha256 = manifest
        .hashes
        .as_ref()
        .map(|h| h.source_sha256.as_str())
        .unwrap_or("");
    if computed == expected_sha256 {
        println!("Source SHA-256: {} ✓ MATCH", &computed[..32]);
        println!();
        println!("Export complete: {}", output.display());
    } else if expected_sha256.is_empty() {
        println!("Source SHA-256: {} (no manifest hash to verify against)", &computed[..32]);
        println!();
        println!("Export complete: {}", output.display());
    } else {
        // Remove the bad output file before bailing
        let _ = fs::remove_file(output);
        anyhow::bail!(
            "source hash MISMATCH\n  expected: {}\n  computed: {}",
            expected_sha256,
            computed
        );
    }

    Ok(())
}
