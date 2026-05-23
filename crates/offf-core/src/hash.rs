use std::io::Read;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::OfffError;

/// Magic bytes for `merkle_tree.bin`.
const MERKLE_MAGIC: &[u8; 4] = b"OFFF";
const MERKLE_VERSION: u8 = 0x01;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SiblingPosition {
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MerkleSibling {
    pub position: SiblingPosition,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MerkleProof {
    pub algorithm: String,
    pub tree_version: String,
    pub leaf_sequence: u64,
    pub leaf_hash: String,
    pub siblings: Vec<MerkleSibling>,
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedMerkleTree {
    pub leaf_count: u32,
    pub leaves: Vec<String>,
    pub root: String,
}

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

pub fn generate_merkle_proof(
    leaf_hashes: &[String],
    sequence: u64,
) -> Result<MerkleProof, OfffError> {
    if leaf_hashes.is_empty() {
        return Err(OfffError::InvalidMerkleTree(
            "cannot generate proof for empty leaf set".into(),
        ));
    }

    let idx = sequence as usize;
    if idx >= leaf_hashes.len() {
        return Err(OfffError::InvalidMerkleTree(format!(
            "leaf sequence out of range: {sequence} (leaf_count={})",
            leaf_hashes.len()
        )));
    }

    let mut siblings = Vec::new();
    let mut level: Vec<[u8; 32]> = leaf_hashes
        .iter()
        .map(|h| hex_to_bytes32(h))
        .collect::<Result<Vec<_>, _>>()?;
    let mut cur_idx = idx;

    while level.len() > 1 {
        let (sib_idx, position) = if cur_idx % 2 == 0 {
            (
                if cur_idx + 1 < level.len() {
                    cur_idx + 1
                } else {
                    cur_idx
                },
                SiblingPosition::Right,
            )
        } else {
            (cur_idx - 1, SiblingPosition::Left)
        };

        siblings.push(MerkleSibling {
            position,
            hash: hex_from_bytes32(level[sib_idx]),
        });

        let mut next: Vec<[u8; 32]> = Vec::with_capacity((level.len() + 1) / 2);
        let mut i = 0;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() {
                level[i + 1]
            } else {
                level[i]
            };
            next.push(hash_pair(left, right));
            i += 2;
        }
        level = next;
        cur_idx /= 2;
    }

    Ok(MerkleProof {
        algorithm: "sha256".to_string(),
        tree_version: "0x01".to_string(),
        leaf_sequence: sequence,
        leaf_hash: leaf_hashes[idx].clone(),
        siblings,
        root: hex_from_bytes32(level[0]),
    })
}

pub fn verify_merkle_proof(
    leaf_hash: &str,
    sequence: u64,
    proof: &MerkleProof,
    expected_root: &str,
) -> Result<bool, OfffError> {
    if proof.algorithm.to_ascii_lowercase() != "sha256" {
        return Err(OfffError::InvalidMerkleTree(format!(
            "unsupported proof algorithm: {}",
            proof.algorithm
        )));
    }
    if proof.leaf_sequence != sequence {
        return Ok(false);
    }
    if proof.leaf_hash != leaf_hash {
        return Ok(false);
    }

    let mut acc = hex_to_bytes32(leaf_hash)?;
    for sibling in &proof.siblings {
        let sib = hex_to_bytes32(&sibling.hash)?;
        acc = match sibling.position {
            SiblingPosition::Left => hash_pair(sib, acc),
            SiblingPosition::Right => hash_pair(acc, sib),
        };
    }

    let computed = hex_from_bytes32(acc);
    Ok(computed == expected_root && proof.root == expected_root)
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

pub fn parse_and_validate_merkle_tree(data: &[u8]) -> Result<ValidatedMerkleTree, OfffError> {
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

    let leaf_count = u32::from_be_bytes([data[5], data[6], data[7], data[8]]) as usize;
    if leaf_count == 0 {
        return Err(OfffError::InvalidMerkleTree(
            "leaf_count must be > 0".into(),
        ));
    }

    let mut level_sizes = Vec::new();
    let mut n = leaf_count;
    while n > 1 {
        level_sizes.push(n);
        n = (n + 1) / 2;
    }
    level_sizes.push(1);

    let nodes_count: usize = level_sizes.iter().sum();
    let expected_len = 9 + nodes_count * 32 + 32;
    if data.len() != expected_len {
        return Err(OfffError::InvalidMerkleTree(format!(
            "unexpected merkle_tree.bin length: expected {expected_len}, got {}",
            data.len()
        )));
    }

    let mut cursor = 9usize;
    let mut levels: Vec<Vec<[u8; 32]>> = Vec::with_capacity(level_sizes.len());
    for size in &level_sizes {
        let mut level = Vec::with_capacity(*size);
        for _ in 0..*size {
            let node: [u8; 32] = data[cursor..cursor + 32]
                .try_into()
                .map_err(|_| OfffError::InvalidMerkleTree("node extraction failed".into()))?;
            level.push(node);
            cursor += 32;
        }
        levels.push(level);
    }

    let root_tail: [u8; 32] = data[cursor..cursor + 32]
        .try_into()
        .map_err(|_| OfffError::InvalidMerkleTree("root tail extraction failed".into()))?;

    for i in 0..(levels.len() - 1) {
        let cur = &levels[i];
        let next = &levels[i + 1];
        if next.len() != (cur.len() + 1) / 2 {
            return Err(OfffError::InvalidMerkleTree(format!(
                "invalid level shape at level {i}: cur={}, next={}",
                cur.len(),
                next.len()
            )));
        }
        let mut j = 0usize;
        while j < cur.len() {
            let left = cur[j];
            let right = if j + 1 < cur.len() { cur[j + 1] } else { cur[j] };
            let expected = hash_pair(left, right);
            let parent_idx = j / 2;
            if next[parent_idx] != expected {
                return Err(OfffError::InvalidMerkleTree(format!(
                    "internal node mismatch at level {i}, parent index {parent_idx}"
                )));
            }
            j += 2;
        }
    }

    let root_level = levels
        .last()
        .ok_or_else(|| OfffError::InvalidMerkleTree("missing root level".into()))?;
    if root_level.len() != 1 {
        return Err(OfffError::InvalidMerkleTree(
            "invalid root level width".into(),
        ));
    }
    if root_level[0] != root_tail {
        return Err(OfffError::InvalidMerkleTree(
            "root tail mismatch".into(),
        ));
    }

    let leaves_hex = levels[0].iter().copied().map(hex_from_bytes32).collect();
    Ok(ValidatedMerkleTree {
        leaf_count: leaf_count as u32,
        leaves: leaves_hex,
        root: hex_from_bytes32(root_tail),
    })
}

/// Deserialise and return the root hash from a `merkle_tree.bin` blob.
pub fn deserialize_merkle_root(data: &[u8]) -> Result<String, OfffError> {
    Ok(parse_and_validate_merkle_tree(data)?.root)
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
    fn merkle_proof_roundtrip_for_all_leaves() {
        let leaves: Vec<String> = (0u8..7)
            .map(|i| format!("{:064x}", i as u64))
            .collect();
        let root = merkle_root(&leaves).unwrap();
        for (seq, leaf) in leaves.iter().enumerate() {
            let proof = generate_merkle_proof(&leaves, seq as u64).unwrap();
            let ok = verify_merkle_proof(leaf, seq as u64, &proof, &root).unwrap();
            assert!(ok);
        }
    }

    #[test]
    fn merkle_proof_fails_on_modified_leaf() {
        let leaves: Vec<String> = (0u8..4)
            .map(|i| format!("{:064x}", i as u64))
            .collect();
        let root = merkle_root(&leaves).unwrap();
        let proof = generate_merkle_proof(&leaves, 2).unwrap();
        let bad_leaf = "f".repeat(64);
        let ok = verify_merkle_proof(&bad_leaf, 2, &proof, &root).unwrap();
        assert!(!ok);
    }

    #[test]
    fn merkle_proof_fails_on_modified_sibling() {
        let leaves: Vec<String> = (0u8..5)
            .map(|i| format!("{:064x}", i as u64))
            .collect();
        let root = merkle_root(&leaves).unwrap();
        let mut proof = generate_merkle_proof(&leaves, 1).unwrap();
        proof.siblings[0].hash = "a".repeat(64);
        let ok = verify_merkle_proof(&leaves[1], 1, &proof, &root).unwrap();
        assert!(!ok);
    }

    #[test]
    fn parse_and_validate_merkle_tree_detects_internal_corruption() {
        let leaves: Vec<String> = (0u8..3)
            .map(|i| format!("{:064x}", i as u64))
            .collect();
        let mut blob = serialize_merkle_tree(&leaves).unwrap();
        // Corrupt first internal node (after header + leaf level).
        let off = 9 + leaves.len() * 32;
        blob[off] ^= 0x01;
        let err = parse_and_validate_merkle_tree(&blob).unwrap_err();
        assert!(matches!(err, OfffError::InvalidMerkleTree(_)));
    }

    #[test]
    fn parse_and_validate_merkle_tree_detects_leaf_count_mismatch() {
        let leaves: Vec<String> = (0u8..4)
            .map(|i| format!("{:064x}", i as u64))
            .collect();
        let mut blob = serialize_merkle_tree(&leaves).unwrap();
        // Set leaf_count to 5 while payload still encodes 4 leaves.
        blob[5..9].copy_from_slice(&(5u32.to_be_bytes()));
        let err = parse_and_validate_merkle_tree(&blob).unwrap_err();
        assert!(matches!(err, OfffError::InvalidMerkleTree(_)));
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
