/// Content-addressed evidence object store for `file_collection` containers.
///
/// Layout: `{container}/evidence/objects/sha256/{ab}/{cd}/{hex}.bin`
///
/// Objects are whole files stored verbatim (no compression, no framing).
/// Deduplication is achieved by checking for an existing file before writing.
/// Every read operation re-verifies the SHA-256 of the stored content.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::error::OfffError;

// ── Path helpers ──────────────────────────────────────────────────────────────

/// Return the storage path for an evidence object given its hex SHA-256.
///
/// The path follows the same two-level shard pattern as chunks:
/// `{base}/evidence/objects/sha256/{ab}/{cd}/{hex}.bin`
pub fn evidence_object_path(base: &Path, sha256_hex: &str) -> PathBuf {
    let sha256_hex = sha256_hex.strip_prefix("sha256:").unwrap_or(sha256_hex);
    assert!(
        sha256_hex.len() >= 4,
        "sha256_hex too short: {sha256_hex}"
    );
    let ab = &sha256_hex[..2];
    let cd = &sha256_hex[2..4];
    base.join("evidence")
        .join("objects")
        .join("sha256")
        .join(ab)
        .join(cd)
        .join(format!("{sha256_hex}.bin"))
}

// ── Write ─────────────────────────────────────────────────────────────────────

/// Store `content` as a content-addressed evidence object.
///
/// Returns the hex-encoded SHA-256 of the content.
/// If the object already exists (dedup), the hash is verified to match before
/// returning — any mismatch indicates filesystem corruption and is an error.
pub fn write_evidence_object(base: &Path, content: &[u8]) -> Result<String, OfffError> {
    let sha256_hex = hex_sha256(content);
    let path = evidence_object_path(base, &sha256_hex);

    if path.exists() {
        // Dedup: verify integrity of the existing file, then return.
        verify_evidence_object(base, &sha256_hex)?;
        return Ok(sha256_hex);
    }

    // Create parent directory structure.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write atomically via a temp file in the same directory.
    let tmp_path = path.with_extension("bin.tmp");
    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(content)?;
        f.flush()?;
    }
    fs::rename(&tmp_path, &path)?;

    Ok(sha256_hex)
}

// ── Read ──────────────────────────────────────────────────────────────────────

/// Read and verify an evidence object by its hex SHA-256 reference.
///
/// Returns the raw bytes. Returns an error if the file is missing or if the
/// SHA-256 of the stored bytes does not match `sha256_ref`.
pub fn read_evidence_object(base: &Path, sha256_ref: &str) -> Result<Vec<u8>, OfffError> {
    let path = evidence_object_path(base, sha256_ref);
    let expected = sha256_ref.strip_prefix("sha256:").unwrap_or(sha256_ref);
    let content = fs::read(&path).map_err(|e| {
        OfffError::Io(std::io::Error::new(
            e.kind(),
            format!(
                "evidence object not found: {} ({})",
                sha256_ref,
                path.display()
            ),
        ))
    })?;

    let computed = hex_sha256(&content);
    if computed != expected {
        return Err(OfffError::HashMismatch {
            chunk_id: sha256_ref.to_string(),
            expected: expected.to_string(),
            actual: computed,
        });
    }

    Ok(content)
}

// ── Verify ────────────────────────────────────────────────────────────────────

/// Verify that the stored evidence object at `sha256_ref` has not been tampered
/// with. Returns `Ok(())` if the file exists and its hash matches.
pub fn verify_evidence_object(base: &Path, sha256_ref: &str) -> Result<(), OfffError> {
    read_evidence_object(base, sha256_ref).map(|_| ())
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn hex_sha256(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn evidence_object_roundtrip() {
        let dir = tempdir().unwrap();
        let content = b"hello OFFF evidence store";
        let sha256 = write_evidence_object(dir.path(), content).unwrap();
        assert_eq!(sha256.len(), 64);

        let read_back = read_evidence_object(dir.path(), &sha256).unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn evidence_object_dedup() {
        let dir = tempdir().unwrap();
        let content = b"duplicate content";
        let sha1 = write_evidence_object(dir.path(), content).unwrap();
        let sha2 = write_evidence_object(dir.path(), content).unwrap();
        assert_eq!(sha1, sha2);

        let path = evidence_object_path(dir.path(), &sha1);
        let entries: Vec<_> = path.parent().unwrap().read_dir().unwrap().collect();
        // Only one file should exist (no .tmp remnants, no duplicate).
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn evidence_object_tamper_detection() {
        let dir = tempdir().unwrap();
        let content = b"sensitive evidence";
        let sha256 = write_evidence_object(dir.path(), content).unwrap();

        // Overwrite the stored file with garbage.
        let path = evidence_object_path(dir.path(), &sha256);
        fs::write(&path, b"tampered!").unwrap();

        let err = read_evidence_object(dir.path(), &sha256).unwrap_err();
        assert!(
            matches!(err, OfffError::HashMismatch { .. }),
            "expected HashMismatch, got: {err:?}"
        );
    }

    #[test]
    fn evidence_object_path_layout() {
        let base = Path::new("/container");
        let sha = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let p = evidence_object_path(base, sha);
        assert_eq!(
            p,
            Path::new("/container/evidence/objects/sha256/ab/cd/abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890.bin")
        );
    }
}
