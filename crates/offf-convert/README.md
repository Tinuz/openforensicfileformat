# offf-convert

## Purpose
Converts source evidence into an OFFF container, including chunk store, hashes, maps, manifest, acquisition metadata, and provenance.

## Supported Input
- raw/dd image
- E01 (via exported raw stream)

## Example (raw/dd)
```bash
cargo run -p offf-convert -- \
  --input evidence.dd \
  --output case.offf \
  --chunk-size 64M \
  --compression zstd
```

## Example (E01)
```bash
cargo run -p offf-convert -- \
  --input sample.E01 \
  --output case.offf \
  --input-type e01 \
  --ewf-export-tool ewfexport
```

## Additional Useful Flags
```bash
cargo run -p offf-convert -- --help
```

## Notes
- On Windows, the E01 pipeline supports a dockerized `ewfexport` fallback.
- For E01 input, `acquisition.json` includes `source_container` and `evidence_stream` metadata.
