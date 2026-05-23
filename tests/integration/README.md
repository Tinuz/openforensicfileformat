# Integration Tests

## Doel
Deze crate valideert kern OFFF gedrag op componentniveau:
- chunking en hash-consistentie
- merkle consistentie
- round-trip reconstructie
- parquet index integriteit
- detectie van corruptie

## Start
```bash
cargo test -p offf-integration-tests -- --nocapture
```

## Dekking
Belangrijke scenario's:
- kleine en niet-chunk-aligned images
- compressie `none` en `zstd`
- manifest en merkle verificatie
- provenance-baseline

## Uitbreiden
1. Voeg eerst een reproduceerbare helperdataset toe.
2. Voeg daarna minimaal een positief en negatief testpad toe.
3. Houd assertions deterministisch.
