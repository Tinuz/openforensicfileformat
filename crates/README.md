# OFFF Rust Crates

## Purpose
This directory contains all Rust crates that implement OFFF conversion, verification, indexing, access services, and workers.

## Main Crates
- `offf-core`: Shared domain types, chunk/hash logic, storage primitives, parquet helpers.
- `offf-convert`: Raw/E01 to OFFF conversion.
- `offf-verify`: Container integrity and consistency verification.
- `offf-index`: Partition/filesystem/object indexing.
- `offf-export`: Reconstruction and packed container export/import helpers.
- `offf-jobs`: Job orchestration and runtime state artifacts.
- `offf-access-service`: REST/gRPC access surface with policy/capability checks.
- `offf-keyword-worker`: Keyword scanning worker.
- `offf-yara-worker`: YARA scanning worker.
- `offf-annotate`: Annotation and append-only annotation workflows.

## Build and Test
```bash
cargo check --workspace
cargo test --workspace -- --nocapture
```

## Development Guidelines
- Keep public types in `offf-core` stable and backward-aware.
- Use smoke/E2E coverage for environment-dependent changes.
- Emit provenance events for write operations where applicable.
