# Object Content Reference Model

OFFF object rows can reference bytes in two ways:

- `content_ref`: canonical pointer to original/referenced bytes
- `storage_ref`: path/hash for materialized object bytes

## Core rule

Filesystem-backed files are referenced, not materialized by default.

## Filesystem-backed objects

For objects created from `file_index.parquet`, use:

```json
{
  "type": "filesystem_file",
  "filesystem_id": "volume-1",
  "file_id": "file-000123",
  "file_index_path": "indexes/filesystems/volume-1/file_index.parquet"
}
```

For these objects:

- `storage_ref` MUST be `null`
- `source_layer` SHOULD be `evidence`
- bytes are reconstructed via verified OFFF reads

## Evidence object store

Use when bytes are already materialized in evidence object storage:

```json
{
  "type": "evidence_object_store",
  "storage_ref": "sha256:<hex>"
}
```

## Derived object store

Use when bytes are produced by workers/tools and stored as derived objects:

```json
{
  "type": "derived_object_store",
  "storage_ref": "derived/objects/sha256/ab/cd/<sha256>.bin"
}
```

## Hash lifecycle

`content_hash_status` values:

- `verified`
- `deferred`
- `unavailable`
- `error`
