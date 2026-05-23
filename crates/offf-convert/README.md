# offf-convert

## Doel
Converteert brondata naar een OFFF container, inclusief chunk store, hashes, maps, manifest, acquisition en provenance.

## Ondersteunde input
- raw/dd image
- E01 (met export naar raw stream)

## Voorbeeld
```bash
cargo run -p offf-convert -- \
  --input evidence.dd \
  --output case.offf \
  --chunk-size 64M \
  --compression zstd
```

## E01 voorbeeld
```bash
cargo run -p offf-convert -- \
  --input sample.E01 \
  --output case.offf \
  --input-type e01 \
  --ewf-export-tool ewfexport
```

## Notes
- Op Windows ondersteunt de E01-flow dockerized `ewfexport` fallback.
- `acquisition.json` bevat bij E01 ook `source_container` en `evidence_stream` metadata.
