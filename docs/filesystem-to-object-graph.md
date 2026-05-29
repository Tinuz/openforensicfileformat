# Filesystem To Object Graph

This document describes the OFFF-native bridge from filesystem index rows to object graph rows.

## Why this step exists

`offf-index filesystem` produces forensic filesystem metadata in `indexes/filesystems/*/file_index.parquet`.
Analysis workers consume object-level rows. The object graph builder closes this gap without external extraction tooling.

Pipeline:

```text
raw/dd image -> OFFF container -> partitions index -> filesystem index -> object graph index
```

## CLI

Build object graph from filesystem indexes:

```bash
cargo run -p offf-index -- objects <case>.offf --from-filesystem
```

Choose hashing mode:

```bash
cargo run -p offf-index -- objects <case>.offf --from-filesystem --hash-content deferred
cargo run -p offf-index -- objects <case>.offf --from-filesystem --hash-content eager
```

Run full indexing pipeline:

```bash
cargo run -p offf-index -- full <case>.offf --hash-content deferred
```

## Outputs

The builder writes both canonical JSONL and Parquet indexes:

- `indexes/objects/object_index.jsonl`
- `indexes/objects/object_edges.jsonl`
- `indexes/objects/derivations.jsonl`
- `indexes/objects/object_index.parquet`
- `indexes/objects/object_edges.parquet`
- `indexes/objects/derivations.parquet`

## Mapping summary

Per non-deleted non-directory file row:

- create `filesystem_file` object row
- attach `content_ref` with `filesystem_id`, `file_id`, and `file_index_path`
- create parent filesystem object if absent
- create `contains` edge from filesystem object to file object
- create derivation row with `method=filesystem_index_materialization` and `storage_mode=referenced_only`

## Hashing modes

- `deferred` (default): `sha256=null`, `content_hash_status=deferred`
- `eager`: reconstruct bytes with verified OFFF reads and store `sha256:...`, `content_hash_status=verified`

## Idempotency and determinism

- Object, edge, and derivation IDs are deterministic.
- Rows are sorted by ID before writing.
- JSONL output is canonical and stable across reruns for identical inputs.
- If output differs from existing JSONL files, the command fails unless `--force` is provided.
