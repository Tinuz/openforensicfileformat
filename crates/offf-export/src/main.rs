use std::{
    fs,
    io::{BufWriter, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use clap::Parser;

use offf_core::{
    chunk::read_chunk,
    parquet_io::read_physical_to_chunk,
    types::ManifestJson,
};

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "offf-export",
    about = "Reconstruct a raw/dd image from an OFFF container",
    version
)]
struct Args {
    /// Path to the OFFF container directory
    container: PathBuf,

    /// Output file path
    #[arg(long, short)]
    output: PathBuf,
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args = Args::parse();
    export(&args.container, &args.output)
}

fn export(base: &PathBuf, output: &PathBuf) -> Result<()> {
    // ── Load manifest ──────────────────────────────────────────────────────
    let manifest_raw = fs::read_to_string(base.join("manifest.json"))
        .context("manifest.json not found")?;
    let manifest: ManifestJson =
        serde_json::from_str(&manifest_raw).context("invalid manifest.json")?;

    // ── Load chunk map ─────────────────────────────────────────────────────
    let map_path = base.join(&manifest.indexes.physical_to_chunk);
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
    if computed == manifest.hashes.source_sha256 {
        println!("Source SHA-256: {} ✓ MATCH", &computed[..32]);
        println!();
        println!("Export complete: {}", output.display());
    } else {
        // Remove the bad output file before bailing
        let _ = fs::remove_file(output);
        anyhow::bail!(
            "source hash MISMATCH\n  expected: {}\n  computed: {}",
            manifest.hashes.source_sha256,
            computed
        );
    }

    Ok(())
}
