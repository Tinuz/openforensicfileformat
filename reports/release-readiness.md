# OFFF Release Readiness Report

Generated: 2026-05-31T20:58:58Z

**Total components:** 22

## Classification breakdown

| Classification | Count | Components |
|---|---|---|
| core | 6 | conformance-suite, evidence-container-schema, extension-model, object-lineage-model, offf-core, schema-catalog |
| demo | 1 | demo-scripts |
| reference | 15 | go-sdk, offf-access-service, offf-annotate, offf-collect, offf-convert, offf-export, offf-index, offf-jobs, offf-keyword-worker, offf-verify, offf-yara-worker, packed-container, python-sdk, tool-registry, worker-runtime-state |

## Maturity breakdown

| Maturity | Count | Components |
|---|---|---|
| forensic-grade-candidate | 6 | evidence-container-schema, object-lineage-model, offf-convert, offf-core, offf-verify, schema-catalog |
| reference | 6 | conformance-suite, extension-model, offf-export, offf-index, offf-jobs, packed-container |
| experimental | 9 | go-sdk, offf-access-service, offf-annotate, offf-collect, offf-keyword-worker, offf-yara-worker, python-sdk, tool-registry, worker-runtime-state |
| demo-only | 1 | demo-scripts |

## Forensic-ready components (6)

- **evidence-container-schema** (`forensic-grade-candidate`)
- **object-lineage-model** (`forensic-grade-candidate`)
- **offf-convert** (`forensic-grade-candidate`)
- **offf-core** (`forensic-grade-candidate`)
- **offf-verify** (`forensic-grade-candidate`)
- **schema-catalog** (`forensic-grade-candidate`)

## Not yet forensic-ready (16)

| Component | Current maturity | Gap to forensic-grade-candidate |
|---|---|---|
| conformance-suite | reference | 1 maturity level(s) |
| demo-scripts | demo-only | 3 maturity level(s) |
| extension-model | reference | 1 maturity level(s) |
| go-sdk | experimental | 2 maturity level(s) |
| offf-access-service | experimental | 2 maturity level(s) |
| offf-annotate | experimental | 2 maturity level(s) |
| offf-collect | experimental | 2 maturity level(s) |
| offf-export | reference | 1 maturity level(s) |
| offf-index | reference | 1 maturity level(s) |
| offf-jobs | reference | 1 maturity level(s) |
| offf-keyword-worker | experimental | 2 maturity level(s) |
| offf-yara-worker | experimental | 2 maturity level(s) |
| packed-container | reference | 1 maturity level(s) |
| python-sdk | experimental | 2 maturity level(s) |
| tool-registry | experimental | 2 maturity level(s) |
| worker-runtime-state | experimental | 2 maturity level(s) |

## Metadata gaps (11)

| Component | Gap type |
|---|---|
| offf-collect | missing-docs |
| offf-annotate | missing-docs |
| offf-keyword-worker | missing-docs |
| offf-yara-worker | missing-docs |
| offf-access-service | missing-docs |
| worker-runtime-state | missing-tests |
| python-sdk | missing-docs |
| go-sdk | missing-docs |
| tool-registry | missing-tests |
| tool-registry | missing-docs |
| demo-scripts | missing-docs |

## 1.0 readiness

Scope policy: core and reference components are in scope for the first 1.0 release; demo, experimental, and legacy components are out of scope unless promoted.

**Ready:** 6
**Blockers:** 15
**Out of scope:** 1

### Ready components

- **evidence-container-schema** (`forensic-grade-candidate`)
- **object-lineage-model** (`forensic-grade-candidate`)
- **offf-convert** (`forensic-grade-candidate`)
- **offf-core** (`forensic-grade-candidate`)
- **offf-verify** (`forensic-grade-candidate`)
- **schema-catalog** (`forensic-grade-candidate`)

### Blockers

| Component | Blockers |
|---|---|
| conformance-suite | maturity=reference |
| extension-model | maturity=reference |
| go-sdk | maturity=experimental, missing-docs |
| offf-access-service | maturity=experimental, missing-docs |
| offf-annotate | maturity=experimental, missing-docs |
| offf-collect | maturity=experimental, missing-docs |
| offf-export | maturity=reference |
| offf-index | maturity=reference |
| offf-jobs | maturity=reference |
| offf-keyword-worker | maturity=experimental, missing-docs |
| offf-yara-worker | maturity=experimental, missing-docs |
| packed-container | maturity=reference |
| python-sdk | maturity=experimental, missing-docs |
| tool-registry | maturity=experimental, missing-tests, missing-docs |
| worker-runtime-state | maturity=experimental, missing-tests |

### Out of scope

- demo-scripts

## 1.0 backlog

The backlog below is ordered by delivery priority, not by component size.

### P0 — Freeze 1.0 scope and gate metadata

**Components:** conformance-suite, extension-model, offf-access-service, offf-annotate, offf-collect, offf-convert, offf-export, offf-index, offf-jobs, offf-keyword-worker, offf-yara-worker, packed-container, python-sdk, go-sdk, tool-registry, worker-runtime-state

**Minimal files:**

- components.toml
- docs/status.md
- docs/maturity-model.md
- docs/component-classification.md
- docs/test-traceability.md
- docs/evidence-of-done.md
- README.md
- .github/workflows/offf-ci.yml
- scripts/check_component_metadata.py
- scripts/check_test_traceability.py
- scripts/generate_release_readiness.py

**Minimal tests:**

- python scripts/check_component_metadata.py
- python scripts/check_test_traceability.py
- python scripts/generate_release_readiness.py

**Acceptance:** 1.0 scope is explicit, the release gate is reproducible, and the readiness report is authoritative.

### P1 — Promote stable reference path components

**Components:** conformance-suite, extension-model, offf-convert, offf-export, offf-index, offf-jobs, packed-container

**Minimal files:**

- crates/offf-core/src/extensions.rs
- crates/offf-core/src/packed.rs
- crates/offf-convert/src/main.rs
- crates/offf-export/src/main.rs
- crates/offf-index/src/main.rs
- crates/offf-jobs/src/main.rs
- docs/conformance-profiles.md
- docs/test-traceability.md

**Minimal tests:**

- cargo test -p offf-core
- cargo test -p offf-convert
- cargo test -p offf-export
- cargo test -p offf-index
- cargo test -p offf-jobs
- cargo test -p offf-integration-tests
- python tests/conformance/run_conformance.py

**Acceptance:** Production/reference path components can be defended as release-stable or explicitly fenced with documented limits.

### P2 — Fence experimental production surfaces

**Components:** offf-access-service, offf-annotate, offf-collect, offf-keyword-worker, offf-yara-worker

**Minimal files:**

- crates/offf-access-service/src/main.rs
- crates/offf-annotate/src/main.rs
- crates/offf-collect/src/main.rs
- crates/offf-keyword-worker/src/main.rs
- crates/offf-yara-worker/src/main.rs
- docs/object-content-ref.md
- docs/filesystem-to-object-graph.md
- docs/conformance-profiles.md

**Minimal tests:**

- cargo test -p offf-access-service
- cargo test -p offf-annotate
- cargo test -p offf-collect
- cargo test -p offf-keyword-worker
- cargo test -p offf-yara-worker
- python tests/e2e/run_cli_e2e.py

**Acceptance:** Experimental surfaces are either promoted with evidence or clearly fenced from the 1.0 guarantee.

### P3 — Stabilize SDK and governance surfaces

**Components:** python-sdk, go-sdk, tool-registry, worker-runtime-state

**Minimal files:**

- sdk/python/offf_sdk/container.py
- sdk/python/offf_sdk/api.py
- sdk/python/tests/test_api_contract.py
- sdk/python/tests/test_container_chunk_reader.py
- sdk/go/sdk.go
- sdk/go/sdk_test.go
- config/tool-registry.example.json
- docs/reference-worker-runtime.md

**Minimal tests:**

- python -m unittest sdk/python/tests/test_api_contract.py sdk/python/tests/test_container_chunk_reader.py
- go test ./...

**Acceptance:** SDKs and governance metadata have a stable, tested minimum contract for 1.0 consumers.


---

*Generated by `scripts/generate_release_readiness.py`*
