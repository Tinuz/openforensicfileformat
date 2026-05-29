# OFFF Definition of Done

A backlog item or feature is **done** when all of the following criteria are met.

---

## Checklist

| # | Criterion | Required for |
|---|---|---|
| 1 | `status: done` in `BACKLOG.txt` | All items |
| 2 | `classification` assigned (core / reference / demo / experimental / legacy) | All items |
| 3 | `maturity` assigned (demo-only / experimental / reference / forensic-grade-candidate / forensic-grade) | All items |
| 4 | `implemented-in:` lists the crate(s) or file path(s) where the feature lives | All items |
| 5 | At least one test type (unit / integration / E2E / negative / conformance) with a named test | All items except `gap`-status items |
| 6 | Test entry in `docs/test-traceability.md` with status ≠ `gap` | All done items |
| 7 | Evidence entry in `docs/evidence-of-done.md` with conclusion | All done items |
| 8 | `docs/status.md` updated to reflect current maturity | All items |
| 9 | CI green (all workflow jobs pass): `cargo test --workspace`, schema validation, conformance scaffold | All items |

---

## Partial vs Gap vs Done

| Status | Meaning |
|---|---|
| `done` | All 9 criteria met |
| `partial` | Some tests exist but not all test types are covered |
| `gap` | No test evidence; documented reason required |

Items with `status: gap` in `docs/test-traceability.md` are accepted only when:
- The item is purely a documentation artifact (e.g., threat-model.md).
- No automated test is technically feasible.
- The gap is explicitly documented.

---

## Classification and maturity rules

See `docs/maturity-model.md` and `docs/component-classification.md` for the
full definitions.

Key rules:
- **Demo is not forensic-grade.** A demo tool may not claim `forensic-grade` or
  `forensic-grade-candidate` maturity.
- **Reference is not Core.** Scheduling, orchestration, and transport layers are
  reference components, not part of the Core specification.
- **Core components at forensic-grade-candidate must have tests and docs** — verified
  by `scripts/check_component_metadata.py`.

---

## Backlog item format (minimum)

```
[x] Sprint N: Short title
    status: done
    classification: core|reference|demo|experimental|legacy
    maturity: demo-only|experimental|reference|forensic-grade-candidate|forensic-grade
    implemented-in: crates/offf-xxx/src/yyy.rs, ...
    tests: test_name_1, test_name_2
    conformance: profile-name (if applicable)
    known-limitations: short description (if any)
    Done: YYYY-MM-DD
```

---

*Last updated: 2026-05-29*
