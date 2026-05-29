# OFFF Maturity Model

## Overview

OFFF uses a five-level maturity classification to communicate the stability, test coverage,
and deployment readiness of each component. Every component carries a `maturity` label chosen
from this model. Component metadata is recorded in `components.toml` and reflected in
`docs/status.md`.

The purpose is to make explicit: what is production-ready, what is a reference example,
and what exists only for concept demonstration.

## Maturity Levels

| Level | Name | Short meaning |
|---|---|---|
| `demo-only` | Demo-only | Concept demonstration; not normative behaviour |
| `experimental` | Experimental | Working prototype; API/schema may change |
| `reference` | Reference implementation | Sound implementation of the OFFF contract; suitable for adoption |
| `forensic-grade-candidate` | Forensic-grade candidate | Mostly meets test/conformance requirements; external review pending |
| `forensic-grade` | Forensic-grade | Stable, tested, documented, conformance-covered, release-ready |

---

## Criteria Per Level

### `demo-only`

| Criterion | Requirement |
|---|---|
| Test coverage | Not required |
| Documentation | Minimal — purpose and scope only |
| Backward compatibility | Not guaranteed |
| Conformance | Not applicable |
| Security review | Not required |
| Breaking changes | Allowed at any time |
| Production advice | **Not suitable for production or as forensic evidence** |

Additional rules:
- May use synthetic or simplified data.
- May use a simplified OFFF container that does not follow the full specification.
- **Shall not** be cited as evidence of OFFF conformance or forensic validity.
- Must carry a visible `demo-only` label in documentation and component metadata.

---

### `experimental`

| Criterion | Requirement |
|---|---|
| Test coverage | Unit tests required |
| Documentation | Design notes or README-level |
| Backward compatibility | Not guaranteed across minor versions |
| Conformance | Optional; at least one conformance scenario recommended |
| Security review | Not required |
| Breaking changes | Allowed in minor version bumps |
| Production advice | Not recommended for production forensic use |

---

### `reference`

| Criterion | Requirement |
|---|---|
| Test coverage | Unit tests + integration tests |
| Documentation | Public API documented; known limitations listed |
| Backward compatibility | Breaking changes require a minor version bump |
| Conformance | At least one conformance profile partially covered |
| Security review | Recommended but not mandatory |
| Breaking changes | Minor version bump required; deprecation notice preferred |
| Production advice | Suitable for adoption and integration testing; not production forensics without further review |

---

### `forensic-grade-candidate`

| Criterion | Requirement |
|---|---|
| Test coverage | Unit + integration + negative/adversarial tests |
| Documentation | Full public API + schema documentation; limitations explicitly listed |
| Backward compatibility | Breaking changes require major version bump |
| Conformance | All applicable conformance profiles fully covered |
| Security review | Done (internal) |
| Breaking changes | Major version bump + deprecation notice required |
| Production advice | Suitable for production with documented limitations; external review recommended before critical deployments |

---

### `forensic-grade`

| Criterion | Requirement |
|---|---|
| Test coverage | Unit + integration + negative + E2E + conformance |
| Documentation | Full documentation including security notes and threat model |
| Backward compatibility | Breaking changes only with major version bump and migration path |
| Conformance | All applicable profiles covered; machine-readable report available |
| Security review | Done — internal + external/third-party |
| Breaking changes | Major version + documented migration path required; no silent breaks |
| Production advice | **Production-grade for forensic use** |

---

## Level Relationship

```
demo-only  →  experimental  →  reference  →  forensic-grade-candidate  →  forensic-grade
```

A component may be adopted at `reference` level. It becomes `forensic-grade-candidate` when it
passes all applicable conformance scenarios and completes an internal security review. It reaches
`forensic-grade` after external review and full documentation are complete.

---

## Important Distinctions

### Demo is not forensic-grade

A component labelled `demo-only` **must not** be cited in forensic reports, legal proceedings,
or compliance documentation as evidence of OFFF conformance or forensic validity.

### Reference implementation is not Core

A `reference` component implements an OFFF contract but is **not part of the normative OFFF
specification**. Third parties may implement their own tools that satisfy the same contract.

### Core components

Components classified as `core` contain the normative logic for OFFF (schema types, chunk IO,
Merkle proofs, object lineage model). They are held to `forensic-grade-candidate` or higher
standards. See `docs/component-classification.md` for the full classification table.

---

## Relationship to Definition of Done

Every backlog item that moves to `done` status must specify a maturity level. See
`docs/definition-of-done.md` for the full checklist.

---

*Last updated: 2026-05-29*
