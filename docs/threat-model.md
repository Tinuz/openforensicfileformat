# OFFF Threat Model

**Version:** 0.1.0
**Date:** 2026-05-29
**Status:** Reference document — not a CI artifact

---

## Scope

This document covers the threat model for the OFFF format, its reference
implementations, and the Access Service. It does not cover threats to the
underlying operating system, storage hardware, or network infrastructure
that OFFF operates on.

---

## Assets

| Asset | Description | Integrity required |
|---|---|---|
| Evidence layer | `chunks/`, `hashes/`, `manifest.json`, `maps/` | Critical |
| Acquisition metadata | `acquisition.json`, `provenance/` | Critical |
| Analysis results | `analysis/jobs/{job_id}/` | High |
| Object index | `indexes/objects/` | High |
| Extension data | `extensions/` | Medium |
| Access audit log | `extensions/access/denied_access_events.jsonl` | High |

---

## Threat vectors and mitigations

### T-01: Evidence tampering

**Threat:** An attacker modifies chunk files after acquisition to alter forensic evidence.

**Mitigation:**
- SHA-256 per chunk stored in `physical_to_chunk.parquet`.
- Merkle tree root in `manifest.json` covers all chunk hashes.
- `offf-verify` re-computes all hashes and the Merkle root on every run.
- `offf-verify` exits non-zero and reports the corrupt chunk.

**Test evidence:** `verify_detects_chunk_corruption` (integration test).

**Residual risk:** SHA-256 collision attacks are computationally infeasible today.
If SHA-256 is deprecated, a migration to SHA-3/BLAKE3 is required.

---

### T-02: Metadata manipulation

**Threat:** An attacker modifies `manifest.json`, `acquisition.json`, or
`provenance_events.jsonl` to falsify forensic metadata.

**Mitigation:**
- `manifest.json` is the finalization point; all other files are written first.
- Schema validation detects missing or invalid required fields.
- Provenance events include `event_id` and `actor` — any deletion breaks the chain.

**Test evidence:** Schema validation in CI (`schema-validation` job).

**Residual risk:** No cryptographic signing of `manifest.json` in v0.1. A signature
field is planned for v0.3. Until then, container integrity must be maintained via
file system access controls.

---

### T-03: Provenance forgery

**Threat:** An attacker injects a fraudulent `provenance_events.jsonl` entry claiming
a different actor or timestamp.

**Mitigation:**
- Access Service blocks all writes to `provenance/` except via `AppendProvenanceEvent`.
- Denied writes are logged to `denied_access_events.jsonl`.
- `AppendProvenanceEvent` requires a valid capability token.

**Test evidence:** `grpc_smoke.rs` denied write test.

**Residual risk:** Actor identity is asserted, not cryptographically verified.
Worker impersonation is a known limitation (see T-09).

---

### T-04: Unauthorized access

**Threat:** An unauthorized party reads or writes to an OFFF container via the
Access Service.

**Mitigation:**
- Access Service enforces capability-gated access per registered tool in tool registry.
- All denied attempts are logged.
- Future: JWT / mTLS authentication (not yet implemented).

**Test evidence:** `grpc_smoke.rs` denied overwrite test.

**Residual risk:** Current auth is capability-token-only; no user identity verification.

---

### T-05: Analysis result poisoning

**Threat:** A malicious worker writes fabricated analysis results (e.g., false YARA
hits, false keyword matches) to influence an investigation.

**Mitigation:**
- Analysis results are stored under `analysis/jobs/{job_id}/` and never overwrite
  the evidence layer.
- Result manifest includes SHA-256 hashes of all artifacts.
- `offf-verify` validates result manifest hashes.
- Job manifests record the tool identity and parameters used.

**Test evidence:** `offf-verify` hash validation (unit test).

**Residual risk:** Tool identity in `JobManifest` is self-reported.

---

### T-06: Path traversal

**Threat:** A crafted input causes a worker or the Access Service to write to a path
outside the OFFF container (e.g., `../../etc/passwd`).

**Mitigation:**
- Access Service validates all write paths: writes are rejected if the resolved path
  does not start with the container root.
- All chunk paths are constructed from SHA-256 hashes (no user-controlled path
  components in the evidence layer).
- Object index paths are validated against the container root before writing.

**Test evidence:** Path traversal rejection is implicitly enforced by the Access
Service path validation logic. Explicit negative test is a known gap.

**Residual risk:** Path traversal in tool-generated job output paths is not yet
explicitly tested. Track as a gap in `docs/test-traceability.md`.

---

### T-07: Hash collision attack

**Threat:** Two different inputs produce the same SHA-256 hash, causing a chunk or
object to be silently deduplicated with a different content.

**Mitigation:**
- SHA-256 has no known practical collision attacks as of 2026.
- Content-addressed storage: identical hash → identical content (by definition).
- If a collision were found, the verifier would not detect it (inherent to hash-based
  deduplication).

**Residual risk:** Theoretical. No mitigation beyond SHA-256 upgrade path to BLAKE3
or SHA-3 in a future format version.

---

### T-08: Injection via user-controlled input

**Threat:** A user-controlled string (file name, volume label, keyword pattern) is
injected into JSONL or Parquet output, causing downstream parsers to misinterpret
the data.

**Mitigation:**
- All JSONL output is written using a JSON serializer (not string concatenation).
- Parquet output is written using the Arrow/Parquet library (not string concatenation).
- YARA rule inputs are validated before execution.
- Keyword inputs are treated as literal strings, not regexes, unless explicitly
  declared as regex patterns.

**Test evidence:** Implicit (use of typed serializers). No explicit injection test.

**Residual risk:** No fuzz testing of JSONL parsers. Track as a gap.

---

### T-09: Worker impersonation

**Threat:** A rogue worker claims to be a trusted worker and writes false analysis
results.

**Mitigation:**
- Job manifests record the claimed `tool_id`.
- Access Service validates capability tokens (but not worker cryptographic identity).
- Denied writes are logged.

**Residual risk:** Worker identity is self-asserted; no cryptographic attestation.
Planned for a future Access Service version.

---

### T-10: Denial of service via large inputs

**Threat:** A maliciously large image, file collection, or YARA ruleset causes the
OFFF pipeline to exhaust memory or disk space.

**Mitigation:**
- Workers process inputs in streaming / chunk-at-a-time fashion.
- Chunk size is bounded by `sector_size` × chunk factor.
- Parallel shard processing allows load distribution.

**Residual risk:** No resource limits enforced at the API level. Worker resource
limits must be configured at the OS or orchestrator level.

---

## Out of scope

- Hardware-level threats (storage media tampering, cold-boot attacks).
- Threats to the investigator's workstation or operating system.
- Cryptographic weaknesses beyond SHA-256 collision resistance.
- Social engineering attacks against forensic analysts.

---

## Revision history

| Version | Date | Change |
|---|---|---|
| 0.1.0 | 2026-05-29 | Initial draft covering T-01 through T-10 |

---

*Last updated: 2026-05-29*
