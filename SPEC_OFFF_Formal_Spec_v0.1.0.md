# OFFF Formal Specification v0.1.0 / v0.2.0

Status: Draft (implementation-aligned, updated 2026-05-29)

This document defines the normative OFFF specification baseline.  
Sections marked **[v0.2.0]** document additions introduced in OFFF v0.2.0;  
unmarked sections apply to both v0.1.0 and v0.2.0.

Normative keywords are interpreted as defined in RFC 2119: MUST, MUST NOT, SHOULD, SHOULD NOT, MAY.

## 1. Scope

This specification defines:
- container layout
- manifest schema
- acquisition schema
- chunk schema
- hashing rules
- Merkle tree definition
- mapping tables
- provenance model
- index formats
- object lineage model [v0.2.0]
- analysis output schemas
- job manifest and result manifest schemas [v0.2.0]
- parallel job / shard model [v0.2.0]
- extension points model [v0.2.0]
- validation rules and profiles
- versioning
- compatibility rules
- conformance levels

## 2. Container Layout

An OFFF container MUST be directory-based and MUST contain at least the following structure:

```text
<case>.offf/
  manifest.json
  acquisition.json
  chunks/                                   # block_image only
    sha256/
      <2 hex>/<2 hex>/<64 hex>.chunk
  hashes/                                   # block_image only
    leaves.parquet
    merkle_tree.bin
  maps/                                     # block_image only
    physical_to_chunk.parquet
  provenance/
    chain_of_custody.jsonl
  indexes/
    partition_table.json                    # block_image only
    filesystems/<partition_id>/
      file_index.parquet
    objects/                                # [v0.2.0] object lineage
      object_index.parquet
      object_edges.parquet
      derivations.parquet
      object_events.jsonl
      object_edge_events.jsonl
  jobs/                                     # [v0.2.0]
    <job_id>.json
    <job_id>/
      shard_plan.json
      <shard_id>/
        shard_manifest.json
        shard_result_manifest.json
  analysis/
    jobs/                                   # [v0.2.0] job-scoped outputs
      <job_id>/
        result_manifest.json
        <worker_output_files>
        errors.jsonl
    annotations.jsonl
  derived/                                  # [v0.2.0] content-addressed derived objects
    objects/
      sha256/<2 hex>/<2 hex>/<64 hex>.bin
  extensions/                               # [v0.2.0] generic extension points
    labels/labels.jsonl
    scopes/scopes.jsonl
    sets/working_sets.jsonl
    sets/release_sets.jsonl
    sets/exclusion_sets.jsonl
    decisions/decisions.jsonl
    policies/policy_refs.jsonl
    access/access_events.jsonl
    access/denied_access_events.jsonl
    audit/audit_events.jsonl
  signatures/
```

Rules:
- All paths in manifest/index references MUST be relative to container root.
- The evidence layer (`chunks/`, `hashes/`, `maps/`) MUST be immutable after successful
  conversion. This layer is absent for `file_collection` and other non-block-image modes.
- Indexes and analysis outputs MUST NOT overwrite evidence bytes.
- Workers MUST write only to `analysis/jobs/{job_id}/`, `provenance/`, and `indexes/objects/`
  (via Access Service); direct writes to the evidence layer are forbidden.
- For object storage use (S3/MinIO), object keys MUST map 1:1 to these relative paths.

## 3. Manifest Schema

File: `manifest.json`  
Format: JSON object

### 3.1 block_image container (v0.1.0)

Required fields for `acquisition_mode: block_image` (or absent, which implies block_image):

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

### 3.2 file_collection container [v0.2.0]

Required fields for `acquisition_mode: file_collection`:

```json
{
  "offf_version": "0.2.0",
  "container_id": "urn:offf:case:<id>",
  "created_at": "RFC3339 timestamp",
  "created_by_tool": { "name": "string", "version": "string" },
  "acquisition_mode": "file_collection",
  "evidence_roots": [
    {
      "root_id": "string",
      "root_type": "file_collection",
      "description": "optional",
      "object_count": 0,
      "root_hash": "sha256:..."
    }
  ],
  "limitations": ["No sector-level integrity; completeness cannot be verified"],
  "indexes": {
    "object_index": "indexes/objects/object_index.parquet",
    "object_edges": "indexes/objects/object_edges.parquet"
  }
}
```

### 3.3 Acquisition mode values [v0.2.0]

| Value | Meaning |
|---|---|
| `block_image` | Full byte-stream image of storage medium (raw/dd/E01). Default for v0.1.0 containers. |
| `file_collection` | Collection of individual files seized as evidence. No chunk layer. |
| `logical_extraction` | Logical extraction from device, app, cloud service, or mailbox. |
| `api_export` | Export received via an API (cloud, SaaS). |
| `mixed` | Multiple evidence roots with different acquisition modes. |

### 3.4 Extensions namespace [v0.2.0]

Containers with `offf_version: 0.2.0` MAY include a top-level `extensions` object:

```json
{
  "extensions": {
    "namespace:key": { ... }
  }
}
```

- Keys MUST follow the `namespace:name` pattern.
- Unknown extension keys MUST be ignored by conformant readers.

Rules:
- `offf_version` MUST be present and MUST follow semantic versioning `x.y.z`.
- `source.type` MUST accurately describe evidence input type (block_image containers).
- `chunking.hash_algorithm` MUST be `sha256` in v0.1.0/v0.2.0.
- `hashes.source_sha256` MUST be SHA-256 of reconstructed evidence stream bytes.
- `hashes.merkle_root_sha256` MUST equal root derived from `leaves.parquet` sequence order.
- `limitations` MUST be present for `file_collection` containers.
- `acquisition_mode` absent implies `block_image` (v0.1.0 backward compat).

## 4. Acquisition Schema [v0.2.0]

File: `acquisition.json`  
Format: JSON object

```json
{
  "container_id":        "urn:offf:case:<id>",
  "acquisition_id":      "urn:offf:acq:<id>",
  "acquisition_mode":    "block_image|file_collection|logical_extraction|api_export|mixed",
  "acquired_at":         "RFC3339 timestamp",
  "acquired_by":         "string (actor/operator name)",
  "method":              "string (e.g. dd, ewfacquire, filesystem-copy)",
  "tool": {
    "name":    "string",
    "version": "string"
  },
  "source": {
    "type":        "raw_image|e01_image|directory|cloud_api|...",
    "size_bytes":  0,
    "sector_size": 512
  },
  "source_context": {
    "device_serial":  "optional",
    "device_model":   "optional",
    "device_vendor":  "optional"
  },
  "source_container": {
    "container_type":  "E01",
    "container_sha256": "sha256:<hex>"
  },
  "evidence_stream": {
    "stream_sha256":  "sha256:<hex>",
    "size_bytes":     0
  },
  "parameters": {
    "key": "value"
  },
  "limitations": ["string"]
}
```

Rules:
- `acquisition_id` MUST be unique across containers in an investigation.
- `acquisition_mode` MUST match the value in `manifest.json` when both are present.
- `source_container` MUST be present for E01 inputs and MUST record the container-file hash.
- `evidence_stream` MUST be present for block_image mode and MUST record the raw-stream hash.
- `limitations` MUST be present and non-empty for `file_collection` mode.
- For `file_collection` containers, `source.size_bytes` is sum of evidence file sizes.

## 5. Chunk Schema

Chunk storage:
- Chunk ID MUST be plaintext SHA-256 represented as `sha256:<64 lowercase hex>`.
- Chunk path MUST be: `chunks/sha256/<h[0..2]>/<h[2..4]>/<h>.chunk`

Chunk metadata row schema (`physical_to_chunk.parquet`):
- `sequence`: u64, required
- `source_offset`: u64, required
- `source_length`: u64, required
- `chunk_id`: utf8, required
- `stored_length`: u64, required
- `compression`: utf8 (`none|zstd`), required
- `plaintext_sha256`: utf8 (64 lowercase hex), required
- `stored_sha256`: utf8 (64 lowercase hex), required

Rules:
- `sequence` MUST be contiguous from 0..N-1.
- `source_offset` MUST be monotonic and non-overlapping.
- `source_length` MUST be >0 for each row.
- `plaintext_sha256` MUST hash decompressed/plain bytes.
- `stored_sha256` MUST hash bytes as stored in the `.chunk` file.

## 6. Hashing Rules

Global rules:
- Hash algorithm for v0.1.0/v0.2.0 MUST be SHA-256.
- Hex digests MUST be lowercase hexadecimal and MUST be 64 chars.

Source stream hash:
- `source_sha256` MUST be computed over the exact logical evidence stream bytes in order.
- For E01 input, `source_sha256` MUST hash the exported raw evidence stream, not the container file.

E01 conversion metadata split:
- `acquisition.json` MUST separate:
  - `source_container.container_sha256` (hash of E01 container file)
  - `evidence_stream.stream_sha256` (hash of exported raw stream)

Root hash for file_collection [v0.2.0]:
- For `file_collection` containers, `evidence_roots[].root_hash` SHOULD be computed as
  the SHA-256 of the newline-joined sorted SHA-256 hashes of all root evidence files.

## 7. Merkle Tree Definition

Leaves:
- `leaf_i` MUST be `plaintext_sha256` of chunk with sequence `i`.
- Leaves MUST be ordered by sequence ascending.

Parent computation:
- `parent = SHA256(left_bytes || right_bytes)`, where child hashes are 32-byte values.
- If a level has odd node count, the last node MUST be duplicated.

Root:
- Root MUST be compared against `manifest.hashes.merkle_root_sha256`.

`merkle_tree.bin` format (version 1):
- bytes[0..4): ASCII `OFFF`
- byte[4]: version = 0x01
- bytes[5..9): leaf_count, u32 big-endian
- then all node hashes by level, left-to-right, each 32 bytes
- final 32 bytes: root hash (repeated for fast extraction)

## 8. Mapping Tables

### 8.1 maps/physical_to_chunk.parquet

Purpose: physical byte offset to chunk mapping for evidence reconstruction and validation.

Constraints:
- Rows MUST be sortable by `sequence` without ambiguity.
- `source_offset + source_length` summed over all rows MUST equal `manifest.source.size_bytes`.

### 8.2 hashes/leaves.parquet

Required columns:
- `sequence`: u64
- `chunk_id`: utf8
- `plaintext_sha256`: utf8

Constraints:
- Every sequence in `physical_to_chunk` MUST appear exactly once in `leaves`.
- `plaintext_sha256` in `leaves` SHOULD match the `physical_to_chunk` value.

## 9. Provenance Model

File: `provenance/chain_of_custody.jsonl`  
Format: JSONL (one event per line)

Event schema:
- `event_id`: string (monotonic within container)
- `timestamp`: RFC3339 string
- `actor`: string
- `action`: string
- `tool.name`: string
- `tool.version`: string
- `details`: object

Rules:
- Provenance MUST be append-only.
- Existing event lines MUST NOT be modified in place.
- Every mutating operation outside the evidence layer SHOULD append one provenance event.
- `event_id` uniqueness MUST hold within a container.

## 10. Index Formats

### 10.1 indexes/partition_table.json

Required top-level fields:
- `generated_at`, `generated_by_tool`, `container_id`, `sector_size`,
  `partition_table_type`, `partitions[]`

Partition entry fields:
- `partition_id`, `partition_type`, `start_offset`, `length`,
  `first_lba`, `last_lba`, `chunk_refs`

Optional fields:
- `name`, `type_guid`, `unique_guid`, `attributes`, `bootable`, `filesystem_type`

Rules:
- `start_offset` and `length` MUST be byte-accurate relative to source stream.
- `chunk_refs` MUST include all chunks overlapping the partition byte range.

### 10.2 indexes/filesystems/\<partition_id\>/file_index.parquet

Required columns:

| Column | Type | Notes |
|---|---|---|
| `file_id` | u64 | |
| `filesystem_id` | utf8 | |
| `partition_id` | utf8 | |
| `path` | utf8 | |
| `filename` | utf8 | |
| `extension` | utf8 | |
| `size_bytes` | u64 | |
| `physical_extents` | utf8 JSON | |
| `chunk_refs` | utf8 JSON | |
| `is_directory` | bool | |
| `is_deleted` | bool | |
| `is_sparse` | bool | **[v0.2.0]** |
| `is_compressed` | bool | **[v0.2.0]** |
| `is_encrypted` | bool | **[v0.2.0]** |
| `ads_streams` | utf8 JSON | **[v0.2.0]** Alternate data streams |
| `parser` | utf8 | |
| `parser_version` | utf8 | |
| `parser_status` | utf8 | `ok\|partial\|error` |
| `parser_error` | utf8 | |

Timestamp columns (utf8 RFC3339 or empty):
- `created_at`, `modified_at`, `accessed_at`, `changed_at`

Rules:
- Parser failures MUST be recorded with `parser_status partial/error`; records MUST NOT silently disappear.
- `ads_streams` is a JSON array of `{name, size_bytes, sha256}` objects.

### 10.3 indexes/objects/ — Object Lineage Indexes [v0.2.0]

Object lineage indexes are required for `file_collection` and `logical_extraction` containers
and SHOULD be used whenever a container uses `evidence_roots`. They track the discovered object
graph and enable reproducible re-analysis.

#### 10.3.1 object_index.parquet — DiscoveredObjectRow

| Column | Type | Notes |
|---|---|---|
| `object_id` | utf8 | Stable, unique `urn:offf:obj:<id>` |
| `object_type` | utf8 | `file\|directory\|container\|email\|artifact\|...` |
| `name` | utf8 | Filename or display name (nullable) |
| `logical_path` | utf8 | Full logical path within container (nullable) |
| `media_type` | utf8 | MIME type (nullable) |
| `size_bytes` | u64 | (nullable) |
| `sha256` | utf8 | Content hash (nullable) |
| `source_layer` | utf8 | `evidence\|derived\|analysis` |
| `storage_ref` | utf8 | Path to content in `derived/objects/` (nullable) |
| `root_id` | utf8 | Links to `evidence_roots[].root_id` (nullable) |
| `collection_relative_path` | utf8 | Path relative to collection root (nullable) |
| `created_by_job_id` | utf8 | Job that discovered this object (nullable) |
| `parser_status` | utf8 | `ok\|partial\|error` |
| `provenance_ref` | utf8 | `event_id` from provenance (nullable) |
| `schema_version` | utf8 | |
| `original_created_at` | utf8 | Original filesystem timestamp (nullable) |
| `original_modified_at` | utf8 | (nullable) |
| `original_accessed_at` | utf8 | (nullable) |

#### 10.3.2 object_edges.parquet — ObjectEdgeRow

| Column | Type | Notes |
|---|---|---|
| `edge_id` | utf8 | Unique |
| `parent_object_id` | utf8 | |
| `child_object_id` | utf8 | |
| `relation_type` | utf8 | `contains\|derived_from\|attached_to\|...` |
| `method` | utf8 | Extraction method (nullable) |
| `logical_path` | utf8 | (nullable) |
| `sequence` | u64 | Order within parent (nullable) |
| `created_by_job_id` | utf8 | (nullable) |
| `provenance_ref` | utf8 | (nullable) |
| `schema_version` | utf8 | |

#### 10.3.3 indexes/objects/derivations.parquet — DerivationRow

Records every tool-derived object transform (extraction, decryption, conversion, etc.):

| Column | Type |
|---|---|
| `derivation_id` | utf8 |
| `parent_object_id` | utf8 |
| `child_object_id` | utf8 |
| `job_id` | utf8 |
| `method` | utf8 |
| `tool_id` | utf8 |
| `tool_name` | utf8 |
| `tool_version` | utf8 |
| `parameters_hash` | utf8 (nullable) |
| `input_sha256` | utf8 (nullable) |
| `output_sha256` | utf8 (nullable) |
| `storage_mode` | utf8 |
| `provenance_ref` | utf8 (nullable) |
| `created_at` | utf8 |
| `schema_version` | utf8 |

#### 10.3.4 object_events.jsonl and object_edge_events.jsonl

Append-only event logs that serve as the authoritative rebuild source for
`object_index.parquet` and `object_edges.parquet`. The Parquet indexes are
derived state and can be rebuilt via `offf-index objects --from-events`.

**ObjectEvent** schema (one JSON object per line):
```json
{
  "event_id":      "uuid",
  "timestamp":     "RFC3339",
  "event_type":    "discovered|updated|removed",
  "object_id":     "urn:offf:obj:<id>",
  "job_id":        "optional",
  "payload":       { "...DiscoveredObjectRow fields..." },
  "schema_version": "0.2.0"
}
```

**ObjectEdgeEvent** schema:
```json
{
  "event_id":         "uuid",
  "timestamp":        "RFC3339",
  "event_type":       "discovered|removed",
  "edge_id":          "uuid",
  "source_object_id": "urn:offf:obj:<id>",
  "target_object_id": "urn:offf:obj:<id>",
  "relationship":     "optional",
  "job_id":           "optional",
  "schema_version":   "0.2.0"
}
```

Rules:
- Event logs MUST be append-only.
- A `removed` event does NOT physically delete prior events; it logically tombstones the object or edge.
- Parquet rebuild MUST produce the same logical state as replaying all events in order.

## 11. Analysis Output Schemas

### 11.1 Job-scoped output layout [v0.2.0]

Workers MUST write outputs to `analysis/jobs/{job_id}/`:

```text
analysis/jobs/<job_id>/
  result_manifest.json    # written by worker on completion
  keyword_hits.parquet    # or yara_hits.parquet, custom_hits.parquet, etc.
  errors.jsonl            # optional: per-object error records
```

Flat `analysis/` paths (v0.1.0-era `analysis/keyword_hits.parquet`) are still
valid for the **legacy** conformance profile (see § 15) but MUST NOT be emitted by
v0.2.0-conformant workers.

### 11.2 analysis/jobs/\<job_id\>/result_manifest.json [v0.2.0]

Written by the worker (or aggregated by the Access Service) on job completion.

```json
{
  "job_id":          "uuid",
  "parent_job_id":   "optional uuid",
  "status":          "completed|failed|partial",
  "worker": {
    "tool_id":       "string",
    "name":          "string",
    "version":       "string",
    "binary_sha256": "optional sha256:..."
  },
  "outputs": [
    {
      "path":       "relative path within analysis/jobs/<job_id>/",
      "sha256":     "sha256:<hex>",
      "schema_ref": "optional JSON Schema URI"
    }
  ],
  "statistics": {
    "objects_in_scope":  0,
    "objects_processed": 0,
    "objects_success":   0,
    "objects_error":     0,
    "objects_skipped":   0
  },
  "created_at":    "RFC3339",
  "completed_at":  "optional RFC3339"
}
```

### 11.3 analysis/jobs/\<job_id\>/errors.jsonl [v0.2.0]

Optional. One JSON object per line describing per-object processing failures:

```json
{
  "input_id":    "string",
  "object_id":   "string",
  "error_code":  "string",
  "message":     "string",
  "timestamp":   "RFC3339"
}
```

### 11.4 analysis/jobs/\<job_id\>/keyword_hits.parquet

Required columns:
- `hit_id`, `job_id`, `keyword`, `chunk_id`, `physical_offset`, `file_id`,
  `context_before`, `context_after`, `encoding`, `worker_id`, `timestamp`

Rules:
- `physical_offset` MUST reference source stream offset.
- `chunk_id` MUST reference an existing chunk in `maps/physical_to_chunk.parquet` (block_image only).

### 11.5 analysis/jobs/\<job_id\>/yara_hits.parquet

Required columns:
- `hit_id`, `job_id`, `rule_name`, `ruleset_hash`, `chunk_id`,
  `physical_offset`, `match_length`, `file_id`, `worker_id`, `timestamp`

Rules:
- `ruleset_hash` SHOULD be `sha256:<hex>` of the exact YARA rule text used.

### 11.6 analysis/annotations.jsonl

Event model:
- `annotation_id`, `timestamp`, `actor`, `origin` (`human|ai`), `annotation_type`, `target`
- optional: `label`, `comment`, `classification`, `confidence`,
  `input_scope`, `model_name`, `model_version`, `model_hash`, `correction_of`

Rules:
- Annotation stream MUST be append-only.
- AI-origin events MUST include `model_name`, `model_version`, `model_hash` when available.
- Corrections SHOULD reference prior annotation via `correction_of`.

## 12. Job Manifest Schema [v0.2.0]

File: `jobs/<job_id>.json`  
Written before or at the time of dispatch; defines what a worker will process.

```json
{
  "job_id":       "uuid",
  "tool_id":      "string",
  "created_at":   "RFC3339",
  "input_scope": {
    "mode":              "all_objects|scoped|selected",
    "root_ids":          ["string"],
    "object_types":      ["file|container|..."],
    "media_types":       ["string"],
    "parser_statuses":   ["ok|partial|error"],
    "labels":            ["string"],
    "sets":              ["string"],
    "select":            { "object_ids": [], "file_ids": [], "artifact_ids": [] },
    "exclude":           { "labels": [], "sets": [] },
    "limits":            { "max_object_size_bytes": null, "min_object_size_bytes": null }
  },
  "output_contract": {
    "may_produce_results":     true,
    "may_produce_objects":     false,
    "may_materialize_objects": false,
    "may_produce_edges":       false,
    "may_produce_derivations": false
  },
  "scope_ref":     "optional scope_id from extensions/scopes/",
  "include_sets":  ["set_id"],
  "policy_refs":   ["policy_ref"]
}
```

Rules:
- `job_id` MUST be UUID v4.
- `output_contract` declares the worker's capabilities so the Access Service can enforce write isolation.
- `scope_ref` references a `ScopeRecord.scope_id` from `extensions/scopes/scopes.jsonl`.

## 13. Parallel Job / Shard Model [v0.2.0]

For large input scopes a worker MAY split work across shards.

### 13.1 jobs/\<job_id\>/shard_plan.json — ShardPlanRecord

```json
{
  "parent_job_id":    "uuid",
  "shard_plan_id":    "uuid",
  "strategy":         "size|count|hash_range|single",
  "shard_count":      8,
  "input_count":      10000,
  "input_scope_hash": "sha256:<hex>",
  "created_at":       "RFC3339",
  "created_by":       "string"
}
```

### 13.2 jobs/\<job_id\>/\<shard_id\>/shard_manifest.json — ShardManifest

```json
{
  "shard_id":          "uuid",
  "parent_job_id":     "uuid",
  "shard_index":       0,
  "shard_count":       8,
  "input_scope_hash":  "sha256:<hex>",
  "input_objects":     [{ "input_id": "input-000001", "object_id": "urn:offf:obj:<id>" }],
  "output_base_path":  "analysis/jobs/<job_id>/<shard_id>/",
  "status":            "planned|in_progress|completed|failed"
}
```

### 13.3 jobs/\<job_id\>/\<shard_id\>/shard_result_manifest.json — ShardResultManifest

Written by the worker shard on completion:

```json
{
  "job_id":          "uuid (equals shard_id for shard results)",
  "parent_job_id":   "uuid",
  "shard_id":        "uuid",
  "status":          "completed|failed|partial",
  "worker": { "tool_id": "", "name": "", "version": "", "binary_sha256": null },
  "input": {
    "input_scope_hash": "sha256:<hex>",
    "objects_in_shard": 0
  },
  "outputs": [{ "path": "", "sha256": "", "schema_ref": null }],
  "statistics": {
    "objects_in_scope":  0, "objects_processed": 0,
    "objects_success":   0, "objects_error":     0, "objects_skipped": 0
  },
  "created_at":   "RFC3339",
  "completed_at": null
}
```

Rules:
- `input_scope_hash` in `ShardPlanRecord` and all `ShardManifest` shards MUST match the
  hash in the parent job's resolved input list.
- Aggregation of all shard `ShardResultManifest` objects produces a `ParentResultManifest`.

## 14. Extensions Model [v0.2.0]

Extension points provide governance, access control, and audit capabilities. All extension
files are append-only JSONL streams under `extensions/`.

### 14.1 extensions/labels/labels.jsonl — LabelEvent

```json
{
  "label_event_id": "uuid",
  "timestamp":      "RFC3339",
  "actor":          "string",
  "tool":           { "name": "", "version": "" },
  "target":         { "object_id": "...", "file_id": "...", "chunk_id": "..." },
  "label":          "string",
  "reason":         "optional",
  "policy_ref":     "optional",
  "provenance_ref": "optional"
}
```

### 14.2 extensions/scopes/scopes.jsonl — ScopeRecord

```json
{
  "scope_id":   "string",
  "created_at": "RFC3339",
  "created_by": "string",
  "description": "optional",
  "include": {
    "file_ids":      [], "object_ids": [], "chunk_ids": [],
    "artifact_types": [], "date_range": { "from": null, "to": null }
  },
  "exclude":      { "labels": [], "sets": [] },
  "policy_refs":  [],
  "provenance_ref": "optional"
}
```

### 14.3 extensions/sets/ — SetRecord

Set type files: `working_sets.jsonl`, `release_sets.jsonl`, `exclusion_sets.jsonl`

```json
{
  "set_id":       "string",
  "set_type":     "working_set|release_set|exclusion_set",
  "created_at":   "RFC3339",
  "created_by":   "string",
  "scope_ref":    "optional",
  "members": {
    "file_ids": [], "object_ids": [], "chunk_ids": [], "artifact_ids": []
  },
  "decision_ref": "optional",
  "policy_refs":  [],
  "provenance_ref": "optional"
}
```

### 14.4 extensions/decisions/decisions.jsonl — DecisionRecord

Generic decision types: `release`, `exclude`, `restrict`, `unrestrict`,
`review_required`, `review_completed`, `export_approved`, `export_denied`,
`processing_allowed`, `processing_denied`.

```json
{
  "decision_id":  "string",
  "timestamp":    "RFC3339",
  "actor":        { "type": "user|tool|system", "id": "", "role": null },
  "decision_type":"string",
  "target":       { "object_id": "...", "file_id": "...", "set_id": "..." },
  "outcome":      "approved|denied|pending",
  "reason":       "optional",
  "policy_refs":  [],
  "provenance_ref": "optional"
}
```

### 14.5 extensions/policies/policy_refs.jsonl — PolicyRef

```json
{
  "policy_ref":   "string",
  "policy_type":  "external|internal",
  "title":        "optional",
  "issuer":       "optional",
  "issued_at":    "optional RFC3339",
  "uri":          "optional",
  "hash":         "optional sha256:...",
  "description":  "optional",
  "provenance_ref": "optional"
}
```

### 14.6 extensions/access/ — Access Events

`access_events.jsonl` (allowed accesses):
```json
{
  "access_event_id": "uuid",
  "timestamp":       "RFC3339",
  "actor":           "string",
  "tool":            { "name": "", "version": "" },
  "action":          "string",
  "target":          { "object_id": "..." },
  "scope_ref":       "optional",
  "policy_refs":     [],
  "result":          "allowed",
  "provenance_ref":  "optional"
}
```

`denied_access_events.jsonl` (denied accesses):
```json
{
  "denied_event_id": "uuid",
  "timestamp":       "RFC3339",
  "actor":           "string",
  "action":          "string",
  "target":          { "object_id": "..." },
  "result":          "denied",
  "reason_code":     "optional",
  "scope_ref":       "optional",
  "policy_refs":     [],
  "provenance_ref":  "optional"
}
```

### 14.7 extensions/audit/audit_events.jsonl — AuditEvent

```json
{
  "audit_event_id": "uuid",
  "timestamp":      "RFC3339",
  "actor":          "string",
  "event_type":     "string",
  "target":         { "optional": "..." },
  "result":         "optional",
  "details":        {},
  "provenance_ref": "optional"
}
```

Rules (all extension streams):
- All extension streams MUST be append-only.
- Existing lines MUST NOT be modified or deleted.
- `target` fields MUST reference valid identifiers within the container.
- Readers MUST ignore unknown fields in extension records.

## 15. Validation Rules and Profiles

A validator MUST declare which profile it operates under. The following
profiles are defined (see also `docs/conformance-profiles.md`):

| Profile | Label |
|---|---|
| `core` | OFFF Core Conformant |
| `core+schemas` | OFFF Core + Schemas Conformant |
| `core+extensions` | OFFF Core + Extensions Conformant |
| `conformance` | OFFF Conformant |
| `legacy` | OFFF Legacy Analysis Conformant |
| `forensic-baseline` | OFFF Forensic Baseline Conformant |

### 15.1 Mandatory checks (all profiles)

- `manifest.json` exists and is parseable
- `manifest.offf_version` is supported
- Container ID is present and non-empty
- Required container files exist

### 15.2 Profile: core

All mandatory checks plus:
- `physical_to_chunk` table parseable (block_image containers)
- Every referenced chunk file exists
- `stored_sha256` verification passes for sampled chunks
- `plaintext_sha256` verification passes for sampled chunks
- `merkle_tree.bin` parseable and root consistent with manifest (full mode)
- Source stream hash equals `manifest.hashes.source_sha256` (full mode)
- `provenance/chain_of_custody.jsonl` exists and is non-empty

Subset mode:
- Validator MAY support subset chunk validation.
- In subset mode, Merkle and source-hash checks MAY be skipped but MUST be reported
  as explicit warnings, not silent omissions.

### 15.3 Profile: core+schemas

All `core` checks plus:
- `acquisition.json` present and parseable
- `indexes/partition_table.json` present (block_image) or `indexes/objects/` present (file_collection)
- File index schema matches § 10.2 (including v0.2.0 columns if `offf_version: 0.2.0`)

### 15.4 Profile: core+extensions

All `core+schemas` checks plus:
- All present extension JSONL files are valid JSONL and schema-conformant
- Job manifests under `jobs/` are parseable
- Result manifests under `analysis/jobs/` are parseable

### 15.5 Profile: conformance

All `core+extensions` checks plus:
- All workers have written `result_manifest.json` for their jobs
- `errors.jsonl` present (even if empty) for each job directory
- Object lineage indexes parseable and internally consistent

### 15.6 Profile: legacy

- Accepts flat `analysis/keyword_hits.parquet` and `analysis/yara_hits.parquet`
  (v0.1.0-era layout) without `jobs/` structure.
- `acquisition_mode` absent implies `block_image`.
- `source`, `hashes`, `chunking` treated as required.

### 15.7 Profile: forensic-baseline

All `conformance` checks plus the BL-01..BL-14 requirements defined in
`docs/forensic-baseline-profile.md`:
- BL-01: acquisition.json present and schema-valid
- BL-02: chain_of_custody.jsonl present, non-empty, and append-only
- BL-03: Merkle root verified
- BL-04: source stream SHA-256 verified
- BL-05: All chunk SHA-256 hashes verified
- BL-06: File index for all detected partitions present
- BL-07: All job manifests have result_manifest.json
- BL-08: No result_manifest with status=failed left unresolved
- BL-09: No evidence layer modification after container seal
- BL-10: All annotation events have provenance_ref
- BL-11: Denied access events present for access-controlled operations
- BL-12: Policy refs present for release decisions
- BL-13: container_id consistent across all files
- BL-14: Verifier report (Markdown or JSON) present and up to date

## 16. Versioning

Version fields:
- Format version: `manifest.offf_version`
- Tool versions: `created_by_tool.version` and provenance `tool.version`

Implemented versions:
- `0.1.0`: block_image only; flat analysis layout; no object lineage
- `0.2.0`: adds acquisition_mode, evidence_roots, object lineage, job-scoped analysis
  outputs, extensions model, parallel shard model

Rules:
- OFFF versions MUST follow semantic versioning.
- Patch changes (`x.y.Z`) MUST preserve schema compatibility.
- Minor changes (`x.Y.z`) MAY add optional fields without breaking existing required fields.
- Major changes (`X.y.z`) MAY introduce breaking schema/layout changes.
- Tools MUST record the version that was active when the container was created.

## 17. Compatibility Rules

Forward compatibility:
- Readers SHOULD ignore unknown object fields unless explicitly declared critical.
- Writers MUST NOT remove or rename required v0.1.0 fields in patch/minor updates.

Backward compatibility:
- v0.2.0 containers with absent `acquisition_mode` are treated as `block_image`.
- Readers targeting v0.1.0 MUST reject containers with `offf_version` major > 0.
- Readers SHOULD accept known-compatible minor/patch updates when required fields are present.

Storage compatibility:
- Implementations MUST support local filesystem paths.
- Implementations SHOULD support `s3://` URIs for container read operations.
- Object-store key mapping MUST preserve relative OFFF path semantics (path separators → `/`).

## 18. Conformance Levels

Tools MUST declare their supported conformance profile(s) and the OFFF version range they support.

| Level | Profile | Minimum Required |
|---|---|---|
| A | `core` | manifest + chunk integrity |
| B | `core+schemas` | + acquisition + partition/object indexes |
| C | `conformance` | + job manifests + result manifests + object lineage |
| D | `forensic-baseline` | + BL-01..BL-14 forensic controls |

Legacy support:
- A tool MAY additionally declare `legacy` profile support to accept v0.1.0-era flat outputs.

A tool claiming OFFF v0.1.0 / v0.2.0 conformance MUST:
1. Declare the profile(s) it implements.
2. Pass the mandatory checks for those profile(s).
3. Report any skipped checks as explicit warnings (not silent omissions).
4. Include tool name and version in `created_by_tool` for any container it creates or modifies.
