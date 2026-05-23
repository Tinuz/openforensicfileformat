use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::OfffError;

const HEADER_MAGIC: [u8; 4] = *b"OFPK";
const VERSION: u32 = 1;
const FOOTER_MAGIC: [u8; 8] = *b"OFPKIDX1";
const HEADER_LEN: u64 = 8;
const FOOTER_LEN: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackedEntry {
    pub path: String,
    pub offset: u64,
    pub length: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackedIndex {
    pub format: String,
    pub version: u32,
    pub entries: Vec<PackedEntry>,
}

pub fn pack_directory(input_dir: &Path, output_file: &Path) -> Result<PackedIndex, OfffError> {
    if !input_dir.exists() || !input_dir.is_dir() {
        return Err(OfffError::InvalidContainer(format!(
            "input directory does not exist: {}",
            input_dir.display()
        )));
    }

    let mut files = Vec::new();
    collect_files_recursive(input_dir, input_dir, &mut files)?;
    files.sort();

    let mut out = fs::File::create(output_file)?;
    out.write_all(&HEADER_MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;

    let mut cursor = HEADER_LEN;
    let mut entries = Vec::with_capacity(files.len());

    for rel in files {
        let src = input_dir.join(&rel);
        let bytes = fs::read(&src)?;
        let hash = sha256_hex(&bytes);
        out.write_all(&bytes)?;

        let path = rel.to_string_lossy().replace('\\', "/");
        entries.push(PackedEntry {
            path,
            offset: cursor,
            length: bytes.len() as u64,
            sha256: hash,
        });
        cursor += bytes.len() as u64;
    }

    let index = PackedIndex {
        format: "offf-packed".to_string(),
        version: VERSION,
        entries,
    };

    let index_bytes = serde_json::to_vec(&index)?;
    let index_hash = sha256_digest(&index_bytes);
    let index_offset = cursor;
    let index_len = index_bytes.len() as u64;

    out.write_all(&index_bytes)?;
    out.write_all(&FOOTER_MAGIC)?;
    out.write_all(&index_offset.to_le_bytes())?;
    out.write_all(&index_len.to_le_bytes())?;
    out.write_all(&index_hash)?;
    out.write_all(&[0u8; 8])?;
    out.flush()?;

    Ok(index)
}

pub fn read_index(container_file: &Path) -> Result<PackedIndex, OfffError> {
    let mut file = fs::File::open(container_file)?;
    let len = file.metadata()?.len();
    if len < HEADER_LEN + FOOTER_LEN as u64 {
        return Err(OfffError::InvalidContainer(
            "packed file too small".to_string(),
        ));
    }

    let mut header = [0u8; 8];
    file.read_exact(&mut header)?;
    if header[0..4] != HEADER_MAGIC {
        return Err(OfffError::InvalidContainer(
            "invalid packed header magic".to_string(),
        ));
    }
    let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    if version != VERSION {
        return Err(OfffError::UnsupportedVersion(format!(
            "packed version {version}"
        )));
    }

    file.seek(SeekFrom::End(-(FOOTER_LEN as i64)))?;
    let mut footer = [0u8; FOOTER_LEN];
    file.read_exact(&mut footer)?;

    if footer[0..8] != FOOTER_MAGIC {
        return Err(OfffError::InvalidContainer(
            "invalid packed footer magic".to_string(),
        ));
    }

    let index_offset = u64::from_le_bytes(footer[8..16].try_into().unwrap_or([0u8; 8]));
    let index_len = u64::from_le_bytes(footer[16..24].try_into().unwrap_or([0u8; 8]));
    let index_hash: [u8; 32] = footer[24..56].try_into().unwrap_or([0u8; 32]);

    if index_offset + index_len > len - FOOTER_LEN as u64 {
        return Err(OfffError::InvalidContainer(
            "index range is outside packed file".to_string(),
        ));
    }

    file.seek(SeekFrom::Start(index_offset))?;
    let mut index_bytes = vec![0u8; index_len as usize];
    file.read_exact(&mut index_bytes)?;
    if sha256_digest(&index_bytes) != index_hash {
        return Err(OfffError::InvalidContainer(
            "index checksum mismatch".to_string(),
        ));
    }

    let index: PackedIndex = serde_json::from_slice(&index_bytes)?;
    Ok(index)
}

pub fn unpack_to_directory(container_file: &Path, output_dir: &Path) -> Result<PackedIndex, OfffError> {
    let index = read_index(container_file)?;
    let mut file = fs::File::open(container_file)?;

    for entry in &index.entries {
        let rel = safe_rel_path(&entry.path)?;
        let target = output_dir.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }

        file.seek(SeekFrom::Start(entry.offset))?;
        let mut bytes = vec![0u8; entry.length as usize];
        file.read_exact(&mut bytes)?;

        let actual = sha256_hex(&bytes);
        if actual != entry.sha256 {
            return Err(OfffError::InvalidContainer(format!(
                "entry hash mismatch for {}",
                entry.path
            )));
        }

        fs::write(target, bytes)?;
    }

    Ok(index)
}

fn safe_rel_path(path: &str) -> Result<PathBuf, OfffError> {
    let clean = path.replace('\\', "/").trim_start_matches('/').to_string();
    if clean.contains("..") {
        return Err(OfffError::InvalidContainer(format!(
            "path traversal not allowed in packed entry: {path}"
        )));
    }
    Ok(PathBuf::from(clean))
}

fn collect_files_recursive(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), OfffError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_files_recursive(root, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map_err(|_| OfffError::InvalidContainer("failed to relativize file path".to_string()))?
                .to_path_buf();
            out.push(rel);
        }
    }
    Ok(())
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn sha256_digest(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn pack_list_unpack_roundtrip() {
        let tmp = tempdir().unwrap();
        let input = tmp.path().join("input.offf");
        let output = tmp.path().join("output.offf");
        let packed = tmp.path().join("case.offfpack");

        fs::create_dir_all(input.join("analysis")).unwrap();
        fs::create_dir_all(input.join("provenance")).unwrap();
        fs::write(input.join("manifest.json"), "{\"offf_version\":\"0.1.0\"}").unwrap();
        fs::write(input.join("analysis").join("a.jsonl"), "{\"k\":1}\n").unwrap();
        fs::write(input.join("provenance").join("chain_of_custody.jsonl"), "").unwrap();

        let idx = pack_directory(&input, &packed).unwrap();
        assert!(!idx.entries.is_empty());

        let listed = read_index(&packed).unwrap();
        assert_eq!(idx.entries.len(), listed.entries.len());

        unpack_to_directory(&packed, &output).unwrap();
        let a = fs::read(input.join("manifest.json")).unwrap();
        let b = fs::read(output.join("manifest.json")).unwrap();
        assert_eq!(a, b);

        let a = fs::read(input.join("analysis").join("a.jsonl")).unwrap();
        let b = fs::read(output.join("analysis").join("a.jsonl")).unwrap();
        assert_eq!(a, b);
    }
}
