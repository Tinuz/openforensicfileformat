use std::io::Read;

use sha2::{Digest, Sha256};

use crate::error::OfffError;

/// Magic bytes for `merkle_tree.bin`.
const MERKLE_MAGIC: &[u8; 4] = b"OFFF";
const MERKLE_VERSION: u8 = 0x01;

// ── Streaming source hash ─────────────────────────────────────────────────────

/// Compute the SHA-256 of every byte produced by `reader` in a streaming
/// fashion.  Returns the lowercase hex digest.
pub fn stream_sha256(reader: &mut impl Read) -> Result<String, OfffError> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 8 * 1024 * 1024]; // 8 MiB read buffer
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

// ── Merkle tree ───────────────────────────────────────────────────────────────

/// Compute the Merkle root of a slice of leaf hashes (lowercase hex SHA-256).
///
/// - Leaves are the `plaintext_sha256` values of each chunk, **in sequence order**.
/// - Odd leaves are duplicated (paired with themselves) before hashing.
/// - Each parent = SHA-256 of `left_hash_bytes || right_hash_bytes`.
/// - Returns the root as a lowercase hex string.
///
/// A single-leaf tree returns that leaf unchanged (no further hashing needed
/// because there is only one "pair").
pub fn merkle_root(leaf_hashes: &[String]) -> Result<String, OfffError> {
    if leaf_hashes.is_empty() {
        return Err(OfffError::InvalidMerkleTree(
            "cannot build Merkle tree from empty leaf set".into(),
        ));
    }

    let leaves: Result<Vec<[u8; 32]>, OfffError> = leaf_hashes
        .iter()
        .map(|h| hex_to_bytes32(h))
        .collect();
    let leaves = leaves?;

    Ok(hex_from_bytes32(merkle_root_from_bytes(&leaves)))
}

fn merkle_root_from_bytes(nodes: &[[u8; 32]]) -> [u8; 32] {
    if nodes.len() == 1 {
        return nodes[0];
    }

    let mut next: Vec<[u8; 32]> = Vec::with_capacity((nodes.len() + 1) / 2);
    let mut i = 0;
    while i < nodes.len() {
        let left = nodes[i];
        let right = if i + 1 < nodes.len() {
            nodes[i + 1]
        } else {
            nodes[i] // duplicate odd leaf
        };
        next.push(hash_pair(left, right));
        i += 2;
    }
    merkle_root_from_bytes(&next)
}

fn hash_pair(left: [u8; 32], right: [u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(left);
    h.update(right);
    h.finalize().into()
}

// ── Binary serialisation ──────────────────────────────────────────────────────

/// Serialise the full Merkle tree (all levels, leaf-to-root) to binary.
///
/// Format:
/// ```text
/// [4]  magic "OFFF"
/// [1]  version 0x01
/// [4]  leaf_count (u32 big-endian)
/// [N*32]  leaf hashes in sequence order
/// [M*32]  internal node hashes, level by level (leaves → root)
///         each level listed left-to-right; odd node duplicated
/// [32] root hash (repeated for easy extraction)
/// ```
pub fn serialize_merkle_tree(leaf_hashes: &[String]) -> Result<Vec<u8>, OfffError> {
    if leaf_hashes.is_empty() {
        return Err(OfffError::InvalidMerkleTree(
            "empty leaf set".into(),
        ));
    }

    let leaves: Result<Vec<[u8; 32]>, OfffError> = leaf_hashes
        .iter()
        .map(|h| hex_to_bytes32(h))
        .collect();
    let leaves = leaves?;

    // Build all levels
    let mut levels: Vec<Vec<[u8; 32]>> = vec![leaves.clone()];
    loop {
        let cur = levels.last().unwrap();
        if cur.len() == 1 {
            break;
        }
        let mut next: Vec<[u8; 32]> = Vec::with_capacity((cur.len() + 1) / 2);
        let mut i = 0;
        while i < cur.len() {
            let left = cur[i];
            let right = if i + 1 < cur.len() { cur[i + 1] } else { cur[i] };
            next.push(hash_pair(left, right));
            i += 2;
        }
        levels.push(next);
    }

    let root = *levels.last().unwrap().first().unwrap();

    let mut out = Vec::new();
    out.extend_from_slice(MERKLE_MAGIC);
    out.push(MERKLE_VERSION);
    out.extend_from_slice(&(leaves.len() as u32).to_be_bytes());

    // All levels (leaf level is levels[0])
    for level in &levels {
        for node in level {
            out.extend_from_slice(node);
        }
    }
    // Root repeated at end for quick extraction
    out.extend_from_slice(&root);

    Ok(out)
}

/// Deserialise and return the root hash from a `merkle_tree.bin` blob.
pub fn deserialize_merkle_root(data: &[u8]) -> Result<String, OfffError> {
    if data.len() < 4 + 1 + 4 + 32 {
        return Err(OfffError::InvalidMerkleTree("data too short".into()));
    }

    if &data[..4] != MERKLE_MAGIC {
        return Err(OfffError::InvalidMerkleTree("bad magic".into()));
    }

    if data[4] != MERKLE_VERSION {
        return Err(OfffError::InvalidMerkleTree(format!(
            "unsupported version {}",
            data[4]
        )));
    }

    // Root is the last 32 bytes
    let root_bytes: [u8; 32] = data[data.len() - 32..]
        .try_into()
        .map_err(|_| OfffError::InvalidMerkleTree("root extraction failed".into()))?;

    Ok(hex_from_bytes32(root_bytes))
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn hex_to_bytes32(hex: &str) -> Result<[u8; 32], OfffError> {
    if hex.len() != 64 {
        return Err(OfffError::InvalidMerkleTree(format!(
            "expected 64-char hex, got {} chars",
            hex.len()
        )));
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| {
            OfffError::InvalidMerkleTree(format!("invalid hex at position {}", i * 2))
        })?;
    }
    Ok(out)
}

fn hex_from_bytes32(b: [u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_leaf_root_is_leaf() {
        let leaf = "a".repeat(64);
        let root = merkle_root(&[leaf.clone()]).unwrap();
        assert_eq!(root, leaf);
    }

    #[test]
    fn two_leaves_root_is_hash_of_pair() {
        let l1 = "00".repeat(32);
        let l2 = "ff".repeat(32);
        let root = merkle_root(&[l1.clone(), l2.clone()]).unwrap();

        let b1 = hex_to_bytes32(&l1).unwrap();
        let b2 = hex_to_bytes32(&l2).unwrap();
        let expected = hex_from_bytes32(hash_pair(b1, b2));
        assert_eq!(root, expected);
    }

    #[test]
    fn odd_leaves_are_duplicated() {
        let leaves: Vec<String> = (0u8..3)
            .map(|i| format!("{:064x}", i as u64))
            .collect();
        // Should not panic
        let root = merkle_root(&leaves).unwrap();
        assert_eq!(root.len(), 64);
    }

    #[test]
    fn serialize_deserialize_root() {
        let leaves: Vec<String> = (0u8..5)
            .map(|i| format!("{:064x}", i as u64))
            .collect();
        let root_direct = merkle_root(&leaves).unwrap();
        let blob = serialize_merkle_tree(&leaves).unwrap();
        let root_from_bin = deserialize_merkle_root(&blob).unwrap();
        assert_eq!(root_direct, root_from_bin);
    }

    #[test]
    fn stream_sha256_correct() {
        let data = b"open forensic file format";
        let mut cursor = std::io::Cursor::new(data);
        let result = stream_sha256(&mut cursor).unwrap();
        // Compare with sha2 direct computation
        let mut h = sha2::Sha256::new();
        sha2::Digest::update(&mut h, data);
        let expected = format!("{:x}", h.finalize());
        assert_eq!(result, expected);
    }
}
