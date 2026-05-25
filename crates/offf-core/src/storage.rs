use std::{fs, path::PathBuf};

use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::{primitives::ByteStream, Client};

use crate::{chunk::hex_sha256, error::OfffError, types::ChunkMetadata};

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
