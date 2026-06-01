# offf-access-service

## Purpose
Provides REST and gRPC access to OFFF cases for read operations and controlled append-only writes (analysis/provenance), with capability and policy enforcement.

## Features
- Manifest/chunk/file retrieval endpoints.
- List/query endpoints for files and artifacts.
- Append analysis result writes.
- Append provenance event writes.
- Authorization by role and write layer.
- Local filesystem and `s3://` parity support (MinIO/Ceph/S3).

## Quick Start
```bash
cargo run -p offf-access-service -- --help
```

## Common Test Commands
```bash
cargo test -p offf-access-service --test grpc_smoke -- --nocapture
cargo test -p offf-access-service --tests -- --nocapture
```

## Configuration Notes
- Use tool-registry configuration from `config/` for governance enforcement.
- Use sample OFFF containers from `tests/samples` for smoke runs.
- For object storage, set `OFFF_CASES_ROOT` to an `s3://bucket/prefix` root or pass `case_id` as a full `s3://` URI.
- For MinIO/Ceph endpoint compatibility, configure `OFFF_S3_ENDPOINT`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and optionally `AWS_REGION`.

## Storage Parity
`tests/grpc_storage_parity.rs` validates local vs `s3://` case path parity.
S3 parity tests are auto-skipped when `OFFF_S3_ENDPOINT` and `OFFF_S3_TEST_BUCKET` are not set.
