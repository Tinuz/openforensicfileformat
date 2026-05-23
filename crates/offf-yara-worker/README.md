# offf-yara-worker

## Doel
Scant OFFF chunks met YARA regels en schrijft hits naar `analysis/yara_hits.parquet`.

## Input
- case pad of `s3://` URI
- job manifest met taak `yara_scan`
- `rules_inline` en `rules_hash`

## Voorbeeld
```bash
cargo run -p offf-yara-worker -- \
  --case tests/samples/4orensics.case2.offf \
  --job tests/samples/yara_job_simple.json \
  --worker-id worker-yara-1
```

## Output
- `analysis/yara_hits.parquet`
- provenance event met scan metadata

## Notes
- Gebruikt `yara-x` compiler/scanner.
- Werkt op local en S3/MinIO cases.
