# offf-yara-worker

## Purpose
Scans OFFF chunks with YARA rules and writes hits to `analysis/yara_hits.parquet`.

## Input
- Case path or `s3://` URI.
- Job manifest with task `yara_scan`.
- `rules_inline` and `rules_hash` payloads.

## Example
```bash
cargo run -p offf-yara-worker -- \
  --case tests/samples/4orensics.case2.offf \
  --job tests/samples/yara_job_simple.json \
  --worker-id worker-yara-1
```

## Output
- `analysis/yara_hits.parquet`
- provenance event with scan metadata

## Notes
- Uses the `yara-x` compiler/scanner.
- Works with local and S3/MinIO cases.
