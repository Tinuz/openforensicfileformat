//! NTFS filesystem indexer.
//!
//! Parses a raw NTFS volume stored in an OFFF chunk store and produces a
//! [`FileIndexRow`] for every MFT entry, including deleted records.
//!
//! The algorithm:
//! 1. Parse the NTFS Volume Boot Record (VBR) at the start of the volume.
//! 2. Read MFT record 0 (`$MFT`) from the cluster address in the VBR.
//! 3. Follow the data-run list in `$MFT`'s `$DATA` attribute to read the
//!    complete MFT into memory.
//! 4. Parse every `FILE` record, extracting `$STANDARD_INFORMATION`,
//!    `$FILE_NAME`, and `$DATA`.
//! 5. Build full paths by following parent directory references.
//! 6. Return one [`FileIndexRow`] per entry.

use std::{
    collections::HashMap,
    path::Path,
};

use chrono::{DateTime, TimeZone, Utc};
use serde_json::json;

use crate::{
    chunk::read_chunk,
    error::OfffError,
    partition::chunk_refs_for_range,
    types::{ChunkMetadata, FileIndexRow, TOOL_VERSION},
};

// ── NTFS Volume Boot Record ───────────────────────────────────────────────────

#[derive(Debug)]
struct NtfsVbr {
    bytes_per_sector: u64,
    sectors_per_cluster: u64,
    bytes_per_cluster: u64,
    bytes_per_record: u64,
    mft_lcn: u64,
}

fn parse_vbr(data: &[u8]) -> Result<NtfsVbr, OfffError> {
    if data.len() < 84 {
        return Err(OfffError::InvalidContainer(
            "VBR too short to be NTFS".into(),
        ));
    }
    if &data[3..11] != b"NTFS    " {
        return Err(OfffError::InvalidContainer(format!(
            "not an NTFS VBR (OEM ID: {:?})",
            &data[3..11]
        )));
    }

    let bytes_per_sector = u16::from_le_bytes([data[11], data[12]]) as u64;
    let sectors_per_cluster = data[13] as u64;

    if bytes_per_sector == 0 || sectors_per_cluster == 0 {
        return Err(OfffError::InvalidContainer(
            "NTFS VBR: zero bytes_per_sector or sectors_per_cluster".into(),
        ));
    }

    let bytes_per_cluster = bytes_per_sector * sectors_per_cluster;

    // Clusters per file record: if the stored i8 is negative, the record size
    // is 2^|value| bytes; otherwise it is value×bytes_per_cluster.
    let cpfr = data[64] as i8;
    let bytes_per_record = if cpfr < 0 {
        1u64 << ((-cpfr) as u64)
    } else {
        cpfr as u64 * bytes_per_cluster
    };

    if bytes_per_record == 0 {
        return Err(OfffError::InvalidContainer(
            "NTFS VBR: zero bytes_per_record".into(),
        ));
    }

    let mft_lcn = u64::from_le_bytes(data[48..56].try_into().unwrap());

    Ok(NtfsVbr {
        bytes_per_sector,
        sectors_per_cluster,
        bytes_per_cluster,
        bytes_per_record,
        mft_lcn,
    })
}

// ── Data runs ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct DataRun {
    /// First logical cluster number; -1 for a sparse (unallocated) run.
    lcn_start: i64,
    /// Number of clusters in this run.
    cluster_count: u64,
}

fn parse_data_runs(buf: &[u8]) -> Vec<DataRun> {
    let mut runs = Vec::new();
    let mut pos = 0usize;
    let mut current_lcn = 0i64;

    while pos < buf.len() {
        let header = buf[pos];
        pos += 1;
        if header == 0x00 {
            break;
        }

        let len_len = (header & 0x0F) as usize; // bytes encoding cluster count
        let off_len = ((header >> 4) & 0x0F) as usize; // bytes encoding LCN delta

        if len_len == 0 || pos + len_len + off_len > buf.len() {
            break;
        }

        // Cluster count (unsigned little-endian)
        let mut cluster_count = 0u64;
        for i in 0..len_len {
            cluster_count |= (buf[pos + i] as u64) << (i * 8);
        }
        pos += len_len;

        if off_len == 0 {
            // Sparse run
            runs.push(DataRun { lcn_start: -1, cluster_count });
        } else {
            // LCN delta (signed, sign-extended from off_len bytes)
            let mut lcn_delta = 0i64;
            for i in 0..off_len {
                lcn_delta |= (buf[pos + i] as i64) << (i * 8);
            }
            // Sign-extend if the top bit of the highest encoded byte is set
            let sign_bit = 1i64 << (off_len * 8 - 1);
            if lcn_delta & sign_bit != 0 {
                lcn_delta |= !0i64 << (off_len * 8);
            }
            pos += off_len;
            current_lcn += lcn_delta;
            runs.push(DataRun { lcn_start: current_lcn, cluster_count });
        }
    }

    runs
}

// ── Update-sequence fixup ─────────────────────────────────────────────────────

/// Apply the NTFS update-sequence-array fixup to a mutable record buffer.
///
/// The update sequence ensures that multi-sector records are written
/// atomically: the last two bytes of every 512-byte sector are replaced by
/// the "update sequence number" (USN) on disk, and the original values are
/// stored in the update sequence array (USA) in the record header.
///
/// Returns `true` if the fixup was applied successfully.
fn apply_fixup(data: &mut [u8]) -> bool {
    if data.len() < 8 {
        return false;
    }
    let usa_offset = u16::from_le_bytes([data[4], data[5]]) as usize;
    let usa_size = u16::from_le_bytes([data[6], data[7]]) as usize;

    if usa_size < 2 || usa_offset + usa_size * 2 > data.len() {
        return false;
    }

    let usn = u16::from_le_bytes([data[usa_offset], data[usa_offset + 1]]);
    let sector_count = (data.len() / 512).min(usa_size - 1);

    for i in 0..sector_count {
        let sector_end = (i + 1) * 512 - 2;
        if sector_end + 2 > data.len() {
            break;
        }
        let stored = u16::from_le_bytes([data[sector_end], data[sector_end + 1]]);
        if stored != usn {
            // Record is corrupt or partially written; apply what we can.
            return false;
        }
        let saved_idx = usa_offset + 2 + i * 2;
        let saved = [data[saved_idx], data[saved_idx + 1]];
        data[sector_end] = saved[0];
        data[sector_end + 1] = saved[1];
    }

    true
}

// ── Attribute structures ──────────────────────────────────────────────────────

struct StdInfo {
    created_at: Option<DateTime<Utc>>,
    modified_at: Option<DateTime<Utc>>,
    changed_at: Option<DateTime<Utc>>,
    accessed_at: Option<DateTime<Utc>>,
    #[allow(dead_code)]
    file_attributes: u32,
}

struct FileName {
    parent_mft: u64,
    name: String,
    namespace: u8, // 0=POSIX, 1=Win32, 2=DOS, 3=Win32&DOS
    real_size: u64,
}

struct MftRecord {
    mft_entry: u64,
    flags: u16, // bit0=in-use, bit1=directory
    std_info: Option<StdInfo>,
    file_names: Vec<FileName>,
    data_size: u64,
    data_runs: Vec<DataRun>,
    has_attr_list: bool,
    parse_error: Option<String>,
}

// ── Timestamp conversion ──────────────────────────────────────────────────────

fn filetime_to_datetime(ft: u64) -> Option<DateTime<Utc>> {
    if ft == 0 {
        return None;
    }
    // Windows FILETIME: 100-ns intervals since 1601-01-01
    const EPOCH_DIFF_100NS: u64 = 116_444_736_000_000_000;
    if ft < EPOCH_DIFF_100NS {
        return None;
    }
    let since_unix = ft - EPOCH_DIFF_100NS;
    let secs = (since_unix / 10_000_000) as i64;
    let nanos = ((since_unix % 10_000_000) * 100) as u32;
    Utc.timestamp_opt(secs, nanos).single()
}

// ── Attribute parsers ─────────────────────────────────────────────────────────

fn parse_std_info(content: &[u8]) -> Option<StdInfo> {
    if content.len() < 32 {
        return None;
    }
    let created = u64::from_le_bytes(content[0..8].try_into().ok()?);
    let modified = u64::from_le_bytes(content[8..16].try_into().ok()?);
    let changed = u64::from_le_bytes(content[16..24].try_into().ok()?);
    let accessed = u64::from_le_bytes(content[24..32].try_into().ok()?);
    let file_attributes = if content.len() >= 36 {
        u32::from_le_bytes(content[32..36].try_into().unwrap_or([0; 4]))
    } else {
        0
    };
    Some(StdInfo {
        created_at: filetime_to_datetime(created),
        modified_at: filetime_to_datetime(modified),
        changed_at: filetime_to_datetime(changed),
        accessed_at: filetime_to_datetime(accessed),
        file_attributes,
    })
}

fn parse_file_name(content: &[u8]) -> Option<FileName> {
    if content.len() < 66 {
        return None;
    }
    let parent_ref = u64::from_le_bytes(content[0..8].try_into().ok()?);
    let parent_mft = parent_ref & 0x0000_FFFF_FFFF_FFFF; // low 48 bits
    let real_size = u64::from_le_bytes(content[48..56].try_into().ok()?);
    let name_len = content[64] as usize;
    let namespace = content[65];

    if 66 + name_len * 2 > content.len() {
        return None;
    }
    let chars: Vec<u16> = content[66..66 + name_len * 2]
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    let name = String::from_utf16_lossy(&chars).to_owned();

    Some(FileName { parent_mft, name, namespace, real_size })
}

// ── MFT record parser ─────────────────────────────────────────────────────────

fn parse_mft_record(raw: &[u8], entry_num: u64, record_size: usize) -> MftRecord {
    let mut rec = MftRecord {
        mft_entry: entry_num,
        flags: 0,
        std_info: None,
        file_names: Vec::new(),
        data_size: 0,
        data_runs: Vec::new(),
        has_attr_list: false,
        parse_error: None,
    };

    if raw.len() < record_size || &raw[..4] != b"FILE" {
        rec.parse_error = Some("not a valid FILE record".into());
        return rec;
    }

    // Work on a copy so we can apply fixup
    let mut data = raw[..record_size].to_vec();
    let _fixup_ok = apply_fixup(&mut data);

    rec.flags = u16::from_le_bytes([data[22], data[23]]);

    let first_attr = u16::from_le_bytes([data[20], data[21]]) as usize;
    let real_size = u32::from_le_bytes(data[24..28].try_into().unwrap_or([0; 4])) as usize;
    let end = real_size.min(data.len());

    let mut pos = first_attr;
    while pos + 8 <= end {
        let attr_type = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        if attr_type == 0xFFFF_FFFF {
            break; // end-of-attributes marker
        }
        let attr_len =
            u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if attr_len == 0 || pos + attr_len > end {
            break;
        }

        let non_resident = data[pos + 8] != 0;

        match attr_type {
            // $STANDARD_INFORMATION
            0x10 => {
                if !non_resident && pos + 22 <= end {
                    let c_len =
                        u32::from_le_bytes(data[pos + 16..pos + 20].try_into().unwrap()) as usize;
                    let c_off =
                        u16::from_le_bytes(data[pos + 20..pos + 22].try_into().unwrap()) as usize;
                    if pos + c_off + c_len <= end {
                        rec.std_info =
                            parse_std_info(&data[pos + c_off..pos + c_off + c_len]);
                    }
                }
            }
            // $ATTRIBUTE_LIST – flag for partial parse
            0x20 => {
                rec.has_attr_list = true;
            }
            // $FILE_NAME
            0x30 => {
                if !non_resident && pos + 22 <= end {
                    let c_len =
                        u32::from_le_bytes(data[pos + 16..pos + 20].try_into().unwrap()) as usize;
                    let c_off =
                        u16::from_le_bytes(data[pos + 20..pos + 22].try_into().unwrap()) as usize;
                    if pos + c_off + c_len <= end {
                        if let Some(fn_attr) =
                            parse_file_name(&data[pos + c_off..pos + c_off + c_len])
                        {
                            rec.file_names.push(fn_attr);
                        }
                    }
                }
            }
            // $DATA (default stream only; skip named streams)
            0x80 => {
                // Check name_length at offset +9 – skip if != 0 (named ADS)
                let name_len = data[pos + 9];
                if name_len == 0 {
                    if non_resident {
                        if pos + 64 <= end {
                            rec.data_size = u64::from_le_bytes(
                                data[pos + 56..pos + 64].try_into().unwrap(),
                            );
                            let rl_off =
                                u16::from_le_bytes(data[pos + 32..pos + 34].try_into().unwrap())
                                    as usize;
                            if pos + rl_off < end {
                                rec.data_runs =
                                    parse_data_runs(&data[pos + rl_off..pos + attr_len]);
                            }
                        }
                    } else if pos + 20 <= end {
                        rec.data_size = u32::from_le_bytes(
                            data[pos + 16..pos + 20].try_into().unwrap(),
                        ) as u64;
                    }
                }
            }
            _ => {}
        }

        pos += attr_len;
    }

    // For directories, data_size = 0 is normal (no $DATA or $DATA is the index)
    // Prefer real_size from the best $FILE_NAME attribute over the $DATA size
    // when $DATA is resident and small (this is the authoritative file size).
    if rec.data_size == 0 && rec.flags & 0x02 == 0 {
        if let Some(fn_attr) = best_file_name(&rec.file_names) {
            if fn_attr.real_size > 0 {
                rec.data_size = fn_attr.real_size;
            }
        }
    }

    rec
}

// ── Name selection ────────────────────────────────────────────────────────────

/// Select the preferred $FILE_NAME attribute: Win32 (1) > Win32&DOS (3) >
/// POSIX (0) > DOS (2).
fn best_file_name(names: &[FileName]) -> Option<&FileName> {
    let priority = |ns: u8| match ns {
        1 => 0u8,
        3 => 1,
        0 => 2,
        2 => 3,
        _ => 4,
    };
    names.iter().min_by_key(|f| priority(f.namespace))
}

// ── Chunk cache ───────────────────────────────────────────────────────────────

/// Read bytes from the OFFF chunk store, caching decompressed chunks to avoid
/// repeated decompression when reading many small regions (e.g., MFT records).
struct ChunkCache<'a> {
    base: &'a Path,
    chunks: &'a [ChunkMetadata],
    cache: HashMap<u64, Vec<u8>>,
}

impl<'a> ChunkCache<'a> {
    fn new(base: &'a Path, chunks: &'a [ChunkMetadata]) -> Self {
        Self { base, chunks, cache: HashMap::new() }
    }

    fn read_at(&mut self, offset: u64, length: u64) -> Result<Vec<u8>, OfffError> {
        if length == 0 {
            return Ok(vec![]);
        }
        let end = offset + length;
        let mut result = vec![0u8; length as usize];
        let mut filled = 0u64;

        for chunk in self.chunks {
            let cs = chunk.source_offset;
            let ce = cs + chunk.source_length;
            if ce <= offset || cs >= end {
                continue;
            }
            // Populate cache if needed
            if !self.cache.contains_key(&chunk.sequence) {
                let plain = read_chunk(self.base, chunk)?;
                self.cache.insert(chunk.sequence, plain);
            }
            let plain = self.cache.get(&chunk.sequence).unwrap();

            let overlap_start = offset.max(cs);
            let overlap_end = end.min(ce);
            let src_off = (overlap_start - cs) as usize;
            let dst_off = (overlap_start - offset) as usize;
            let len = (overlap_end - overlap_start) as usize;

            if src_off + len <= plain.len() && dst_off + len <= result.len() {
                result[dst_off..dst_off + len].copy_from_slice(&plain[src_off..src_off + len]);
                filled += len as u64;
            }
        }

        if filled < length {
            return Err(OfffError::InvalidContainer(format!(
                "NTFS read at offset {offset}: requested {length} bytes, got {filled}"
            )));
        }
        Ok(result)
    }
}

// ── MFT reading ───────────────────────────────────────────────────────────────

/// Read the complete MFT data into a `Vec<u8>` by:
/// 1. Reading MFT record 0 from the cluster address in the VBR.
/// 2. Parsing record 0's `$DATA` data-run list.
/// 3. Following the runs to reconstruct the full MFT byte sequence.
fn read_mft(
    cache: &mut ChunkCache,
    volume_offset: u64,
    vbr: &NtfsVbr,
) -> Result<Vec<u8>, OfffError> {
    const MAX_MFT_BYTES: u64 = 2 * 1024 * 1024 * 1024; // safety cap: 2 GiB

    let rec_size = vbr.bytes_per_record as usize;
    let mft0_offset = volume_offset + vbr.mft_lcn * vbr.bytes_per_cluster;

    // Read MFT record 0
    let mut rec0 = cache.read_at(mft0_offset, rec_size as u64)?;
    if rec0.len() < rec_size || &rec0[..4] != b"FILE" {
        return Err(OfffError::InvalidContainer(
            "MFT record 0 is not a valid FILE record".into(),
        ));
    }
    apply_fixup(&mut rec0);

    // Find $DATA non-resident attribute in record 0
    let first_attr = u16::from_le_bytes([rec0[20], rec0[21]]) as usize;
    let mut mft_runs: Vec<DataRun> = Vec::new();
    let mut mft_allocated = 0u64;
    let mut mft_real_size = 0u64;

    let mut pos = first_attr;
    while pos + 8 <= rec0.len() {
        let attr_type = u32::from_le_bytes(rec0[pos..pos + 4].try_into().unwrap());
        if attr_type == 0xFFFF_FFFF {
            break;
        }
        let attr_len =
            u32::from_le_bytes(rec0[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if attr_len == 0 || pos + attr_len > rec0.len() {
            break;
        }

        if attr_type == 0x80 && rec0[pos + 8] != 0 {
            // Non-resident $DATA
            if pos + 64 <= rec0.len() {
                mft_allocated =
                    u64::from_le_bytes(rec0[pos + 40..pos + 48].try_into().unwrap());
                mft_real_size =
                    u64::from_le_bytes(rec0[pos + 56..pos + 64].try_into().unwrap());
                let rl_off =
                    u16::from_le_bytes(rec0[pos + 32..pos + 34].try_into().unwrap()) as usize;
                if pos + rl_off < rec0.len() {
                    mft_runs = parse_data_runs(&rec0[pos + rl_off..pos + attr_len]);
                }
            }
            break;
        }
        pos += attr_len;
    }

    if mft_runs.is_empty() {
        return Err(OfffError::InvalidContainer(
            "MFT record 0: $DATA attribute not found or not non-resident".into(),
        ));
    }
    let mft_size = mft_real_size.min(mft_allocated).min(MAX_MFT_BYTES);
    if mft_size == 0 {
        return Err(OfffError::InvalidContainer("MFT $DATA size is zero".into()));
    }

    eprintln!(
        "MFT size: {} bytes ({} records), reading…",
        mft_size,
        mft_size / vbr.bytes_per_record
    );

    let bpc = vbr.bytes_per_cluster;
    let mut mft_data: Vec<u8> = Vec::with_capacity(mft_size as usize);

    for run in &mft_runs {
        if mft_data.len() as u64 >= mft_size {
            break;
        }
        let remaining = mft_size - mft_data.len() as u64;
        let run_bytes = (run.cluster_count * bpc).min(remaining);

        if run.lcn_start < 0 {
            // Sparse run – fill with zeros
            mft_data.resize(mft_data.len() + run_bytes as usize, 0);
        } else {
            let run_offset = volume_offset + run.lcn_start as u64 * bpc;
            let chunk = cache.read_at(run_offset, run_bytes)?;
            mft_data.extend_from_slice(&chunk);
        }
    }

    mft_data.truncate(mft_size as usize);
    Ok(mft_data)
}

// ── Path building ─────────────────────────────────────────────────────────────

const ROOT_MFT: u64 = 5;

/// Build full Windows-style paths (`\Windows\System32\notepad.exe`) for every
/// MFT entry using iterative BFS from the root.
fn build_paths(records: &HashMap<u64, MftRecord>) -> HashMap<u64, String> {
    let mut paths: HashMap<u64, String> = HashMap::new();
    paths.insert(ROOT_MFT, String::new()); // root = empty prefix

    let entries: Vec<u64> = records.keys().cloned().collect();
    let mut prev_size = 0usize;

    loop {
        for &entry in &entries {
            if paths.contains_key(&entry) {
                continue;
            }
            let rec = &records[&entry];
            if let Some(fn_attr) = best_file_name(&rec.file_names) {
                let parent = fn_attr.parent_mft;
                if let Some(parent_path) = paths.get(&parent).cloned() {
                    let path = if parent_path.is_empty() {
                        format!("\\{}", fn_attr.name)
                    } else {
                        format!("{}\\{}", parent_path, fn_attr.name)
                    };
                    paths.insert(entry, path);
                }
            }
        }
        if paths.len() == prev_size {
            break; // no progress – remaining entries are orphans or in cycles
        }
        prev_size = paths.len();
    }

    paths
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Index an NTFS volume stored in an OFFF container.
///
/// # Parameters
/// - `base`: path to the OFFF container directory.
/// - `chunks`: chunk map from `physical_to_chunk.parquet`.
/// - `volume_offset`: byte offset of the NTFS volume within the image
///   (0 for a standalone volume image; `partition.start_offset` for a
///   partition inside a full disk image).
/// - `total_volume_size`: size in bytes of the NTFS volume.
/// - `partition_id`: identifier written into every [`FileIndexRow`]
///   (e.g. `"volume-1"`, `"gpt-2"`).
/// - `filesystem_id`: identifier for this filesystem instance
///   (e.g. `"ntfs-volume-1"`).
/// - `tool_name`: name of the calling tool for `parser` field.
pub fn index_ntfs(
    base: &Path,
    chunks: &[ChunkMetadata],
    volume_offset: u64,
    _total_volume_size: u64,
    partition_id: &str,
    filesystem_id: &str,
    tool_name: &str,
) -> Result<Vec<FileIndexRow>, OfffError> {
    let mut cache = ChunkCache::new(base, chunks);

    // Parse VBR
    let vbr_data = cache.read_at(volume_offset, 512)?;
    let vbr = parse_vbr(&vbr_data)?;

    eprintln!(
        "NTFS VBR: bytes_per_cluster={}, bytes_per_record={}, mft_lcn={}",
        vbr.bytes_per_cluster, vbr.bytes_per_record, vbr.mft_lcn
    );

    // Read and parse all MFT records
    let mft_data = read_mft(&mut cache, volume_offset, &vbr)?;
    let rec_size = vbr.bytes_per_record as usize;
    let record_count = mft_data.len() / rec_size;

    eprintln!("Parsing {record_count} MFT records…");

    let mut record_map: HashMap<u64, MftRecord> = HashMap::with_capacity(record_count);
    for i in 0..record_count {
        let slice = &mft_data[i * rec_size..(i + 1) * rec_size];
        if &slice[..4] != b"FILE" {
            continue; // unallocated slot
        }
        let rec = parse_mft_record(slice, i as u64, rec_size);
        record_map.insert(i as u64, rec);
    }

    eprintln!("Valid FILE records: {}", record_map.len());

    // Build full paths
    let paths = build_paths(&record_map);

    // Build FileIndexRow list
    let mut rows: Vec<FileIndexRow> = Vec::with_capacity(record_map.len());

    for (&entry, rec) in &record_map {
        let is_directory = rec.flags & 0x02 != 0;
        let is_deleted = rec.flags & 0x01 == 0; // bit 0 = in-use

        let best_fn = best_file_name(&rec.file_names);
        let filename = best_fn
            .map(|f| f.name.clone())
            .unwrap_or_else(|| format!("[{}]", entry));

        let extension = if is_directory {
            String::new()
        } else {
            filename
                .rfind('.')
                .map(|p| filename[p + 1..].to_lowercase())
                .unwrap_or_default()
        };

        let path = paths
            .get(&entry)
            .cloned()
            .unwrap_or_else(|| format!("[orphan]\\{}", filename));

        // Physical extents from $DATA data runs
        let extents: Vec<(u64, u64)> = rec
            .data_runs
            .iter()
            .filter(|r| r.lcn_start >= 0)
            .map(|r| {
                let offset = volume_offset + r.lcn_start as u64 * vbr.bytes_per_cluster;
                let length = r.cluster_count * vbr.bytes_per_cluster;
                (offset, length)
            })
            .collect();

        let physical_extents_json = serde_json::to_string(
            &extents
                .iter()
                .map(|(o, l)| json!({"offset": o, "length": l}))
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".into());

        // Chunk refs
        let mut chunk_ref_set: Vec<String> = extents
            .iter()
            .flat_map(|(o, l)| chunk_refs_for_range(chunks, *o, *l))
            .collect();
        chunk_ref_set.sort_unstable();
        chunk_ref_set.dedup();
        let chunk_refs_json =
            serde_json::to_string(&chunk_ref_set).unwrap_or_else(|_| "[]".into());

        let (parser_status, parser_error) = if rec.parse_error.is_some() {
            ("error".to_string(), rec.parse_error.clone().unwrap_or_default())
        } else if rec.has_attr_list {
            (
                "partial".to_string(),
                "$ATTRIBUTE_LIST present; only primary attributes parsed".to_string(),
            )
        } else {
            ("ok".to_string(), String::new())
        };

        let std = rec.std_info.as_ref();

        rows.push(FileIndexRow {
            file_id: entry,
            filesystem_id: filesystem_id.to_string(),
            partition_id: partition_id.to_string(),
            path,
            filename,
            extension,
            size_bytes: rec.data_size,
            created_at: std.and_then(|s| s.created_at),
            modified_at: std.and_then(|s| s.modified_at),
            accessed_at: std.and_then(|s| s.accessed_at),
            changed_at: std.and_then(|s| s.changed_at),
            physical_extents: physical_extents_json,
            chunk_refs: chunk_refs_json,
            is_directory,
            is_deleted,
            parser: tool_name.to_string(),
            parser_version: TOOL_VERSION.to_string(),
            parser_status,
            parser_error,
        });
    }

    rows.sort_by_key(|r| r.file_id);
    Ok(rows)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_run_simple() {
        // Single run: header=0x11 (1 byte length, 1 byte offset)
        // length=8 clusters, offset=0x20
        let buf = [0x11u8, 0x08, 0x20, 0x00];
        let runs = parse_data_runs(&buf);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].cluster_count, 8);
        assert_eq!(runs[0].lcn_start, 0x20);
    }

    #[test]
    fn data_run_two_relative() {
        // Run 1: length=10, offset=+100 (LCN=100)
        // Run 2: length=5,  offset=+50  (LCN=150)
        // Encoding: 0x11 0x0A 0x64  0x11 0x05 0x32  0x00
        let buf = [0x11u8, 0x0A, 0x64, 0x11, 0x05, 0x32, 0x00];
        let runs = parse_data_runs(&buf);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].lcn_start, 100);
        assert_eq!(runs[1].lcn_start, 150);
    }

    #[test]
    fn data_run_negative_offset() {
        // LCN delta = -1 encoded as 0xFF in 1 byte
        // Run 1 places us at LCN=0x100, run 2 goes back by 1 → LCN=0xFF
        let buf = [
            0x11u8, 0x08, 0x00, // run 1: length=8, delta=0 → LCN=0
            0x11, 0x04, 0xFF, // run 2: length=4, delta=-1 (signed) → LCN=-1
            0x00,
        ];
        let runs = parse_data_runs(&buf);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].lcn_start, 0);
        assert_eq!(runs[1].lcn_start, -1); // 0 + (-1) = -1
    }

    #[test]
    fn filetime_epoch() {
        // Windows epoch (1601-01-01) → before Unix epoch → None
        assert_eq!(filetime_to_datetime(0), None);
    }

    #[test]
    fn filetime_known_date() {
        // 2026-01-01 00:00:00 UTC
        // Python: (datetime(2026,1,1) - datetime(1601,1,1)).total_seconds() * 10_000_000
        let ft: u64 = 134_116_992_000_000_000;
        let dt = filetime_to_datetime(ft).expect("should parse");
        assert_eq!(dt.to_string(), "2026-01-01 00:00:00 UTC");
    }

    #[test]
    fn fixup_applied() {
        // Craft a 1024-byte "FILE" record with a valid USA
        let mut rec = vec![0u8; 1024];
        rec[0..4].copy_from_slice(b"FILE");
        // usa_offset = 48, usa_size = 3 (1 USN + 2 saved values)
        rec[4..6].copy_from_slice(&48u16.to_le_bytes());
        rec[6..8].copy_from_slice(&3u16.to_le_bytes());
        let usn: u16 = 0xBEEF;
        rec[48..50].copy_from_slice(&usn.to_le_bytes()); // USN
        rec[50..52].copy_from_slice(&0x1111u16.to_le_bytes()); // saved sector 0
        rec[52..54].copy_from_slice(&0x2222u16.to_le_bytes()); // saved sector 1
        // Place USN at end of each 512-byte sector
        rec[510..512].copy_from_slice(&usn.to_le_bytes());
        rec[1022..1024].copy_from_slice(&usn.to_le_bytes());

        assert!(apply_fixup(&mut rec));
        assert_eq!(u16::from_le_bytes([rec[510], rec[511]]), 0x1111);
        assert_eq!(u16::from_le_bytes([rec[1022], rec[1023]]), 0x2222);
    }
}
