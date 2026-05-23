# offf-keyword-worker

## Doel
Scant OFFF chunk-data op keyword patronen en schrijft hits naar `analysis/keyword_hits.parquet`.

## Input
- case pad of `s3://` URI
- job manifest met taak `keyword_scan`
- scope op chunks
- keywords en encodings

## Voorbeeld
```bash
cargo run -p offf-keyword-worker -- \
  --case tests/samples/4orensics.case2.offf \
  --job tests/samples/keyword_job.json \
  --worker-id worker-1
```

## Output
- `analysis/keyword_hits.parquet`
- provenance append event voor voltooide scan

## Notes
- Ondersteunt local en S3/MinIO containers.
- Provenance append op S3 is ontworpen voor concurrente workers.
