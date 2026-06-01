# offf-keyword-worker

## Purpose
Scans OFFF chunk data for keyword patterns and writes hits to `analysis/keyword_hits.parquet`.

## Input
- Case path or `s3://` URI.
- Job manifest with task `keyword_scan`.
- Chunk/input scope information.
- Keywords and encoding configuration.

## Example
```bash
cargo run -p offf-keyword-worker -- \
  --case tests/samples/4orensics.case2.offf \
  --job tests/samples/keyword_job.json \
  --worker-id worker-1
```

## Output
- `analysis/keyword_hits.parquet`
- provenance append event for completed scan

## Notes
- Supports local and S3/MinIO containers.
- S3 provenance append behavior is designed for concurrent workers.
