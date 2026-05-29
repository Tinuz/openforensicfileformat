# OFFF Component Classification

## Overview

OFFF separates its components into four classifications:

| Classification | Meaning |
|---|---|
| `core` | Normative part of the OFFF specification or core library |
| `reference` | Reference implementation of a normative OFFF contract |
| `demo` | Demonstration only — not normative behaviour |
| `experimental` | Unstable prototype — API/contract subject to change |
| `legacy` | Supported for compatibility but not the recommended approach |

This separation ensures:
- Scheduling, orchestration, and worker health logic does not become part of the Core specification.
- Demo tooling cannot be cited as conformance evidence.
- External tools can implement the same contracts without depending on OFFF reference crates.

The machine-readable record is in `components.toml` at the repository root.

---

## Classification Table

| Component | Classification | Maturity | Path |
|---|---|---|---|
| **offf-core** | `core` | `forensic-grade-candidate` | `crates/offf-core` |
| **Evidence container schema** | `core` | `forensic-grade-candidate` | `docs/schema/offf-manifest-*.json` |
| **Object lineage model** | `core` | `forensic-grade-candidate` | `docs/schema/offf-object-*-row-*.json` |
| **Extension model** | `core` | `reference` | `docs/schema/offf-annotation-event-*.json` |
| **Schema catalog** | `core` | `forensic-grade-candidate` | `docs/schema/offf-schema-catalog-*.json` |
| **Conformance suite** | `core` | `reference` | `tests/conformance/` |
| **offf-convert** | `reference` | `forensic-grade-candidate` | `crates/offf-convert` |
| **offf-verify** | `reference` | `forensic-grade-candidate` | `crates/offf-verify` |
| **offf-export** | `reference` | `reference` | `crates/offf-export` |
| **offf-index** | `reference` | `reference` | `crates/offf-index` |
| **offf-jobs** | `reference` | `reference` | `crates/offf-jobs` |
| **offf-collect** | `reference` | `experimental` | `crates/offf-collect` |
| **offf-annotate** | `reference` | `experimental` | `crates/offf-annotate` |
| **offf-keyword-worker** | `reference` | `experimental` | `crates/offf-keyword-worker` |
| **offf-yara-worker** | `reference` | `experimental` | `crates/offf-yara-worker` |
| **offf-access-service** | `reference` | `experimental` | `crates/offf-access-service` |
| **Packed container transport** | `reference` | `reference` | `crates/offf-core/src/packed.rs` |
| **Worker runtime state** | `reference` | `experimental` | `jobs/runtime/` (container-level) |
| **Worker health registry** | `reference` | `experimental` | `jobs/runtime/worker_health.jsonl` |
| **Assignment audit trail** | `reference` | `experimental` | `jobs/runtime/assignment_audit.jsonl` |
| **Deterministic job replay** | `reference` | `experimental` | `offf-jobs run` subcommand |
| **Retry/failure policy** | `reference` | `experimental` | `jobs/runtime/*.state.json` |
| **Python SDK** | `reference` | `experimental` | `sdk/python/` |
| **Go SDK** | `reference` | `experimental` | `sdk/go/` |
| **Tool registry** | `reference` | `experimental` | `config/tool-registry.example.json` |
| **E01 conversion path** | `reference` | `experimental` | `crates/offf-convert` (--input-type e01) |
| **NTFS indexer** | `reference` | `experimental` | `crates/offf-core/src/ntfs.rs`, `crates/offf-index` |
| **MinIO/S3 backend** | `reference` | `experimental` | `crates/offf-access-service`, storage refs |
| **Demo scripts** | `demo` | `demo-only` | `scripts/` |
| **Docker Compose demo** | `demo` | `demo-only` | (scripts/run_demo.sh) |

---

## Key Rules

### Scheduling and orchestration is NOT Core

Worker runtime state, retry/failure policy, worker health registry, and assignment audit trail
are all classified as `reference`. They represent one valid way to implement a worker scheduler
on top of OFFF contracts. External teams may implement their own schedulers without violating
OFFF conformance.

See `docs/reference-worker-runtime.md` for the rationale.

### Packed container is transport, not canonical

The `.offfpack` single-file format is a `reference`-level transport representation. The exploded
directory layout (`manifest.json`, `chunks/`, `hashes/`, `maps/`, `indexes/`, `analysis/`,
`extensions/`) is the **canonical representation** of an OFFF container.

See `docs/packed-container.md` for details.

### Demo tools are not conformance evidence

Scripts in `scripts/` (e.g., `create_demo_case.py`, `offf_demo.py`, `run_demo.ps1`) are
classified `demo` / `demo-only`. They may use simplified containers and synthetic data. They
**must not** be cited as evidence of OFFF conformance or forensic validity.

### Reference does not imply Core

A component classified `reference` implements a contract defined by OFFF Core, but is not
part of the normative specification. Third parties may implement the same contract with
different tools.

---

## Related Documents

- `docs/maturity-model.md` — criteria for each maturity level
- `docs/status.md` — current status per component with test evidence
- `components.toml` — machine-readable component metadata
- `docs/conformance-profiles.md` — conformance profile definitions
- `docs/reference-worker-runtime.md` — why worker runtime is reference, not Core
- `docs/packed-container.md` — exploded directory vs .offfpack transport

---

*Last updated: 2026-05-29*
