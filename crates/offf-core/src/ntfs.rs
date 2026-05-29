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

use std::{collections::{HashMap, VecDeque}, path::Path};

use chrono::{DateTime, TimeZone, Utc};
use serde_json::json;

use crate::{
    chunk::read_chunk,
    error::OfffError,
    partition::chunk_refs_for_range,
    types::{ChunkMetadata, FileIndexRow, TOOL_VERSION},
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Chunk cache FIFO eviction threshold; at most this many decompressed chunks
/// are held in memory simultaneously (~128 × chunk_size worst-case).
const MAX_CACHED_CHUNKS: usize = 128;

/// Maximum named $DATA streams (ADS) collected per MFT record.
const MAX_ADS_PER_FILE: usize = 64;

/// Maximum $ATTRIBUTE_LIST entries followed per resolution pass.
const MAX_ATTR_LIST_ENTRIES: usize = 4_096;

// Windows $STANDARD_INFORMATION file attribute flags
const FILE_ATTR_SPARSE: u32 = 0x0200;
const FILE_ATTR_COMPRESSED: u32 = 0x0800;
const FILE_ATTR_ENCRYPTED: u32 = 0x4000;

// ── NTFS Volume Boot Record ───────────────────────────────────────────────────

#[derive(Debug)]
struct NtfsVbr {
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
            runs.push(DataRun {
                lcn_start: -1,
                cluster_count,
            });
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
            runs.push(DataRun {
                lcn_start: current_lcn,
                cluster_count,
            });
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
    file_attributes: u32,
}

#[derive(Clone)]
struct FileName {
    parent_mft: u64,
    name: String,
    namespace: u8, // 0=POSIX, 1=Win32, 2=DOS, 3=Win32&DOS
    real_size: u64,
}

/// An alternate data stream attached to a file.
#[derive(Clone)]
struct AdsEntry {
    name: String,
    size: u64,
}

struct MftRecord {
    flags: u16, // bit0=in-use, bit1=directory
    std_info: Option<StdInfo>,
    file_names: Vec<FileName>,
    data_size: u64,
    data_runs: Vec<DataRun>,
    /// Named alternate data streams ($DATA with a name).
    ads_streams: Vec<AdsEntry>,
    /// Derived from $STANDARD_INFORMATION FILE_ATTRIBUTE_SPARSE_FILE (0x200).
    is_sparse: bool,
    /// Derived from $STANDARD_INFORMATION FILE_ATTRIBUTE_COMPRESSED (0x800).
    is_compressed: bool,
    /// Derived from $STANDARD_INFORMATION FILE_ATTRIBUTE_ENCRYPTED (0x4000).
    is_encrypted: bool,
    has_attr_list: bool,
    /// Resident $ATTRIBUTE_LIST content for second-pass resolution.
    attr_list_resident: Option<Vec<u8>>,
    /// Data runs for a non-resident $ATTRIBUTE_LIST (resolved in index_ntfs).
    attr_list_runs: Vec<DataRun>,
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

    Some(FileName {
        parent_mft,
        name,
        namespace,
        real_size,
    })
}

// ── MFT record parser ─────────────────────────────────────────────────────────

fn parse_mft_record(raw: &[u8], record_size: usize) -> MftRecord {
    let mut rec = MftRecord {
        flags: 0,
        std_info: None,
        file_names: Vec::new(),
        data_size: 0,
        data_runs: Vec::new(),
        ads_streams: Vec::new(),
        is_sparse: false,
        is_compressed: false,
        is_encrypted: false,
        has_attr_list: false,
        attr_list_resident: None,
        attr_list_runs: Vec::new(),
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
        let attr_len = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if attr_len == 0 || pos + attr_len > end {
            break;
        }

        let non_resident = data[pos + 8] != 0;

        match attr_type {
            // $STANDARD_INFORMATION
            0x10 if !non_resident && pos + 22 <= end => {
                let c_len =
                    u32::from_le_bytes(data[pos + 16..pos + 20].try_into().unwrap()) as usize;
                let c_off =
                    u16::from_le_bytes(data[pos + 20..pos + 22].try_into().unwrap()) as usize;
                if pos + c_off + c_len <= end {
                    rec.std_info = parse_std_info(&data[pos + c_off..pos + c_off + c_len]);
                    if let Some(ref si) = rec.std_info {
                        rec.is_sparse = si.file_attributes & FILE_ATTR_SPARSE != 0;
                        rec.is_compressed = si.file_attributes & FILE_ATTR_COMPRESSED != 0;
                        rec.is_encrypted = si.file_attributes & FILE_ATTR_ENCRYPTED != 0;
                    }
                }
            }
            // $ATTRIBUTE_LIST – store for second-pass resolution
            0x20 => {
                rec.has_attr_list = true;
                if !non_resident && pos + 22 <= end {
                    let c_len =
                        u32::from_le_bytes(data[pos + 16..pos + 20].try_into().unwrap()) as usize;
                    let c_off =
                        u16::from_le_bytes(data[pos + 20..pos + 22].try_into().unwrap()) as usize;
                    if c_len > 0 && pos + c_off + c_len <= end {
                        rec.attr_list_resident =
                            Some(data[pos + c_off..pos + c_off + c_len].to_vec());
                    }
                } else if non_resident && pos + 34 <= end {
                    let rl_off =
                        u16::from_le_bytes(data[pos + 32..pos + 34].try_into().unwrap()) as usize;
                    if pos + rl_off < end {
                        rec.attr_list_runs = parse_data_runs(&data[pos + rl_off..pos + attr_len]);
                    }
                }
            }
            // $FILE_NAME
            0x30 if !non_resident && pos + 22 <= end => {
                let c_len =
                    u32::from_le_bytes(data[pos + 16..pos + 20].try_into().unwrap()) as usize;
                let c_off =
                    u16::from_le_bytes(data[pos + 20..pos + 22].try_into().unwrap()) as usize;
                if pos + c_off + c_len <= end {
                    if let Some(fn_attr) = parse_file_name(&data[pos + c_off..pos + c_off + c_len])
                    {
                        rec.file_names.push(fn_attr);
                    }
                }
            }
            // $DATA – default (unnamed) stream and named ADS
            0x80 => {
                let name_len_chars = data[pos + 9] as usize;
                if name_len_chars == 0 {
                    // Unnamed (default) data stream
                    if non_resident {
                        if pos + 64 <= end {
                            rec.data_size =
                                u64::from_le_bytes(data[pos + 56..pos + 64].try_into().unwrap());
                            let rl_off =
                                u16::from_le_bytes(data[pos + 32..pos + 34].try_into().unwrap())
                                    as usize;
                            if pos + rl_off < end {
                                rec.data_runs =
                                    parse_data_runs(&data[pos + rl_off..pos + attr_len]);
                            }
                        }
                    } else if pos + 20 <= end {
                        rec.data_size =
                            u32::from_le_bytes(data[pos + 16..pos + 20].try_into().unwrap()) as u64;
                    }
                } else if rec.ads_streams.len() < MAX_ADS_PER_FILE {
                    // Named alternate data stream
                    let name_off =
                        u16::from_le_bytes(data[pos + 10..pos + 12].try_into().unwrap()) as usize;
                    let name_byte_len = name_len_chars.min(256) * 2;
                    if pos + name_off + name_byte_len <= end {
                        let name_utf16: Vec<u16> =
                            data[pos + name_off..pos + name_off + name_byte_len]
                                .chunks_exact(2)
                                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                                .collect();
                        let stream_name = String::from_utf16_lossy(&name_utf16).to_owned();
                        let stream_size = if non_resident {
                            if pos + 0x38 + 8 <= end {
                                u64::from_le_bytes(
                                    data[pos + 0x30..pos + 0x38].try_into().unwrap(),
                                )
                            } else {
                                0
                            }
                        } else if pos + 20 <= end {
                            u32::from_le_bytes(data[pos + 16..pos + 20].try_into().unwrap()) as u64
                        } else {
                            0
                        };
                        rec.ads_streams.push(AdsEntry {
                            name: stream_name,
                            size: stream_size,
                        });
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

// ── Attribute list resolution ─────────────────────────────────────────────────

/// Parse a `$ATTRIBUTE_LIST` attribute content and return the MFT entry
/// numbers of all *extension* records (i.e., records other than `base_entry`)
/// referenced by the list.
///
/// Each entry in the attribute list has the layout:
///   +0  u32  attribute type
///   +4  u16  record length (including name, padded to 8-byte boundary)
///   +6  u8   name length (in UTF-16 code units)
///   +7  u8   name offset (from start of this entry)
///   +8  u64  starting VCN
///  +16  u64  base file reference (48-bit MFT# | 16-bit sequence)
///  +24  u16  attribute ID
fn parse_attr_list_mft_refs(data: &[u8], base_entry: u64) -> Vec<u64> {
    let mut refs: Vec<u64> = Vec::new();
    let mut pos = 0usize;
    let mut count = 0usize;

    while pos + 26 <= data.len() && count < MAX_ATTR_LIST_ENTRIES {
        let record_len = u16::from_le_bytes([data[pos + 4], data[pos + 5]]) as usize;
        if record_len < 26 || pos + record_len > data.len() {
            break;
        }
        // MFT file reference: low 48 bits = MFT entry number
        let mft_ref = u64::from_le_bytes(
            data[pos + 16..pos + 24].try_into().unwrap_or([0u8; 8]),
        ) & 0x0000_FFFF_FFFF_FFFF;
        // Skip the base record itself and the reserved NTFS meta-files (0–11)
        if mft_ref != base_entry && mft_ref > 11 {
            refs.push(mft_ref);
        }
        pos += record_len;
        count += 1;
    }

    refs.sort_unstable();
    refs.dedup();
    refs
}

// ── Chunk cache ───────────────────────────────────────────────────────────────

/// Read bytes from the OFFF chunk store, caching decompressed chunks to avoid
/// repeated decompression when reading many small regions (e.g., MFT records).
/// A FIFO eviction policy bounds resident memory to ≤ `MAX_CACHED_CHUNKS` chunks.
struct ChunkCache<'a> {
    base: &'a Path,
    chunks: &'a [ChunkMetadata],
    cache: HashMap<u64, Vec<u8>>,
    /// Insertion order for FIFO eviction.
    fifo: VecDeque<u64>,
}

impl<'a> ChunkCache<'a> {
    fn new(base: &'a Path, chunks: &'a [ChunkMetadata]) -> Self {
        Self {
            base,
            chunks,
            cache: HashMap::new(),
            fifo: VecDeque::new(),
        }
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
                // Evict oldest entry when the cache is full
                if self.cache.len() >= MAX_CACHED_CHUNKS {
                    if let Some(oldest) = self.fifo.pop_front() {
                        self.cache.remove(&oldest);
                    }
                }
                let plain = read_chunk(self.base, chunk)?;
                self.cache.insert(chunk.sequence, plain);
                self.fifo.push_back(chunk.sequence);
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
        let attr_len = u32::from_le_bytes(rec0[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if attr_len == 0 || pos + attr_len > rec0.len() {
            break;
        }

        if attr_type == 0x80 && rec0[pos + 8] != 0 {
            // Non-resident $DATA
            if pos + 64 <= rec0.len() {
                mft_allocated = u64::from_le_bytes(rec0[pos + 40..pos + 48].try_into().unwrap());
                mft_real_size = u64::from_le_bytes(rec0[pos + 56..pos + 64].try_into().unwrap());
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
        let rec = parse_mft_record(slice, rec_size);
        record_map.insert(i as u64, rec);
    }

    eprintln!("Valid FILE records: {}", record_map.len());

    // Second pass: resolve $ATTRIBUTE_LIST entries by merging attributes from
    // extension MFT records into their respective base records.
    {
        let work: Vec<(u64, Vec<u8>)> = record_map
            .iter()
            .filter(|(_, r)| r.has_attr_list)
            .filter_map(|(&id, r)| {
                if let Some(resident) = &r.attr_list_resident {
                    return Some((id, resident.clone()));
                }
                if !r.attr_list_runs.is_empty() {
                    // Read non-resident $ATTRIBUTE_LIST from the chunk store
                    let mut buf: Vec<u8> = Vec::new();
                    for run in &r.attr_list_runs {
                        if run.lcn_start < 0 {
                            continue; // sparse run
                        }
                        let offset = volume_offset + run.lcn_start as u64 * vbr.bytes_per_cluster;
                        let len = run.cluster_count * vbr.bytes_per_cluster;
                        if let Ok(data) = cache.read_at(offset, len) {
                            buf.extend_from_slice(&data);
                        }
                    }
                    if buf.is_empty() { None } else { Some((id, buf)) }
                } else {
                    None
                }
            })
            .collect();

        for (base_id, attr_list_bytes) in work {
            let ext_refs = parse_attr_list_mft_refs(&attr_list_bytes, base_id);
            let mut new_ads: Vec<AdsEntry> = Vec::new();
            let mut new_fns: Vec<FileName> = Vec::new();
            for ext_id in &ext_refs {
                if let Some(ext) = record_map.get(ext_id) {
                    for a in &ext.ads_streams {
                        new_ads.push(a.clone());
                    }
                    for f in &ext.file_names {
                        new_fns.push(f.clone());
                    }
                }
            }
            if let Some(base) = record_map.get_mut(&base_id) {
                for a in new_ads {
                    if base.ads_streams.len() < MAX_ADS_PER_FILE
                        && !base.ads_streams.iter().any(|x| x.name == a.name)
                    {
                        base.ads_streams.push(a);
                    }
                }
                for f in new_fns {
                    if !base
                        .file_names
                        .iter()
                        .any(|x| x.name == f.name && x.namespace == f.namespace)
                    {
                        base.file_names.push(f);
                    }
                }
                // Successfully resolved – clear the partial-parse marker
                base.has_attr_list = false;
            }
        }
    }


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
        let chunk_refs_json = serde_json::to_string(&chunk_ref_set).unwrap_or_else(|_| "[]".into());

        let (parser_status, parser_error) = if rec.parse_error.is_some() {
            (
                "error".to_string(),
                rec.parse_error.clone().unwrap_or_default(),
            )
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
            is_sparse: rec.is_sparse,
            is_compressed: rec.is_compressed,
            is_encrypted: rec.is_encrypted,
            ads_streams: if rec.ads_streams.is_empty() {
                "[]".to_string()
            } else {
                serde_json::to_string(
                    &rec.ads_streams
                        .iter()
                        .map(|a| json!({"name": a.name, "size": a.size}))
                        .collect::<Vec<_>>(),
                )
                .unwrap_or_else(|_| "[]".into())
            },
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

    #[test]
    fn std_info_flags_decoded() {
        // $STANDARD_INFORMATION: timestamps (4 × 8 bytes) + file_attributes at +32
        let mut content = [0u8; 48];
        // sparse (0x200) + encrypted (0x4000) = 0x4200
        let flags: u32 = 0x4200;
        content[32..36].copy_from_slice(&flags.to_le_bytes());
        let si = parse_std_info(&content).expect("should parse StdInfo");
        assert!(si.file_attributes & FILE_ATTR_SPARSE != 0, "sparse bit should be set");
        assert!(si.file_attributes & FILE_ATTR_ENCRYPTED != 0, "encrypted bit should be set");
        assert!(si.file_attributes & FILE_ATTR_COMPRESSED == 0, "compressed bit should not be set");
    }

    /// Build a synthetic 1024-byte MFT record containing two named $DATA
    /// attributes ("Zone.Identifier" and "thumb") and verify they surface as
    /// ADS entries.
    #[test]
    fn ads_streams_detected() {
        let record_size = 1024usize;
        let mut rec = vec![0u8; record_size];
        rec[0..4].copy_from_slice(b"FILE");
        // USA: offset=48, count=3 (1 USN + 2 sector entries)
        rec[4..6].copy_from_slice(&48u16.to_le_bytes());
        rec[6..8].copy_from_slice(&3u16.to_le_bytes());
        let usn: u16 = 0x1234;
        rec[48..50].copy_from_slice(&usn.to_le_bytes());
        rec[50..52].copy_from_slice(&0xAAAAu16.to_le_bytes());
        rec[52..54].copy_from_slice(&0xBBBBu16.to_le_bytes());
        rec[510..512].copy_from_slice(&usn.to_le_bytes());
        rec[1022..1024].copy_from_slice(&usn.to_le_bytes());
        // flags = in-use, first_attr_offset = 56
        rec[22..24].copy_from_slice(&1u16.to_le_bytes());
        rec[20..22].copy_from_slice(&56u16.to_le_bytes());

        let mut pos = 56usize;

        // Helper: append a resident named $DATA attribute
        let append_ads = |rec: &mut Vec<u8>, pos: &mut usize, name: &str, size: u32| {
            let name_utf16: Vec<u16> = name.encode_utf16().collect();
            let name_bytes: Vec<u8> = name_utf16.iter().flat_map(|c| c.to_le_bytes()).collect();
            let name_len_chars = name_utf16.len() as u8;
            // Header (16 bytes) + name + padding
            let hdr = 24usize; // enough room for resident fields + name_offset
            let total = (hdr + name_bytes.len() + 7) & !7usize;
            rec[*pos..*pos + 4].copy_from_slice(&0x80u32.to_le_bytes()); // $DATA
            rec[*pos + 4..*pos + 8].copy_from_slice(&(total as u32).to_le_bytes());
            rec[*pos + 8] = 0; // resident
            rec[*pos + 9] = name_len_chars;
            rec[*pos + 10..*pos + 12].copy_from_slice(&(hdr as u16).to_le_bytes()); // name_offset
            rec[*pos + 16..*pos + 20].copy_from_slice(&size.to_le_bytes()); // value_length
            let val_off = (hdr + name_bytes.len() + 7) & !7usize; // after name, aligned
            rec[*pos + 20..*pos + 22].copy_from_slice(&(val_off as u16).to_le_bytes());
            rec[*pos + hdr..*pos + hdr + name_bytes.len()].copy_from_slice(&name_bytes);
            *pos += total;
        };

        append_ads(&mut rec, &mut pos, "Zone.Identifier", 42);
        append_ads(&mut rec, &mut pos, "thumb", 100);

        // End-of-attributes marker
        rec[pos..pos + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        let end_pos = pos + 8;
        rec[24..28].copy_from_slice(&(end_pos as u32).to_le_bytes()); // real_size

        let mft_rec = parse_mft_record(&rec, record_size);
        assert_eq!(mft_rec.ads_streams.len(), 2, "expected 2 ADS entries");
        let by_name = |n: &str| {
            mft_rec
                .ads_streams
                .iter()
                .find(|a| a.name == n)
                .unwrap_or_else(|| panic!("ADS '{}' not found", n))
        };
        assert_eq!(by_name("Zone.Identifier").size, 42);
        assert_eq!(by_name("thumb").size, 100);
    }
}
