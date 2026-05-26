#![allow(dead_code, unused_imports)]
#![allow(clippy::explicit_counter_loop, clippy::identity_op)]

/// Integration tests for the OFFF Evidence Container MVP.
///
/// Each test generates a synthetic raw image, converts it to OFFF, verifies
/// the container, exports back to raw and checks byte-exact hash equality.
///
/// The tests exercise the public API of `offf-core` directly so they compile
/// as part of the workspace without requiring the CLI binaries to be built.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use tempfile::TempDir;

use offf_core::{
    chunk::{hex_sha256, read_chunk, verify_chunk, write_chunk},
    hash::{deserialize_merkle_root, merkle_root, serialize_merkle_tree},
    lineage::ObjectLineageValidator,
    parquet_io::{read_leaves, read_physical_to_chunk, write_leaves, write_physical_to_chunk},
    provenance::ProvenanceWriter,
    types::{
        AcquisitionJson, AcquisitionParameters, AcquisitionSource, ChunkMetadata, ChunkingInfo,
        Compression, DerivationRow, DiscoveredObjectRow, ManifestExtensions, ManifestHashes,
        ManifestIndexes, ManifestJson, ObjectEdgeRow, SourceInfo, ToolInfo, OFFF_V2_VERSION,
        OFFF_VERSION, TOOL_VERSION,
    },
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Generate a deterministic synthetic binary file of `size` bytes.
fn make_image(dir: &Path, name: &str, size: usize) -> PathBuf {
    let path = dir.join(name);
    let mut f = fs::File::create(&path).unwrap();
    // Fill with a repeating pattern (not all-zeros, to test compression)
    let pattern: Vec<u8> = (0u8..=255).cycle().take(size).collect();
    f.write_all(&pattern).unwrap();
    path
}

/// SHA-256 of a file, returned as lowercase hex.
fn file_sha256(path: &Path) -> String {
    let data = fs::read(path).unwrap();
    hex_sha256(&data)
}

/// Convert a raw image to an OFFF container and return the list of chunk metas.
fn convert_image(
    image: &Path,
    container: &Path,
    chunk_size: u64,
    compression: Compression,
) -> Vec<ChunkMetadata> {
    // Create directory structure
    for dir in &["chunks/sha256", "hashes", "maps", "indexes", "provenance"] {
        fs::create_dir_all(container.join(dir)).unwrap();
    }

    let data = fs::read(image).unwrap();
    let source_size = data.len() as u64;

    // Chunk the data
    let mut chunks = Vec::new();
    let mut sequence = 0u64;
    let mut offset = 0u64;
    let mut source_hasher = Sha256::new();

    for chunk_data in data.chunks(chunk_size as usize) {
        source_hasher.update(chunk_data);
        let meta = write_chunk(container, sequence, offset, chunk_data, &compression).unwrap();
        offset += chunk_data.len() as u64;
        sequence += 1;
        chunks.push(meta);
    }

    let source_sha256 = format!("{:x}", source_hasher.finalize());

    // Write parquet tables
    write_physical_to_chunk(&container.join("maps/physical_to_chunk.parquet"), &chunks).unwrap();
    write_leaves(&container.join("hashes/leaves.parquet"), &chunks).unwrap();

    // Merkle tree
    let leaves: Vec<String> = chunks.iter().map(|c| c.plaintext_sha256.clone()).collect();
    let root = merkle_root(&leaves).unwrap();
    let tree_bytes = serialize_merkle_tree(&leaves).unwrap();
    fs::write(container.join("hashes/merkle_tree.bin"), tree_bytes).unwrap();

    // Manifest
    let manifest = ManifestJson {
        offf_version: OFFF_VERSION.to_string(),
        container_id: format!("urn:offf:case:{}", &source_sha256[..32]),
        created_at: chrono::Utc::now(),
        created_by_tool: ToolInfo {
            name: "offf-integration-test".to_string(),
            version: TOOL_VERSION.to_string(),
        },
        source: SourceInfo {
            source_type: "raw_image".to_string(),
            size_bytes: source_size,
            sector_size: 512,
        },
        hashes: ManifestHashes {
            source_sha256: source_sha256.clone(),
            merkle_root_sha256: root,
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
    fs::write(
        container.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    // Acquisition
    let acquisition = AcquisitionJson {
        container_id: manifest.container_id.clone(),
        acquired_at: manifest.created_at,
        tool: manifest.created_by_tool.clone(),
        source: AcquisitionSource {
            path: image.display().to_string(),
            size_bytes: source_size,
            sha256: source_sha256,
        },
        source_container: None,
        evidence_stream: None,
        parameters: AcquisitionParameters {
            chunk_size,
            sector_size: 512,
            compression: compression.as_str().to_string(),
            hash_algorithm: "sha256".to_string(),
            deterministic: true,
        },
    };
    fs::write(
        container.join("acquisition.json"),
        serde_json::to_string_pretty(&acquisition).unwrap(),
    )
    .unwrap();

    // Provenance
    let mut prov =
        ProvenanceWriter::new(&container.join("provenance/chain_of_custody.jsonl")).unwrap();
    prov.record(
        "converted_raw_to_offf",
        "offf-integration-test",
        TOOL_VERSION,
        "system",
        serde_json::json!({"test": true}),
    )
    .unwrap();

    chunks
}

/// Reconstruct a raw image from an OFFF container and return the output path.
fn export_image(container: &Path, out: &Path) -> String {
    let manifest: ManifestJson =
        serde_json::from_str(&fs::read_to_string(container.join("manifest.json")).unwrap())
            .unwrap();

    let chunks =
        read_physical_to_chunk(&container.join(&manifest.indexes.physical_to_chunk)).unwrap();

    let mut out_file = fs::File::create(out).unwrap();
    let mut source_hasher = Sha256::new();

    for chunk in &chunks {
        let plaintext = read_chunk(container, chunk).unwrap();
        source_hasher.update(&plaintext);
        out_file.write_all(&plaintext).unwrap();
    }

    format!("{:x}", source_hasher.finalize())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn small_image_round_trip() {
    let tmp = TempDir::new().unwrap();
    let image = make_image(tmp.path(), "small.dd", 1 * 1024 * 1024); // 1 MiB
    let container = tmp.path().join("small.offf");
    let reconstructed = tmp.path().join("reconstructed.dd");

    let original_hash = file_sha256(&image);

    convert_image(&image, &container, 256 * 1024, Compression::Zstd); // 256 KiB chunks
    let exported_hash = export_image(&container, &reconstructed);

    assert_eq!(
        original_hash, exported_hash,
        "round-trip hash mismatch for small image"
    );
    assert_eq!(
        fs::metadata(&image).unwrap().len(),
        fs::metadata(&reconstructed).unwrap().len(),
        "file sizes differ"
    );
}

#[test]
fn non_aligned_image_round_trip() {
    // Size not divisible by chunk size
    let chunk_size = 256 * 1024u64;
    let size = (chunk_size * 3 + 73_000) as usize; // 3 full chunks + partial

    let tmp = TempDir::new().unwrap();
    let image = make_image(tmp.path(), "unaligned.dd", size);
    let container = tmp.path().join("unaligned.offf");
    let reconstructed = tmp.path().join("reconstructed.dd");

    let original_hash = file_sha256(&image);
    let chunks = convert_image(&image, &container, chunk_size, Compression::Zstd);

    // Should have 4 chunks (3 full + 1 partial)
    assert_eq!(chunks.len(), 4, "expected 4 chunks for non-aligned image");
    assert!(
        chunks[3].source_length < chunk_size,
        "last chunk should be smaller than chunk_size"
    );

    let exported_hash = export_image(&container, &reconstructed);
    assert_eq!(
        original_hash, exported_hash,
        "non-aligned round-trip failed"
    );
}

#[test]
fn compression_none_round_trip() {
    let tmp = TempDir::new().unwrap();
    let image = make_image(tmp.path(), "none.dd", 512 * 1024); // 512 KiB
    let container = tmp.path().join("none.offf");
    let reconstructed = tmp.path().join("reconstructed.dd");

    let original_hash = file_sha256(&image);
    convert_image(&image, &container, 128 * 1024, Compression::None);
    let exported_hash = export_image(&container, &reconstructed);

    assert_eq!(
        original_hash, exported_hash,
        "no-compression round-trip failed"
    );
}

#[test]
fn verify_detects_chunk_corruption() {
    let tmp = TempDir::new().unwrap();
    let image = make_image(tmp.path(), "corrupt.dd", 512 * 1024);
    let container = tmp.path().join("corrupt.offf");

    let chunks = convert_image(&image, &container, 128 * 1024, Compression::None);

    // Corrupt a byte in the first chunk
    let bad_path = offf_core::chunk::chunk_path(&container, &chunks[0].plaintext_sha256);
    let mut data = fs::read(&bad_path).unwrap();
    data[0] ^= 0xFF;
    fs::write(&bad_path, data).unwrap();

    // Verification should detect it
    let result = verify_chunk(&container, &chunks[0]);
    assert!(result.is_err(), "corrupted chunk should fail verification");
}

#[test]
fn merkle_root_matches_manifest() {
    let tmp = TempDir::new().unwrap();
    let image = make_image(tmp.path(), "merkle.dd", 1 * 1024 * 1024);
    let container = tmp.path().join("merkle.offf");

    let chunks = convert_image(&image, &container, 256 * 1024, Compression::Zstd);

    let manifest: ManifestJson =
        serde_json::from_str(&fs::read_to_string(container.join("manifest.json")).unwrap())
            .unwrap();

    // Recompute Merkle root from chunk metadata
    let leaves: Vec<String> = chunks.iter().map(|c| c.plaintext_sha256.clone()).collect();
    let computed_root = merkle_root(&leaves).unwrap();
    assert_eq!(computed_root, manifest.hashes.merkle_root_sha256);

    // Also verify the binary file
    let blob = fs::read(container.join("hashes/merkle_tree.bin")).unwrap();
    let bin_root = deserialize_merkle_root(&blob).unwrap();
    assert_eq!(bin_root, manifest.hashes.merkle_root_sha256);
}

#[test]
fn parquet_tables_survive_round_trip() {
    let tmp = TempDir::new().unwrap();
    let image = make_image(tmp.path(), "parquet.dd", 768 * 1024);
    let container = tmp.path().join("parquet.offf");

    let chunks = convert_image(&image, &container, 256 * 1024, Compression::Zstd);

    let loaded = read_physical_to_chunk(&container.join("maps/physical_to_chunk.parquet")).unwrap();
    assert_eq!(loaded.len(), chunks.len());

    for (orig, loaded) in chunks.iter().zip(loaded.iter()) {
        assert_eq!(orig.sequence, loaded.sequence);
        assert_eq!(orig.source_offset, loaded.source_offset);
        assert_eq!(orig.plaintext_sha256, loaded.plaintext_sha256);
        assert_eq!(orig.stored_sha256, loaded.stored_sha256);
    }

    let leaves = read_leaves(&container.join("hashes/leaves.parquet")).unwrap();
    assert_eq!(leaves.len(), chunks.len());
    for (i, (seq, hash)) in leaves.iter().enumerate() {
        assert_eq!(*seq, i as u64);
        assert_eq!(hash, &chunks[i].plaintext_sha256);
    }
}

#[test]
fn provenance_is_written() {
    let tmp = TempDir::new().unwrap();
    let image = make_image(tmp.path(), "prov.dd", 256 * 1024);
    let container = tmp.path().join("prov.offf");

    convert_image(&image, &container, 128 * 1024, Compression::Zstd);

    let prov_path = container.join("provenance/chain_of_custody.jsonl");
    assert!(prov_path.exists(), "provenance file should exist");

    let content = fs::read_to_string(&prov_path).unwrap();
    let events: Vec<serde_json::Value> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    assert!(!events.is_empty(), "at least one provenance event expected");
    assert_eq!(events[0]["action"], "converted_raw_to_offf");
}

// ╔══════════════════════════════════════════════════════════════════════════╗
// ║  FASE 2 – Partition & Volume Mapping                                     ║
// ╚══════════════════════════════════════════════════════════════════════════╝

use offf_core::partition::{chunk_refs_for_range, detect_and_parse, read_bytes_at};
use offf_core::types::PartitionTableJson;

// ── Helpers: synthetic disk images with known partition layouts ───────────────

/// Build a minimal 10 MiB MBR disk image with two partitions in memory.
///
/// Layout (512-byte sectors):
///   LBA 0         : MBR (protective signature 0x55AA, 2 partition entries)
///   LBA 2048–4095 : "partition 1" (1 MiB, type 0x07 NTFS)
///   LBA 4096–6143 : "partition 2" (1 MiB, type 0x83 Linux)
///   Total         : 20_480 sectors × 512 = 10 MiB
fn make_mbr_image(dir: &Path, name: &str) -> PathBuf {
    const SECTORS: u64 = 20_480;
    const SECTOR: usize = 512;
    let mut disk = vec![0u8; SECTORS as usize * SECTOR];

    // MBR signature
    disk[510] = 0x55;
    disk[511] = 0xAA;

    // Partition entry helper: write one 16-byte entry at `offset`
    fn write_entry(buf: &mut [u8], offset: usize, status: u8, ptype: u8, lba: u32, count: u32) {
        buf[offset] = status;
        buf[offset + 4] = ptype;
        buf[offset + 8..offset + 12].copy_from_slice(&lba.to_le_bytes());
        buf[offset + 12..offset + 16].copy_from_slice(&count.to_le_bytes());
    }

    write_entry(&mut disk, 0x1BE, 0x80, 0x07, 2048, 2048); // entry 1: NTFS
    write_entry(&mut disk, 0x1CE, 0x00, 0x83, 4096, 2048); // entry 2: Linux

    let path = dir.join(name);
    fs::write(&path, &disk).unwrap();
    path
}

/// Build a minimal 10 MiB GPT disk image in memory.
///
/// Layout (512-byte sectors):
///   LBA 0  : Protective MBR (entry type 0xEE)
///   LBA 1  : GPT Header
///   LBA 2  : Partition entry array (128 entries × 128 bytes = 16 KiB)
///   LBA 34+: usable area
///   LBA 2048–4095: partition 1 (EFI System, 1 MiB)
///   LBA 4096–6143: partition 2 (Basic Data, 1 MiB)
fn make_gpt_image(dir: &Path, name: &str) -> PathBuf {
    const SECTORS: u64 = 20_480;
    const SECTOR: usize = 512;
    let mut disk = vec![0u8; SECTORS as usize * SECTOR];

    // Protective MBR
    disk[510] = 0x55;
    disk[511] = 0xAA;
    // Protective MBR entry (type 0xEE, covers whole disk)
    disk[0x1BE] = 0x00; // not bootable
    disk[0x1BE + 4] = 0xEE;
    disk[0x1BE + 8..0x1BE + 12].copy_from_slice(&1u32.to_le_bytes());
    let lba_count = (SECTORS - 1).min(0xFFFF_FFFF) as u32;
    disk[0x1BE + 12..0x1BE + 16].copy_from_slice(&lba_count.to_le_bytes());

    // GPT Header at LBA 1
    let hdr_off = SECTOR;
    disk[hdr_off..hdr_off + 8].copy_from_slice(b"EFI PART");
    // revision 1.0
    disk[hdr_off + 8..hdr_off + 12].copy_from_slice(&[0x00, 0x00, 0x01, 0x00]);
    // header size = 92
    disk[hdr_off + 12..hdr_off + 16].copy_from_slice(&92u32.to_le_bytes());
    // my LBA = 1
    disk[hdr_off + 24..hdr_off + 32].copy_from_slice(&1u64.to_le_bytes());
    // alternate LBA = last sector
    disk[hdr_off + 32..hdr_off + 40].copy_from_slice(&(SECTORS - 1).to_le_bytes());
    // first usable LBA = 34
    disk[hdr_off + 40..hdr_off + 48].copy_from_slice(&34u64.to_le_bytes());
    // last usable LBA = SECTORS - 34
    disk[hdr_off + 48..hdr_off + 56].copy_from_slice(&(SECTORS - 34).to_le_bytes());
    // disk GUID (arbitrary)
    disk[hdr_off + 56..hdr_off + 72].copy_from_slice(&[
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10,
    ]);
    // partition entry start LBA = 2
    disk[hdr_off + 72..hdr_off + 80].copy_from_slice(&2u64.to_le_bytes());
    // entry count = 128
    disk[hdr_off + 80..hdr_off + 84].copy_from_slice(&128u32.to_le_bytes());
    // entry size = 128
    disk[hdr_off + 84..hdr_off + 88].copy_from_slice(&128u32.to_le_bytes());

    // GPT partition entries at LBA 2 (offset = 2 * 512 = 1024)
    // Entry 0: EFI System Partition {c12a7328-f81f-11d2-ba4b-00a0c93ec93b}
    let entry0_off = 2 * SECTOR;
    let efi_type: [u8; 16] = [
        0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9,
        0x3b,
    ];
    disk[entry0_off..entry0_off + 16].copy_from_slice(&efi_type);
    // unique GUID (arbitrary)
    disk[entry0_off + 16..entry0_off + 32].copy_from_slice(&[0xAA; 16]);
    // first LBA = 2048, last LBA = 4095
    disk[entry0_off + 32..entry0_off + 40].copy_from_slice(&2048u64.to_le_bytes());
    disk[entry0_off + 40..entry0_off + 48].copy_from_slice(&4095u64.to_le_bytes());
    // name "EFI" in UTF-16LE
    let efi_name: Vec<u8> = "EFI\0"
        .encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect();
    disk[entry0_off + 56..entry0_off + 56 + efi_name.len()].copy_from_slice(&efi_name);

    // Entry 1: Basic Data Partition {ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}
    let entry1_off = 2 * SECTOR + 128;
    let data_type: [u8; 16] = [
        0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99,
        0xc7,
    ];
    disk[entry1_off..entry1_off + 16].copy_from_slice(&data_type);
    disk[entry1_off + 16..entry1_off + 32].copy_from_slice(&[0xBB; 16]);
    disk[entry1_off + 32..entry1_off + 40].copy_from_slice(&4096u64.to_le_bytes());
    disk[entry1_off + 40..entry1_off + 48].copy_from_slice(&6143u64.to_le_bytes());
    let data_name: Vec<u8> = "Data\0"
        .encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect();
    disk[entry1_off + 56..entry1_off + 56 + data_name.len()].copy_from_slice(&data_name);

    let path = dir.join(name);
    fs::write(&path, &disk).unwrap();
    path
}

// ── MBR tests ─────────────────────────────────────────────────────────────────

#[test]
fn mbr_partition_table_detected() {
    let tmp = TempDir::new().unwrap();
    let image = make_mbr_image(tmp.path(), "mbr.dd");
    let container = tmp.path().join("mbr.offf");

    // Use 1 MiB chunks so the first chunk definitely covers sectors 0–2048
    convert_image(&image, &container, 1024 * 1024, Compression::None);

    let chunks = read_physical_to_chunk(&container.join("maps/physical_to_chunk.parquet")).unwrap();

    let manifest: ManifestJson =
        serde_json::from_str(&fs::read_to_string(container.join("manifest.json")).unwrap())
            .unwrap();

    let table = detect_and_parse(
        &container,
        &chunks,
        manifest.source.sector_size,
        &manifest.container_id,
        "test",
    )
    .unwrap();

    assert_eq!(table.partition_table_type, "mbr");
    assert_eq!(table.partitions.len(), 2, "expected 2 MBR partitions");

    let p0 = &table.partitions[0];
    assert_eq!(p0.partition_id, "mbr-1");
    assert_eq!(p0.first_lba, 2048);
    assert_eq!(p0.last_lba, 4095);
    assert_eq!(p0.start_offset, 2048 * 512);
    assert_eq!(p0.length, 2048 * 512);
    assert_eq!(p0.bootable, Some(true));
    assert!(p0.partition_type.contains("NTFS"));
    assert!(
        !p0.chunk_refs.is_empty(),
        "partition 1 should reference at least one chunk"
    );

    let p1 = &table.partitions[1];
    assert_eq!(p1.partition_id, "mbr-2");
    assert_eq!(p1.first_lba, 4096);
    assert_eq!(p1.bootable, Some(false));
    assert!(p1.partition_type.contains("Linux"));
}

#[test]
fn mbr_indexing_does_not_modify_evidence() {
    let tmp = TempDir::new().unwrap();
    let image = make_mbr_image(tmp.path(), "mbr_immut.dd");
    let container = tmp.path().join("mbr_immut.offf");

    convert_image(&image, &container, 1024 * 1024, Compression::None);

    // Snapshot chunk hashes before indexing
    let before: Vec<_> = fs::read_dir(container.join("chunks/sha256"))
        .unwrap()
        .flat_map(|e| {
            let d = e.unwrap().path();
            fs::read_dir(d)
                .unwrap()
                .flat_map(|e2| {
                    let d2 = e2.unwrap().path();
                    fs::read_dir(d2)
                        .unwrap()
                        .map(|e3| e3.unwrap().path())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .map(|p| (p.clone(), file_sha256(&p)))
        .collect();

    // Run indexing
    let chunks = read_physical_to_chunk(&container.join("maps/physical_to_chunk.parquet")).unwrap();
    let manifest: ManifestJson =
        serde_json::from_str(&fs::read_to_string(container.join("manifest.json")).unwrap())
            .unwrap();
    detect_and_parse(&container, &chunks, 512, &manifest.container_id, "test").unwrap();

    // Verify chunk files are unchanged
    for (path, hash_before) in before {
        let hash_after = file_sha256(&path);
        assert_eq!(
            hash_before,
            hash_after,
            "chunk file was modified by indexing: {}",
            path.display()
        );
    }
}

// ── GPT tests ─────────────────────────────────────────────────────────────────

#[test]
fn gpt_partition_table_detected() {
    let tmp = TempDir::new().unwrap();
    let image = make_gpt_image(tmp.path(), "gpt.dd");
    let container = tmp.path().join("gpt.offf");

    convert_image(&image, &container, 1024 * 1024, Compression::None);

    let chunks = read_physical_to_chunk(&container.join("maps/physical_to_chunk.parquet")).unwrap();
    let manifest: ManifestJson =
        serde_json::from_str(&fs::read_to_string(container.join("manifest.json")).unwrap())
            .unwrap();

    let table = detect_and_parse(
        &container,
        &chunks,
        manifest.source.sector_size,
        &manifest.container_id,
        "test",
    )
    .unwrap();

    assert_eq!(table.partition_table_type, "gpt");
    assert!(table.disk_guid.is_some(), "GPT should have disk GUID");
    assert_eq!(table.partitions.len(), 2, "expected 2 GPT partitions");

    let efi = &table.partitions[0];
    assert_eq!(efi.partition_id, "gpt-1");
    assert_eq!(efi.first_lba, 2048);
    assert_eq!(efi.last_lba, 4095);
    assert_eq!(efi.start_offset, 2048 * 512);
    assert_eq!(efi.length, 2048 * 512);
    assert_eq!(efi.partition_type, "EFI System Partition");
    assert_eq!(
        efi.type_guid.as_deref(),
        Some("c12a7328-f81f-11d2-ba4b-00a0c93ec93b")
    );
    assert_eq!(efi.name.as_deref(), Some("EFI"));
    assert!(!efi.chunk_refs.is_empty());

    let data = &table.partitions[1];
    assert_eq!(data.partition_id, "gpt-2");
    assert_eq!(data.first_lba, 4096);
    assert_eq!(data.last_lba, 6143);
    assert_eq!(data.partition_type, "Basic Data Partition");
    assert_eq!(data.name.as_deref(), Some("Data"));
}

#[test]
fn gpt_partition_table_json_written() {
    let tmp = TempDir::new().unwrap();
    let image = make_gpt_image(tmp.path(), "gpt_json.dd");
    let container = tmp.path().join("gpt_json.offf");

    convert_image(&image, &container, 1024 * 1024, Compression::None);

    let chunks = read_physical_to_chunk(&container.join("maps/physical_to_chunk.parquet")).unwrap();
    let manifest: ManifestJson =
        serde_json::from_str(&fs::read_to_string(container.join("manifest.json")).unwrap())
            .unwrap();

    let table = detect_and_parse(
        &container,
        &chunks,
        manifest.source.sector_size,
        &manifest.container_id,
        "test",
    )
    .unwrap();

    // Serialise (as offf-index would)
    let json_path = container.join("indexes/partition_table.json");
    fs::create_dir_all(json_path.parent().unwrap()).unwrap();
    fs::write(&json_path, serde_json::to_string_pretty(&table).unwrap()).unwrap();

    // Round-trip deserialise
    let loaded: PartitionTableJson =
        serde_json::from_str(&fs::read_to_string(&json_path).unwrap()).unwrap();
    assert_eq!(loaded.partition_table_type, "gpt");
    assert_eq!(loaded.partitions.len(), 2);
}

#[test]
fn read_bytes_at_crosses_chunk_boundary() {
    let tmp = TempDir::new().unwrap();
    // 3-sector image, each sector has a distinctive pattern
    const SS: usize = 512;
    let mut img = vec![0u8; SS * 3];
    img[..SS].fill(0xAA);
    img[SS..SS * 2].fill(0xBB);
    img[SS * 2..].fill(0xCC);
    let path = tmp.path().join("sectors.dd");
    fs::write(&path, &img).unwrap();

    // Convert with 1-sector chunks so every read crosses a boundary
    let container = tmp.path().join("sectors.offf");
    convert_image(&path, &container, SS as u64, Compression::None);

    let chunks = read_physical_to_chunk(&container.join("maps/physical_to_chunk.parquet")).unwrap();

    // Read across the boundary between sector 0 and sector 1
    let data = read_bytes_at(&container, &chunks, SS as u64 - 4, 8).unwrap();
    assert_eq!(&data[..4], &[0xAA; 4]);
    assert_eq!(&data[4..], &[0xBB; 4]);
}

#[test]
fn chunk_refs_cover_full_partition_range() {
    let tmp = TempDir::new().unwrap();
    let image = make_mbr_image(tmp.path(), "refs.dd");
    let container = tmp.path().join("refs.offf");

    // Use 512 KiB chunks: partitions span two chunks each
    convert_image(&image, &container, 512 * 1024, Compression::None);

    let chunks = read_physical_to_chunk(&container.join("maps/physical_to_chunk.parquet")).unwrap();

    // Partition 1: LBA 2048–4095 = 1 MiB at offset 1 MiB
    let refs = chunk_refs_for_range(&chunks, 2048 * 512, 2048 * 512);
    // With 512 KiB chunks, 1 MiB partition = 2 chunks
    assert_eq!(
        refs.len(),
        2,
        "1 MiB partition should span 2 × 512 KiB chunks"
    );
}

// ── Object lineage tests ──────────────────────────────────────────────────────

/// Helper: build a minimal `DiscoveredObjectRow` with defaults for all optional fields.
fn make_object(id: &str) -> DiscoveredObjectRow {
    DiscoveredObjectRow {
        object_id: id.to_string(),
        object_type: "file".to_string(),
        name: Some(id.to_string()),
        logical_path: None,
        media_type: None,
        size_bytes: None,
        sha256: None,
        source_layer: "carved".to_string(),
        storage_ref: None,
        root_source_ref: None,
        created_by_job_id: Some("job-1".to_string()),
        parser_status: "ok".to_string(),
        provenance_ref: None,
        schema_version: "0.1.0".to_string(),
    }
}

/// Helper: build a minimal `ObjectEdgeRow`.
fn make_edge(edge_id: &str, parent: &str, child: &str) -> ObjectEdgeRow {
    ObjectEdgeRow {
        edge_id: edge_id.to_string(),
        parent_object_id: parent.to_string(),
        child_object_id: child.to_string(),
        relation_type: "contains".to_string(),
        method: Some("carved".to_string()),
        logical_path: None,
        sequence: None,
        created_by_job_id: Some("job-1".to_string()),
        provenance_ref: None,
        schema_version: "0.1.0".to_string(),
    }
}

/// Helper: build a minimal `DerivationRow`.
fn make_derivation(deriv_id: &str, parent: &str, child: &str) -> DerivationRow {
    DerivationRow {
        derivation_id: deriv_id.to_string(),
        parent_object_id: parent.to_string(),
        child_object_id: child.to_string(),
        job_id: "job-1".to_string(),
        method: "extract".to_string(),
        tool_id: "tool-a".to_string(),
        tool_name: "tool-a".to_string(),
        tool_version: "1.0".to_string(),
        parameters_hash: None,
        input_sha256: None,
        output_sha256: None,
        storage_mode: "indexed".to_string(),
        provenance_ref: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        schema_version: "0.1.0".to_string(),
    }
}

#[test]
fn lineage_valid_graph_passes() {
    // parent → child (via edge and derivation)
    let objects = vec![make_object("obj-parent"), make_object("obj-child")];
    let edges = vec![make_edge("edge-1", "obj-parent", "obj-child")];
    let derivations = vec![make_derivation("deriv-1", "obj-parent", "obj-child")];

    let report = ObjectLineageValidator::validate(&objects, &edges, &derivations);

    assert!(
        report.is_valid(),
        "valid graph should pass: {:?}",
        (
            &report.missing_edge_parents,
            &report.missing_edge_children,
            &report.missing_derivation_parents,
            &report.missing_derivation_children,
            &report.invalid_derivation_links,
            &report.cycles,
        )
    );
    assert!(report.missing_edge_parents.is_empty());
    assert!(report.missing_edge_children.is_empty());
    assert!(report.cycles.is_empty());
}

#[test]
fn lineage_missing_child_object_fails() {
    // Edge references a child that has no object row
    let objects = vec![make_object("obj-parent")];
    let edges = vec![make_edge("edge-1", "obj-parent", "obj-child-missing")];
    let derivations = vec![];

    let report = ObjectLineageValidator::validate(&objects, &edges, &derivations);

    assert!(
        !report.is_valid(),
        "graph with missing child object should fail"
    );
    assert!(
        !report.missing_edge_children.is_empty(),
        "should report missing edge children: {:?}",
        report.missing_edge_children
    );
}

#[test]
fn lineage_cycle_in_object_graph_fails() {
    // A → B → C → A is a cycle
    let objects = vec![
        make_object("obj-a"),
        make_object("obj-b"),
        make_object("obj-c"),
    ];
    let edges = vec![
        make_edge("edge-ab", "obj-a", "obj-b"),
        make_edge("edge-bc", "obj-b", "obj-c"),
        make_edge("edge-ca", "obj-c", "obj-a"),
    ];
    let derivations = vec![];

    let report = ObjectLineageValidator::validate(&objects, &edges, &derivations);

    assert!(!report.is_valid(), "graph with cycle should fail");
    assert!(
        !report.cycles.is_empty(),
        "should report cycles: {:?}",
        report.cycles
    );
}

// ── Manifest v0.2 extension tests ─────────────────────────────────────────────

/// Helper: build a minimal `ManifestJson` for unit tests (no actual chunks written).
fn make_manifest(offf_version: &str) -> ManifestJson {
    use chrono::TimeZone;
    ManifestJson {
        offf_version: offf_version.to_string(),
        container_id: "urn:offf:case:test-case-001".to_string(),
        created_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        created_by_tool: ToolInfo {
            name: "offf-convert".to_string(),
            version: "0.1.0".to_string(),
        },
        source: SourceInfo {
            source_type: "raw_image".to_string(),
            size_bytes: 1024 * 1024,
            sector_size: 512,
        },
        hashes: ManifestHashes {
            source_sha256: "a".repeat(64),
            merkle_root_sha256: "b".repeat(64),
        },
        chunking: ChunkingInfo {
            chunk_size: 512 * 1024,
            chunking_mode: "fixed".to_string(),
            compression: "none".to_string(),
            hash_algorithm: "sha256".to_string(),
        },
        indexes: ManifestIndexes {
            physical_to_chunk: "maps/physical_to_chunk.parquet".to_string(),
        },
        extensions: None,
    }
}

#[test]
fn manifest_v010_round_trip_no_extensions() {
    let m = make_manifest(OFFF_VERSION);
    let json = serde_json::to_string(&m).unwrap();
    let loaded: ManifestJson = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.offf_version, OFFF_VERSION);
    assert!(loaded.extensions.is_none());
    // serialised JSON must not contain 'extensions' key
    assert!(
        !json.contains("\"extensions\""),
        "v0.1.0 manifest must not serialise extensions field"
    );
}

#[test]
fn manifest_v020_round_trip_with_extensions() {
    let mut m = make_manifest(OFFF_V2_VERSION);
    let mut entries = std::collections::HashMap::new();
    entries.insert(
        "acme-forensics:chain-of-custody".to_string(),
        serde_json::json!({ "officer": "J. Smith", "badge": "42" }),
    );
    entries.insert(
        "lab-tools:acquisition-metadata".to_string(),
        serde_json::json!({ "device_make": "Tableau", "firmware": "3.14" }),
    );
    m.extensions = Some(ManifestExtensions { entries });

    let json = serde_json::to_string_pretty(&m).unwrap();
    let loaded: ManifestJson = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.offf_version, OFFF_V2_VERSION);
    let ext = loaded
        .extensions
        .as_ref()
        .expect("extensions must be present");
    assert_eq!(ext.entries.len(), 2);
    assert!(ext.entries.contains_key("acme-forensics:chain-of-custody"));
    assert!(ext.entries.contains_key("lab-tools:acquisition-metadata"));
    let coc = &ext.entries["acme-forensics:chain-of-custody"];
    assert_eq!(coc["officer"], "J. Smith");
}

#[test]
fn manifest_v010_json_loadable_by_v020_reader() {
    // A v0.1.0 manifest JSON (no extensions field) must parse into ManifestJson
    // with extensions == None even though the struct now has that field.
    let raw = r#"{
        "offf_version": "0.1.0",
        "container_id": "urn:offf:case:compat-test",
        "created_at": "2026-01-01T00:00:00Z",
        "created_by_tool": { "name": "offf-convert", "version": "0.1.0" },
        "source": { "type": "raw_image", "size_bytes": 1048576, "sector_size": 512 },
        "hashes": {
            "source_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "merkle_root_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        },
        "chunking": {
            "chunk_size": 524288,
            "chunking_mode": "fixed",
            "compression": "none",
            "hash_algorithm": "sha256"
        },
        "indexes": { "physical_to_chunk": "maps/physical_to_chunk.parquet" }
    }"#;
    let m: ManifestJson = serde_json::from_str(raw).expect("v0.1.0 manifest must parse");
    assert_eq!(m.offf_version, "0.1.0");
    assert!(m.extensions.is_none(), "extensions must be None for v0.1.0");
}

#[test]
fn manifest_v020_json_loadable_by_strict_fields() {
    // A v0.2.0 manifest with extensions must round-trip correctly.
    let raw = r#"{
        "offf_version": "0.2.0",
        "container_id": "urn:offf:case:v2-test",
        "created_at": "2026-01-01T00:00:00Z",
        "created_by_tool": { "name": "offf-convert", "version": "0.2.0" },
        "source": { "type": "raw_image", "size_bytes": 2097152, "sector_size": 512 },
        "hashes": {
            "source_sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "merkle_root_sha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
        },
        "chunking": {
            "chunk_size": 524288,
            "chunking_mode": "fixed",
            "compression": "zstd",
            "hash_algorithm": "sha256"
        },
        "indexes": { "physical_to_chunk": "maps/physical_to_chunk.parquet" },
        "extensions": {
            "example-ns:meta": { "key": "value" }
        }
    }"#;
    let m: ManifestJson = serde_json::from_str(raw).expect("v0.2.0 manifest must parse");
    assert_eq!(m.offf_version, "0.2.0");
    let ext = m.extensions.as_ref().expect("extensions must be Some");
    assert!(ext.entries.contains_key("example-ns:meta"));
}
