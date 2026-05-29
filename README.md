# OFFF — Open Forensic File Format

OFFF is an **open, verifiable forensic evidence and interoperability format** for evidence
objects, analysis results, object lineage, provenance, and validation. It is designed to
enable interoperability between forensic tools via open schemas and a conformance suite,
without replacing existing platforms.

---

## What OFFF Is

- An open specification for forensic containers: evidence bytes, acquisition metadata,
  analysis results, provenance, and object lineage.
- A reference implementation in Rust with Python and Go SDKs.
- A conformance suite for validating tools against OFFF profiles.
- A foundation for forensic interoperability — not a forensic suite.

## What OFFF Is Not

- Not a forensic suite or case management system.
- Not a replacement for Hansken, FTK, Autopsy, Cellebrite, GrayKey, or similar platforms.
- Not a legal decision engine. OFFF makes no legal findings. See `docs/legal-neutrality.md`.
- Not a scheduler or orchestration engine.
- Not all components are production-grade. See the maturity table below.

---

## Current Maturity

| Area | Maturity | Notes |
|---|---|---|
| Core chunk/hash store (`offf-core`) | `forensic-grade-candidate` | SHA-256 + Merkle; external review pending |
| Raw/dd acquisition (`offf-convert`) | `forensic-grade-candidate` | Crash-safe; deterministic mode |
| Verification (`offf-verify`) | `forensic-grade-candidate` | 4 conformance profiles supported |
| Schema catalog | `forensic-grade-candidate` | v0.1.0 + v0.2.0 |
| Indexing, jobs, export | `reference` | Suitable for adoption; not independently reviewed |
| Workers (keyword, YARA) | `experimental` | Working; not conformance-complete |
| Access service | `experimental` | JWT mode implemented; no independent security review |
| SDKs (Python, Go) | `experimental` | API parity incomplete |
| Demo scripts | `demo-only` | Synthetic data only; not conformance evidence |

See `docs/status.md` for the full per-component status matrix.
See `docs/maturity-model.md` for criteria per level.

---

## Core / Reference / Demo

| Classification | Meaning | Examples |
|---|---|---|
| `core` | Normative part of OFFF specification | `offf-core`, schemas, conformance suite |
| `reference` | Reference implementation of OFFF contracts | `offf-convert`, `offf-verify`, `offf-jobs` |
| `demo` | Demonstration only — not normative | Demo scripts, Docker demo workers |

See `docs/component-classification.md` for the full classification table.

---

## Forensic Baseline Status

The **OFFF Forensic Baseline Profile** defines the minimum requirements for using an OFFF
container in a formal forensic context. The baseline covers:

- Manifest and acquisition metadata.
- Immutable evidence layer with SHA-256 chunk hashes.
- Merkle proof (block_image mode).
- Append-only analysis output with result manifests.
- Provenance events per job.
- Machine-verifiable with `offf-verify --profile forensic-baseline`.

**Current status:** Core components (`offf-core`, `offf-convert`, `offf-verify`) meet the
forensic baseline requirements at `forensic-grade-candidate` maturity. External review is
required before production deployment in critical forensic workflows.

See `docs/forensic-baseline-profile.md` for the full specification.

---

## Conformance Profiles

OFFF defines 8 conformance profiles:

| Profile | Scope |
|---|---|
| OFFF Reader Conformant | Opens and verifies OFFF containers |
| OFFF Acquisition Conformant | Creates forensically valid containers |
| OFFF Indexer Conformant | Indexes partitions and object graphs |
| OFFF Analysis Worker Conformant | Writes append-only analysis results |
| OFFF Object-Lineage Conformant | Produces/queries object derivation graphs |
| OFFF Access Service Conformant | Capability-gated container access |
| OFFF Extension Conformant | Reads/writes generic extension JSONL |
| OFFF Forensic Baseline Conformant | Meets minimum forensic use requirements |

See `docs/conformance-profiles.md` for full profile definitions.

---

## Adoption Path

For organisations evaluating OFFF for formal forensic use:

1. Synthetic POC → 2. Real data POC → 3. Forensic expert review → 4. Legal review →
5. Security review → 6. Conformance review → 7. Controlled pilot → 8. Adoption decision.

See `docs/adoption-playbook.md` for the full playbook.
See `docs/pilot-template.md` and `docs/risk-assessment-template.md` for pilot materials.

---

## Why OFFF
- Preserve forensic integrity with immutable evidence bytes and SHA-256 + Merkle proofs.
- Scale analysis using chunk-level processing and indexed metadata.
- Keep provenance and analysis results traceable and auditable.
- Enable interoperability via open specification and JSON schemas.

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

## Demo (demo-only, not conformance evidence)

The Docker demo runs three demo workers (Tika, Elasticsearch, unsupervised classifier)
against a synthetic OFFF case. Demo components are classified `demo-only` and must not be
cited as evidence of OFFF conformance or forensic validity.

```bash
# Linux/macOS
bash scripts/run_demo.sh

# Windows
pwsh scripts/run_demo.ps1
```

---

## Project Structure
- `crates/`: Rust crates (`offf-core`, `offf-convert`, `offf-verify`, workers, services)
- `docs/`: documentation, schemas, and governance docs
- `sdk/`: Python and Go SDKs
- `tests/`: integration, conformance, and sample datasets
- `scripts/`: demo scripts (`demo-only`) and CI/metadata check scripts
- `config/`: tool registry and operational config examples
- `reports/`: generated release readiness reports

## Key Documentation

### Standards and specification
- Formal spec: `SPEC_OFFF_Formal_Spec_v0.1.0.md`
- Schema catalog: `docs/schema/offf-schema-catalog-0.2.0.json`

### Classification, maturity, and status
- Component classification: `docs/component-classification.md`
- Maturity model: `docs/maturity-model.md`
- Status matrix: `docs/status.md`

### Forensic use
- Forensic baseline profile: `docs/forensic-baseline-profile.md`
- Conformance profiles: `docs/conformance-profiles.md`
- Evidence root model: `docs/evidence-root-model.md`
- Chain of evidence: `docs/chain-of-evidence.md`
- Chain of custody: `docs/chain-of-custody.md`
- Legal neutrality: `docs/legal-neutrality.md`
- Forensic limitations: `docs/forensic-limitations.md`
- Scope and exclusion model: `docs/scope-and-exclusion-model.md`

### Adoption and pilot
- Adoption playbook: `docs/adoption-playbook.md`
- Pilot template: `docs/pilot-template.md`
- Risk assessment template: `docs/risk-assessment-template.md`
- Tool adapter guide: `docs/tool-adapter-guide.md`

### Developer and ops
- Threat model: `docs/threat-model.md`
- Test traceability: `docs/test-traceability.md`
- Evidence of done: `docs/evidence-of-done.md`
- Versioning policy: `docs/versioning.md`

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
