use thiserror::Error;

#[derive(Debug, Error)]
pub enum OfffError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),

    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("Hash mismatch for chunk {chunk_id}: expected {expected}, got {actual}")]
    HashMismatch {
        chunk_id: String,
        expected: String,
        actual: String,
    },

    #[error("Chunk not found: {chunk_id}")]
    ChunkNotFound { chunk_id: String },

    #[error("Decompression error for chunk {chunk_id}: {msg}")]
    DecompressionError { chunk_id: String, msg: String },

    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    #[error("Invalid container: {0}")]
    InvalidContainer(String),

    #[error("Unsupported OFFF version: {0}")]
    UnsupportedVersion(String),

    #[error("Merkle root mismatch: expected {expected}, got {actual}")]
    MerkleRootMismatch { expected: String, actual: String },

    #[error("Source hash mismatch: expected {expected}, got {actual}")]
    SourceHashMismatch { expected: String, actual: String },

    #[error("Invalid merkle tree binary data: {0}")]
    InvalidMerkleTree(String),
}
