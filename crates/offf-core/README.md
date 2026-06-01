# offf-core

## Purpose
`offf-core` contains shared domain logic used across OFFF crates:
- chunk read/write/verify primitives
- hash and Merkle utilities
- parquet I/O helpers
- provenance append helpers
- storage abstraction for local and S3
- packed container helpers (`.offfpack` pack/list/unpack)
- central type definitions and error handling

## Usage
This crate is consumed by other crates and is typically not run as a standalone CLI.

## Important Behavior
- S3/MinIO access uses the endpoint configured by `OFFF_S3_ENDPOINT`.
- Custom endpoints use path-style addressing for compatibility.
- S3 JSONL appends use optimistic-concurrency retries.

## Test Command
```bash
cargo test -p offf-core -- --nocapture
```
