# OFFF Forensic Baseline Profile

## Purpose

This document defines the minimum set of OFFF requirements that a container must satisfy before
it may be used in a formal forensic context. The Forensic Baseline Profile is not about what is
possible with OFFF; it is about what is non-negotiable for evidentiary integrity.

A container that passes all required checks in this profile is a **Forensic Baseline Conformant**
container. A tool that produces or verifies such containers may claim **OFFF Forensic Baseline
Conformant** status (see `docs/conformance-profiles.md`).

Passing this profile does not constitute a legal finding. Legal interpretation of forensic
evidence remains outside the scope of OFFF Core (see `docs/legal-neutrality.md`).

---

## Required Elements

### 1. Manifest and Acquisition Metadata

- `manifest.json` must be present, parseable, and schema-valid against
  `offf-manifest-0.1.0.schema.json` or `offf-manifest-0.2.0.schema.json`.
- `acquisition.json` must be present, parseable, and schema-valid against
  `offf-acquisition-0.1.0.schema.json`.
- Both files must be written as the last step of container creation (finalization point).
  No further changes to these files are permitted after finalization.

**Verifier check:** `manifest.json` and `acquisition.json` parseable and schema-valid — **PASS/FAIL**

---

### 2. Acquisition Mode

- `acquisition.json` must contain a valid `acquisition_mode` field.
- Accepted values: `block_image`, `file_collection`, `logical_extraction`, `api_export`, `mixed`.
- Each mode implies a different evidence root model (see `docs/evidence-root-model.md`).

**Verifier check:** `acquisition_mode` present and is a known value — **PASS/FAIL**

---

### 3. Evidence Root Model

- The evidence root is the primary source object from which all analysis derives.
- For `block_image`: the image file; sector size and source hash must be recorded.
- For `file_collection`: the collection source; object count and collection scope must be recorded.
- For other modes: root identity must be explicitly declared in `acquisition.json`.

See `docs/evidence-root-model.md` for full per-mode requirements.

**Verifier check:** Evidence root is identifiable from `acquisition.json` — **PASS/FAIL**

---

### 4. Immutable Evidence Layer

- The following paths must not be modified after container finalization:
  - `manifest.json`
  - `acquisition.json`
  - `chunks/` and their contents
  - `hashes/`
  - `maps/`
- Only the analysis layer (`analysis/`), provenance layer (`provenance/`), and
  extension layer (`extensions/`) are append-only after finalization.

**Verifier check:** No evidence layer files have been modified after `manifest.json` was written — **PASS/FAIL** (timestamp comparison; see notes on limitations below)

---

### 5. SHA-256 Minimum

- Every chunk stored in `chunks/sha256/` must have its SHA-256 hash recorded in
  `hashes/leaves.parquet`.
- The verifier must re-compute SHA-256 for each chunk and compare against the stored hash.
- A mismatch must cause the verifier to report **FAIL** for that chunk.

**Verifier check:** All stored chunk hashes match computed hashes — **PASS/FAIL**

---

### 6. Merkle Tree / Proofs (block_image mode)

For containers with `acquisition_mode = block_image`:

- `hashes/merkle_tree.bin` must be present and non-empty.
- `hashes/leaves.parquet` must be present.
- The Merkle root stored in `manifest.json` must equal the root computed from `leaves.parquet`.
- The verifier must reconstruct the Merkle root and validate it against the manifest.

**Verifier check:** Merkle root in manifest matches computed root — **PASS/FAIL**

For other acquisition modes, `hashes/leaves.parquet` with per-object SHA-256 records is
strongly recommended but not required for baseline conformance.

---

### 7. Object Index (file_collection and derived objects)

For containers with `acquisition_mode = file_collection` or containing derived objects:

- An object index (`indexes/file_index.parquet` or `indexes/object_index.parquet`) must be
  present.
- Each indexed object must have at minimum: a stable identifier, a name or path, and a
  SHA-256 hash.

**Verifier check:** Object index is present and parseable — **PASS/FAIL** (for applicable modes)

---

### 8. Object Lineage (nested and derived objects)

When an OFFF container contains objects derived from other objects (e.g., files extracted from
an archive, OCR results from a document, parsed structures from a binary):

- Derivation records must be present in `indexes/derivations.parquet` or equivalent.
- Each derived object must reference its source object by stable identifier.
- Dangling references (source object not found) must be reported by the verifier.

**Verifier check:** No dangling lineage references — **PASS/FAIL** (if derivations are present)

---

### 9. Append-Only Analysis Output

- All analysis job output must reside under `analysis/jobs/{job_id}/`.
- A job's output directory must not overwrite or delete outputs from another job.
- Workers must not write to `chunks/`, `hashes/`, `maps/`, `manifest.json`,
  `acquisition.json`, or `provenance/`.

**Verifier check:** No evidence or provenance layer mutations from analysis jobs — **PASS/FAIL**

---

### 10. Result Manifest per Job

- Every completed analysis job must produce a `result_manifest.json` under its job directory.
- The result manifest must contain SHA-256 hashes for all output artifacts.
- The verifier must re-compute artifact hashes and compare against the result manifest.

**Verifier check:** All artifact hashes in result manifest match actual files — **PASS/FAIL**

---

### 11. Provenance per Job

- A provenance event must be appended to `provenance/provenance_events.jsonl` for each
  completed job.
- The event must record: job identifier, tool identity, actor, timestamp, and outcome.

**Verifier check:** At least one acquisition provenance event is present and parseable — **PASS/FAIL**

---

### 12. Skipped, Error, and Denied Events

- When a tool skips processing an object, it must record a skipped event.
- When a tool encounters an error, it must record an error event.
- When the access service denies a write, it must record a denied access event.
- These events must not be silently discarded.

**Verifier check:** Any declared skipped/error/denied events are parseable — **PASS/FAIL**

---

### 13. Verifier Report

- The container must be verifiable by `offf-verify` without modification to its contents.
- The verifier must produce a structured report (JSON and/or Markdown) covering all required
  checks.
- The report must distinguish ERROR, WARNING, and INFO severity.
- The report must explicitly list failed checks and known limitations.

**Verifier check (meta):** `offf-verify --profile forensic-baseline` exits with code 0 on a
valid container and non-zero on a container with any FAIL-level check.

---

### 14. Known Limitations

The container producer must document any known deviations from baseline requirements.
Acceptable limitation documentation:

- Missing Merkle tree for non-block_image acquisitions.
- Partial object index for streaming acquisitions.
- No lineage records when no derived objects are present.

An undocumented deviation from a required element is a baseline conformance failure.

**Verifier check:** No undocumented deviations detected — **PASS/FAIL**

---

### 15. Conformance Report

A container that has been verified against this profile must be accompanied by a machine-readable
conformance report. The report must:

- Identify the profile checked (`forensic-baseline`).
- List each required check and its result (pass / fail / not-applicable).
- Record the verifier version and the date of verification.
- Be stored outside the container (not as part of the immutable evidence layer).

See `docs/conformance-profiles.md` for the report schema.

---

## Explicitly Out of Scope

The following are **not** required for baseline conformance and **must not** be treated as
preconditions for forensic validity:

| Out-of-scope item | Reason |
|---|---|
| Specific forensic UI or viewer | OFFF is format-agnostic with respect to UI |
| Specific scheduler or orchestration platform | Worker scheduling is reference-level, not normative |
| Specific parser or file format handler | Tool choice is outside OFFF Core |
| Specific AI/ML tool or classifier | AI output is analysis-layer, not evidence-layer |
| Automated legal decision-making | Juridical interpretation is outside OFFF scope |
| Specific vendor integration (Hansken, FTK, Cellebrite, etc.) | OFFF is vendor-neutral |
| OFFF packed container (.offfpack) | Transport format; verification requires unpacking |

---

## Baseline Pass/Fail Summary

| ID | Check | Severity | Required for mode |
|---|---|---|---|
| BL-01 | `manifest.json` present, parseable, schema-valid | FAIL | all |
| BL-02 | `acquisition.json` present, parseable, schema-valid | FAIL | all |
| BL-03 | `acquisition_mode` present and known | FAIL | all |
| BL-04 | Evidence root identifiable from `acquisition.json` | FAIL | all |
| BL-05 | Evidence layer not modified after finalization | FAIL | all |
| BL-06 | All chunk SHA-256 hashes match stored hashes | FAIL | all |
| BL-07 | Merkle root in manifest matches computed root | FAIL | block_image |
| BL-08 | Object index present and parseable | FAIL | file_collection, derived |
| BL-09 | No dangling lineage references | FAIL | when derivations present |
| BL-10 | No evidence layer mutations from analysis jobs | FAIL | all |
| BL-11 | All result manifest artifact hashes correct | FAIL | when jobs present |
| BL-12 | At least one acquisition provenance event | FAIL | all |
| BL-13 | Skipped/error/denied events are parseable | FAIL | when events declared |
| BL-14 | No undocumented deviations from required elements | FAIL | all |

---

## How to Verify

```bash
# Full baseline profile check
cargo run -p offf-verify -- <container.offf> --profile forensic-baseline

# With structured reports
cargo run -p offf-verify -- <container.offf> \
  --profile forensic-baseline \
  --report-json verification-report.json \
  --report-md  verification-report.md
```

The exit code of `offf-verify` is:
- `0` — all required checks passed.
- `1` — one or more FAIL-level checks failed.

---

## Related Documents

- `docs/evidence-root-model.md` — acquisition mode specifics
- `docs/conformance-profiles.md` — formal OFFF Forensic Baseline Conformant profile
- `docs/legal-neutrality.md` — what OFFF does and does not assert legally
- `docs/forensic-limitations.md` — known limitations per mode
- `docs/chain-of-evidence.md` — technical provenance chain model
- `docs/threat-model.md` — security threat model

*Last updated: 2026-05-29*
