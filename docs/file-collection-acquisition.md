# File Collection Acquisition

`offf-collect` ingests filesystem trees into OFFF containers using the `file_collection` acquisition mode.

## Scope

- Walk a directory tree and record file evidence as OFFF objects.
- Preserve source path, size, and hash metadata in the acquisition and object indexes.
- Keep the evidence layer immutable once written.

## Output Expectations

- `manifest.json` describes a `file_collection` container.
- `acquisition.json` records acquisition metadata and source context.
- `indexes/objects/object_index.parquet` contains filesystem-backed objects.
- `indexes/objects/object_edges.parquet` records parent/child relationships.

## Current Limitations

- Dedicated file_collection integration coverage is still under active hardening.
- End-to-end verify-and-upload workflow should be treated as release-evidence work, not a demo shortcut.

See also: [filesystem to object graph](filesystem-to-object-graph.md), [object content refs](object-content-ref.md), and [test traceability](test-traceability.md).
