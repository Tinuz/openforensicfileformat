use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::{
    error::OfffError,
    types::{ChunkMetadata, Compression},
};

/// Derive the storage path for a chunk from its hex-encoded SHA-256 ID.
///
/// Layout: `<base>/chunks/sha256/<ab>/<cd>/<full_hash>.chunk`
pub fn chunk_path(base: &Path, hex_id: &str) -> PathBuf {
    // strip the "sha256:" prefix if present
    let hex = hex_id.strip_prefix("sha256:").unwrap_or(hex_id);
    base.join("chunks")
        .join("sha256")
        .join(&hex[..2])
        .join(&hex[2..4])
        .join(format!("{hex}.chunk"))
}

/// Write a single chunk to the container and return its metadata.
///
/// The chunk is identified solely by the SHA-256 of its plaintext bytes
/// (content-addressed storage).  If the chunk already exists on disk it is
/// not written again – the existing file is assumed to be correct (identical
/// content → identical hash).
pub fn write_chunk(
    base: &Path,
    sequence: u64,
    source_offset: u64,
    plaintext: &[u8],
    compression: &Compression,
) -> Result<ChunkMetadata, OfffError> {
    // 1. Hash plaintext
    let plaintext_hash = hex_sha256(plaintext);
    let chunk_id = format!("sha256:{plaintext_hash}");

    // 2. Compress
    let stored_bytes: Vec<u8> = match compression {
        Compression::None => plaintext.to_vec(),
        Compression::Zstd => {
            zstd::encode_all(plaintext, 3).map_err(|e| OfffError::DecompressionError {
                chunk_id: chunk_id.clone(),
                msg: e.to_string(),
            })?
        }
    };

    // 3. Hash stored bytes
    let stored_hash = hex_sha256(&stored_bytes);

    // 4. Write (skip only after validating existing chunk bytes)
    let path = chunk_path(base, &plaintext_hash);
    if !path.exists() {
        fs::create_dir_all(path.parent().unwrap())?;
        let mut f = fs::File::create(&path)?;
        f.write_all(&stored_bytes)?;
        f.sync_all()?;
    } else {
        // Existing chunk must cryptographically match before dedup skip.
        let existing_stored = fs::read(&path)?;
        let existing_stored_hash = hex_sha256(&existing_stored);
        if existing_stored_hash != stored_hash {
            return Err(OfffError::HashMismatch {
                chunk_id: chunk_id.clone(),
                expected: stored_hash,
                actual: existing_stored_hash,
            });
        }

        let existing_plain = match compression {
            Compression::None => existing_stored,
            Compression::Zstd => {
                let mut decoder = zstd::Decoder::new(existing_stored.as_slice()).map_err(|e| {
                    OfffError::DecompressionError {
                        chunk_id: chunk_id.clone(),
                        msg: e.to_string(),
                    }
                })?;
                let mut buf = Vec::with_capacity(plaintext.len());
                decoder
                    .read_to_end(&mut buf)
                    .map_err(|e| OfffError::DecompressionError {
                        chunk_id: chunk_id.clone(),
                        msg: e.to_string(),
                    })?;
                buf
            }
        };

        let existing_plain_hash = hex_sha256(&existing_plain);
        if existing_plain_hash != plaintext_hash {
            return Err(OfffError::HashMismatch {
                chunk_id: chunk_id.clone(),
                expected: plaintext_hash,
                actual: existing_plain_hash,
            });
        }
    }

    Ok(ChunkMetadata {
        sequence,
        chunk_id,
        source_offset,
        source_length: plaintext.len() as u64,
        stored_length: stored_bytes.len() as u64,
        compression: compression.as_str().to_string(),
        plaintext_sha256: plaintext_hash,
        stored_sha256: stored_hash,
        read_errors: vec![],
    })
}

/// Read, decompress and verify a chunk.  Returns the plaintext bytes.
pub fn read_chunk(base: &Path, meta: &ChunkMetadata) -> Result<Vec<u8>, OfffError> {
    let path = chunk_path(base, &meta.plaintext_sha256);

    if !path.exists() {
        return Err(OfffError::ChunkNotFound {
            chunk_id: meta.chunk_id.clone(),
        });
    }

    // Read stored bytes
    let stored_bytes = fs::read(&path)?;

    // Verify stored hash
    let actual_stored = hex_sha256(&stored_bytes);
    if actual_stored != meta.stored_sha256 {
        return Err(OfffError::HashMismatch {
            chunk_id: meta.chunk_id.clone(),
            expected: meta.stored_sha256.clone(),
            actual: actual_stored,
        });
    }

    // Decompress
    let plaintext: Vec<u8> = match meta.compression.as_str() {
        "none" => stored_bytes,
        "zstd" => {
            let mut decoder = zstd::Decoder::new(stored_bytes.as_slice()).map_err(|e| {
                OfffError::DecompressionError {
                    chunk_id: meta.chunk_id.clone(),
                    msg: e.to_string(),
                }
            })?;
            let mut buf = Vec::with_capacity(meta.source_length as usize);
            decoder
                .read_to_end(&mut buf)
                .map_err(|e| OfffError::DecompressionError {
                    chunk_id: meta.chunk_id.clone(),
                    msg: e.to_string(),
                })?;
            buf
        }
        other => {
            return Err(OfffError::InvalidManifest(format!(
                "unknown compression '{other}'"
            )))
        }
    };

    // Verify plaintext hash
    let actual_plain = hex_sha256(&plaintext);
    if actual_plain != meta.plaintext_sha256 {
        return Err(OfffError::HashMismatch {
            chunk_id: meta.chunk_id.clone(),
            expected: meta.plaintext_sha256.clone(),
            actual: actual_plain,
        });
    }

    Ok(plaintext)
}

/// Verify a stored chunk without returning its bytes (cheaper for audit).
pub fn verify_chunk(base: &Path, meta: &ChunkMetadata) -> Result<(), OfffError> {
    let path = chunk_path(base, &meta.plaintext_sha256);

    if !path.exists() {
        return Err(OfffError::ChunkNotFound {
            chunk_id: meta.chunk_id.clone(),
        });
    }

    let stored_bytes = fs::read(&path)?;

    // Check stored hash
    let actual_stored = hex_sha256(&stored_bytes);
    if actual_stored != meta.stored_sha256 {
        return Err(OfffError::HashMismatch {
            chunk_id: meta.chunk_id.clone(),
            expected: meta.stored_sha256.clone(),
            actual: actual_stored,
        });
    }

    // Decompress and check plaintext hash
    let plaintext: Vec<u8> = match meta.compression.as_str() {
        "none" => stored_bytes,
        "zstd" => {
            let mut decoder = zstd::Decoder::new(stored_bytes.as_slice()).map_err(|e| {
                OfffError::DecompressionError {
                    chunk_id: meta.chunk_id.clone(),
                    msg: e.to_string(),
                }
            })?;
            let mut buf = Vec::with_capacity(meta.source_length as usize);
            decoder
                .read_to_end(&mut buf)
                .map_err(|e| OfffError::DecompressionError {
                    chunk_id: meta.chunk_id.clone(),
                    msg: e.to_string(),
                })?;
            buf
        }
        other => {
            return Err(OfffError::InvalidManifest(format!(
                "unknown compression '{other}'"
            )))
        }
    };

    let actual_plain = hex_sha256(&plaintext);
    if actual_plain != meta.plaintext_sha256 {
        return Err(OfffError::HashMismatch {
            chunk_id: meta.chunk_id.clone(),
            expected: meta.plaintext_sha256.clone(),
            actual: actual_plain,
        });
    }

    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

pub fn hex_sha256(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trip_zstd() {
        let dir = tempdir().unwrap();
        let data: Vec<u8> = (0u8..=255).cycle().take(1024).collect();

        let meta = write_chunk(dir.path(), 0, 0, &data, &Compression::Zstd).unwrap();
        assert_eq!(meta.sequence, 0);
        assert!(meta.stored_length < meta.source_length); // compressed

        let back = read_chunk(dir.path(), &meta).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn round_trip_none() {
        let dir = tempdir().unwrap();
        let data = b"hello offf world";

        let meta = write_chunk(dir.path(), 0, 0, data, &Compression::None).unwrap();
        assert_eq!(meta.stored_length, data.len() as u64);

        let back = read_chunk(dir.path(), &meta).unwrap();
        assert_eq!(back.as_slice(), data);
    }

    #[test]
    fn detects_stored_corruption() {
        let dir = tempdir().unwrap();
        let data: Vec<u8> = vec![0xAB; 512];

        let meta = write_chunk(dir.path(), 0, 0, &data, &Compression::None).unwrap();
        let path = chunk_path(dir.path(), &meta.plaintext_sha256);

        // Corrupt one byte
        let mut bytes = fs::read(&path).unwrap();
        bytes[0] ^= 0xFF;
        fs::write(&path, bytes).unwrap();

        let err = verify_chunk(dir.path(), &meta).unwrap_err();
        assert!(matches!(err, OfffError::HashMismatch { .. }));
    }

    #[test]
    fn write_chunk_reuses_valid_existing_chunk() {
        let dir = tempdir().unwrap();
        let data: Vec<u8> = vec![0x42; 4096];

        let meta1 = write_chunk(dir.path(), 0, 0, &data, &Compression::None).unwrap();
        let meta2 = write_chunk(dir.path(), 1, 4096, &data, &Compression::None).unwrap();

        assert_eq!(meta1.chunk_id, meta2.chunk_id);
        assert_eq!(meta1.plaintext_sha256, meta2.plaintext_sha256);

        // Existing file should still verify and contain original bytes.
        let back = read_chunk(dir.path(), &meta2).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn write_chunk_fails_on_corrupt_existing_chunk() {
        let dir = tempdir().unwrap();
        let data: Vec<u8> = vec![0xCD; 2048];

        let meta = write_chunk(dir.path(), 0, 0, &data, &Compression::None).unwrap();
        let path = chunk_path(dir.path(), &meta.plaintext_sha256);

        // Corrupt existing stored bytes before second write attempt.
        let mut bytes = fs::read(&path).unwrap();
        bytes[5] ^= 0xFF;
        fs::write(&path, bytes).unwrap();

        let err = write_chunk(dir.path(), 1, 2048, &data, &Compression::None).unwrap_err();
        assert!(matches!(err, OfffError::HashMismatch { .. }));
    }
}
