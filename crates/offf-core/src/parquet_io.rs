use std::{fs, path::Path, sync::Arc};

use arrow::{
    array::{ArrayRef, BooleanArray, StringArray, UInt64Array},
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
    types::{ChunkMetadata, FileIndexRow, KeywordHitRow, YaraHitRow},
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
            Arc::new(parser) as ArrayRef,
            Arc::new(parser_version) as ArrayRef,
            Arc::new(parser_status) as ArrayRef,
            Arc::new(parser_error) as ArrayRef,
        ],
    )?;

    write_batch(path, batch)?;
    Ok(())
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
