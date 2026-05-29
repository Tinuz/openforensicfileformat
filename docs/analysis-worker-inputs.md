# Analysis Worker Inputs

OFFF workers consume object-level inputs and resolve bytes through OFFF verified reads.

## Worker contract

Workers SHOULD receive or resolve:

- `object_id`
- `object_type`
- `logical_path`
- `media_type` (if known)
- `size_bytes`
- `content_ref` and/or `storage_ref`

Workers SHOULD NOT depend on host paths or external extraction bridges.

## Read path

Preferred API path:

1. resolve object row from object index
2. call verified read (`read_object_verified`)
3. process bytes
4. write output artifacts and result manifest

For filesystem-backed objects, verified read resolves `content_ref` to `file_index` extents and reconstructs bytes from OFFF chunks.

## Provenance and source references

Result rows SHOULD preserve source references (`object_id`, `file_id`, `filesystem_id`, `provenance_ref`) so analysis output remains auditable.

## No external extraction dependency

Workers must not require:

- direct host filesystem paths
- external disk-image extraction glue
- ad-hoc pytsk3 bridges

OFFF indexing + verified read APIs provide the canonical ingestion path.
