# offf-verify

## Doel
Valideert de integriteit en volledigheid van een OFFF container.

## Checks
- manifest parse/version
- chunk stored/plaintext hashes
- merkle root consistentie
- source hash reconstructie
- verplichte bestanden aanwezig
- provenance bestand aanwezig en niet leeg

## Voorbeeld (local)
```bash
cargo run -p offf-verify -- path/to/case.offf
```

## Voorbeeld (S3/MinIO)
```bash
OFFF_S3_ENDPOINT=http://localhost:9000 \
AWS_ACCESS_KEY_ID=offfadmin \
AWS_SECRET_ACCESS_KEY=offfadmin123 \
AWS_REGION=us-east-1 \
cargo run -p offf-verify -- s3://bucket/prefix
```

## Exit codes
- `0`: VALID
- `1`: INVALID
