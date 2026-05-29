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

---

## Project Structure
- `crates/`: Rust crates (`offf-core`, `offf-convert`, `offf-verify`, workers, services)
- `docs/`: documentation, schemas, and governance docs
- `sdk/`: Python and Go SDKs
- `tests/`: integration, conformance, and sample datasets
- `scripts/`: CI/metadata check scripts (see also: [offf-demo](https://github.com/Tinuz/offf-demo))
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

## Demo environment

The demo environment (Docker workers, Tika, Elasticsearch, unsupervised classifier, demo data) lives in a separate repository:

**[https://github.com/Tinuz/offf-demo](https://github.com/Tinuz/offf-demo)**

Demo components are classified `demo-only` and must not be cited as evidence of OFFF conformance or forensic validity.
