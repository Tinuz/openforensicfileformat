# OFFF Evidence of Done

This document records, per completed backlog item, *why* the item qualifies as done:
what was implemented, which tests prove it, and what limitations remain.

For items without evidence, the gap is made explicit.

---

## Hardening Sprint 0 — Repository baseline and docs entrypoint

**Classification:** core  
**Maturity:** forensic-grade-candidate  
**Done:** 2026-05-23

**Implemented in:**
- `README.md` (root)
- `docs/status.md`

**Test evidence:**
- Manual verification: README exists, links to spec, schema catalog, status matrix.

**Known limitations:**
- Status matrix used outdated terminology at time of writing (now corrected).

**Conclusion:** Done criteria met.

---

## Hardening Sprint 2 — Verify existing chunks before dedup skip

**Classification:** core  
**Maturity:** forensic-grade-candidate  
**Done:** 2026-05-23

**Implemented in:**
- `crates/offf-core/src/chunk.rs` — `write_chunk` cryptographic verification before dedup skip

**Test evidence:**
- `cargo test -p offf-core` — unit test: valid existing chunk reuse, corrupt existing chunk fail-fast.
- `cargo test -p offf-integration-tests` — `verify_detects_chunk_corruption`

**Conformance:** OFFF Reader Conformant (negative: corrupt chunk detected)

**Known limitations:** None.

**Conclusion:** Done criteria met with test evidence.

---

## Hardening Sprint 3 — True deterministic mode + sector-size parameter

**Classification:** reference  
**Maturity:** forensic-grade-candidate  
**Done:** 2026-05-23

**Implemented in:**
- `crates/offf-convert/src/main.rs` — `--sector-size` flag, deterministic timestamps
- `crates/offf-core/src/types.rs` — `sector_size` in `ManifestJson`, `AcquisitionJson`

**Test evidence:**
- `cargo test -p offf-integration-tests` — `small_image_round_trip`, `non_aligned_image_round_trip`
- Deterministic repeated runs produce byte-equivalent `manifest.json` and `acquisition.json`

**Known limitations:** None.

**Conclusion:** Done criteria met.

---

## Hardening Sprint 4 — Merkle proofs + full tree validation

**Classification:** core  
**Maturity:** forensic-grade-candidate  
**Done:** 2026-05-23

**Implemented in:**
- `crates/offf-core/src/hash.rs` — `generate_merkle_proof`, `verify_merkle_proof`, `merkle_tree.bin` validation
- `crates/offf-verify/src/main.rs` — `--proof-chunk` CLI interface

**Test evidence:**
- `cargo test -p offf-core` — `merkle_proof_*` unit tests
- `cargo test -p offf-integration-tests` — `merkle_root_matches_manifest`
- `offf-verify` validates magic, version, levels, root consistency

**Conformance:** OFFF Reader Conformant (Merkle validation)

**Known limitations:** None.

**Conclusion:** Done criteria met with test evidence.

---

## Hardening Sprint 5 — Verifier profiles + leaves consistency

**Classification:** reference  
**Maturity:** forensic-grade-candidate  
**Done:** 2026-05-23

**Implemented in:**
- `crates/offf-verify/src/main.rs` — `--profile` (core, core+schemas, core+extensions, conformance, legacy), `--report`
- Profile `conformance` writes machine-readable JSON report

**Test evidence:**
- `cargo test -p offf-integration-tests` — `verify_detects_chunk_corruption`
- `python tests/conformance/run_conformance.py` — positive and negative scenarios
- `python tests/e2e/run_cli_e2e.py` — end-to-end verify profile exercise

**Conformance:** OFFF Reader Conformant, OFFF Acquisition Conformant

**Known limitations:** None.

**Conclusion:** Done criteria met.

---

## Hardening Sprint 6 — Append-only analysis model

**Classification:** core  
**Maturity:** forensic-grade-candidate  
**Done:** 2026-05-24

**Implemented in:**
- `crates/offf-keyword-worker/src/main.rs` — writes to `analysis/jobs/{job_id}/`
- `crates/offf-yara-worker/src/main.rs` — same layout
- `crates/offf-access-service/src/main.rs` — `AppendAnalysisCorrection` append-only endpoint

**Test evidence:**
- `cargo test -p offf-keyword-worker`
- `cargo test -p offf-yara-worker`
- `python tests/e2e/run_cli_e2e.py` — full convert → keyword → yara chain

**Conformance:** OFFF Analysis Worker Conformant

**Known limitations:** Workers refuse to overwrite existing job artifacts; correction events are append-only.

**Conclusion:** Done criteria met.

---

## Hardening Sprint 7 — Access Service production auth + denied audit

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-24

**Implemented in:**
- `crates/offf-access-service/src/main.rs` — `OFFF_AUTH_MODE` (dev_headers, jwt, mtls)
- Denied write attempts appended to `extensions/access/denied_access_events.jsonl`
- Write path validation: no evidence layer writes, no traversal, no overwrite

**Test evidence:**
- `cargo test -p offf-access-service` — includes denied overwrite logging verification
- `crates/offf-access-service/tests/grpc_smoke.rs`
- `crates/offf-access-service/tests/grpc_storage_parity.rs`

**Conformance:** OFFF Access Service Conformant (partial — auth modes not independently reviewed)

**Known limitations:**
- JWT and mTLS auth modes not independently security-reviewed.
- S3 backend parity is smoke-tested only.

**Conclusion:** Done criteria met at experimental maturity; external security review needed for forensic-grade.

---

## Hardening Sprint 8 — CI + CLI E2E + negative conformance datasets

**Classification:** core  
**Maturity:** reference  
**Done:** 2026-05-24

**Implemented in:**
- `.github/workflows/offf-ci.yml` — fmt, clippy, test, release build, E2E, conformance, SDK
- `tests/e2e/run_cli_e2e.py` — end-to-end CLI chain
- `tests/conformance/negative_cases.json` — dataset-driven negative scenarios

**Test evidence:**
- CI pipeline runs on every PR and push to main.

**Conformance:** OFFF Reader Conformant, OFFF Acquisition Conformant (via conformance-scaffold CI job)

**Known limitations:** Conformance suite not yet exhaustive for all negative scenarios.

**Conclusion:** Done criteria met.

---

## Lineage Sprint 9 — Object lineage core model + schemas

**Classification:** core  
**Maturity:** forensic-grade-candidate  
**Done:** 2026-05-25

**Implemented in:**
- `crates/offf-core/src/types.rs` — `DiscoveredObjectRow`, `ObjectEdgeRow`, `DerivationRow`, `ObjectSourceRef`, `ObjectStorageRef`
- `crates/offf-core/src/parquet_io.rs` — read/write helpers for object index, edges, derivations
- `crates/offf-core/src/lineage.rs` — `ObjectLineageValidator` with referential checks and cycle detection
- `docs/schema/` — `offf-object-index-row-0.2.0.schema.json`, `offf-object-edge-row-0.2.0.schema.json`, `offf-derivation-row-0.2.0.schema.json`, `offf-derived-object-store-0.2.0.schema.json`, `offf-lineage-report-0.2.0.schema.json`, `offf-object-producing-result-manifest-0.2.0.schema.json`

**Test evidence:**
- `cargo test -p offf-core` — lineage unit tests
- `cargo test -p offf-integration-tests` — `lineage_valid_graph_passes`, `lineage_cycle_in_object_graph_fails`, `lineage_missing_child_object_fails`

**Conformance:** OFFF Object-Lineage Conformant

**Known limitations:**
- Lineage verify CLI (`--object --lineage` in offf-verify) partially complete.
- Deterministic object-index rebuild from events is experimental.

**Conclusion:** Done criteria met; lineage verify CLI gap noted.

---

## Lineage Sprint 10 — Derived object store + worker output contract v0.2

**Classification:** core + reference  
**Maturity:** reference  
**Done:** 2026-05-25

**Implemented in:**
- `crates/offf-core/src/storage.rs` — `write_derived_object`, `read_derived_object`, hash-verify on reuse
- `crates/offf-keyword-worker/src/main.rs`, `crates/offf-yara-worker/src/main.rs` — v0.2 result manifest
- `crates/offf-jobs/src/main.rs` — `create-object-worker` subcommand with `output_contract` flags

**Test evidence:**
- `cargo test -p offf-core` — storage unit tests
- `cargo test -p offf-keyword-worker`, `cargo test -p offf-yara-worker`
- `python tests/e2e/run_cli_e2e.py`

**Conformance:** OFFF Analysis Worker Conformant

**Known limitations:**
- Materialized objects are never written to evidence/chunks layer (enforced).
- Result manifest is finalization point and hash-complete.

**Conclusion:** Done criteria met.

---

## Lineage Sprint 11 — SDK + Access API for object-producing jobs

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-26

**Implemented in:**
- `sdk/python/offf_sdk/` — `write_object_delta`, `write_edge_delta`, `write_derivation_delta`, `materialize_derived_object`
- `crates/offf-access-service/` — append-only endpoints for objects, object-edges, derivations, materialized-objects
- `config/tool-registry.example.json` — object-producing capabilities matrix

**Test evidence:**
- `python -m unittest sdk/python/tests/test_api_contract.py`
- `cargo test -p offf-access-service`

**Conformance:** OFFF Access Service Conformant (partial)

**Known limitations:**
- Access Service endpoints enforce capability vs `output_contract` checks (experimental).

**Conclusion:** Done criteria met at experimental maturity.

---

## Lineage Sprint 12 — Deterministic object-index rebuild + lineage verify CLI

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-26

**Implemented in:**
- `crates/offf-index/src/main.rs` — `objects` subcommand with `--from-events` flag
- `crates/offf-verify/src/main.rs` — `--object <id> --lineage` path validation

**Test evidence:**
- `cargo test -p offf-index`
- `cargo test -p offf-verify`
- `cargo test -p offf-integration-tests` — lineage graph tests

**Conformance:** OFFF Indexer Conformant (partial), OFFF Object-Lineage Conformant (partial)

**Known limitations:**
- Full conformance test dataset for lineage positive/negative scenarios not yet added.

**Conclusion:** Done criteria met at experimental maturity; conformance dataset gap noted.

---

## Sprint 13 — OFFF v0.2 manifest extensions foundation

**Classification:** core  
**Maturity:** reference  
**Done:** 2026-05-26

**Implemented in:**
- `crates/offf-core/src/types.rs` — `ManifestExtensions` field in `ManifestJson`
- `docs/schema/offf-manifest-0.2.0.schema.json`

**Test evidence:**
- `cargo test -p offf-integration-tests` — `manifest_v020_round_trip_with_extensions`, `manifest_v010_json_loadable_by_v020_reader`

**Conformance:** OFFF Reader Conformant (backward compat: v0.1 readable by v0.2 reader)

**Known limitations:** None.

**Conclusion:** Done criteria met.

---

## Sprint 14 — Demo proofability core CLI + nested lineage + tamper checks

**Classification:** demo  
**Maturity:** demo-only  
**Done:** 2026-05-27

**Implemented in:**
- `scripts/` — demo scripts with verify-case, verify-analysis, lineage, provenance, report, tamper commands

**Test evidence:**
- `scripts/smoke_check_demo.ps1` — smoke verification

**Known limitations:** Demo-only; not normative OFFF behaviour.

**Conclusion:** Done criteria met at demo-only level.

---

## Sprint 15 — Generic extension types + append-only APIs

**Classification:** core  
**Maturity:** reference  
**Done:** 2026-05-27

**Implemented in:**
- `crates/offf-core/src/extensions.rs` — 8 typed JSONL-event structs, append/read generics
- `crates/offf-verify/src/main.rs` — `--profile core+extensions` validation
- `sdk/python/offf_sdk/` — `append_label_event`, `append_scope`, etc.

**Test evidence:**
- `cargo test -p offf-core`
- `cargo test -p offf-verify`
- `python -m unittest sdk/python/tests/test_api_contract.py`

**Conformance:** OFFF Extension Conformant

**Known limitations:** None.

**Conclusion:** Done criteria met.

---

## Sprint 16 — Scope-aware jobs and workers

**Classification:** reference  
**Maturity:** reference  
**Done:** 2026-05-28

**Implemented in:**
- `crates/offf-core/src/types.rs` — `scope_ref`, `include_sets`, `policy_refs` in `JobManifest`
- `crates/offf-keyword-worker/src/main.rs`, `crates/offf-yara-worker/src/main.rs` — `scope_evaluated` audit events
- `crates/offf-jobs/src/main.rs` — `--scope-ref`, `--include-set`, `--policy-ref` flags

**Test evidence:**
- `cargo test -p offf-keyword-worker`, `cargo test -p offf-yara-worker`

**Conformance:** OFFF Analysis Worker Conformant

**Known limitations:** None.

**Conclusion:** Done criteria met.

---

## Sprint 17 — Legacy compatibility profile

**Classification:** reference  
**Maturity:** reference  
**Done:** 2026-05-28

**Implemented in:**
- `crates/offf-verify/src/main.rs` — `VerifyProfile::Legacy` variant; scans flat `analysis/` files, emits WARN for non-forensic-grade jobs

**Test evidence:**
- `cargo test -p offf-verify`

**Conformance:** OFFF Reader Conformant (legacy profile)

**Known limitations:** None.

**Conclusion:** Done criteria met.

---

## Sprint 18 — Object graph read/query APIs

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-28

**Implemented in:**
- `sdk/python/offf_sdk/api.py` — `get_object`, `list_objects`, `get_object_children`, `get_object_parents`, `get_object_lineage_path`, `export_lineage_report`

**Test evidence:**
- `python -m unittest sdk/python/tests/test_api_contract.py`

**Conformance:** OFFF Object-Lineage Conformant (partial)

**Known limitations:** None.

**Conclusion:** Done criteria met.

---

## Sprint 19 — Object-per-event append model

**Classification:** core  
**Maturity:** reference  
**Done:** 2026-05-31

**Implemented in:**
- `crates/offf-core/src/extensions.rs` — `ObjectEvent`, `ObjectEdgeEvent`, `append_object_event`, `read_object_events`, `rebuild_object_index_from_events`
- `crates/offf-index/src/main.rs` — `objects --from-events` flag
- `sdk/python/offf_sdk/` — `append_object_event`, `append_edge_event`, `rebuild_object_index_from_events`

**Test evidence:**
- `cargo test -p offf-core`

**Known limitations:** None.

**Conclusion:** Done criteria met.

---

## P1 — Indexing hardening GPT/MBR and worker quality

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-31

**Implemented in:**
- `crates/offf-core/src/ntfs.rs` — ATTRIBUTE_LIST, ADS, sparse/compressed/encrypted, streaming hardening
- `crates/offf-index/src/main.rs` — GPT CRC32 validation, backup GPT fallback, EBR chain
- `crates/offf-keyword-worker/src/main.rs` — file_id resolution, cross-chunk junction scanning
- `crates/offf-yara-worker/src/main.rs` — file_id resolution

**Test evidence:**
- `cargo test -p offf-core` — `std_info_flags_decoded`, `ads_streams_detected`
- `cargo test -p offf-integration-tests` — `mbr_partition_table_detected`, `gpt_partition_table_detected`

**Known limitations:** NTFS parser is experimental — not full forensic-grade.

**Conclusion:** Done criteria met at experimental maturity.

---

## P1 — Python SDK hardening

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-31

**Implemented in:**
- `sdk/python/offf_sdk/` — `_LRUCache`, `OfffContainer` (bounded cache, profile support), `JobWriter` context manager

**Test evidence:**
- `python -m unittest sdk/python/tests/test_api_contract.py`

**Known limitations:** LRU cache not battle-tested at scale.

**Conclusion:** Done criteria met.

---

## P2 — Object lineage scale/performance

**Classification:** reference  
**Maturity:** reference  
**Done:** 2026-05-30

**Implemented in:**
- `crates/offf-core/src/storage.rs` — `read_derived_object_streaming`
- `crates/offf-core/src/lineage.rs` — `export_dot`, `export_lineage_json`
- `crates/offf-core/src/parquet_io.rs` — `for_each_object_batch`, `for_each_edge_batch`, `for_each_derivation_batch`
- `crates/offf-index/src/main.rs` — `export-lineage` subcommand, `--batch-size` flag

**Test evidence:**
- `cargo test -p offf-core`
- `cargo test -p offf-integration-tests`

**Known limitations:** None.

**Conclusion:** Done criteria met.

---

## P2 — Packed container (single-file)

**Classification:** reference  
**Maturity:** reference  
**Done:** 2026-05-23

**Implemented in:**
- `crates/offf-core/src/packed.rs` — `pack_directory`, `read_index`, `unpack_to_directory`
- `crates/offf-export/src/main.rs` — `pack`, `list`, `unpack`, `export` subcommands

**Test evidence:**
- `cargo test -p offf-core` — packed unit tests

**Known limitations:**
- Not normative; verify requires unpacking first; no E2E round-trip test.

**Conclusion:** Done criteria met at reference level.

---

## P2 — MinIO E2E smoke test

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-23

**Implemented in:**
- `tests/smoke/phase5_minio_smoke.py`

**Test evidence:**
- `python tests/smoke/phase5_minio_smoke.py` (requires running MinIO instance)

**Known limitations:** Requires external MinIO instance; not run in standard CI.

**Conclusion:** Done criteria met at experimental level.

---

## P2 — E01 smoke test

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-23

**Implemented in:**
- `tests/smoke/phase7_e01_smoke.py`

**Test evidence:**
- `python tests/smoke/phase7_e01_smoke.py` (requires libewf/ewfexport in PATH)

**Known limitations:** Requires external ewfexport tool.

**Conclusion:** Done criteria met at experimental level.

---

## P2 — Threat model and security mapping

**Classification:** core  
**Maturity:** reference  
**Done:** 2026-05-29

**Implemented in:**
- `docs/threat-model.md`

**Test evidence:**
- Document exists and covers all major threat vectors with mitigation and test references.

**Known limitations:** External security review not yet done; threat model is internal only.

**Conclusion:** Done criteria met at reference level.

---

## P2 — Versioning policy and migration path

**Classification:** core  
**Maturity:** reference  
**Done:** 2026-05-29

**Implemented in:**
- `docs/versioning.md`

**Test evidence:**
- `cargo test -p offf-integration-tests` — `manifest_v010_json_loadable_by_v020_reader`, `manifest_v020_round_trip_with_extensions`

**Known limitations:** `offf-migrate` CLI tool is not yet implemented.

**Conclusion:** Done criteria met at reference level.

---

## Phases A–H — File_collection acquisition mode

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-29

**Implemented in:**
- `crates/offf-collect/src/main.rs`
- `crates/offf-core/src/evidence.rs`

**Test evidence:**
- `cargo test -p offf-collect`
- `cargo test -p offf-integration-tests` — `file_collection_single_file`, `file_collection_directory`, `file_collection_tamper_detection`

**Known limitations:**
- No dedicated E2E file_collection upload/verify cycle yet.

**Conclusion:** Done criteria met at experimental level with dedicated file_collection integration coverage.

---

## Phases J–P — Parallel processing support

**Classification:** reference  
**Maturity:** reference  
**Done:** 2026-05-29

**Implemented in:**
- `crates/offf-core/src/scope.rs` — `ScopeResolver`, `compute_input_scope_hash`
- `crates/offf-core/src/shard.rs` — `plan_shards`, shard I/O, `validate_parallel_job`, coverage reports
- `crates/offf-core/src/worker_context.rs` — `AnalysisWorkerContext` with staged atomic commit
- `crates/offf-jobs/src/main.rs` — `plan-shards`, `finalize-job` subcommands
- `crates/offf-verify/src/main.rs` — `--analysis-job`, `--shard`, `--coverage` flags

**Test evidence:**
- `cargo test -p offf-integration-tests` — 5 parallel processing integration tests:
  - `parallel_job_4_shards_valid`
  - `parallel_job_missing_shard_result_detected`
  - `parallel_job_corrupt_artifact_hash_detected`
  - `parallel_job_duplicate_input_detected`
  - `parallel_job_scope_hash_mismatch_detected`

**Conformance:** OFFF Analysis Worker Conformant

**Known limitations:**
- Parent result manifest and full finalize-job cycle have reference-level coverage.

**Conclusion:** Done criteria met with test evidence for all five validation scenarios.

---

## Access Service — gRPC smoke + capability model + storage backends

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-22–2026-05-24

**Implemented in:**
- `crates/offf-access-service/tests/grpc_smoke.rs`
- `crates/offf-access-service/tests/grpc_storage_parity.rs`

**Test evidence:**
- `cargo test -p offf-access-service`

**Known limitations:** Production auth not independently audited.

**Conclusion:** Done criteria met at experimental level.

---

## Worker framework hardening

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-22

**Implemented in:**
- `crates/offf-jobs/src/main.rs` — `run` subcommand with runtime state, replay id, assignment audit, worker health

**Test evidence:**
- `cargo test -p offf-jobs` — includes `run_writes_runtime_state_artifacts_for_failed_job_attempt`

**Known limitations:**
- Worker health registry is reference only, not OFFF Core.

**Conclusion:** Done criteria met at experimental level; runtime state lifecycle now has direct artifact coverage.

---

## Tool registry + governance artifacts

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-22

**Implemented in:**
- `config/tool-registry.example.json`
- `docs/tool-registry.md`

**Test evidence:**
- `cargo test -p offf-integration-tests` — `tool_registry_example_has_expected_capabilities`

**Known limitations:**
- Example registry only; no standalone enforcement tooling yet.

**Conclusion:** Done criteria met at experimental level with integration coverage for the shipped example registry.

---

## SDK contract test + CI guard

**Classification:** reference  
**Maturity:** experimental  
**Done:** 2026-05-22

**Implemented in:**
- `sdk/python/tests/test_api_contract.py`
- `.github/workflows/offf-ci.yml` — `python-sdk-contract` job

**Test evidence:**
- `python -m unittest sdk/python/tests/test_api_contract.py`

**Known limitations:** Contract test covers API surface only, not correctness.

**Conclusion:** Done criteria met.

---

## Go SDK minimal profile implementation

**Classification:** reference
**Maturity:** experimental
**Done:** 2026-05-23

**Implemented in:**
- `sdk/go/sdk.go` — `OpenContainer`, `ReadManifest`, `VerifyContainer`, `ReadChunk`,
  `VerifyChunk`, `MapOffsetToChunk`, `ReadFileIndex`, `WriteAnalysisResult`,
  `AppendProvenanceEvent`
- `sdk/go/doc.go` — package documentation
- `sdk/go/go.mod`, `sdk/go/go.sum` — module definition

**Test evidence:**
- `sdk/go/sdk_test.go` — smoke tests for read/verify/write paths
- CI job `go-sdk-smoke` in `.github/workflows/offf-ci.yml`

**Known limitations:**
- Experimental: API surface may change before v1.0
- Requires Go toolchain; not part of Rust workspace tests
- Write paths (analysis result, provenance) are not independently verified against
  Rust implementation in CI

**Conclusion:** Done criteria met.

---

*Last updated: 2026-06-01*
