# OFFF Threat Model

**Version:** 0.1.0
**Date:** 2026-05-29
**Updated:** 2026-05-30
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
- **T-02 RESOLVED:** `offf-convert` writes a `manifest.hmac` HMAC-SHA256 sidecar when
  `OFFF_MANIFEST_HMAC_KEY` is set. `offf-verify` checks this sidecar and exits non-zero
  on signature mismatch, detecting any post-acquisition tampering of `manifest.json`.

**Test evidence:** Schema validation in CI (`schema-validation` job).
Manifest HMAC sidecar logic verified by code path in `offf-convert/src/main.rs` and
`offf-verify/src/main.rs`.

**Residual risk (resolved):** Manifest integrity is now cryptographically protected when
`OFFF_MANIFEST_HMAC_KEY` is set. In deployments without the key, container integrity
still relies on file system access controls.

---

### T-03: Provenance forgery

**Threat:** An attacker injects a fraudulent `provenance_events.jsonl` entry claiming
a different actor or timestamp.

**Mitigation:**
- Access Service blocks all writes to `provenance/` except via `AppendProvenanceEvent`.
- Denied writes are logged to `denied_access_events.jsonl`.
- `AppendProvenanceEvent` requires a valid capability token.
- **T-03 RESOLVED:** In `jwt` auth mode, the actor's `tool_id` and `role` are embedded
  in a cryptographically signed HMAC-SHA256 token (see T-04). Actor identity in
  provenance events is therefore cryptographically tied to the token, not self-asserted.

**Test evidence:** `grpc_smoke.rs` denied write test.
JWT token validation: `tests::jwt_valid_token_accepted`, `tests::jwt_tampered_payload_rejected`,
`jwt_mode_valid_token_accepted_and_invalid_rejected` (integration test in `grpc_smoke.rs`).

**Residual risk (resolved in jwt mode):** In `dev_headers` mode, actor identity remains
self-asserted for backward compatibility. Production deployments must use `jwt` mode.

---

### T-04: Unauthorized access

**Threat:** An unauthorized party reads or writes to an OFFF container via the
Access Service.

**Mitigation:**
- Access Service enforces capability-gated access per registered tool in tool registry.
- All denied attempts are logged.
- **T-04 RESOLVED:** `jwt` auth mode now performs full HMAC-SHA256 bearer token
  validation. Token format: `<base64url_payload>.<base64url_hmac_sig>` where the
  signature covers the payload and is verified with `OFFF_JWT_SECRET`. Invalid,
  expired, or tampered tokens are rejected with HTTP 401 / gRPC Unauthenticated.
  Constant-time comparison prevents timing-based signature forgery.

**Test evidence:** `grpc_smoke.rs` denied overwrite test.
Unit tests: `tests::jwt_valid_token_accepted`, `tests::jwt_wrong_secret_rejected`,
`tests::jwt_expired_token_rejected`, `tests::jwt_tampered_payload_rejected`,
`tests::jwt_missing_dot_separator_rejected`.
Integration: `jwt_mode_valid_token_accepted_and_invalid_rejected`.

**Residual risk (resolved in jwt mode):** `dev_headers` mode remains available for
development use without validation. `mTLS` mode is still not fully implemented.

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
- **T-06 RESOLVED:** Explicit unit tests verify path traversal rejection:
  `tests::path_traversal_dot_dot_blocked`, `tests::path_traversal_encoded_dot_dot_blocked`,
  `tests::path_traversal_dot_dot_in_segment_blocked`,
  `tests::path_traversal_backslash_with_dot_dot_blocked` (all in `main.rs #[cfg(test)]`).
  Backslash normalisation is also explicitly tested (`tests::path_traversal_backslash_converted`).

**Test evidence:** See test names above in `crates/offf-access-service/src/main.rs`.

**Residual risk (resolved):** Path traversal attacks are blocked by `normalize_rel_path()`
and covered by explicit negative tests.

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
- **T-08 RESOLVED:** Explicit round-trip serialisation tests verify that filenames,
  keywords, and rule names containing `"`, `\n`, `\r`, `\0`, `,` and other special
  characters are properly escaped. Tests also verify that JSONL output is correctly
  newline-delimited even when field values contain embedded newlines.

**Test evidence:** `serialisation_safety_tests` module in `crates/offf-core/src/types.rs`:
`filename_with_embedded_quote_round_trips`, `filename_with_newline_round_trips`,
`keyword_with_comma_and_quote_round_trips`, `yara_rule_name_with_special_chars_round_trips`,
`jsonl_rows_are_newline_delimited_correctly` (and more).

**Residual risk (resolved):** Serialisation safety is now explicitly tested.
Fuzz testing remains a gap for future sprints.

---

### T-09: Worker impersonation

**Threat:** A rogue worker claims to be a trusted worker and writes false analysis
results.

**Mitigation:**
- Job manifests record the claimed `tool_id`.
- Access Service validates capability tokens (but not worker cryptographic identity).
- Denied writes are logged.
- **T-09 RESOLVED:** In `jwt` auth mode, worker identity is embedded in a
  cryptographically signed token. The token's `tool_id` claim is verified against
  the tool registry — a worker that does not hold the signing secret cannot
  successfully impersonate another registered tool.

**Test evidence:** Integration test `jwt_mode_valid_token_accepted_and_invalid_rejected`
verifies that a token with a different identity cannot be forged.

**Residual risk (resolved in jwt mode):** Worker identity is now cryptographically
backed in `jwt` auth mode. `dev_headers` mode (development only) remains open.

---

### T-10: Denial of service via large inputs

**Threat:** A maliciously large image, file collection, or YARA ruleset causes the
OFFF pipeline to exhaust memory or disk space.

**Mitigation:**
- Workers process inputs in streaming / chunk-at-a-time fashion.
- Chunk size is bounded by `sector_size` × chunk factor.
- Parallel shard processing allows load distribution.
- **T-10 RESOLVED:** Access Service now enforces:
  - HTTP request body size limit: 10 MB (`DefaultBodyLimit::max(10 * 1024 * 1024)`).
  - Maximum rows per write request: `MAX_ROWS_PER_REQUEST = 50_000`.
  Both limits are applied before any write is processed.

**Test evidence:** `tests::max_rows_per_request_constant_is_reasonable` (unit test in
`crates/offf-access-service/src/main.rs`). Body size limit enforced by axum middleware.

**Residual risk (resolved at API level):** API-level resource limits are now enforced.
OS/orchestrator-level resource limits remain the responsibility of the deployment
environment.

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
| 0.1.1 | 2026-05-30 | T-02: manifest HMAC sidecar; T-03/T-04/T-09: HMAC-signed JWT tokens; T-06: explicit path traversal tests; T-08: injection serialisation tests; T-10: request body size limit + max rows |

---

*Last updated: 2026-05-29*
