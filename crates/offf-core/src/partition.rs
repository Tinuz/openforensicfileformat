//! MBR and GPT partition table parsing.
//!
//! Reading strategy: physical bytes are assembled on-demand from the chunk
//! store via `read_bytes_at`, which decompresses only the chunks that overlap
//! the requested byte range.  For the tiny reads needed by partition parsing
//! (≤ a few KiB from the start of the image) this is fast even on large
//! containers.

use std::path::Path;

use crate::{
    chunk::read_chunk,
    error::OfffError,
    types::{ChunkMetadata, PartitionEntry, PartitionTableJson, ToolInfo, TOOL_VERSION},
};

// ── Known GPT type GUIDs ──────────────────────────────────────────────────────

static KNOWN_TYPE_GUIDS: &[(&str, &str)] = &[
    (
        "c12a7328-f81f-11d2-ba4b-00a0c93ec93b",
        "EFI System Partition",
    ),
    (
        "21686148-6449-6e6f-744e-656564454649",
        "BIOS Boot Partition",
    ),
    ("e3c9e316-0b5c-4db8-817d-f92df00215ae", "Microsoft Reserved"),
    (
        "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7",
        "Basic Data Partition",
    ),
    (
        "de94bba4-06d1-4d40-a16a-bfd50179d6ac",
        "Windows Recovery Environment",
    ),
    (
        "0fc63daf-8483-4772-8e79-3d69d8477de4",
        "Linux Filesystem Data",
    ),
    ("0657fd6d-a4ab-43c4-84e5-0933c84b4f4f", "Linux Swap"),
    ("e6d6d379-f507-44c2-a23c-238f2a3df928", "Linux LVM"),
    ("48465300-0000-11aa-aa11-00306543ecac", "Apple HFS+"),
    ("7c3457ef-0000-11aa-aa11-00306543ecac", "Apple APFS"),
];

fn lookup_type_guid(guid: &str) -> &'static str {
    KNOWN_TYPE_GUIDS
        .iter()
        .find(|(g, _)| *g == guid)
        .map(|(_, name)| *name)
        .unwrap_or("Unknown")
}

// ── Known MBR partition types ─────────────────────────────────────────────────

fn mbr_type_name(type_byte: u8) -> &'static str {
    match type_byte {
        0x00 => "Empty",
        0x01 => "FAT12",
        0x04 => "FAT16 < 32 MiB",
        0x05 => "Extended (CHS)",
        0x06 => "FAT16",
        0x07 => "NTFS / exFAT",
        0x0B => "FAT32 (CHS)",
        0x0C => "FAT32 (LBA)",
        0x0E => "FAT16 (LBA)",
        0x0F => "Extended (LBA)",
        0x11 => "Hidden FAT12",
        0x14 => "Hidden FAT16 < 32 MiB",
        0x16 => "Hidden FAT16",
        0x17 => "Hidden NTFS",
        0x1B => "Hidden FAT32 (CHS)",
        0x1C => "Hidden FAT32 (LBA)",
        0x1E => "Hidden FAT16 (LBA)",
        0x42 => "Dynamic Disk",
        0x82 => "Linux Swap",
        0x83 => "Linux Filesystem",
        0x85 => "Linux Extended",
        0x8E => "Linux LVM",
        0xA5 => "FreeBSD",
        0xA8 => "Apple macOS",
        0xEE => "GPT Protective MBR",
        0xEF => "EFI System Partition (FAT)",
        0xFB => "VMware VMFS",
        0xFC => "VMware kcore crash protection",
        _ => "Unknown",
    }
}

// ── Byte-range reader ─────────────────────────────────────────────────────────

/// Read `length` bytes starting at `offset` bytes from the start of the image.
///
/// Only the overlapping chunks are decompressed; for the small reads used in
/// partition parsing (sector 0/1 and the GPT partition entry array) this is
/// very fast.
pub fn read_bytes_at(
    base: &Path,
    chunks: &[ChunkMetadata],
    offset: u64,
    length: u64,
) -> Result<Vec<u8>, OfffError> {
    if length == 0 {
        return Ok(vec![]);
    }

    let end = offset + length;
    let mut result = vec![0u8; length as usize];
    let mut filled: u64 = 0;

    for chunk in chunks {
        let chunk_start = chunk.source_offset;
        let chunk_end = chunk_start + chunk.source_length;

        // Skip chunks that don't overlap [offset, end)
        if chunk_end <= offset || chunk_start >= end {
            continue;
        }

        let plaintext = read_chunk(base, chunk)?;

        let overlap_start = offset.max(chunk_start);
        let overlap_end = end.min(chunk_end);
        let src_off = (overlap_start - chunk_start) as usize;
        let dst_off = (overlap_start - offset) as usize;
        let len = (overlap_end - overlap_start) as usize;

        result[dst_off..dst_off + len].copy_from_slice(&plaintext[src_off..src_off + len]);
        filled += len as u64;
    }

    if filled < length {
        return Err(OfffError::InvalidContainer(format!(
            "requested {length} bytes at offset {offset} but only {filled} bytes available"
        )));
    }

    Ok(result)
}

// ── Partition detection and parsing ──────────────────────────────────────────

/// Detect and parse the partition table from an OFFF container.
///
/// Returns `(table_type, disk_guid, entries)`.
/// `table_type` is `"mbr"`, `"gpt"`, or `"unknown"`.
/// Attempt to detect a filesystem type directly from a volume boot record.
///
/// Returns the filesystem name ("NTFS", "exFAT", "FAT32", "FAT16", "FAT12")
/// when the first sector looks like a VBR rather than an MBR/GPT partition
/// table header.
pub fn detect_volume_type(sector0: &[u8]) -> Option<String> {
    if sector0.len() < 11 {
        return None;
    }
    let oem = &sector0[3..11];
    if oem == b"NTFS    " {
        return Some("NTFS".to_string());
    }
    if oem == b"EXFAT   " {
        return Some("exFAT".to_string());
    }
    if sector0.len() >= 90 && &sector0[82..87] == b"FAT32" {
        return Some("FAT32".to_string());
    }
    if sector0.len() >= 62 {
        let ft = &sector0[54..59];
        if ft == b"FAT16" {
            return Some("FAT16".to_string());
        }
        if ft == b"FAT12" {
            return Some("FAT12".to_string());
        }
        if ft == b"FAT  " {
            return Some("FAT".to_string());
        }
    }
    None
}

pub fn detect_and_parse(
    base: &Path,
    chunks: &[ChunkMetadata],
    sector_size: u32,
    container_id: &str,
    tool_name: &str,
) -> Result<PartitionTableJson, OfffError> {
    let ss = sector_size as u64;

    // Read sector 0
    let sector0 = read_bytes_at(base, chunks, 0, ss)?;

    // ── Volume image detection (check before MBR/GPT) ─────────────────────
    // If sector 0 is a filesystem VBR (NTFS, FAT32, exFAT…) rather than an
    // MBR, treat the whole container as a single-volume image.
    if let Some(fs_type) = detect_volume_type(&sector0) {
        let total_size = chunks
            .iter()
            .map(|c| c.source_offset + c.source_length)
            .max()
            .unwrap_or(0);
        let chunk_refs = chunk_refs_for_range(chunks, 0, total_size);

        return Ok(PartitionTableJson {
            generated_at: chrono::Utc::now(),
            generated_by_tool: ToolInfo {
                name: tool_name.to_string(),
                version: TOOL_VERSION.to_string(),
            },
            container_id: container_id.to_string(),
            sector_size,
            partition_table_type: "volume_image".to_string(),
            disk_guid: None,
            partitions: vec![PartitionEntry {
                partition_id: "volume-1".to_string(),
                name: Some(format!("{} volume", fs_type)),
                partition_type: fs_type.clone(),
                type_guid: None,
                unique_guid: None,
                start_offset: 0,
                length: total_size,
                first_lba: 0,
                last_lba: if sector_size > 0 {
                    total_size / sector_size as u64
                } else {
                    0
                },
                attributes: None,
                bootable: None,
                chunk_refs,
                filesystem_type: Some(fs_type),
            }],
        });
    }

    // Check MBR signature
    let has_mbr_sig = sector0.len() >= 512 && sector0[510] == 0x55 && sector0[511] == 0xAA;

    // Check for GPT: protective MBR entry (type 0xEE) + "EFI PART" at sector 1
    let is_gpt = if has_mbr_sig && sector0.len() >= 512 {
        // Any of the 4 MBR entries has type 0xEE
        (0..4).any(|i| sector0[0x1BE + i * 16 + 4] == 0xEE)
    } else {
        false
    };

    if is_gpt {
        // Try to read GPT header at sector 1
        let sector1 = read_bytes_at(base, chunks, ss, ss)?;
        match parse_gpt(base, chunks, &sector1, ss, container_id, tool_name) {
            Ok(t) => return Ok(t),
            Err(e) => {
                // Fall through to MBR if GPT parse fails
                eprintln!("GPT parse failed ({e}), falling back to MBR");
            }
        }
    }

    if has_mbr_sig {
        return parse_mbr(chunks, &sector0, ss, container_id, tool_name);
    }

    // No recognisable partition table
    Ok(PartitionTableJson {
        generated_at: chrono::Utc::now(),
        generated_by_tool: ToolInfo {
            name: tool_name.to_string(),
            version: TOOL_VERSION.to_string(),
        },
        container_id: container_id.to_string(),
        sector_size,
        partition_table_type: "unknown".to_string(),
        disk_guid: None,
        partitions: vec![],
    })
}

// ── MBR parsing ───────────────────────────────────────────────────────────────

fn parse_mbr(
    chunks: &[ChunkMetadata],
    sector0: &[u8],
    sector_size: u64,
    container_id: &str,
    tool_name: &str,
) -> Result<PartitionTableJson, OfffError> {
    if sector0.len() < 512 {
        return Err(OfffError::InvalidContainer(
            "sector 0 too small for MBR".into(),
        ));
    }

    let mut partitions = Vec::new();
    let mut mbr_index = 1u32;

    for i in 0..4usize {
        let entry = &sector0[0x1BE + i * 16..0x1BE + (i + 1) * 16];
        let status = entry[0];
        let part_type = entry[4];

        if part_type == 0x00 {
            continue; // empty slot
        }

        let lba_start = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as u64;
        let lba_count = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as u64;

        if lba_count == 0 {
            continue;
        }

        let start_offset = lba_start * sector_size;
        let length = lba_count * sector_size;

        let chunk_refs = chunk_refs_for_range(chunks, start_offset, length);

        partitions.push(PartitionEntry {
            partition_id: format!("mbr-{}", mbr_index),
            name: None,
            partition_type: format!("{} (0x{:02X})", mbr_type_name(part_type), part_type),
            type_guid: None,
            unique_guid: None,
            start_offset,
            length,
            first_lba: lba_start,
            last_lba: lba_start + lba_count - 1,
            attributes: None,
            bootable: Some(status == 0x80),
            chunk_refs,
            filesystem_type: None,
        });

        mbr_index += 1;
    }

    Ok(PartitionTableJson {
        generated_at: chrono::Utc::now(),
        generated_by_tool: ToolInfo {
            name: tool_name.to_string(),
            version: TOOL_VERSION.to_string(),
        },
        container_id: container_id.to_string(),
        sector_size: sector_size as u32,
        partition_table_type: "mbr".to_string(),
        disk_guid: None,
        partitions,
    })
}

// ── GPT parsing ───────────────────────────────────────────────────────────────

fn parse_gpt(
    base: &Path,
    chunks: &[ChunkMetadata],
    header: &[u8],
    sector_size: u64,
    container_id: &str,
    tool_name: &str,
) -> Result<PartitionTableJson, OfffError> {
    if header.len() < 92 {
        return Err(OfffError::InvalidContainer(
            "GPT header buffer too small".into(),
        ));
    }

    // Signature: "EFI PART" at offset 0
    if &header[..8] != b"EFI PART" {
        return Err(OfffError::InvalidContainer(
            "GPT signature not found".into(),
        ));
    }

    let disk_guid = format_gpt_guid(&header[56..72]);

    let entry_start_lba = u64::from_le_bytes(header[72..80].try_into().unwrap());
    let entry_count = u32::from_le_bytes(header[80..84].try_into().unwrap());
    let entry_size = u32::from_le_bytes(header[84..88].try_into().unwrap());

    if entry_size < 128 {
        return Err(OfffError::InvalidContainer(format!(
            "unexpected GPT entry size: {entry_size}"
        )));
    }

    let total_bytes = entry_count as u64 * entry_size as u64;
    let entries_offset = entry_start_lba * sector_size;

    let entries_data = read_bytes_at(base, chunks, entries_offset, total_bytes)?;

    let mut partitions = Vec::new();
    let mut gpt_index = 1u32;

    for i in 0..entry_count as usize {
        let e = &entries_data[i * entry_size as usize..(i + 1) * entry_size as usize];

        // Skip empty entries (all-zero type GUID)
        if e[..16].iter().all(|&b| b == 0) {
            continue;
        }

        let type_guid = format_gpt_guid(&e[..16]);
        let unique_guid = format_gpt_guid(&e[16..32]);
        let first_lba = u64::from_le_bytes(e[32..40].try_into().unwrap());
        let last_lba = u64::from_le_bytes(e[40..48].try_into().unwrap());
        let attributes = u64::from_le_bytes(e[48..56].try_into().unwrap());
        let name = parse_utf16le_name(&e[56..128.min(entry_size as usize)]);

        if last_lba < first_lba {
            continue;
        }

        let start_offset = first_lba * sector_size;
        let length = (last_lba - first_lba + 1) * sector_size;
        let type_description = lookup_type_guid(&type_guid);
        let chunk_refs = chunk_refs_for_range(chunks, start_offset, length);

        partitions.push(PartitionEntry {
            partition_id: format!("gpt-{}", gpt_index),
            name: if name.is_empty() { None } else { Some(name) },
            partition_type: type_description.to_string(),
            type_guid: Some(type_guid),
            unique_guid: Some(unique_guid),
            start_offset,
            length,
            first_lba,
            last_lba,
            attributes: Some(attributes),
            bootable: None,
            chunk_refs,
            filesystem_type: None,
        });

        gpt_index += 1;
    }

    Ok(PartitionTableJson {
        generated_at: chrono::Utc::now(),
        generated_by_tool: ToolInfo {
            name: tool_name.to_string(),
            version: TOOL_VERSION.to_string(),
        },
        container_id: container_id.to_string(),
        sector_size: sector_size as u32,
        partition_table_type: "gpt".to_string(),
        disk_guid: Some(disk_guid),
        partitions,
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Collect the `chunk_id`s of all chunks that overlap `[start, start+length)`.
pub fn chunk_refs_for_range(chunks: &[ChunkMetadata], start: u64, length: u64) -> Vec<String> {
    if length == 0 {
        return vec![];
    }
    let end = start + length;
    chunks
        .iter()
        .filter(|c| {
            let cs = c.source_offset;
            let ce = cs + c.source_length;
            cs < end && ce > start
        })
        .map(|c| c.chunk_id.clone())
        .collect()
}

/// Format 16 raw bytes as a GPT GUID string.
///
/// GPT stores the first three components in little-endian; the last two are
/// big-endian (stored as raw bytes in network order).
fn format_gpt_guid(b: &[u8]) -> String {
    let p1 = u32::from_le_bytes(b[0..4].try_into().unwrap());
    let p2 = u16::from_le_bytes(b[4..6].try_into().unwrap());
    let p3 = u16::from_le_bytes(b[6..8].try_into().unwrap());
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        p1, p2, p3, b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

/// Decode a UTF-16LE string, stopping at the first null character.
fn parse_utf16le_name(bytes: &[u8]) -> String {
    let chars: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16_lossy(&chars).to_owned()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChunkMetadata;

    fn dummy_chunk(seq: u64, offset: u64, length: u64) -> ChunkMetadata {
        ChunkMetadata {
            sequence: seq,
            chunk_id: format!("sha256:{:064x}", seq),
            source_offset: offset,
            source_length: length,
            stored_length: length,
            compression: "none".to_string(),
            plaintext_sha256: format!("{:064x}", seq),
            stored_sha256: format!("{:064x}", seq),
            read_errors: vec![],
        }
    }

    #[test]
    fn chunk_refs_basic() {
        let chunks = vec![
            dummy_chunk(0, 0, 100),
            dummy_chunk(1, 100, 100),
            dummy_chunk(2, 200, 100),
        ];
        // Range [50, 150) → overlaps chunks 0 and 1
        let refs = chunk_refs_for_range(&chunks, 50, 100);
        assert_eq!(refs.len(), 2);
        assert!(refs.contains(
            &"sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string()
        ));
        assert!(refs.contains(
            &"sha256:0000000000000000000000000000000000000000000000000000000000000001".to_string()
        ));
    }

    #[test]
    fn format_gpt_guid_known() {
        // EFI System Partition type GUID bytes (LE encoded)
        // {c12a7328-f81f-11d2-ba4b-00a0c93ec93b}
        let raw: [u8; 16] = [
            0x28, 0x73, 0x2a, 0xc1, // p1 LE: c12a7328
            0x1f, 0xf8, // p2 LE: f81f
            0xd2, 0x11, // p3 LE: 11d2
            0xba, 0x4b, // p4 (BE)
            0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b, // p5 (BE)
        ];
        let g = format_gpt_guid(&raw);
        assert_eq!(g, "c12a7328-f81f-11d2-ba4b-00a0c93ec93b");
        assert_eq!(lookup_type_guid(&g), "EFI System Partition");
    }

    #[test]
    fn parse_utf16le_name_basic() {
        let mut bytes = vec![0u8; 72];
        let name = "Test";
        for (i, c) in name.encode_utf16().enumerate() {
            let b = c.to_le_bytes();
            bytes[i * 2] = b[0];
            bytes[i * 2 + 1] = b[1];
        }
        assert_eq!(parse_utf16le_name(&bytes), "Test");
    }

    #[test]
    fn mbr_type_names_spot_check() {
        assert_eq!(mbr_type_name(0x07), "NTFS / exFAT");
        assert_eq!(mbr_type_name(0xEE), "GPT Protective MBR");
        assert_eq!(mbr_type_name(0x83), "Linux Filesystem");
    }
}
