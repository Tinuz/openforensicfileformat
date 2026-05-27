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

## Docker Demo: OFFF Analysis Worker Contract (POC)

This repository includes a Docker-based demo environment that shows OFFF as a tool-agnostic analysis contract layer.

Deze demo is bedoeld om het OFFF Analysis Worker Contract te demonstreren.
De demo gebruikt een vereenvoudigde OFFF-containerstructuur.
De demo is niet bedoeld als forensisch gevalideerde productie-implementatie.

### Demo Goals
- Evidence layer remains immutable/read-only for workers.
- Workers use verified references to OFFF objects.
- Analysis workers are tool-agnostic and independently composable.
- Analysis output is append-only and isolated per job.
- Result manifests and provenance events are written for each job.
- Extracted text is indexed in Elasticsearch.
- Extracted text is clustered via unsupervised classification.

### Services
- `elasticsearch`
- `kibana`
- `tika`
- `offf-tika-worker`
- `offf-elastic-index-worker`
- `offf-unsupervised-classifier-worker`

### Demo Paths
- Compose file: `docker-compose.yml`
- Demo data root: `demo-data/`
- OFFF demo case: `demo-data/case.offf/`
- Job manifests: `demo-data/jobs/`
- Analysis outputs: `demo-data/case.offf/analysis/jobs/<job_id>/`
- Demo reports: `demo-data/case.offf/reports/`
- Provenance log: `demo-data/case.offf/provenance/chain_of_custody.jsonl`

### Source Evidence For Demo
The demo case creation script uses the sample forensic image:

- `tests/samples/4orensics.case2/4orensics.001`

### Run The Demo
Linux/macOS:

```bash
bash scripts/run_demo.sh
```

Windows PowerShell:

```powershell
./scripts/run_demo.ps1
```

Manual flow:

```bash
docker compose up -d elasticsearch kibana tika
python scripts/create_demo_case.py
docker compose run --rm offf-tika-worker --job /data/jobs/job-tika-001.json
docker compose run --rm offf-elastic-index-worker --job /data/jobs/job-elastic-index-001.json
docker compose run --rm offf-unsupervised-classifier-worker --job /data/jobs/job-unsupervised-classify-001.json
```

Quick smoke-check (PowerShell query set):

```powershell
./scripts/smoke_check_demo.ps1
```

This smoke check validates:
- expected job output files and manifests,
- provenance entries for all three jobs,
- scope audit entries for worker selection/denials,
- lineage report export under `demo-data/case.offf/reports/lineage_report.json`,
- Elasticsearch document availability via both `Invoke-RestMethod` and `curl.exe`.

### View Results
- Kibana: `http://localhost:5601`
- Elasticsearch: `http://localhost:9200`
- Elasticsearch index: `offf-documents`

### What Is Out Of Scope In This Demo
- Real raw/dd chunk conversion in demo pipeline execution.
- Merkle proof generation/verification in the demo worker flow.
- Production auth/access-control.
- Production object storage integration.
- Full legal scope enforcement.

### Worker Output Contract Notes
- Workers write only to:
  - `analysis/jobs/{job_id}/`
  - `provenance/`
  - `audit/`
  - `reports/`
- Workers must not write to:
  - `chunks/`, `hashes/`, `maps/`, `manifest.json`, `acquisition.json`, `evidence_files/`
- `result_manifest.json` is written last per job.
- Scope denials are recorded as append-only JSONL audit events in the worker output directory.
- Parser errors/skips are captured in `errors.jsonl` and reflected in manifests.
- For this demo, provenance is append-only JSONL (`provenance/chain_of_custody.jsonl`); future hardening should migrate to object-per-event provenance records.
