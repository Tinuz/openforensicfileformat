# OFFF Formal Specification v0.1.0

Status: Draft (implementation-aligned)

This document defines the normative OFFF specification baseline for version 0.1.0.
Normative keywords are interpreted as defined in RFC 2119: MUST, MUST NOT, SHOULD, SHOULD NOT, MAY.

## 1. Scope

This specification defines:
- container layout
- manifest schema
- chunk schema
- hashing rules
- Merkle tree definition
- mapping tables
- provenance model
- index formats
- analysis output schemas
- validation rules
- versioning
- compatibility rules

## 2. Container Layout

An OFFF container MUST be directory-based and MUST contain at least the following structure:

```text
<case>.offf/
  manifest.json
  acquisition.json
  chunks/
    sha256/
      <2 hex>/<2 hex>/<64 hex>.chunk
  hashes/
    leaves.parquet
    merkle_tree.bin
  maps/
    physical_to_chunk.parquet
  provenance/
    chain_of_custody.jsonl
  indexes/
  analysis/
  signatures/
```

Rules:
- All paths in manifest/index references MUST be relative to container root.
- The evidence layer (chunks/, hashes/, maps/) MUST be immutable after successful conversion.
- Indexes and analysis outputs MUST NOT overwrite evidence bytes.
- For object storage use (S3/MinIO), object keys MUST map 1:1 to these relative paths.

## 3. Manifest Schema

File: manifest.json
Format: JSON object

Required fields:

```json
{
  "offf_version": "0.1.0",
  "container_id": "urn:offf:case:<id>",
  "created_at": "RFC3339 timestamp",
  "created_by_tool": {
    "name": "string",
    "version": "string"
  },
  "source": {
    "type": "raw_image|e01_image",
    "size_bytes": 0,
    "sector_size": 512
  },
  "hashes": {
    "source_sha256": "64 lowercase hex",
    "merkle_root_sha256": "64 lowercase hex"
  },
  "chunking": {
    "chunk_size": 67108864,
    "chunking_mode": "fixed",
    "compression": "none|zstd",
    "hash_algorithm": "sha256"
  },
  "indexes": {
    "physical_to_chunk": "maps/physical_to_chunk.parquet"
  }
}
```

Rules:
- offf_version MUST be present and MUST follow semantic versioning x.y.z.
- source.type MUST accurately describe evidence input type.
- chunking.hash_algorithm MUST be sha256 in v0.1.0.
- hashes.source_sha256 MUST be SHA-256 of reconstructed evidence stream bytes.
- hashes.merkle_root_sha256 MUST equal root derived from leaves.parquet sequence order.

## 4. Chunk Schema

Chunk storage:
- chunk ID MUST be plaintext SHA-256 represented as sha256:<64 lowercase hex>.
- chunk path MUST be:
  - chunks/sha256/<h[0..2]>/<h[2..4]>/<h>.chunk

Chunk metadata row schema (physical_to_chunk.parquet):
- sequence: u64, required
- source_offset: u64, required
- source_length: u64, required
- chunk_id: utf8, required
- stored_length: u64, required
- compression: utf8 (none|zstd), required
- plaintext_sha256: utf8 (64 lowercase hex), required
- stored_sha256: utf8 (64 lowercase hex), required

Rules:
- sequence MUST be contiguous from 0..N-1.
- source_offset MUST be monotonic and non-overlapping.
- source_length MUST be >0 for each row.
- plaintext_sha256 MUST hash decompressed/plain bytes.
- stored_sha256 MUST hash bytes as stored in .chunk file.

## 5. Hashing Rules

Global rules:
- Hash algorithm for v0.1.0 MUST be SHA-256.
- Hex digests MUST be lowercase hexadecimal and MUST be 64 chars.

Source stream hash:
- source_sha256 MUST be computed over the exact logical evidence stream bytes in order.
- For E01 input, source_sha256 MUST hash exported raw evidence stream, not container bytes.

E01 conversion metadata split:
- acquisition.json MUST separate:
  - source_container.container_sha256 (hash of E01 container file)
  - evidence_stream.stream_sha256 (hash of exported raw stream)

## 6. Merkle Tree Definition

Leaves:
- leaf_i MUST be plaintext_sha256 of chunk with sequence i.
- Leaves MUST be ordered by sequence ascending.

Parent computation:
- parent = SHA256(left_bytes || right_bytes), where child hashes are interpreted as 32-byte values.
- If a level has odd node count, last node MUST be duplicated.

Root:
- Root MUST be compared against manifest.hashes.merkle_root_sha256.

merkle_tree.bin format (version 1):
- bytes[0..4): ASCII OFFF
- byte[4]: version = 0x01
- bytes[5..9): leaf_count, u32 big-endian
- then all node hashes by level, left-to-right, each 32 bytes
- final 32 bytes: root hash repeated for fast extraction

## 7. Mapping Tables

### 7.1 maps/physical_to_chunk.parquet

Purpose:
- Physical byte offset to chunk mapping for evidence reconstruction and validation.

Constraints:
- Rows MUST be sorted by sequence or sortable by sequence without ambiguity.
- source_offset + source_length MUST reconstruct stream length equal to manifest.source.size_bytes.

### 7.2 hashes/leaves.parquet

Required columns:
- sequence: u64
- chunk_id: utf8
- plaintext_sha256: utf8

Constraints:
- Every sequence in physical_to_chunk MUST appear exactly once in leaves.
- plaintext_sha256 in leaves SHOULD match physical_to_chunk plaintext_sha256.

## 8. Provenance Model

File: provenance/chain_of_custody.jsonl
Format: JSONL (one event per line)

Event schema:
- event_id: string (monotonic within container)
- timestamp: RFC3339 string
- actor: string
- action: string
- tool.name: string
- tool.version: string
- details: object

Rules:
- Provenance MUST be append-only.
- Existing event lines MUST NOT be modified in place.
- Every mutating operation outside evidence layer SHOULD append one provenance event.
- event_id uniqueness MUST hold within a container.

## 9. Index Formats

### 9.1 indexes/partition_table.json

Required top-level fields:
- generated_at
- generated_by_tool
- container_id
- sector_size
- partition_table_type
- partitions[]

Partition entry fields:
- partition_id, partition_type, start_offset, length, first_lba, last_lba, chunk_refs

Optional fields:
- name, type_guid, unique_guid, attributes, bootable, filesystem_type

Rules:
- start_offset and length MUST be byte-accurate relative to source stream.
- chunk_refs MUST include all chunks overlapping partition byte range.

### 9.2 indexes/filesystems/<partition_id>/file_index.parquet

Required columns:
- file_id (u64)
- filesystem_id (utf8)
- partition_id (utf8)
- path (utf8)
- filename (utf8)
- extension (utf8)
- size_bytes (u64)
- physical_extents (utf8 JSON)
- chunk_refs (utf8 JSON)
- is_directory (bool)
- is_deleted (bool)
- parser (utf8)
- parser_version (utf8)
- parser_status (utf8: ok|partial|error)
- parser_error (utf8)

Timestamp columns (utf8 RFC3339 or empty):
- created_at, modified_at, accessed_at, changed_at

Rules:
- Parser failures MUST be represented with parser_status partial/error; records MUST NOT silently disappear.

## 10. Analysis Output Schemas

### 10.1 analysis/keyword_hits.parquet

Required columns:
- hit_id, job_id, keyword, chunk_id, physical_offset, file_id,
  context_before, context_after, encoding, worker_id, timestamp

Rules:
- physical_offset MUST reference source stream offset.
- chunk_id MUST reference an existing chunk in maps/physical_to_chunk.parquet.

### 10.2 analysis/yara_hits.parquet

Required columns:
- hit_id, job_id, rule_name, ruleset_hash, chunk_id,
  physical_offset, match_length, file_id, worker_id, timestamp

Rules:
- ruleset_hash SHOULD identify exact rule text used.

### 10.3 analysis/annotations.jsonl

Event model:
- annotation_id, timestamp, actor, origin(human|ai), annotation_type, target
- optional fields: label, comment, classification, confidence,
  input_scope, model_name, model_version, model_hash, correction_of

Rules:
- Annotation stream MUST be append-only.
- AI-origin events MUST include model_name, model_version, model_hash when available.
- Corrections SHOULD reference prior annotation via correction_of.

## 11. Validation Rules

A container is VALID if all mandatory checks pass.

Mandatory checks:
- manifest.json exists and is parseable
- manifest.offf_version supported
- physical_to_chunk table exists and parseable
- every referenced chunk exists
- stored_sha256 verification passes for all checked chunks
- plaintext_sha256 verification passes for all checked chunks
- merkle_tree.bin parseable and root consistent with manifest (full mode)
- source stream hash equals manifest.hashes.source_sha256 (full mode)
- required container files exist
- provenance file exists and is non-empty

Subset mode:
- Validator MAY support subset chunk validation.
- In subset mode, Merkle and full source-hash checks MAY be skipped and MUST be reported as warnings, not silent omissions.

## 12. Versioning

Version fields:
- format version: manifest.offf_version
- tool versions: created_by_tool.version and provenance tool.version

Rules:
- OFFF versions MUST follow semantic versioning.
- Patch changes (x.y.Z) MUST preserve schema compatibility.
- Minor changes (x.Y.z) MAY add optional fields without breaking existing required fields.
- Major changes (X.y.z) MAY introduce breaking schema/layout changes.

## 13. Compatibility Rules

Forward compatibility:
- Readers SHOULD ignore unknown object fields unless explicitly declared critical.
- Writers MUST NOT remove or rename required v0.1.0 fields in patch/minor updates.

Backward compatibility:
- v0.1.0 readers MUST reject unsupported major versions.
- v0.1.0 readers SHOULD accept known-compatible minor/patch updates when required fields are present.

Storage compatibility:
- Implementations MUST support local filesystem paths.
- Implementations SHOULD support s3:// URIs for container read operations.
- Object-store key mapping MUST preserve relative OFFF path semantics.

## 14. Conformance Levels

Level A (Core):
- Container layout
- Manifest, acquisition, chunk map, leaves, Merkle
- Full validation

Level B (Structure):
- Partition and filesystem indexes

Level C (Distributed and Analysis):
- Job manifests, worker outputs, provenance events, annotations

A tool claiming OFFF v0.1.0 conformance MUST declare supported level(s).
