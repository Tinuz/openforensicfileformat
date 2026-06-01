# Integration Tests

## Purpose
This crate validates core OFFF behavior at component level:
- chunking and hash consistency
- Merkle consistency
- round-trip reconstruction
- parquet index integrity
- corruption detection

## Run
```bash
cargo test -p offf-integration-tests -- --nocapture
```

## Coverage
Key scenarios include:
- small and non-chunk-aligned images
- `none` and `zstd` compression
- manifest and Merkle verification
- provenance baseline checks

## Extending the Suite
1. Add a reproducible helper dataset first.
2. Add at least one positive and one negative test path.
3. Keep assertions deterministic.
