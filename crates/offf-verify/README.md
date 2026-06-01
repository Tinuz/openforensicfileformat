# offf-verify

## Purpose
Validates the integrity and completeness of an OFFF container.

## Core Checks
- manifest parse/version validity
- chunk stored/plaintext hashes
- Merkle root consistency
- source hash reconstruction
- required files are present
- provenance file is present and non-empty

## Example (local)
```bash
cargo run -p offf-verify -- path/to/case.offf
```

## Example (S3/MinIO)
```bash
OFFF_S3_ENDPOINT=http://localhost:9000 \
AWS_ACCESS_KEY_ID=offfadmin \
AWS_SECRET_ACCESS_KEY=offfadmin123 \
AWS_REGION=us-east-1 \
cargo run -p offf-verify -- s3://bucket/prefix
```

## Exit Codes
- `0`: VALID
- `1`: INVALID
