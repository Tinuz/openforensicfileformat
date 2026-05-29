use std::{fs, path::PathBuf};

use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::{primitives::ByteStream, Client};
use serde::Deserialize;

use crate::{
    chunk::hex_sha256,
    error::OfffError,
    evidence,
    parquet_io::{read_file_index, read_object_index, read_physical_to_chunk},
    partition::read_bytes_at,
    types::{ChunkMetadata, DiscoveredObjectRow},
};

#[derive(Debug, Clone)]
pub enum ContainerRef {
    Local(PathBuf),
    S3 { bucket: String, prefix: String },
}

impl ContainerRef {
    pub fn parse(input: &str) -> Result<Self, OfffError> {
        if let Some(rest) = input.strip_prefix("s3://") {
            let mut parts = rest.splitn(2, '/');
            let bucket = parts.next().unwrap_or_default().to_string();
            let prefix = parts
                .next()
                .unwrap_or_default()
                .trim_matches('/')
                .to_string();
            if bucket.is_empty() {
                return Err(OfffError::InvalidContainer(
                    "missing S3 bucket in URI".to_string(),
                ));
            }
            Ok(Self::S3 { bucket, prefix })
        } else {
            Ok(Self::Local(PathBuf::from(input)))
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Local(p) => p.display().to_string(),
            Self::S3 { bucket, prefix } => {
                if prefix.is_empty() {
                    format!("s3://{bucket}")
                } else {
                    format!("s3://{bucket}/{prefix}")
                }
            }
        }
    }

    pub fn local_path(&self, rel: &str) -> Option<PathBuf> {
        match self {
            Self::Local(base) => Some(base.join(rel)),
            Self::S3 { .. } => None,
        }
    }

    pub fn read_text(&self, rel: &str) -> Result<String, OfffError> {
        let bytes = self.read_bytes(rel)?;
        String::from_utf8(bytes)
            .map_err(|e| OfffError::InvalidContainer(format!("{rel} is not UTF-8: {e}")))
    }

    pub fn read_bytes(&self, rel: &str) -> Result<Vec<u8>, OfffError> {
        match self {
            Self::Local(base) => Ok(fs::read(base.join(rel))?),
            Self::S3 { bucket, prefix } => {
                let client = s3_client()?;
                let key = join_key(prefix, rel);
                let rt = tokio::runtime::Runtime::new().map_err(|e| {
                    OfffError::InvalidContainer(format!("failed to start async runtime: {e}"))
                })?;
                let out = rt
                    .block_on(async { client.get_object().bucket(bucket).key(&key).send().await });
                let out = out.map_err(|e| {
                    OfffError::InvalidContainer(format!("failed to read s3://{bucket}/{key}: {e}"))
                })?;
                let data = rt
                    .block_on(async { out.body.collect().await })
                    .map_err(|e| {
                        OfffError::InvalidContainer(format!(
                            "failed to download s3://{bucket}/{key}: {e}"
                        ))
                    })?;
                Ok(data.into_bytes().to_vec())
            }
        }
    }

    pub fn write_bytes(&self, rel: &str, data: &[u8]) -> Result<(), OfffError> {
        match self {
            Self::Local(base) => {
                let path = base.join(rel);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, data)?;
                Ok(())
            }
            Self::S3 { bucket, prefix } => {
                let client = s3_client()?;
                let key = join_key(prefix, rel);
                let body = ByteStream::from(data.to_vec());
                let rt = tokio::runtime::Runtime::new().map_err(|e| {
                    OfffError::InvalidContainer(format!("failed to start async runtime: {e}"))
                })?;
                rt.block_on(async {
                    client
                        .put_object()
                        .bucket(bucket)
                        .key(&key)
                        .body(body)
                        .send()
                        .await
                })
                .map_err(|e| {
                    OfffError::InvalidContainer(format!("failed to write s3://{bucket}/{key}: {e}"))
                })?;
                Ok(())
            }
        }
    }

    pub fn exists(&self, rel: &str) -> Result<bool, OfffError> {
        match self {
            Self::Local(base) => Ok(base.join(rel).exists()),
            Self::S3 { bucket, prefix } => {
                let client = s3_client()?;
                let key = join_key(prefix, rel);
                let rt = tokio::runtime::Runtime::new().map_err(|e| {
                    OfffError::InvalidContainer(format!("failed to start async runtime: {e}"))
                })?;
                let out = rt
                    .block_on(async { client.head_object().bucket(bucket).key(&key).send().await });
                match out {
                    Ok(_) => Ok(true),
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("NotFound") || msg.contains("404") {
                            Ok(false)
                        } else {
                            Err(OfffError::InvalidContainer(format!(
                                "failed checking s3://{bucket}/{key}: {e}"
                            )))
                        }
                    }
                }
            }
        }
    }

    pub fn append_jsonl_line(&self, rel: &str, line: &str) -> Result<(), OfffError> {
        match self {
            Self::Local(base) => {
                let path = base.join(rel);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut content = if path.exists() {
                    fs::read_to_string(&path)?
                } else {
                    String::new()
                };
                content.push_str(line);
                content.push('\n');
                fs::write(path, content)?;
                Ok(())
            }
            Self::S3 { bucket, prefix } => {
                // Optimistic concurrency loop: read current object, append line,
                // and write it back guarded by ETag.
                let client = s3_client()?;
                let key = join_key(prefix, rel);
                let rt = tokio::runtime::Runtime::new().map_err(|e| {
                    OfffError::InvalidContainer(format!("failed to start async runtime: {e}"))
                })?;

                const MAX_ATTEMPTS: usize = 8;
                for _ in 0..MAX_ATTEMPTS {
                    let get_out = rt.block_on(async {
                        client.get_object().bucket(bucket).key(&key).send().await
                    });

                    match get_out {
                        Ok(obj) => {
                            let etag = obj.e_tag().unwrap_or_default().to_string();
                            let bytes =
                                rt.block_on(async { obj.body.collect().await })
                                    .map_err(|e| {
                                        OfffError::InvalidContainer(format!(
                                            "failed to download s3://{bucket}/{key}: {e}"
                                        ))
                                    })?;
                            let mut content = String::from_utf8(bytes.into_bytes().to_vec())
                                .map_err(|e| {
                                    OfffError::InvalidContainer(format!("{rel} is not UTF-8: {e}"))
                                })?;
                            content.push_str(line);
                            content.push('\n');

                            let put = rt.block_on(async {
                                client
                                    .put_object()
                                    .bucket(bucket)
                                    .key(&key)
                                    .if_match(etag)
                                    .body(ByteStream::from(content.into_bytes()))
                                    .send()
                                    .await
                            });

                            match put {
                                Ok(_) => return Ok(()),
                                Err(e) if is_precondition_error(&e.to_string()) => continue,
                                Err(e) => {
                                    return Err(OfffError::InvalidContainer(format!(
                                        "failed to write s3://{bucket}/{key}: {e}"
                                    )));
                                }
                            }
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            if msg.contains("NoSuchKey")
                                || msg.contains("NotFound")
                                || msg.contains("404")
                            {
                                let content = format!("{line}\n");
                                let put = rt.block_on(async {
                                    client
                                        .put_object()
                                        .bucket(bucket)
                                        .key(&key)
                                        .if_none_match("*")
                                        .body(ByteStream::from(content.into_bytes()))
                                        .send()
                                        .await
                                });
                                match put {
                                    Ok(_) => return Ok(()),
                                    Err(e) if is_precondition_error(&e.to_string()) => continue,
                                    Err(e) => {
                                        return Err(OfffError::InvalidContainer(format!(
                                            "failed to write s3://{bucket}/{key}: {e}"
                                        )));
                                    }
                                }
                            } else {
                                return Err(OfffError::InvalidContainer(format!(
                                    "failed to read s3://{bucket}/{key}: {e}"
                                )));
                            }
                        }
                    }
                }

                Err(OfffError::InvalidContainer(format!(
                    "failed to append to s3://{bucket}/{key} after repeated concurrent update retries"
                )))
            }
        }
    }

    pub fn list_relative_keys(&self, rel_prefix: &str) -> Result<Vec<String>, OfffError> {
        match self {
            Self::Local(base) => {
                let root = base.join(rel_prefix);
                if !root.exists() {
                    return Ok(Vec::new());
                }

                let mut out = Vec::new();
                collect_local_relative(&root, base, &mut out)?;
                out.sort();
                Ok(out)
            }
            Self::S3 { bucket, prefix } => {
                let list_prefix = join_key(prefix, rel_prefix);
                let client = s3_client()?;
                let rt = tokio::runtime::Runtime::new().map_err(|e| {
                    OfffError::InvalidContainer(format!("failed to start async runtime: {e}"))
                })?;

                let mut continuation: Option<String> = None;
                let mut out = Vec::new();
                loop {
                    let mut req = client.list_objects_v2().bucket(bucket).prefix(&list_prefix);
                    if let Some(token) = continuation.as_deref() {
                        req = req.continuation_token(token);
                    }

                    let resp = rt.block_on(async { req.send().await }).map_err(|e| {
                        OfffError::InvalidContainer(format!(
                            "failed to list s3://{bucket}/{list_prefix}: {e}"
                        ))
                    })?;

                    for obj in resp.contents() {
                        if let Some(key) = obj.key() {
                            if key.ends_with('/') {
                                continue;
                            }
                            let rel = if prefix.is_empty() {
                                key.to_string()
                            } else {
                                key.strip_prefix(&format!("{}/", prefix.trim_matches('/')))
                                    .unwrap_or(key)
                                    .to_string()
                            };
                            out.push(rel);
                        }
                    }

                    continuation = resp.next_continuation_token().map(|s| s.to_string());
                    if continuation.is_none() {
                        break;
                    }
                }

                out.sort();
                Ok(out)
            }
        }
    }
}

fn collect_local_relative(
    dir: &std::path::Path,
    base: &std::path::Path,
    out: &mut Vec<String>,
) -> Result<(), OfffError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_local_relative(&path, base, out)?;
        } else {
            let rel = path
                .strip_prefix(base)
                .map_err(|_| {
                    OfffError::InvalidContainer("failed to relativize local path".to_string())
                })?
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    Ok(())
}

fn is_precondition_error(msg: &str) -> bool {
    msg.contains("PreconditionFailed") || msg.contains("condition") || msg.contains("412")
}

pub fn read_chunk_verified(
    container: &ContainerRef,
    meta: &ChunkMetadata,
) -> Result<Vec<u8>, OfffError> {
    let rel = chunk_relative_path(&meta.plaintext_sha256);
    let stored_bytes = container.read_bytes(&rel)?;

    let actual_stored = hex_sha256(&stored_bytes);
    if actual_stored != meta.stored_sha256 {
        return Err(OfffError::HashMismatch {
            chunk_id: meta.chunk_id.clone(),
            expected: meta.stored_sha256.clone(),
            actual: actual_stored,
        });
    }

    let plaintext: Vec<u8> = match meta.compression.as_str() {
        "none" => stored_bytes,
        "zstd" => zstd::decode_all(stored_bytes.as_slice()).map_err(|e| {
            OfffError::DecompressionError {
                chunk_id: meta.chunk_id.clone(),
                msg: e.to_string(),
            }
        })?,
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

    Ok(plaintext)
}

pub fn chunk_relative_path(plaintext_sha256: &str) -> String {
    let hex = plaintext_sha256
        .strip_prefix("sha256:")
        .unwrap_or(plaintext_sha256);
    format!("chunks/sha256/{}/{}/{}.chunk", &hex[..2], &hex[2..4], hex)
}

/// Relative path within the container for a materialized derived object.
/// Layout: `derived/objects/sha256/<hex[0..2]>/<hex[2..4]>/<hex>.bin`
pub fn derived_object_path(sha256: &str) -> String {
    let hex = sha256.strip_prefix("sha256:").unwrap_or(sha256);
    format!(
        "derived/objects/sha256/{}/{}/{}.bin",
        &hex[..2],
        &hex[2..4],
        hex
    )
}

/// Write bytes to the content-addressed derived object store.
/// Returns the `sha256:<hex>` digest of the written bytes.
/// If the file already exists, the existing bytes are verified before
/// returning `Ok`.  A hash mismatch is a hard error.
pub fn write_derived_object(container: &ContainerRef, bytes: &[u8]) -> Result<String, OfffError> {
    let hex = hex_sha256(bytes);
    let sha256 = format!("sha256:{hex}");
    let rel = derived_object_path(&sha256);
    if container.exists(&rel)? {
        let existing = container.read_bytes(&rel)?;
        let existing_hex = hex_sha256(&existing);
        if existing_hex != hex {
            return Err(OfffError::HashMismatch {
                chunk_id: rel,
                expected: hex,
                actual: existing_hex,
            });
        }
    } else {
        container.write_bytes(&rel, bytes)?;
    }
    Ok(sha256)
}

/// Read a materialized derived object and verify its hash.
/// Returns the raw object bytes.
pub fn read_derived_object(container: &ContainerRef, sha256: &str) -> Result<Vec<u8>, OfffError> {
    let rel = derived_object_path(sha256);
    let bytes = container.read_bytes(&rel)?;
    let expected = sha256.strip_prefix("sha256:").unwrap_or(sha256);
    let actual = hex_sha256(&bytes);
    if actual != expected {
        return Err(OfffError::HashMismatch {
            chunk_id: rel,
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(bytes)
}

/// Read a derived object in fixed-size chunks, calling `on_chunk` for each
/// slice of bytes as it is read from disk.
///
/// For local containers the file is read through a [`std::io::BufReader`]
/// sized to `buf_size` (0 ⇒ 4 MiB default), which keeps peak heap usage at
/// `O(buf_size)` rather than `O(file_size)`.  For S3 containers the full
/// object is fetched before chunking (the SDK has no partial-read API in
/// synchronous mode), so only the callback invocations are chunked.
///
/// A SHA-256 digest is accumulated across all chunks and verified against
/// `sha256` after the last chunk, mirroring the integrity guarantee of
/// [`read_derived_object`].
pub fn read_derived_object_streaming(
    container: &ContainerRef,
    sha256: &str,
    buf_size: usize,
    mut on_chunk: impl FnMut(&[u8]),
) -> Result<(), OfffError> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let expected = sha256.strip_prefix("sha256:").unwrap_or(sha256);
    let rel = derived_object_path(sha256);
    let chunk = if buf_size == 0 { 4 * 1024 * 1024 } else { buf_size };

    match container {
        ContainerRef::Local(base) => {
            let path = base.join(&rel);
            let file = std::fs::File::open(&path)
                .map_err(OfffError::Io)?;
            let mut reader = std::io::BufReader::with_capacity(chunk, file);
            let mut hasher = Sha256::new();
            let mut buf = vec![0u8; chunk];
            loop {
                let n = reader.read(&mut buf)
                    .map_err(OfffError::Io)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                on_chunk(&buf[..n]);
            }
            let digest = hasher.finalize();
            let actual = format!("{digest:x}");
            if actual != expected {
                return Err(OfffError::HashMismatch {
                    chunk_id: rel,
                    expected: expected.to_string(),
                    actual,
                });
            }
        }
        ContainerRef::S3 { .. } => {
            // S3 sync path: full fetch, then slice into chunks for the callback.
            let bytes = container.read_bytes(&rel)?;
            let actual = hex_sha256(&bytes);
            if actual != expected {
                return Err(OfffError::HashMismatch {
                    chunk_id: rel,
                    expected: expected.to_string(),
                    actual,
                });
            }
            for slice in bytes.chunks(chunk) {
                on_chunk(slice);
            }
        }
    }
    Ok(())
}

fn join_key(prefix: &str, rel: &str) -> String {
    if prefix.is_empty() {
        rel.trim_start_matches('/').to_string()
    } else {
        format!(
            "{}/{}",
            prefix.trim_matches('/'),
            rel.trim_start_matches('/')
        )
    }
}

/// Read an evidence file from a `file_collection` container by its `object_id`.
///
/// Looks up the `storage_ref` (hex SHA-256) in the index, then reads and
/// verifies the evidence object from `{base}/evidence/objects/...`.
///
/// Returns `OfffError::InvalidContainer` if the object is not found in the
/// index or has no `storage_ref`.
pub fn read_evidence_file_verified(
    base: &std::path::Path,
    object_id: &str,
    index: &[DiscoveredObjectRow],
) -> Result<Vec<u8>, OfffError> {
    let sha256_ref = resolve_evidence_storage_ref(object_id, index)?;
    evidence::read_evidence_object(base, sha256_ref)
}

/// Compute (or confirm) the SHA-256 of an evidence object without loading it
/// fully into memory via the index `storage_ref`.
///
/// Returns the hex-encoded SHA-256. This is a lightweight path that just
/// verifies the stored hash without decompression.
pub fn compute_object_sha256(
    base: &std::path::Path,
    object_id: &str,
    index: &[DiscoveredObjectRow],
) -> Result<String, OfffError> {
    let sha256_ref = resolve_evidence_storage_ref(object_id, index)?;
    // Verifying the object re-reads it, which confirms the hash matches.
    evidence::verify_evidence_object(base, sha256_ref)?;
    Ok(sha256_ref.to_string())
}

fn resolve_evidence_storage_ref<'a>(
    object_id: &str,
    index: &'a [DiscoveredObjectRow],
) -> Result<&'a str, OfffError> {
    let row = index
        .iter()
        .find(|r| r.object_id == object_id)
        .ok_or_else(|| {
            OfffError::InvalidContainer(format!(
                "object_id '{object_id}' not found in index"
            ))
        })?;
    row.storage_ref.as_deref().ok_or_else(|| {
        OfffError::InvalidContainer(format!(
            "object_id '{object_id}' has no storage_ref"
        ))
    })
}

#[derive(Debug, Deserialize)]
struct PhysicalExtent {
    offset: u64,
    length: u64,
}

/// Read one filesystem file row by `(filesystem_id, file_id)` from all
/// `indexes/filesystems/*/file_index.parquet` indexes and reconstruct bytes.
pub fn read_file_verified(
    base: &std::path::Path,
    filesystem_id: &str,
    file_id: &str,
) -> Result<Vec<u8>, OfffError> {
    let parsed_file_id = parse_file_id_ref(file_id)?;
    let fs_root = base.join("indexes/filesystems");
    if !fs_root.exists() {
        return Err(OfffError::InvalidContainer(
            "indexes/filesystems directory not found".to_string(),
        ));
    }

    let map_path = base.join("maps/physical_to_chunk.parquet");
    let chunks = read_physical_to_chunk(&map_path)?;

    for entry in std::fs::read_dir(&fs_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        let idx_path = entry.path().join("file_index.parquet");
        if !idx_path.exists() {
            continue;
        }
        let rows = read_file_index(&idx_path)?;
        let candidate = rows.iter().find(|r| {
            r.file_id == parsed_file_id
                && (r.filesystem_id == filesystem_id || dir_name == filesystem_id)
                && !r.is_directory
                && !r.is_deleted
        });
        if let Some(row) = candidate {
            return reconstruct_from_extents(base, &chunks, &row.physical_extents);
        }
    }

    Err(OfffError::InvalidContainer(format!(
        "file_id '{file_id}' not found for filesystem_id '{filesystem_id}'"
    )))
}

/// Read an object using object_index `content_ref`/`storage_ref` and verify hash
/// when `sha256` is present.
pub fn read_object_verified(
    base: &std::path::Path,
    object_id: &str,
) -> Result<Vec<u8>, OfffError> {
    let index = read_object_index(&base.join("indexes/objects/object_index.parquet"))?;
    let row = index
        .iter()
        .find(|r| r.object_id == object_id)
        .ok_or_else(|| {
            OfffError::InvalidContainer(format!("object_id '{object_id}' not found in object_index"))
        })?;

    let bytes = if let Some(content_ref) = &row.content_ref {
        match content_ref.ref_type.as_str() {
            "filesystem_file" => {
                let fsid = content_ref.filesystem_id.as_deref().ok_or_else(|| {
                    OfffError::InvalidContainer(format!(
                        "object_id '{object_id}' content_ref missing filesystem_id"
                    ))
                })?;
                let fid = content_ref.file_id.as_deref().ok_or_else(|| {
                    OfffError::InvalidContainer(format!(
                        "object_id '{object_id}' content_ref missing file_id"
                    ))
                })?;
                read_file_verified(base, fsid, fid)?
            }
            "evidence_object_store" | "derived_object_store" => {
                let sr = content_ref.storage_ref.as_deref().ok_or_else(|| {
                    OfffError::InvalidContainer(format!(
                        "object_id '{object_id}' content_ref missing storage_ref"
                    ))
                })?;
                read_storage_ref_bytes(base, sr)?
            }
            other => {
                return Err(OfffError::InvalidContainer(format!(
                    "unsupported content_ref type '{other}' for object_id '{object_id}'"
                )));
            }
        }
    } else if let Some(sr) = row.storage_ref.as_deref() {
        read_storage_ref_bytes(base, sr)?
    } else {
        return Err(OfffError::InvalidContainer(format!(
            "object_id '{object_id}' has no content_ref or storage_ref"
        )));
    };

    if let Some(expected) = row.sha256.as_deref() {
        let expected_hex = expected.strip_prefix("sha256:").unwrap_or(expected);
        let actual = hex_sha256(&bytes);
        if actual != expected_hex {
            return Err(OfffError::HashMismatch {
                chunk_id: object_id.to_string(),
                expected: expected_hex.to_string(),
                actual,
            });
        }
    }

    Ok(bytes)
}

/// Compute SHA-256 for an object via `read_object_verified`.
pub fn compute_object_sha256_for_object(
    base: &std::path::Path,
    object_id: &str,
) -> Result<String, OfffError> {
    let bytes = read_object_verified(base, object_id)?;
    Ok(format!("sha256:{}", hex_sha256(&bytes)))
}

fn read_storage_ref_bytes(base: &std::path::Path, storage_ref: &str) -> Result<Vec<u8>, OfffError> {
    if storage_ref.starts_with("sha256:") {
        return evidence::read_evidence_object(base, storage_ref);
    }
    std::fs::read(base.join(storage_ref)).map_err(OfffError::Io)
}

fn parse_file_id_ref(file_id: &str) -> Result<u64, OfffError> {
    let trimmed = file_id.trim();
    if let Some(rest) = trimmed.strip_prefix("file-") {
        return rest.parse::<u64>().map_err(|_| {
            OfffError::InvalidContainer(format!("invalid file_id reference '{file_id}'"))
        });
    }
    trimmed.parse::<u64>().map_err(|_| {
        OfffError::InvalidContainer(format!("invalid file_id reference '{file_id}'"))
    })
}

fn reconstruct_from_extents(
    base: &std::path::Path,
    chunks: &[ChunkMetadata],
    physical_extents_json: &str,
) -> Result<Vec<u8>, OfffError> {
    let extents: Vec<PhysicalExtent> = serde_json::from_str(physical_extents_json).map_err(|e| {
        OfffError::InvalidContainer(format!("invalid physical_extents JSON: {e}"))
    })?;

    let mut out = Vec::new();
    for ext in extents {
        let bytes = read_bytes_at(base, chunks, ext.offset, ext.length)?;
        out.extend_from_slice(&bytes);
    }
    Ok(out)
}

fn s3_client() -> Result<Client, OfffError> {
    let endpoint = std::env::var("OFFF_S3_ENDPOINT").ok();
    let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| OfffError::InvalidContainer(format!("failed to start async runtime: {e}")))?;
    let conf = rt.block_on(async {
        let mut loader =
            aws_config::defaults(BehaviorVersion::latest()).region(Region::new(region));
        if let Some(ep) = endpoint {
            loader = loader.endpoint_url(ep);
        }
        loader.load().await
    });
    let mut s3_builder = aws_sdk_s3::config::Builder::from(&conf);
    if std::env::var("OFFF_S3_ENDPOINT").is_ok() {
        // MinIO/Ceph test endpoints typically require path-style addressing.
        s3_builder = s3_builder.force_path_style(true);
    }
    Ok(Client::from_conf(s3_builder.build()))
}

#[cfg(test)]
mod tests {
    use super::parse_file_id_ref;

    #[test]
    fn parses_file_id_reference_formats() {
        assert_eq!(parse_file_id_ref("file-000123").unwrap(), 123);
        assert_eq!(parse_file_id_ref("123").unwrap(), 123);
    }

    #[test]
    fn rejects_invalid_file_id_reference() {
        assert!(parse_file_id_ref("file-abc").is_err());
    }
}
