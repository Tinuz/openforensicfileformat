# OFFF Test Traceability Matrix

This matrix maps backlog requirements to test evidence. Items without any test evidence
are marked as **gap**. The `check_test_traceability.py` script parses this table and
reports gaps automatically.

Column guide:
- **Unit** — `cargo test -p <crate>` or `cargo test -p offf-core`
- **Integration** — `cargo test -p offf-integration-tests`
- **E2E** — `python tests/e2e/run_cli_e2e.py`
- **Negative** — adversarial/negative test cases (unit or conformance)
- **Conformance** — `python tests/conformance/run_conformance.py`
- **Status** — `done` / `partial` / `gap`

---

## Matrix

| Requirement / Backlog item | Unit tests | Integration tests | E2E tests | Negative tests | Conformance | Status |
|---|---|---|---|---|---|---|
| Sprint 0: Repository baseline | — | — | — | — | — | gap |
| Sprint 1: Crash-safe convert | `offf-convert` | `small_image_round_trip` | `run_cli_e2e.py` | — | — | partial |
| Sprint 2: Verify chunks before dedup | `chunk::verify_chunk` | `verify_detects_chunk_corruption` | — | `verify_detects_chunk_corruption` | `negative_cases.json` | done |
| Sprint 3: Deterministic mode + sector-size | `offf-convert` | `small_image_round_trip`, `non_aligned_image_round_trip` | — | — | — | partial |
| Sprint 4: Merkle proofs + tree validation | `hash::merkle_proof_*` | `merkle_root_matches_manifest` | — | — | `negative_cases.json` | done |
| Sprint 5: Verifier profiles + leaves | `offf-verify` | `verify_detects_chunk_corruption` | `run_cli_e2e.py` | `verify_detects_chunk_corruption` | `run_conformance.py` | done |
| Sprint 6: Append-only analysis model | `offf-keyword-worker`, `offf-yara-worker` | — | `run_cli_e2e.py` | — | — | partial |
| Sprint 7: Access Service auth + denied audit | `offf-access-service` | — | — | `grpc_smoke.rs` denied writes | — | partial |
| Sprint 8: CI + CLI E2E + negative conformance | — | — | `run_cli_e2e.py` | `negative_cases.json` | `run_conformance.py` | done |
| Sprint 9: Object lineage model + schemas | `offf-core lineage` | `lineage_valid_graph_passes`, `lineage_cycle_in_object_graph_fails`, `lineage_missing_child_object_fails` | — | `lineage_cycle_*`, `lineage_missing_*` | — | done |
| Sprint 10: Derived object store + v0.2 contract | `offf-core storage` | — | `run_cli_e2e.py` | — | — | partial |
| Sprint 11: SDK + Access API object-producing | SDK contract test | — | — | — | — | partial |
| Sprint 12: Object-index rebuild + lineage verify CLI | `offf-index`, `offf-verify` | `lineage_valid_graph_passes` | — | `lineage_cycle_*` | — | partial |
| Sprint 13: Manifest extensions v0.2 | `offf-core` | `manifest_v020_round_trip_with_extensions`, `manifest_v010_json_loadable_by_v020_reader` | — | — | — | done |
| Sprint 14: Demo proofability CLI | — | — | `smoke_check_demo.ps1` | — | — | partial |
| Sprint 15: Generic extension types | `offf-core extensions` | — | — | — | — | partial |
| Sprint 16: Scope-aware jobs and workers | `offf-keyword-worker`, `offf-yara-worker` | — | — | — | — | partial |
| Sprint 17: Legacy compatibility profile | `offf-verify` | — | — | — | — | partial |
| Sprint 18: Object graph read/query APIs | SDK contract test | — | — | — | — | partial |
| Sprint 19: Object-per-event append model | `offf-core extensions` | — | — | — | — | partial |
| P1: Indexing hardening GPT/MBR + workers | `offf-core ntfs` | `mbr_partition_table_detected`, `gpt_partition_table_detected`, `gpt_partition_table_json_written` | — | — | — | done |
| P1: Python SDK hardening | SDK contract test | — | — | — | — | partial |
| P2: Object lineage scale/performance | `offf-core` | `parquet_tables_survive_round_trip` | — | — | — | partial |
| P2: Packed container | `offf-core packed` | — | — | — | — | partial |
| P2: MinIO E2E smoke | — | — | `phase5_minio_smoke.py` | — | — | partial |
| P2: E01 smoke | — | — | `phase7_e01_smoke.py` | — | — | partial |
| P2: Threat model | — | — | — | — | — | gap |
| P2: Versioning policy | `manifest_v010_json_loadable_by_v020_reader` | `manifest_v020_round_trip_with_extensions` | — | — | — | partial |
| P0: Access Service gRPC smoke | `grpc_smoke.rs` | — | — | denied writes | — | done |
| P0: Access Service capability model | `offf-access-service` | — | — | denied overwrite | — | done |
| P0: Access Service storage parity | `grpc_storage_parity.rs` | — | — | — | — | done |
| P1: Worker framework hardening | `offf-jobs` | — | — | — | — | partial |
| P0: SDK contract test + CI guard | SDK contract test | — | — | — | — | done |
| P1: Go SDK | `sdk/go go test` | — | — | — | — | partial |
| P1: Tool registry + governance | — | — | — | — | — | gap |
| P1: Conformance suite + integration profiles | — | — | — | `negative_cases.json` | `run_conformance.py` | done |
| Phases A–H: File_collection acquisition | `offf-collect` | `parquet_tables_survive_round_trip` | — | — | — | partial |
| Phases J–P: Parallel processing | `offf-core shard/scope` | `parallel_job_4_shards_valid`, `parallel_job_missing_shard_result_detected`, `parallel_job_corrupt_artifact_hash_detected`, `parallel_job_duplicate_input_detected`, `parallel_job_scope_hash_mismatch_detected` | — | `parallel_job_missing_*`, `parallel_job_corrupt_*`, `parallel_job_duplicate_*`, `parallel_job_scope_hash_*` | — | done |

---

## Summary

| Status | Count |
|---|---|
| `done` | 12 |
| `partial` | 21 |
| `gap` | 3 |

### Known gaps

1. **Sprint 0 — Repository baseline**: No automated test verifies README/status.md exist and link correctly.
2. **P2 — Threat model**: The threat model is a documentation artifact; no automated test traces it. Consider a link-check or test that asserts required sections exist.
3. **P1 — Tool registry + governance**: Example JSON only; no tests.

### Partial coverage items (missing test types)

Most `partial` items are missing one or more of: E2E test, negative test dataset, or conformance scenario. See `docs/evidence-of-done.md` for per-item details.

---

## CI reference

Tests that run on every PR via `.github/workflows/offf-ci.yml`:

| Job | Command |
|---|---|
| rust-quality-gates | `cargo fmt --check`, `cargo clippy --workspace`, `cargo test --workspace`, `cargo build --release` |
| cli-e2e | `python tests/e2e/run_cli_e2e.py` |
| python-sdk-contract | `python -m unittest sdk/python/tests/test_api_contract.py` |
| go-sdk-smoke | `go test ./...` (sdk/go) |
| schema-validation | Validates all JSON schema files |
| conformance-scaffold | `python tests/conformance/run_conformance.py` |
| check-maturity-metadata | `python scripts/check_component_metadata.py` |
| check-test-traceability | `python scripts/check_test_traceability.py` |
| generate-release-readiness | `python scripts/generate_release_readiness.py` |

---

*Last updated: 2026-05-29*
