# OFFF Component Status Matrix

This matrix is the canonical per-component status record. For maturity level definitions see
`docs/maturity-model.md`. For Core/Reference/Demo classification see
`docs/component-classification.md`. Machine-readable metadata is in `components.toml`.

## 1.0 Scope

The first OFFF 1.0 release is intentionally narrower than the full repository surface. The 1.0 stability promise currently covers only the components explicitly marked with `release_1_0 = true` in `components.toml`:

- `offf-core`
- Evidence container schema
- Object lineage model
- Schema catalog
- `offf-convert` (raw/dd path)
- `offf-verify`

All other components remain available, but are reference or experimental surfaces outside the first 1.0 guarantee until they are explicitly promoted.

## Status Table

| Component | Classification | Maturity | Status | Test evidence | Conformance profile | Known limitations | Last verified |
|---|---|---|---|---|---|---|---|
| offf-core | core | forensic-grade-candidate | done | unit (50 tests), integration (25 tests) | Reader, Acquisition, Object-Lineage | NTFS parser experimental; packed I/O reference-level; parallel worker context reference-level | 2026-05-29 |
| Evidence container schema | core | forensic-grade-candidate | done | conformance suite, schema validation CI | Reader, Acquisition | — | 2026-05-29 |
| Object lineage model | core | forensic-grade-candidate | done | unit + integration | Object-Lineage | Lineage verify CLI partially complete; object-index rebuild experimental | 2026-05-29 |
| Extension model | core | reference | done | unit | Extension | Validates known JSONL types only | 2026-05-29 |
| Schema catalog | core | forensic-grade-candidate | done | schema validation CI | — | — | 2026-05-29 |
| Conformance suite | core | reference | done | python tests/conformance/run_conformance.py | — | Negative cases not yet exhaustive | 2026-05-29 |
| offf-convert (raw/dd) | reference | forensic-grade-candidate | done | unit + integration + E2E | Acquisition | Atomic rename on network FS may not work; S3 completion marker missing | 2026-05-29 |
| offf-convert (E01) | reference | experimental | done | smoke test (tests/smoke/phase7_e01_smoke.py) | Acquisition (partial) | Requires ewfexport/libewf in PATH | 2026-05-29 |
| offf-verify | reference | forensic-grade-candidate | done | unit + integration + conformance + E2E | Reader, Acquisition, Object-Lineage, Extension | Parallel job verify and lineage verify are reference/experimental | 2026-05-29 |
| offf-export | reference | reference | done | unit + integration | Reader | Remote/S3 export experimental; no E2E pack/unpack round-trip test | 2026-05-29 |
| offf-index (partitions) | reference | reference | done | unit + integration | Indexer | GPT CRC/backup experimental; NTFS parser experimental | 2026-05-29 |
| offf-index (objects) | reference | experimental | done | unit + integration | Indexer | Object index rebuild (--from-events) experimental | 2026-05-29 |
| offf-jobs | reference | reference | done | unit + integration | Analysis Worker | Deterministic replay experimental; plan-shards/finalize-job reference | 2026-05-29 |
| offf-collect | reference | experimental | done | integration (single file, directory, tamper detection) | Acquisition (partial) | No dedicated E2E file_collection upload/verify cycle yet | 2026-05-31 |
| offf-annotate | reference | experimental | done | external worker workspace tests | Extension | Moved to `Tinuz/offf-workers`; integration evidence now maintained there | 2026-06-02 |
| offf-keyword-worker | reference | experimental | done | external worker workspace tests | Analysis Worker | Moved to `Tinuz/offf-workers`; cross-chunk conformance evidence still pending | 2026-06-02 |
| offf-yara-worker | reference | experimental | done | external worker workspace tests | Analysis Worker | Moved to `Tinuz/offf-workers`; dedicated negative dataset still pending | 2026-06-02 |
| offf-access-service | reference | experimental | done | unit + gRPC smoke + storage parity | Access Service | JWT/mTLS not independently security-reviewed; S3 smoke-tested only | 2026-05-29 |
| Packed container (.offfpack) | reference | reference | done | unit | — | Not normative; verify requires unpacking; no E2E round-trip test | 2026-05-29 |
| Worker runtime state | reference | experimental | done | unit (`offf-jobs` runtime-state artifact test) | — | Not part of OFFF Core; external schedulers may use own state store | 2026-05-31 |
| Python SDK | reference | experimental | done | SDK contract test | Reader, Analysis Worker | v0.2 API parity incomplete; LRU cache not battle-tested at scale | 2026-05-29 |
| Go SDK | reference | experimental | done | go test ./... | Reader | v0.2 API parity incomplete; no lineage/extension API | 2026-05-29 |
| Tool registry | reference | experimental | done | integration (`tool_registry_example_has_expected_capabilities`) | Access Service | Example file only; no enforcement tooling | 2026-05-31 |
| Demo scripts | demo | demo-only | done | smoke_check_demo.ps1 | — | Not production-grade; not conformance evidence | 2026-05-29 |
| Parallel processing | reference | reference | done | integration (5 tests) | Analysis Worker | Shard planner and worker context are reference-level; finalize/verify in progress | 2026-05-29 |

## Maturity Legend

| Level | Meaning |
|---|---|
| `demo-only` | Concept demonstration only; not normative |
| `experimental` | Working prototype; API/schema may change |
| `reference` | Sound implementation; suitable for adoption |
| `forensic-grade-candidate` | Mostly meets test/conformance requirements; external review pending |
| `forensic-grade` | Stable, tested, conformance-covered, release-ready |

See `docs/maturity-model.md` for full criteria per level.

## Classification Legend

| Label | Meaning |
|---|---|
| `core` | Normative part of OFFF specification or core library |
| `reference` | Reference implementation of an OFFF contract |
| `demo` | Demonstration only |
| `experimental` | Unstable prototype |

See `docs/component-classification.md` for full classification table.

## Related

- Root overview: `README.md`
- Maturity model: `docs/maturity-model.md`
- Classification: `docs/component-classification.md`
- Formal spec: `SPEC_OFFF_Formal_Spec_v0.1.0.md`
- Schema catalog: `docs/schema/offf-schema-catalog-0.2.0.json`
- Machine-readable metadata: `components.toml`
- Evidence of done: `docs/evidence-of-done.md`
- Test traceability: `docs/test-traceability.md`
- Conformance profiles: `docs/conformance-profiles.md`
- Hardening program: `BACKLOG.txt`
