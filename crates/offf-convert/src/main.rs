use std::{
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use clap::Parser;
use uuid::Uuid;

use sha2::{Digest, Sha256};

use offf_core::{
    chunk::write_chunk,
    hash::serialize_merkle_tree,
    parquet_io::{write_leaves, write_physical_to_chunk},
    provenance::ProvenanceWriter,
    types::{
        AcquisitionJson, AcquisitionParameters, AcquisitionSource, ChunkingInfo, Compression,
        EvidenceStreamInfo, ManifestHashes, ManifestIndexes, ManifestJson, SourceContainerInfo,
        SourceInfo, ToolInfo, OFFF_VERSION, TOOL_VERSION,
    },
};

const TOOL_NAME: &str = "offf-convert";

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "offf-convert",
    about = "Convert a raw/dd image to an OFFF directory container",
    version
)]
struct Args {
    /// Input image file (raw/dd or E01)
    #[arg(long, short)]
    input: PathBuf,

    /// Output OFFF container directory (will be created)
    #[arg(long, short)]
    output: PathBuf,

    /// Chunk size, e.g. 64M (supports K, M, G suffixes)
    #[arg(long, default_value = "64M")]
    chunk_size: String,

    /// Compression algorithm [zstd, none]
    #[arg(long, default_value = "zstd")]
    compression: String,

    /// Hash algorithm (only sha256 supported)
    #[arg(long, default_value = "sha256")]
    hash: String,

    /// Input type: auto | raw | e01
    #[arg(long, default_value = "auto")]
    input_type: String,

    /// Tool used to export E01 to raw stream (Phase 7), e.g. ewfexport
    #[arg(long, default_value = "ewfexport")]
    ewf_export_tool: String,

    /// Keep intermediate raw stream exported from E01
    #[arg(long)]
    keep_intermediate: bool,

    /// Deterministic mode: fixed container ID and no real timestamp in IDs
    #[arg(long)]
    deterministic: bool,

    /// Source sector size in bytes (e.g. 512, 4096)
    #[arg(long, default_value_t = 512, value_parser = parse_sector_size)]
    sector_size: u32,
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args = Args::parse();
    convert(args)
}

fn convert(args: Args) -> Result<()> {
    // ── Validate inputs ────────────────────────────────────────────────────
    let input_path = args
        .input
        .canonicalize()
        .with_context(|| format!("input file not found: {}", args.input.display()))?;

    let final_output = args.output;
    if final_output.exists() {
        anyhow::bail!("output path already exists: {}", final_output.display());
    }

    let tmp_output = build_temp_output_path(&final_output);
    if tmp_output.exists() {
        anyhow::bail!(
            "temporary output path already exists: {}",
            tmp_output.display()
        );
    }

    if let Some(parent) = final_output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directory: {}", parent.display()))?;
    }

    fs::create_dir_all(&tmp_output).with_context(|| {
        format!(
            "failed to create temporary output directory: {}",
            tmp_output.display()
        )
    })?;

    let chunk_size = parse_size(&args.chunk_size)
        .with_context(|| format!("invalid chunk size: {}", args.chunk_size))?;

    let compression: Compression = args
        .compression
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    if args.hash != "sha256" {
        anyhow::bail!("only 'sha256' is supported in this version");
    }

    let input_kind = detect_input_kind(&input_path, &args.input_type)?;

    let e01_container_sha256 = if input_kind == InputKind::E01 {
        Some(hash_file_sha256(&input_path).context("failed to hash E01 container")?)
    } else {
        None
    };

    let mut _tmp_dir: Option<tempfile::TempDir> = None;
    let stream_path: PathBuf = if input_kind == InputKind::E01 {
        let td = tempfile::tempdir().context("failed to create temp directory for E01 export")?;
        let raw = export_e01_to_raw(&input_path, td.path(), &args.ewf_export_tool)?;
        if args.keep_intermediate {
            println!("Intermediate raw stream: {}", raw.display());
        }
        _tmp_dir = Some(td);
        raw
    } else {
        input_path.clone()
    };

    let conversion_result = (|| -> Result<String> {
        // ── Create container directory structure ───────────────────────────
        let base = &tmp_output;
        for dir in &[
            "chunks/sha256",
            "hashes",
            "maps",
            "indexes",
            "analysis",
            "provenance",
            "signatures",
        ] {
            fs::create_dir_all(base.join(dir))
                .with_context(|| format!("failed to create directory {dir}"))?;
        }

        // ── Phase 1: compute source SHA-256 (single pass with chunk writing) ───
        let source_size = stream_path.metadata()?.len();

        println!("Source: {}", input_path.display());
        if input_kind == InputKind::E01 {
            println!("Input type: E01 (exported to raw stream)");
            println!("Stream: {}", stream_path.display());
        } else {
            println!("Input type: raw");
        }
        println!("Size:   {} bytes", source_size);
        println!("Chunks: {} bytes each", chunk_size);
        println!("Compression: {}", compression.as_str());
        println!();
        println!("Processing…");

        let file = fs::File::open(&stream_path)
            .with_context(|| format!("cannot open {}", stream_path.display()))?;
        let mut reader = BufReader::new(file);

        let mut source_hasher = Sha256::new();
        let mut chunks = Vec::new();
        let mut sequence: u64 = 0;
        let mut source_offset: u64 = 0;
        let mut buf = vec![0u8; chunk_size as usize];

        loop {
            let n = read_exact_or_partial(&mut reader, &mut buf)?;
            if n == 0 {
                break;
            }
            let plaintext = &buf[..n];

            // Update source hash
            source_hasher.update(plaintext);

            // Write chunk
            let meta = write_chunk(base, sequence, source_offset, plaintext, &compression)
                .with_context(|| format!("failed to write chunk {sequence}"))?;

            source_offset += n as u64;
            sequence += 1;

            if sequence.is_multiple_of(100) || n < chunk_size as usize {
                println!(
                    "  chunk {:>6} / offset {:>15} bytes written",
                    sequence, source_offset
                );
            }

            chunks.push(meta);
        }

        let source_sha256 = format!("{:x}", source_hasher.finalize());
        println!("\nSource SHA-256: {source_sha256}");
        println!("Total chunks:  {}", chunks.len());

        // ── Phase 2: Merkle tree ───────────────────────────────────────────
        let leaf_hashes: Vec<String> = chunks.iter().map(|c| c.plaintext_sha256.clone()).collect();
        let merkle_root =
            offf_core::hash::merkle_root(&leaf_hashes).context("failed to compute Merkle root")?;
        let merkle_bytes =
            serialize_merkle_tree(&leaf_hashes).context("failed to serialise Merkle tree")?;

        fs::write(base.join("hashes/merkle_tree.bin"), &merkle_bytes)
            .context("failed to write merkle_tree.bin")?;

        println!("Merkle root:   {merkle_root}");

        // ── Phase 3: Parquet tables ────────────────────────────────────────
        write_physical_to_chunk(&base.join("maps/physical_to_chunk.parquet"), &chunks)
            .context("failed to write physical_to_chunk.parquet")?;

        write_leaves(&base.join("hashes/leaves.parquet"), &chunks)
            .context("failed to write leaves.parquet")?;

        // ── Phase 4: Container ID ──────────────────────────────────────────
        let container_id = if args.deterministic {
            // Deterministic: derive from source hash
            format!("urn:offf:case:{}", &source_sha256[..32])
        } else {
            format!("urn:offf:case:{}", Uuid::new_v4())
        };

        let now = if args.deterministic {
            deterministic_timestamp()
        } else {
            chrono::Utc::now()
        };

        // ── Phase 5: acquisition.json ──────────────────────────────────────
        let acquisition = AcquisitionJson {
            container_id: container_id.clone(),
            acquired_at: now,
            tool: ToolInfo {
                name: TOOL_NAME.to_string(),
                version: TOOL_VERSION.to_string(),
            },
            source: AcquisitionSource {
                path: input_path.display().to_string(),
                size_bytes: source_size,
                sha256: source_sha256.clone(),
            },
            source_container: e01_container_sha256.as_ref().map(|h| SourceContainerInfo {
                container_type: "E01".to_string(),
                container_sha256: h.clone(),
                tool_used: args.ewf_export_tool.clone(),
                conversion_time: now,
            }),
            evidence_stream: if input_kind == InputKind::E01 {
                Some(EvidenceStreamInfo {
                    stream_sha256: source_sha256.clone(),
                })
            } else {
                None
            },
            parameters: AcquisitionParameters {
                chunk_size,
                sector_size: args.sector_size,
                compression: compression.as_str().to_string(),
                hash_algorithm: "sha256".to_string(),
                deterministic: args.deterministic,
            },
        };

        let acq_json = serde_json::to_string_pretty(&acquisition)
            .context("failed to serialise acquisition")?;
        fs::write(base.join("acquisition.json"), acq_json)
            .context("failed to write acquisition.json")?;

        // ── Phase 6: Provenance ────────────────────────────────────────────
        let prov_path = base.join("provenance/chain_of_custody.jsonl");
        let mut prov =
            ProvenanceWriter::new(&prov_path).context("failed to open provenance writer")?;
        let output_ref = if args.deterministic {
            container_id.clone()
        } else {
            final_output.display().to_string()
        };

        prov.record_at(
            match input_kind {
                InputKind::Raw => "converted_raw_to_offf",
                InputKind::E01 => "converted_e01_to_offf",
            },
            TOOL_NAME,
            TOOL_VERSION,
            "system",
            serde_json::json!({
                "input": {
                    "path": input_path.display().to_string(),
                    "type": match input_kind {
                        InputKind::Raw => "raw",
                        InputKind::E01 => "e01",
                    },
                    "size_bytes": source_size,
                    "sha256": source_sha256,
                    "source_container_sha256": e01_container_sha256,
                },
                "output": {
                    "container": output_ref,
                    "merkle_root_sha256": merkle_root,
                },
                "parameters": {
                    "chunk_size": chunk_size,
                    "sector_size": args.sector_size,
                    "compression": compression.as_str(),
                    "hash_algorithm": "sha256",
                    "deterministic": args.deterministic,
                }
            }),
            now.to_rfc3339(),
        )
        .context("failed to write provenance event")?;

        // ── Phase 7: manifest.json (finalization point) ───────────────────
        let manifest = ManifestJson {
            offf_version: OFFF_VERSION.to_string(),
            container_id: container_id.clone(),
            created_at: now,
            created_by_tool: ToolInfo {
                name: TOOL_NAME.to_string(),
                version: TOOL_VERSION.to_string(),
            },
            source: SourceInfo {
                source_type: match input_kind {
                    InputKind::Raw => "raw_image".to_string(),
                    InputKind::E01 => "e01_image".to_string(),
                },
                size_bytes: source_size,
                sector_size: args.sector_size,
            },
            hashes: ManifestHashes {
                source_sha256: source_sha256.clone(),
                merkle_root_sha256: merkle_root.clone(),
            },
            chunking: ChunkingInfo {
                chunk_size,
                chunking_mode: "fixed".to_string(),
                compression: compression.as_str().to_string(),
                hash_algorithm: "sha256".to_string(),
            },
            indexes: ManifestIndexes {
                physical_to_chunk: "maps/physical_to_chunk.parquet".to_string(),
            },
            extensions: None,
        };

        let manifest_json =
            serde_json::to_string_pretty(&manifest).context("failed to serialise manifest")?;
        fs::write(base.join("manifest.json"), manifest_json)
            .context("failed to write manifest.json")?;

        // ── Phase 8: Self-check before publish ─────────────────────────────
        self_check_container(base).context("internal container self-check failed")?;

        Ok(container_id)
    })();

    let container_id = match conversion_result {
        Ok(id) => id,
        Err(e) => {
            let cleanup_result = fs::remove_dir_all(&tmp_output);
            if let Err(cleanup_err) = cleanup_result {
                return Err(e).context(format!(
                    "conversion failed and temporary output cleanup failed: {} ({cleanup_err})",
                    tmp_output.display()
                ));
            }
            return Err(e).context(format!(
                "conversion failed; temporary output cleaned: {}",
                tmp_output.display()
            ));
        }
    };

    // Publish temporary container atomically on local filesystem.
    fs::rename(&tmp_output, &final_output).with_context(|| {
        format!(
            "failed to publish container atomically: {} -> {}",
            tmp_output.display(),
            final_output.display()
        )
    })?;

    println!();
    println!("Container written to: {}", final_output.display());
    println!("Container ID:         {container_id}");
    println!("Done.");

    Ok(())
}

fn build_temp_output_path(final_output: &Path) -> PathBuf {
    let parent = final_output.parent().unwrap_or_else(|| Path::new("."));
    let name = final_output
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("offf-container");
    parent.join(format!("{name}.tmp-{}", Uuid::new_v4()))
}

fn self_check_container(base: &Path) -> Result<()> {
    let required_files = [
        "manifest.json",
        "acquisition.json",
        "maps/physical_to_chunk.parquet",
        "hashes/leaves.parquet",
        "hashes/merkle_tree.bin",
        "provenance/chain_of_custody.jsonl",
    ];

    for rel in required_files {
        let p = base.join(rel);
        if !p.exists() {
            anyhow::bail!("required file missing after conversion: {}", p.display());
        }
        let meta = fs::metadata(&p)?;
        if meta.len() == 0 {
            anyhow::bail!("required file is empty after conversion: {}", p.display());
        }
    }

    let manifest_raw = fs::read_to_string(base.join("manifest.json"))?;
    let _manifest: ManifestJson =
        serde_json::from_str(&manifest_raw).context("failed to parse manifest in self-check")?;

    let acq_raw = fs::read_to_string(base.join("acquisition.json"))?;
    let _acq: AcquisitionJson =
        serde_json::from_str(&acq_raw).context("failed to parse acquisition in self-check")?;

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputKind {
    Raw,
    E01,
}

fn detect_input_kind(input_path: &std::path::Path, arg: &str) -> Result<InputKind> {
    match arg {
        "raw" => Ok(InputKind::Raw),
        "e01" => Ok(InputKind::E01),
        "auto" => {
            let ext = input_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if ext == "e01" {
                Ok(InputKind::E01)
            } else {
                Ok(InputKind::Raw)
            }
        }
        other => anyhow::bail!("invalid --input-type '{other}' (use auto|raw|e01)"),
    }
}

fn hash_file_sha256(path: &std::path::Path) -> Result<String> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 8 * 1024 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn export_e01_to_raw(
    input: &std::path::Path,
    tmp_dir: &std::path::Path,
    tool: &str,
) -> Result<PathBuf> {
    #[cfg(windows)]
    {
        if tool.eq_ignore_ascii_case("ewfexport") {
            return export_e01_to_raw_via_docker(input, tmp_dir);
        }
    }

    let target_prefix = tmp_dir.join("evidence_stream");
    let target_prefix = normalize_external_tool_path(&target_prefix);
    let input = normalize_external_tool_path(input);
    let run_export = |program: &str| {
        Command::new(program)
            .arg("-f")
            .arg("raw")
            .arg("-u")
            .arg("-t")
            .arg(&target_prefix)
            .arg(&input)
            .status()
    };

    let status = match run_export(tool) {
        Ok(s) => s,
        Err(e) => {
            #[cfg(windows)]
            {
                if e.kind() == std::io::ErrorKind::NotFound {
                    let fallback = format!("{tool}.cmd");
                    run_export(&fallback).with_context(|| {
                        format!(
                            "failed to execute '{}' for E01 export (install libewf tools and ensure '{}' is in PATH)",
                            tool, tool
                        )
                    })?
                } else {
                    return Err(e).with_context(|| {
                        format!(
                            "failed to execute '{}' for E01 export (install libewf tools and ensure '{}' is in PATH)",
                            tool, tool
                        )
                    });
                }
            }
            #[cfg(not(windows))]
            {
                return Err(e).with_context(|| {
                    format!(
                        "failed to execute '{}' for E01 export (install libewf tools and ensure '{}' is in PATH)",
                        tool, tool
                    )
                });
            }
        }
    };

    if !status.success() {
        anyhow::bail!("{} exited with status {} while exporting E01", tool, status);
    }

    let candidates = find_raw_candidates(tmp_dir)?;
    let raw = candidates
        .into_iter()
        .next()
        .context("E01 export succeeded but no raw output file was found")?;
    Ok(raw)
}

#[cfg(windows)]
fn export_e01_to_raw_via_docker(
    input: &std::path::Path,
    tmp_dir: &std::path::Path,
) -> Result<PathBuf> {
    let input = normalize_external_tool_path(input);
    let tmp_dir = normalize_external_tool_path(tmp_dir);
    let input_dir = input
        .parent()
        .context("E01 input has no parent directory")?;
    let input_name = input
        .file_name()
        .and_then(|s| s.to_str())
        .context("invalid E01 input filename")?;

    let status = Command::new("docker")
        .arg("run")
        .arg("--rm")
        .arg("-i")
        .arg("-v")
        .arg(format!("{}:/input", input_dir.display()))
        .arg("-v")
        .arg(format!("{}:/out", tmp_dir.display()))
        .arg("--entrypoint")
        .arg("ewfexport")
        .arg("offf/ewf-tools:latest")
        .arg("-f")
        .arg("raw")
        .arg("-u")
        .arg("-t")
        .arg("/out/evidence_stream")
        .arg(format!("/input/{input_name}"))
        .status()
        .context(
            "failed to execute dockerized ewfexport (ensure Docker is installed and running)",
        )?;

    if !status.success() {
        anyhow::bail!(
            "dockerized ewfexport exited with status {} while exporting E01",
            status
        );
    }

    let candidates = find_raw_candidates(&tmp_dir)?;
    let raw = candidates
        .into_iter()
        .next()
        .context("E01 export succeeded but no raw output file was found")?;
    Ok(raw)
}

fn find_raw_candidates(dir: &std::path::Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for ent in fs::read_dir(dir)? {
        let ent = ent?;
        let p = ent.path();
        if p.is_file() {
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if matches!(ext.as_str(), "raw" | "dd" | "img") {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn normalize_external_tool_path(path: &std::path::Path) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(s) = path.to_str() {
            if let Some(stripped) = s.strip_prefix(r"\\?\") {
                return PathBuf::from(stripped);
            }
        }
    }
    path.to_path_buf()
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Read exactly `buf.len()` bytes or fewer if EOF is reached.
fn read_exact_or_partial(reader: &mut impl Read, buf: &mut [u8]) -> Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        let n = reader.read(&mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    Ok(total)
}

/// Parse a human-readable size string like "64M", "512K", "1G".
fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num_str, mult) = match s.chars().last() {
        Some('K') | Some('k') => (&s[..s.len() - 1], 1_024u64),
        Some('M') | Some('m') => (&s[..s.len() - 1], 1_024 * 1_024),
        Some('G') | Some('g') => (&s[..s.len() - 1], 1_024 * 1_024 * 1_024),
        _ => (s, 1),
    };
    let n: u64 = num_str.parse().context("not a valid number")?;
    Ok(n * mult)
}

fn parse_sector_size(s: &str) -> Result<u32, String> {
    let n: u32 = s
        .trim()
        .parse()
        .map_err(|_| format!("invalid sector size: {s}"))?;
    if n == 0 {
        return Err("sector size must be > 0".to_string());
    }
    Ok(n)
}

fn deterministic_timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
        .expect("fixed RFC3339 timestamp must parse")
        .with_timezone(&chrono::Utc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_variants() {
        assert_eq!(parse_size("64M").unwrap(), 64 * 1024 * 1024);
        assert_eq!(parse_size("512K").unwrap(), 512 * 1024);
        assert_eq!(parse_size("1G").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("4096").unwrap(), 4096);
    }

    #[test]
    fn detect_input_kind_auto() {
        assert_eq!(
            detect_input_kind(std::path::Path::new("a.E01"), "auto").unwrap(),
            InputKind::E01
        );
        assert_eq!(
            detect_input_kind(std::path::Path::new("a.dd"), "auto").unwrap(),
            InputKind::Raw
        );
    }

    #[test]
    fn parse_sector_size_variants() {
        assert_eq!(parse_sector_size("512").unwrap(), 512);
        assert_eq!(parse_sector_size("4096").unwrap(), 4096);
        assert!(parse_sector_size("0").is_err());
    }

    #[test]
    fn deterministic_timestamp_is_stable() {
        let a = deterministic_timestamp();
        let b = deterministic_timestamp();
        assert_eq!(a, b);
        assert_eq!(a.to_rfc3339(), "1970-01-01T00:00:00+00:00");
    }
}
