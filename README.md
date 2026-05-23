# OFFF - Open Forensic File Format

OFFF is an open, verifiable, chunk-based forensic container and ecosystem for distributed analysis.

## Why OFFF
- Preserve forensic integrity with immutable evidence bytes.
- Scale analysis using chunk-level processing and indexed metadata.
- Keep provenance and analysis results traceable and auditable.
- Enable interoperability via open specification and JSON schemas.

## Project Status
- Stable MVP areas: core chunk storage, raw convert, verify baseline.
- Experimental areas: E01 conversion path, access service auth hardening, advanced indexing, workers.
- Planned areas: OFFF v0.2 generic extensions and scope-aware processing.

See component-level status in `docs/status.md`.

## Architecture Overview
- Evidence layer: `manifest.json`, `acquisition.json`, `chunks/`, `hashes/`, `maps/`
- Processing layer: indexing, jobs, workers
- Access layer: REST/gRPC access service with capability controls
- SDK layer: Python and Go SDKs
- Conformance layer: schema checks and profile-based tests

## Quickstart
```bash
cargo build --workspace

cargo run -p offf-convert -- \
  --input sample.dd \
  --output sample.offf \
  --chunk-size 64M \
  --compression zstd

cargo run -p offf-verify -- sample.offf

cargo run -p offf-export -- export sample.offf --output reconstructed.dd

cargo run -p offf-index -- partitions sample.offf
```

## Build And Test
```bash
cargo check --workspace
cargo test --workspace -- --nocapture
python tests/conformance/run_conformance.py
```

## Examples
- Raw/dd to OFFF:
```bash
cargo run -p offf-convert -- --input sample.dd --output sample.offf
```

- Verify container:
```bash
cargo run -p offf-verify -- sample.offf
```

- Export reconstructed image:
```bash
cargo run -p offf-export -- export sample.offf --output reconstructed.dd
```

- Index partitions:
```bash
cargo run -p offf-index -- partitions sample.offf
```

- Create and run keyword job:
```bash
cargo run -p offf-jobs -- create-keyword --case sample.offf --keywords password,secret
cargo run -p offf-jobs -- run --case sample.offf --job sample.offf/jobs/<job_id>.json
```

## Project Structure
- `crates/`: Rust crates (`offf-core`, `offf-convert`, `offf-verify`, workers, services)
- `docs/`: documentation and schemas
- `sdk/`: Python and Go SDKs
- `tests/`: integration, conformance, smoke, sample datasets
- `config/`: tool registry and operational config examples

## Key References
- Formal spec: `SPEC_OFFF_Formal_Spec_v0.1.0.md`
- Architecture/background: `README_OFFF_Open_Forensic_File_Format.md`
- Schema catalog: `docs/schema/offf-schema-catalog-0.1.0.json`
- Status matrix: `docs/status.md`

## Stability Matrix
A summarized matrix is maintained in `docs/status.md`.

## License
This repository is distributed under the license terms defined in the project root license files.
