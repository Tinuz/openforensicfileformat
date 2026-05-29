use std::{fs, path::Path, sync::Arc};

use arrow::{
    array::{Array, ArrayRef, BooleanArray, StringArray, UInt64Array},
    datatypes::{DataType, Field, Schema},
    record_batch::RecordBatch,
};
use bytes::Bytes;
use parquet::{
    arrow::{arrow_reader::ParquetRecordBatchReaderBuilder, ArrowWriter},
    file::properties::WriterProperties,
};

use crate::{
    error::OfffError,
    types::{
        ChunkMetadata, DerivationRow, DiscoveredObjectRow, FileIndexRow, KeywordHitRow,
        ObjectEdgeRow, YaraHitRow,
    },
};

// ── physical_to_chunk.parquet ─────────────────────────────────────────────────

/// Write the physical-to-chunk mapping table.
///
/// Schema:
/// sequence (u64), source_offset (u64), source_length (u64),
/// chunk_id (utf8), stored_length (u64), compression (utf8),
/// plaintext_sha256 (utf8), stored_sha256 (utf8)
pub fn write_physical_to_chunk(path: &Path, chunks: &[ChunkMetadata]) -> Result<(), OfffError> {
    let schema = physical_to_chunk_schema();

    let sequence: UInt64Array = chunks.iter().map(|c| Some(c.sequence)).collect();
    let source_offset: UInt64Array = chunks.iter().map(|c| Some(c.source_offset)).collect();
    let source_length: UInt64Array = chunks.iter().map(|c| Some(c.source_length)).collect();
    let chunk_id: StringArray = chunks.iter().map(|c| Some(c.chunk_id.as_str())).collect();
    let stored_length: UInt64Array = chunks.iter().map(|c| Some(c.stored_length)).collect();
    let compression: StringArray = chunks
        .iter()
        .map(|c| Some(c.compression.as_str()))
        .collect();
    let plaintext_sha256: StringArray = chunks
        .iter()
        .map(|c| Some(c.plaintext_sha256.as_str()))
        .collect();
    let stored_sha256: StringArray = chunks
        .iter()
        .map(|c| Some(c.stored_sha256.as_str()))
        .collect();

    let batch = RecordBatch::try_new(
        Arc::new(schema),
        vec![
            Arc::new(sequence) as ArrayRef,
            Arc::new(source_offset) as ArrayRef,
            Arc::new(source_length) as ArrayRef,
            Arc::new(chunk_id) as ArrayRef,
            Arc::new(stored_length) as ArrayRef,
            Arc::new(compression) as ArrayRef,
            Arc::new(plaintext_sha256) as ArrayRef,
            Arc::new(stored_sha256) as ArrayRef,
        ],
    )?;

    write_batch(path, batch)?;
    Ok(())
}

/// Read the physical-to-chunk mapping table, returning chunks sorted by
/// sequence number.
pub fn read_physical_to_chunk(path: &Path) -> Result<Vec<ChunkMetadata>, OfffError> {
    let file = fs::File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;
    read_physical_to_chunk_reader(reader)
}

/// Read the physical-to-chunk mapping table from in-memory Parquet bytes.
pub fn read_physical_to_chunk_bytes(data: &[u8]) -> Result<Vec<ChunkMetadata>, OfffError> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(Bytes::from(data.to_vec()))?;
    let reader = builder.build()?;
    read_physical_to_chunk_reader(reader)
}

fn read_physical_to_chunk_reader(
    reader: impl Iterator<Item = Result<RecordBatch, arrow::error::ArrowError>>,
) -> Result<Vec<ChunkMetadata>, OfffError> {
    let mut chunks = Vec::new();
    for batch in reader {
        let batch = batch?;
        let n = batch.num_rows();

        let sequence = as_u64_col(&batch, "sequence")?;
        let source_offset = as_u64_col(&batch, "source_offset")?;
        let source_length = as_u64_col(&batch, "source_length")?;
        let chunk_id = as_str_col(&batch, "chunk_id")?;
        let stored_length = as_u64_col(&batch, "stored_length")?;
        let compression = as_str_col(&batch, "compression")?;
        let plaintext_sha256 = as_str_col(&batch, "plaintext_sha256")?;
        let stored_sha256 = as_str_col(&batch, "stored_sha256")?;

        for i in 0..n {
            chunks.push(ChunkMetadata {
                sequence: sequence.value(i),
                source_offset: source_offset.value(i),
                source_length: source_length.value(i),
                chunk_id: chunk_id.value(i).to_string(),
                stored_length: stored_length.value(i),
                compression: compression.value(i).to_string(),
                plaintext_sha256: plaintext_sha256.value(i).to_string(),
                stored_sha256: stored_sha256.value(i).to_string(),
                read_errors: vec![],
            });
        }
    }

    chunks.sort_by_key(|c| c.sequence);
    Ok(chunks)
}

// ── hashes/leaves.parquet ─────────────────────────────────────────────────────

/// Write the Merkle leaf table.
///
/// Schema: sequence (u64), chunk_id (utf8), plaintext_sha256 (utf8)
pub fn write_leaves(path: &Path, chunks: &[ChunkMetadata]) -> Result<(), OfffError> {
    let schema = leaves_schema();

    let sequence: UInt64Array = chunks.iter().map(|c| Some(c.sequence)).collect();
    let chunk_id: StringArray = chunks.iter().map(|c| Some(c.chunk_id.as_str())).collect();
    let plaintext_sha256: StringArray = chunks
        .iter()
        .map(|c| Some(c.plaintext_sha256.as_str()))
        .collect();

    let batch = RecordBatch::try_new(
        Arc::new(schema),
        vec![
            Arc::new(sequence) as ArrayRef,
            Arc::new(chunk_id) as ArrayRef,
            Arc::new(plaintext_sha256) as ArrayRef,
        ],
    )?;

    write_batch(path, batch)?;
    Ok(())
}

/// Read the leaves table.  Returns `(sequence, plaintext_sha256)` pairs sorted
/// by sequence.
pub fn read_leaves(path: &Path) -> Result<Vec<(u64, String)>, OfffError> {
    let file = fs::File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;

    let mut rows = Vec::new();
    for batch in reader {
        let batch = batch?;
        let n = batch.num_rows();
        let sequence = as_u64_col(&batch, "sequence")?;
        let plaintext_sha256 = as_str_col(&batch, "plaintext_sha256")?;

        for i in 0..n {
            rows.push((sequence.value(i), plaintext_sha256.value(i).to_string()));
        }
    }

    rows.sort_by_key(|(seq, _)| *seq);
    Ok(rows)
}

// ── internal helpers ──────────────────────────────────────────────────────────

fn physical_to_chunk_schema() -> Schema {
    Schema::new(vec![
        Field::new("sequence", DataType::UInt64, false),
        Field::new("source_offset", DataType::UInt64, false),
        Field::new("source_length", DataType::UInt64, false),
        Field::new("chunk_id", DataType::Utf8, false),
        Field::new("stored_length", DataType::UInt64, false),
        Field::new("compression", DataType::Utf8, false),
        Field::new("plaintext_sha256", DataType::Utf8, false),
        Field::new("stored_sha256", DataType::Utf8, false),
    ])
}

fn leaves_schema() -> Schema {
    Schema::new(vec![
        Field::new("sequence", DataType::UInt64, false),
        Field::new("chunk_id", DataType::Utf8, false),
        Field::new("plaintext_sha256", DataType::Utf8, false),
    ])
}

fn write_batch(path: &Path, batch: RecordBatch) -> Result<(), OfffError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(path)?;
    let props = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(())
}

fn as_u64_col<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a UInt64Array, OfffError> {
    batch
        .column_by_name(name)
        .ok_or_else(|| OfffError::InvalidManifest(format!("missing column '{name}'")))?
        .as_any()
        .downcast_ref::<UInt64Array>()
        .ok_or_else(|| OfffError::InvalidManifest(format!("column '{name}' is not UInt64")))
}

fn as_str_col<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a StringArray, OfffError> {
    batch
        .column_by_name(name)
        .ok_or_else(|| OfffError::InvalidManifest(format!("missing column '{name}'")))?
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| OfffError::InvalidManifest(format!("column '{name}' is not Utf8")))
}

// ── file_index.parquet ────────────────────────────────────────────────────────

/// Write the filesystem file index table.
///
/// Path should be `indexes/filesystems/<partition_id>/file_index.parquet`.
pub fn write_file_index(path: &Path, rows: &[FileIndexRow]) -> Result<(), OfffError> {
    fn opt_dt(v: Option<chrono::DateTime<chrono::Utc>>) -> String {
        v.map(|d| d.to_rfc3339()).unwrap_or_default()
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("file_id", DataType::UInt64, false),
        Field::new("filesystem_id", DataType::Utf8, false),
        Field::new("partition_id", DataType::Utf8, false),
        Field::new("path", DataType::Utf8, false),
        Field::new("filename", DataType::Utf8, false),
        Field::new("extension", DataType::Utf8, false),
        Field::new("size_bytes", DataType::UInt64, false),
        Field::new("created_at", DataType::Utf8, true),
        Field::new("modified_at", DataType::Utf8, true),
        Field::new("accessed_at", DataType::Utf8, true),
        Field::new("changed_at", DataType::Utf8, true),
        Field::new("physical_extents", DataType::Utf8, false),
        Field::new("chunk_refs", DataType::Utf8, false),
        Field::new("is_directory", DataType::Boolean, false),
        Field::new("is_deleted", DataType::Boolean, false),
        Field::new("is_sparse", DataType::Boolean, false),
        Field::new("is_compressed", DataType::Boolean, false),
        Field::new("is_encrypted", DataType::Boolean, false),
        Field::new("ads_streams", DataType::Utf8, false),
        Field::new("parser", DataType::Utf8, false),
        Field::new("parser_version", DataType::Utf8, false),
        Field::new("parser_status", DataType::Utf8, false),
        Field::new("parser_error", DataType::Utf8, false),
    ]));

    let file_id: UInt64Array = rows.iter().map(|r| Some(r.file_id)).collect();
    let filesystem_id: StringArray = rows
        .iter()
        .map(|r| Some(r.filesystem_id.as_str()))
        .collect();
    let partition_id: StringArray = rows.iter().map(|r| Some(r.partition_id.as_str())).collect();
    let path_col: StringArray = rows.iter().map(|r| Some(r.path.as_str())).collect();
    let filename: StringArray = rows.iter().map(|r| Some(r.filename.as_str())).collect();
    let extension: StringArray = rows.iter().map(|r| Some(r.extension.as_str())).collect();
    let size_bytes: UInt64Array = rows.iter().map(|r| Some(r.size_bytes)).collect();
    let created_at: StringArray = rows.iter().map(|r| Some(opt_dt(r.created_at))).collect();
    let modified_at: StringArray = rows.iter().map(|r| Some(opt_dt(r.modified_at))).collect();
    let accessed_at: StringArray = rows.iter().map(|r| Some(opt_dt(r.accessed_at))).collect();
    let changed_at: StringArray = rows.iter().map(|r| Some(opt_dt(r.changed_at))).collect();
    let physical_extents: StringArray = rows
        .iter()
        .map(|r| Some(r.physical_extents.as_str()))
        .collect();
    let chunk_refs: StringArray = rows.iter().map(|r| Some(r.chunk_refs.as_str())).collect();
    let is_directory: BooleanArray = rows.iter().map(|r| Some(r.is_directory)).collect();
    let is_deleted: BooleanArray = rows.iter().map(|r| Some(r.is_deleted)).collect();
    let is_sparse: BooleanArray = rows.iter().map(|r| Some(r.is_sparse)).collect();
    let is_compressed: BooleanArray = rows.iter().map(|r| Some(r.is_compressed)).collect();
    let is_encrypted: BooleanArray = rows.iter().map(|r| Some(r.is_encrypted)).collect();
    let ads_streams: StringArray = rows.iter().map(|r| Some(r.ads_streams.as_str())).collect();
    let parser: StringArray = rows.iter().map(|r| Some(r.parser.as_str())).collect();
    let parser_version: StringArray = rows
        .iter()
        .map(|r| Some(r.parser_version.as_str()))
        .collect();
    let parser_status: StringArray = rows
        .iter()
        .map(|r| Some(r.parser_status.as_str()))
        .collect();
    let parser_error: StringArray = rows.iter().map(|r| Some(r.parser_error.as_str())).collect();

    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(file_id) as ArrayRef,
            Arc::new(filesystem_id) as ArrayRef,
            Arc::new(partition_id) as ArrayRef,
            Arc::new(path_col) as ArrayRef,
            Arc::new(filename) as ArrayRef,
            Arc::new(extension) as ArrayRef,
            Arc::new(size_bytes) as ArrayRef,
            Arc::new(created_at) as ArrayRef,
            Arc::new(modified_at) as ArrayRef,
            Arc::new(accessed_at) as ArrayRef,
            Arc::new(changed_at) as ArrayRef,
            Arc::new(physical_extents) as ArrayRef,
            Arc::new(chunk_refs) as ArrayRef,
            Arc::new(is_directory) as ArrayRef,
            Arc::new(is_deleted) as ArrayRef,
            Arc::new(is_sparse) as ArrayRef,
            Arc::new(is_compressed) as ArrayRef,
            Arc::new(is_encrypted) as ArrayRef,
            Arc::new(ads_streams) as ArrayRef,
            Arc::new(parser) as ArrayRef,
            Arc::new(parser_version) as ArrayRef,
            Arc::new(parser_status) as ArrayRef,
            Arc::new(parser_error) as ArrayRef,
        ],
    )?;

    write_batch(path, batch)?;
    Ok(())
}

/// Read the minimal fields needed for file_id resolution from file_index.parquet.
///
/// Returns a vec of `(file_id, chunk_refs_json)` pairs.  The `chunk_refs_json`
/// field is a JSON array of chunk_id strings (e.g. `["sha256:abc...", ...]`).
/// Only non-deleted, non-directory rows are returned.
pub fn read_file_index_for_resolution(path: &Path) -> Result<Vec<(u64, String)>, OfffError> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let file = fs::File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;
    let mut rows = Vec::new();
    for batch in reader {
        let batch = batch?;
        if batch.num_rows() == 0 {
            continue;
        }
        let file_id = as_u64_col(&batch, "file_id")?;
        let chunk_refs = as_str_col(&batch, "chunk_refs")?;
        // is_directory / is_deleted
        let is_dir_col = batch.column_by_name("is_directory").and_then(|c| {
            c.as_any()
                .downcast_ref::<arrow::array::BooleanArray>()
                .cloned()
        });
        let is_del_col = batch.column_by_name("is_deleted").and_then(|c| {
            c.as_any()
                .downcast_ref::<arrow::array::BooleanArray>()
                .cloned()
        });
        for i in 0..batch.num_rows() {
            let is_dir = is_dir_col.as_ref().map(|c| c.value(i)).unwrap_or(false);
            let is_del = is_del_col.as_ref().map(|c| c.value(i)).unwrap_or(false);
            if is_dir || is_del {
                continue;
            }
            rows.push((file_id.value(i), chunk_refs.value(i).to_string()));
        }
    }
    Ok(rows)
}

// ── keyword_hits.parquet ──────────────────────────────────────────────────────

pub fn write_keyword_hits(path: &Path, rows: &[KeywordHitRow]) -> Result<(), OfffError> {
    let schema = Arc::new(arrow::datatypes::Schema::new(vec![
        Field::new("hit_id", DataType::Utf8, false),
        Field::new("job_id", DataType::Utf8, false),
        Field::new("keyword", DataType::Utf8, false),
        Field::new("chunk_id", DataType::Utf8, false),
        Field::new("physical_offset", DataType::UInt64, false),
        Field::new("file_id", DataType::Utf8, false),
        Field::new("context_before", DataType::Utf8, false),
        Field::new("context_after", DataType::Utf8, false),
        Field::new("encoding", DataType::Utf8, false),
        Field::new("worker_id", DataType::Utf8, false),
        Field::new("timestamp", DataType::Utf8, false),
    ]));

    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.hit_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.job_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.keyword.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.chunk_id.as_str()),
            )) as ArrayRef,
            Arc::new(UInt64Array::from_iter_values(
                rows.iter().map(|r| r.physical_offset),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.file_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.context_before.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.context_after.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.encoding.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.worker_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.timestamp.as_str()),
            )) as ArrayRef,
        ],
    )?;

    write_batch(path, batch)?;
    Ok(())
}

// ── yara_hits.parquet ─────────────────────────────────────────────────────────

pub fn write_yara_hits(path: &Path, rows: &[YaraHitRow]) -> Result<(), OfffError> {
    let schema = Arc::new(arrow::datatypes::Schema::new(vec![
        Field::new("hit_id", DataType::Utf8, false),
        Field::new("job_id", DataType::Utf8, false),
        Field::new("rule_name", DataType::Utf8, false),
        Field::new("ruleset_hash", DataType::Utf8, false),
        Field::new("chunk_id", DataType::Utf8, false),
        Field::new("physical_offset", DataType::UInt64, false),
        Field::new("match_length", DataType::UInt64, false),
        Field::new("file_id", DataType::Utf8, false),
        Field::new("worker_id", DataType::Utf8, false),
        Field::new("timestamp", DataType::Utf8, false),
    ]));

    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.hit_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.job_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.rule_name.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.ruleset_hash.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.chunk_id.as_str()),
            )) as ArrayRef,
            Arc::new(UInt64Array::from_iter_values(
                rows.iter().map(|r| r.physical_offset),
            )) as ArrayRef,
            Arc::new(UInt64Array::from_iter_values(
                rows.iter().map(|r| r.match_length),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.file_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.worker_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.timestamp.as_str()),
            )) as ArrayRef,
        ],
    )?;

    write_batch(path, batch)?;
    Ok(())
}

// ── indexes/objects/object_index.parquet ─────────────────────────────────────

pub fn write_object_index(path: &Path, rows: &[DiscoveredObjectRow]) -> Result<(), OfffError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("object_id", DataType::Utf8, false),
        Field::new("object_type", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, true),
        Field::new("logical_path", DataType::Utf8, true),
        Field::new("media_type", DataType::Utf8, true),
        Field::new("size_bytes", DataType::UInt64, true),
        Field::new("sha256", DataType::Utf8, true),
        Field::new("source_layer", DataType::Utf8, false),
        Field::new("storage_ref", DataType::Utf8, true),
        Field::new("root_source_ref", DataType::Utf8, true),
        Field::new("root_id", DataType::Utf8, true),
        Field::new("collection_relative_path", DataType::Utf8, true),
        Field::new("created_by_job_id", DataType::Utf8, true),
        Field::new("parser_status", DataType::Utf8, false),
        Field::new("provenance_ref", DataType::Utf8, true),
        Field::new("schema_version", DataType::Utf8, false),
        Field::new("original_created_at", DataType::Utf8, true),
        Field::new("original_modified_at", DataType::Utf8, true),
        Field::new("original_accessed_at", DataType::Utf8, true),
    ]));

    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.object_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.object_type.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.name.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.logical_path.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.media_type.as_deref()),
            )) as ArrayRef,
            Arc::new(UInt64Array::from_iter(rows.iter().map(|r| r.size_bytes))) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.sha256.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.source_layer.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.storage_ref.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.root_source_ref.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.root_id.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.collection_relative_path.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.created_by_job_id.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.parser_status.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.provenance_ref.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.schema_version.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.original_created_at.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.original_modified_at.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.original_accessed_at.as_deref()),
            )) as ArrayRef,
        ],
    )?;

    write_batch(path, batch)?;
    Ok(())
}

pub fn read_object_index(path: &Path) -> Result<Vec<DiscoveredObjectRow>, OfffError> {
    let file = fs::File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;

    let mut rows = Vec::new();
    for batch in reader {
        let batch = batch?;
        let n = batch.num_rows();
        let object_id = as_str_col(&batch, "object_id")?;
        let object_type = as_str_col(&batch, "object_type")?;
        let name = as_str_col(&batch, "name")?;
        let logical_path = as_str_col(&batch, "logical_path")?;
        let media_type = as_str_col(&batch, "media_type")?;
        let size_bytes = as_u64_col(&batch, "size_bytes")?;
        let sha256 = as_str_col(&batch, "sha256")?;
        let source_layer = as_str_col(&batch, "source_layer")?;
        let storage_ref = as_str_col(&batch, "storage_ref")?;
        let root_source_ref = as_str_col(&batch, "root_source_ref")?;
        // New nullable columns (absent in old parquet files → None)
        let root_id = as_str_col(&batch, "root_id").ok();
        let collection_relative_path = as_str_col(&batch, "collection_relative_path").ok();
        let original_created_at = as_str_col(&batch, "original_created_at").ok();
        let original_modified_at = as_str_col(&batch, "original_modified_at").ok();
        let original_accessed_at = as_str_col(&batch, "original_accessed_at").ok();
        let created_by_job_id = as_str_col(&batch, "created_by_job_id")?;
        let parser_status = as_str_col(&batch, "parser_status")?;
        let provenance_ref = as_str_col(&batch, "provenance_ref")?;
        let schema_version = as_str_col(&batch, "schema_version")?;

        for i in 0..n {
            rows.push(DiscoveredObjectRow {
                object_id: object_id.value(i).to_string(),
                object_type: object_type.value(i).to_string(),
                name: str_value_or_none(name, i),
                logical_path: str_value_or_none(logical_path, i),
                media_type: str_value_or_none(media_type, i),
                size_bytes: u64_value_or_none(size_bytes, i),
                sha256: str_value_or_none(sha256, i),
                source_layer: source_layer.value(i).to_string(),
                storage_ref: str_value_or_none(storage_ref, i),
                root_source_ref: str_value_or_none(root_source_ref, i),
                root_id: root_id.as_ref().and_then(|c| str_value_or_none(c, i)),
                collection_relative_path: collection_relative_path.as_ref().and_then(|c| str_value_or_none(c, i)),
                original_created_at: original_created_at.as_ref().and_then(|c| str_value_or_none(c, i)),
                original_modified_at: original_modified_at.as_ref().and_then(|c| str_value_or_none(c, i)),
                original_accessed_at: original_accessed_at.as_ref().and_then(|c| str_value_or_none(c, i)),
                created_by_job_id: str_value_or_none(created_by_job_id, i),
                parser_status: parser_status.value(i).to_string(),
                provenance_ref: str_value_or_none(provenance_ref, i),
                schema_version: schema_version.value(i).to_string(),
            });
        }
    }

    Ok(rows)
}

// ── indexes/objects/object_edges.parquet ─────────────────────────────────────

pub fn write_object_edges(path: &Path, rows: &[ObjectEdgeRow]) -> Result<(), OfffError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("edge_id", DataType::Utf8, false),
        Field::new("parent_object_id", DataType::Utf8, false),
        Field::new("child_object_id", DataType::Utf8, false),
        Field::new("relation_type", DataType::Utf8, false),
        Field::new("method", DataType::Utf8, true),
        Field::new("logical_path", DataType::Utf8, true),
        Field::new("sequence", DataType::UInt64, true),
        Field::new("created_by_job_id", DataType::Utf8, true),
        Field::new("provenance_ref", DataType::Utf8, true),
        Field::new("schema_version", DataType::Utf8, false),
    ]));

    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.edge_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.parent_object_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.child_object_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.relation_type.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.method.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.logical_path.as_deref()),
            )) as ArrayRef,
            Arc::new(UInt64Array::from_iter(rows.iter().map(|r| r.sequence))) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.created_by_job_id.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.provenance_ref.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.schema_version.as_str()),
            )) as ArrayRef,
        ],
    )?;

    write_batch(path, batch)?;
    Ok(())
}

pub fn read_object_edges(path: &Path) -> Result<Vec<ObjectEdgeRow>, OfffError> {
    let file = fs::File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;

    let mut rows = Vec::new();
    for batch in reader {
        let batch = batch?;
        let n = batch.num_rows();
        let edge_id = as_str_col(&batch, "edge_id")?;
        let parent_object_id = as_str_col(&batch, "parent_object_id")?;
        let child_object_id = as_str_col(&batch, "child_object_id")?;
        let relation_type = as_str_col(&batch, "relation_type")?;
        let method = as_str_col(&batch, "method")?;
        let logical_path = as_str_col(&batch, "logical_path")?;
        let sequence = as_u64_col(&batch, "sequence")?;
        let created_by_job_id = as_str_col(&batch, "created_by_job_id")?;
        let provenance_ref = as_str_col(&batch, "provenance_ref")?;
        let schema_version = as_str_col(&batch, "schema_version")?;

        for i in 0..n {
            rows.push(ObjectEdgeRow {
                edge_id: edge_id.value(i).to_string(),
                parent_object_id: parent_object_id.value(i).to_string(),
                child_object_id: child_object_id.value(i).to_string(),
                relation_type: relation_type.value(i).to_string(),
                method: str_value_or_none(method, i),
                logical_path: str_value_or_none(logical_path, i),
                sequence: u64_value_or_none(sequence, i),
                created_by_job_id: str_value_or_none(created_by_job_id, i),
                provenance_ref: str_value_or_none(provenance_ref, i),
                schema_version: schema_version.value(i).to_string(),
            });
        }
    }

    Ok(rows)
}

// ── indexes/objects/derivations.parquet ──────────────────────────────────────

pub fn write_derivations(path: &Path, rows: &[DerivationRow]) -> Result<(), OfffError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("derivation_id", DataType::Utf8, false),
        Field::new("parent_object_id", DataType::Utf8, false),
        Field::new("child_object_id", DataType::Utf8, false),
        Field::new("job_id", DataType::Utf8, false),
        Field::new("method", DataType::Utf8, false),
        Field::new("tool_id", DataType::Utf8, false),
        Field::new("tool_name", DataType::Utf8, false),
        Field::new("tool_version", DataType::Utf8, false),
        Field::new("parameters_hash", DataType::Utf8, true),
        Field::new("input_sha256", DataType::Utf8, true),
        Field::new("output_sha256", DataType::Utf8, true),
        Field::new("storage_mode", DataType::Utf8, false),
        Field::new("provenance_ref", DataType::Utf8, true),
        Field::new("created_at", DataType::Utf8, false),
        Field::new("schema_version", DataType::Utf8, false),
    ]));

    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.derivation_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.parent_object_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.child_object_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.job_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.method.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.tool_id.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.tool_name.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.tool_version.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.parameters_hash.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.input_sha256.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.output_sha256.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.storage_mode.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|r| r.provenance_ref.as_deref()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.created_at.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.schema_version.as_str()),
            )) as ArrayRef,
        ],
    )?;

    write_batch(path, batch)?;
    Ok(())
}

pub fn read_derivations(path: &Path) -> Result<Vec<DerivationRow>, OfffError> {
    let file = fs::File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;

    let mut rows = Vec::new();
    for batch in reader {
        let batch = batch?;
        let n = batch.num_rows();
        let derivation_id = as_str_col(&batch, "derivation_id")?;
        let parent_object_id = as_str_col(&batch, "parent_object_id")?;
        let child_object_id = as_str_col(&batch, "child_object_id")?;
        let job_id = as_str_col(&batch, "job_id")?;
        let method = as_str_col(&batch, "method")?;
        let tool_id = as_str_col(&batch, "tool_id")?;
        let tool_name = as_str_col(&batch, "tool_name")?;
        let tool_version = as_str_col(&batch, "tool_version")?;
        let parameters_hash = as_str_col(&batch, "parameters_hash")?;
        let input_sha256 = as_str_col(&batch, "input_sha256")?;
        let output_sha256 = as_str_col(&batch, "output_sha256")?;
        let storage_mode = as_str_col(&batch, "storage_mode")?;
        let provenance_ref = as_str_col(&batch, "provenance_ref")?;
        let created_at = as_str_col(&batch, "created_at")?;
        let schema_version = as_str_col(&batch, "schema_version")?;

        for i in 0..n {
            rows.push(DerivationRow {
                derivation_id: derivation_id.value(i).to_string(),
                parent_object_id: parent_object_id.value(i).to_string(),
                child_object_id: child_object_id.value(i).to_string(),
                job_id: job_id.value(i).to_string(),
                method: method.value(i).to_string(),
                tool_id: tool_id.value(i).to_string(),
                tool_name: tool_name.value(i).to_string(),
                tool_version: tool_version.value(i).to_string(),
                parameters_hash: str_value_or_none(parameters_hash, i),
                input_sha256: str_value_or_none(input_sha256, i),
                output_sha256: str_value_or_none(output_sha256, i),
                storage_mode: storage_mode.value(i).to_string(),
                provenance_ref: str_value_or_none(provenance_ref, i),
                created_at: created_at.value(i).to_string(),
                schema_version: schema_version.value(i).to_string(),
            });
        }
    }

    Ok(rows)
}

fn str_value_or_none(col: &StringArray, i: usize) -> Option<String> {
    (!col.is_null(i)).then(|| col.value(i).to_string())
}

fn u64_value_or_none(col: &UInt64Array, i: usize) -> Option<u64> {
    (!col.is_null(i)).then(|| col.value(i))
}

// ── Streaming batch reads ─────────────────────────────────────────────────────

/// Stream object-index rows from `path` in batches, calling `f` for each
/// decoded batch. `batch_size` limits rows per invocation; pass `0` to use
/// the Parquet file's native row-group size.
pub fn for_each_object_batch(
    path: &Path,
    batch_size: usize,
    mut f: impl FnMut(&[DiscoveredObjectRow]) -> Result<(), OfffError>,
) -> Result<(), OfffError> {
    let file = fs::File::open(path)?;
    let mut builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    if batch_size > 0 {
        builder = builder.with_batch_size(batch_size);
    }
    let reader = builder.build()?;
    for batch in reader {
        let batch = batch?;
        let n = batch.num_rows();
        let object_id = as_str_col(&batch, "object_id")?;
        let object_type = as_str_col(&batch, "object_type")?;
        let name = as_str_col(&batch, "name")?;
        let logical_path = as_str_col(&batch, "logical_path")?;
        let media_type = as_str_col(&batch, "media_type")?;
        let size_bytes = as_u64_col(&batch, "size_bytes")?;
        let sha256 = as_str_col(&batch, "sha256")?;
        let source_layer = as_str_col(&batch, "source_layer")?;
        let storage_ref = as_str_col(&batch, "storage_ref")?;
        let root_source_ref = as_str_col(&batch, "root_source_ref")?;
        let root_id = as_str_col(&batch, "root_id").ok();
        let collection_relative_path = as_str_col(&batch, "collection_relative_path").ok();
        let original_created_at = as_str_col(&batch, "original_created_at").ok();
        let original_modified_at = as_str_col(&batch, "original_modified_at").ok();
        let original_accessed_at = as_str_col(&batch, "original_accessed_at").ok();
        let created_by_job_id = as_str_col(&batch, "created_by_job_id")?;
        let parser_status = as_str_col(&batch, "parser_status")?;
        let provenance_ref = as_str_col(&batch, "provenance_ref")?;
        let schema_version = as_str_col(&batch, "schema_version")?;
        let mut rows = Vec::with_capacity(n);
        for i in 0..n {
            rows.push(DiscoveredObjectRow {
                object_id: object_id.value(i).to_string(),
                object_type: object_type.value(i).to_string(),
                name: str_value_or_none(name, i),
                logical_path: str_value_or_none(logical_path, i),
                media_type: str_value_or_none(media_type, i),
                size_bytes: u64_value_or_none(size_bytes, i),
                sha256: str_value_or_none(sha256, i),
                source_layer: source_layer.value(i).to_string(),
                storage_ref: str_value_or_none(storage_ref, i),
                root_source_ref: str_value_or_none(root_source_ref, i),
                root_id: root_id.as_ref().and_then(|c| str_value_or_none(c, i)),
                collection_relative_path: collection_relative_path.as_ref().and_then(|c| str_value_or_none(c, i)),
                original_created_at: original_created_at.as_ref().and_then(|c| str_value_or_none(c, i)),
                original_modified_at: original_modified_at.as_ref().and_then(|c| str_value_or_none(c, i)),
                original_accessed_at: original_accessed_at.as_ref().and_then(|c| str_value_or_none(c, i)),
                created_by_job_id: str_value_or_none(created_by_job_id, i),
                parser_status: parser_status.value(i).to_string(),
                provenance_ref: str_value_or_none(provenance_ref, i),
                schema_version: schema_version.value(i).to_string(),
            });
        }
        f(&rows)?;
    }
    Ok(())
}

/// Stream edge rows from `path` in batches. See [`for_each_object_batch`].
pub fn for_each_edge_batch(
    path: &Path,
    batch_size: usize,
    mut f: impl FnMut(&[ObjectEdgeRow]) -> Result<(), OfffError>,
) -> Result<(), OfffError> {
    let file = fs::File::open(path)?;
    let mut builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    if batch_size > 0 {
        builder = builder.with_batch_size(batch_size);
    }
    let reader = builder.build()?;
    for batch in reader {
        let batch = batch?;
        let n = batch.num_rows();
        let edge_id = as_str_col(&batch, "edge_id")?;
        let parent_object_id = as_str_col(&batch, "parent_object_id")?;
        let child_object_id = as_str_col(&batch, "child_object_id")?;
        let relation_type = as_str_col(&batch, "relation_type")?;
        let method = as_str_col(&batch, "method")?;
        let logical_path = as_str_col(&batch, "logical_path")?;
        let sequence = as_u64_col(&batch, "sequence")?;
        let created_by_job_id = as_str_col(&batch, "created_by_job_id")?;
        let provenance_ref = as_str_col(&batch, "provenance_ref")?;
        let schema_version = as_str_col(&batch, "schema_version")?;
        let mut rows = Vec::with_capacity(n);
        for i in 0..n {
            rows.push(ObjectEdgeRow {
                edge_id: edge_id.value(i).to_string(),
                parent_object_id: parent_object_id.value(i).to_string(),
                child_object_id: child_object_id.value(i).to_string(),
                relation_type: relation_type.value(i).to_string(),
                method: str_value_or_none(method, i),
                logical_path: str_value_or_none(logical_path, i),
                sequence: u64_value_or_none(sequence, i),
                created_by_job_id: str_value_or_none(created_by_job_id, i),
                provenance_ref: str_value_or_none(provenance_ref, i),
                schema_version: schema_version.value(i).to_string(),
            });
        }
        f(&rows)?;
    }
    Ok(())
}

/// Stream derivation rows from `path` in batches. See [`for_each_object_batch`].
pub fn for_each_derivation_batch(
    path: &Path,
    batch_size: usize,
    mut f: impl FnMut(&[DerivationRow]) -> Result<(), OfffError>,
) -> Result<(), OfffError> {
    let file = fs::File::open(path)?;
    let mut builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    if batch_size > 0 {
        builder = builder.with_batch_size(batch_size);
    }
    let reader = builder.build()?;
    for batch in reader {
        let batch = batch?;
        let n = batch.num_rows();
        let derivation_id = as_str_col(&batch, "derivation_id")?;
        let parent_object_id = as_str_col(&batch, "parent_object_id")?;
        let child_object_id = as_str_col(&batch, "child_object_id")?;
        let job_id = as_str_col(&batch, "job_id")?;
        let method = as_str_col(&batch, "method")?;
        let tool_id = as_str_col(&batch, "tool_id")?;
        let tool_name = as_str_col(&batch, "tool_name")?;
        let tool_version = as_str_col(&batch, "tool_version")?;
        let parameters_hash = as_str_col(&batch, "parameters_hash")?;
        let input_sha256 = as_str_col(&batch, "input_sha256")?;
        let output_sha256 = as_str_col(&batch, "output_sha256")?;
        let storage_mode = as_str_col(&batch, "storage_mode")?;
        let provenance_ref = as_str_col(&batch, "provenance_ref")?;
        let created_at = as_str_col(&batch, "created_at")?;
        let schema_version = as_str_col(&batch, "schema_version")?;
        let mut rows = Vec::with_capacity(n);
        for i in 0..n {
            rows.push(DerivationRow {
                derivation_id: derivation_id.value(i).to_string(),
                parent_object_id: parent_object_id.value(i).to_string(),
                child_object_id: child_object_id.value(i).to_string(),
                job_id: job_id.value(i).to_string(),
                method: method.value(i).to_string(),
                tool_id: tool_id.value(i).to_string(),
                tool_name: tool_name.value(i).to_string(),
                tool_version: tool_version.value(i).to_string(),
                parameters_hash: str_value_or_none(parameters_hash, i),
                input_sha256: str_value_or_none(input_sha256, i),
                output_sha256: str_value_or_none(output_sha256, i),
                storage_mode: storage_mode.value(i).to_string(),
                provenance_ref: str_value_or_none(provenance_ref, i),
                created_at: created_at.value(i).to_string(),
                schema_version: schema_version.value(i).to_string(),
            });
        }
        f(&rows)?;
    }
    Ok(())
}

// ── Streaming batch writes ────────────────────────────────────────────────────

/// Write object-index rows to `path` as multiple Parquet row groups.
///
/// Each `Vec<DiscoveredObjectRow>` in `batches` becomes one row group, letting
/// callers bound peak memory during large incremental rebuilds.
pub fn write_object_index_batched(
    path: &Path,
    batches: impl Iterator<Item = Vec<DiscoveredObjectRow>>,
) -> Result<(), OfffError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("object_id", DataType::Utf8, false),
        Field::new("object_type", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, true),
        Field::new("logical_path", DataType::Utf8, true),
        Field::new("media_type", DataType::Utf8, true),
        Field::new("size_bytes", DataType::UInt64, true),
        Field::new("sha256", DataType::Utf8, true),
        Field::new("source_layer", DataType::Utf8, false),
        Field::new("storage_ref", DataType::Utf8, true),
        Field::new("root_source_ref", DataType::Utf8, true),
        Field::new("created_by_job_id", DataType::Utf8, true),
        Field::new("parser_status", DataType::Utf8, false),
        Field::new("provenance_ref", DataType::Utf8, true),
        Field::new("schema_version", DataType::Utf8, false),
    ]));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(path)?;
    let props = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))?;
    for rows in batches {
        if rows.is_empty() {
            continue;
        }
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.object_id.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.object_type.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.name.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.logical_path.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.media_type.as_deref()),
                )) as ArrayRef,
                Arc::new(UInt64Array::from_iter(rows.iter().map(|r| r.size_bytes))) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.sha256.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.source_layer.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.storage_ref.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.root_source_ref.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.created_by_job_id.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.parser_status.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.provenance_ref.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.schema_version.as_str()),
                )) as ArrayRef,
            ],
        )?;
        writer.write(&batch)?;
    }
    writer.close()?;
    Ok(())
}

/// Write edge rows to `path` as multiple Parquet row groups.
/// See [`write_object_index_batched`] for the streaming write contract.
pub fn write_object_edges_batched(
    path: &Path,
    batches: impl Iterator<Item = Vec<ObjectEdgeRow>>,
) -> Result<(), OfffError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("edge_id", DataType::Utf8, false),
        Field::new("parent_object_id", DataType::Utf8, false),
        Field::new("child_object_id", DataType::Utf8, false),
        Field::new("relation_type", DataType::Utf8, false),
        Field::new("method", DataType::Utf8, true),
        Field::new("logical_path", DataType::Utf8, true),
        Field::new("sequence", DataType::UInt64, true),
        Field::new("created_by_job_id", DataType::Utf8, true),
        Field::new("provenance_ref", DataType::Utf8, true),
        Field::new("schema_version", DataType::Utf8, false),
    ]));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(path)?;
    let props = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))?;
    for rows in batches {
        if rows.is_empty() {
            continue;
        }
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.edge_id.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.parent_object_id.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.child_object_id.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.relation_type.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.method.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.logical_path.as_deref()),
                )) as ArrayRef,
                Arc::new(UInt64Array::from_iter(rows.iter().map(|r| r.sequence))) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.created_by_job_id.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.provenance_ref.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.schema_version.as_str()),
                )) as ArrayRef,
            ],
        )?;
        writer.write(&batch)?;
    }
    writer.close()?;
    Ok(())
}

/// Write derivation rows to `path` as multiple Parquet row groups.
/// See [`write_object_index_batched`] for the streaming write contract.
pub fn write_derivations_batched(
    path: &Path,
    batches: impl Iterator<Item = Vec<DerivationRow>>,
) -> Result<(), OfffError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("derivation_id", DataType::Utf8, false),
        Field::new("parent_object_id", DataType::Utf8, false),
        Field::new("child_object_id", DataType::Utf8, false),
        Field::new("job_id", DataType::Utf8, false),
        Field::new("method", DataType::Utf8, false),
        Field::new("tool_id", DataType::Utf8, false),
        Field::new("tool_name", DataType::Utf8, false),
        Field::new("tool_version", DataType::Utf8, false),
        Field::new("parameters_hash", DataType::Utf8, true),
        Field::new("input_sha256", DataType::Utf8, true),
        Field::new("output_sha256", DataType::Utf8, true),
        Field::new("storage_mode", DataType::Utf8, false),
        Field::new("provenance_ref", DataType::Utf8, true),
        Field::new("created_at", DataType::Utf8, false),
        Field::new("schema_version", DataType::Utf8, false),
    ]));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(path)?;
    let props = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))?;
    for rows in batches {
        if rows.is_empty() {
            continue;
        }
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.derivation_id.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.parent_object_id.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.child_object_id.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.job_id.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.method.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.tool_id.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.tool_name.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.tool_version.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.parameters_hash.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.input_sha256.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.output_sha256.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.storage_mode.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter(
                    rows.iter().map(|r| r.provenance_ref.as_deref()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.created_at.as_str()),
                )) as ArrayRef,
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.schema_version.as_str()),
                )) as ArrayRef,
            ],
        )?;
        writer.write(&batch)?;
    }
    writer.close()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_chunk(seq: u64, offset: u64) -> ChunkMetadata {
        ChunkMetadata {
            sequence: seq,
            chunk_id: format!("sha256:{:064x}", seq),
            source_offset: offset,
            source_length: 1024,
            stored_length: 512,
            compression: "zstd".to_string(),
            plaintext_sha256: format!("{:064x}", seq),
            stored_sha256: format!("{:064x}", seq + 100),
            read_errors: vec![],
        }
    }

    #[test]
    fn round_trip_physical_to_chunk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("physical_to_chunk.parquet");
        let chunks: Vec<ChunkMetadata> = (0..5).map(|i| make_chunk(i, i * 1024)).collect();

        write_physical_to_chunk(&path, &chunks).unwrap();
        let back = read_physical_to_chunk(&path).unwrap();

        assert_eq!(back.len(), 5);
        for (original, loaded) in chunks.iter().zip(back.iter()) {
            assert_eq!(original.sequence, loaded.sequence);
            assert_eq!(original.plaintext_sha256, loaded.plaintext_sha256);
        }
    }

    #[test]
    fn round_trip_leaves() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("leaves.parquet");
        let chunks: Vec<ChunkMetadata> = (0..3).map(|i| make_chunk(i, i * 512)).collect();

        write_leaves(&path, &chunks).unwrap();
        let leaves = read_leaves(&path).unwrap();

        assert_eq!(leaves.len(), 3);
        assert_eq!(leaves[0].0, 0);
        assert_eq!(leaves[2].0, 2);
    }
}
