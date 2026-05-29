# OFFF Conformance Profiles

This document defines the official OFFF conformance profiles. A tool claims a profile
when it implements the required functions, accepts the required schemas, and passes the
associated verification checks.

Profiles are not exclusive: a tool may claim multiple profiles. The conformance suite
at `tests/conformance/run_conformance.py` reports pass/fail per profile.

---

## Profile Definitions

### OFFF Reader Conformant

**Scope:** A tool that opens and reads an OFFF evidence container.

**Required functions:**
- Open a container given a local path or `s3://` URI.
- Read and parse `manifest.json`.
- Read raw bytes from a chunk given a physical offset.
- Verify a chunk by SHA-256 (compare stored hash vs computed hash).
- Map a physical byte offset to the correct chunk using `physical_to_chunk.parquet`.
- Validate the Merkle root against the chunk leaf set.

**Required schemas:**
- `offf-manifest-0.1.0.schema.json` or `offf-manifest-0.2.0.schema.json`
- `offf-partition-table-0.1.0.schema.json`

**Required verifier checks (pass/fail):**
- `manifest.json` parseable and schema-valid.
- `chunks/` directory exists with at least one chunk file.
- Chunk SHA-256 matches stored hash.
- Merkle root matches computed root from `hashes/leaves.parquet`.

**Required negative tests:**
- Corrupt chunk detected (hash mismatch → FAIL).
- Missing chunk referenced in index → FAIL.

**Optional:**
- `--profile core+schemas` additional schema validation.
- Inclusion proof generation and verification.

---

### OFFF Acquisition Conformant

**Scope:** A tool that creates a forensically valid OFFF container from a source image or file system.

**Required functions:**
- Write `manifest.json` strictly as the finalization point (last written file).
- Write `acquisition.json` with source hash, sector size, and tool identity.
- Write `provenance/provenance_events.jsonl` with at least one acquisition event.
- Use content-addressed chunk storage under `chunks/sha256/`.
- Compute and store `hashes/leaves.parquet` and `hashes/merkle_tree.bin`.
- Support crash-safe finalization (write to temp, atomic rename on local FS).

**Required schemas:**
- `offf-acquisition-0.1.0.schema.json`
- `offf-manifest-0.1.0.schema.json` or `offf-manifest-0.2.0.schema.json`
- `offf-provenance-event-0.1.0.schema.json`

**Required verifier checks (pass/fail):**
- `acquisition.json` parseable and schema-valid.
- `acquisition.parameters.sector_size` present.
- Source SHA-256 in `acquisition.json` matches provenance event.
- Provenance events have valid `event_id` and `actor` fields.

**Required negative tests:**
- Tampered `manifest.json` → `offf-verify` FAIL.
- Missing `acquisition.json` → FAIL.

**Optional:**
- Deterministic mode: repeated runs produce byte-equivalent metadata.
- `_OFFF_COMPLETE` marker for S3 object storage mode.

---

### OFFF Indexer Conformant

**Scope:** A tool that builds and queries object indexes within an OFFF container.

**Required functions:**
- Write `indexes/objects/object_index.parquet` conforming to `offf-object-index-row-0.2.0.schema.json`.
- Assign `object_id` values that are stable and unique within the container.
- Set `parser_status` to `ok`, `error`, or `skip` for each object.

**Required schemas:**
- `offf-object-index-row-0.2.0.schema.json`

**Required verifier checks (pass/fail):**
- `object_index.parquet` loadable and schema-valid.
- No duplicate `object_id` values within the index.

**Required negative tests:**
- Duplicate `object_id` → reported as lineage error.

**Optional:**
- `indexes/objects/object_edges.parquet` for parent/child relationships.
- `indexes/objects/derivations.parquet` for derived object tracking.
- MBR/GPT partition detection and output to `indexes/partitions/`.

---

### OFFF Analysis Worker Conformant

**Scope:** A tool that runs analysis on OFFF evidence and writes results in the append-only analysis layer.

**Required functions:**
- Read inputs by resolving `JobManifest.scope` against a container.
- Write all outputs under `analysis/jobs/{job_id}/`.
- Never write to `chunks/`, `hashes/`, `maps/`, `manifest.json`, or `acquisition.json`.
- Write `analysis/jobs/{job_id}/result_manifest.json` strictly as the finalization point.
- Include SHA-256 hashes for all output artifacts in the result manifest.
- Never overwrite an existing `result_manifest.json` (append-only contract).

**Required schemas:**
- `offf-job-manifest-0.1.0.schema.json`

**Required verifier checks (pass/fail):**
- `result_manifest.json` exists and is parseable.
- All artifact hashes in result manifest match actual files.
- No writes to the evidence layer detected.

**Required negative tests:**
- Attempt to overwrite existing job artifacts → refused.
- Missing `result_manifest.json` → verifier FAIL.

**Optional:**
- `output_contract` in `JobManifest` for object-producing workers.
- `errors.jsonl` and `statistics` in result manifest for error-tolerant runs.
- Parallel shard execution with `ShardManifest` / `ShardResultManifest`.

---

### OFFF Object-Lineage Conformant

**Scope:** A tool that produces or queries the OFFF object graph (object index, edges, derivations).

**Required functions:**
- Write `object_index.parquet`, `object_edges.parquet`, and/or `derivations.parquet` conforming to v0.2 schemas.
- Validate parent/child references (no dangling references).
- Detect and report cycles in the object graph.
- Trace a lineage path from a derived object back to its root evidence object.

**Required schemas:**
- `offf-object-index-row-0.2.0.schema.json`
- `offf-object-edge-row-0.2.0.schema.json`
- `offf-derivation-row-0.2.0.schema.json`
- `offf-lineage-report-0.2.0.schema.json`

**Required verifier checks (pass/fail):**
- All `parent_id` references in edges resolve to existing objects.
- No cycles detected in the directed object graph.

**Required negative tests:**
- Missing parent object → lineage validation FAIL.
- Cycle in object graph → cycle detection triggers.

**Optional:**
- `export_dot` / `export_lineage_json` for offline graph reporting.
- `offf-verify --object <id> --lineage` CLI validation.

---

### OFFF Access Service Conformant

**Scope:** A service that provides capability-gated read/write access to OFFF containers via gRPC or REST.

**Required functions:**
- `GetManifest`, `GetChunk`, `VerifyChunk` — read paths.
- `ListFiles`, `GetFile` — file index access.
- `WriteAnalysisResults` — append-only write to `analysis/jobs/{job_id}/`.
- `AppendProvenanceEvent` — append-only write to `provenance/`.
- Block all writes to the evidence layer (`chunks/`, `hashes/`, `maps/`, `manifest.json`).
- Append denied write attempts to `extensions/access/denied_access_events.jsonl`.

**Required schemas:**
- Tool registry schema for capability enforcement.

**Required verifier checks (pass/fail):**
- Denied writes are logged to `denied_access_events.jsonl`.
- No evidence layer mutation after container creation.

**Required negative tests:**
- Write to `chunks/` → denied and logged.
- Overwrite of existing `result_manifest.json` → denied and logged.
- Unauthenticated write attempt → denied.

**Optional:**
- JWT or mTLS authentication modes.
- S3/MinIO backend support.
- Object graph write endpoints (`/objects`, `/object-edges`, `/derivations`).

---

### OFFF Extension Conformant

**Scope:** A tool that reads or writes OFFF generic extension JSONL files.

**Required functions:**
- Append events to known extension JSONL types without overwriting existing events.
- Ignore unknown extension keys gracefully (forward compatibility).
- Validate known extension fields per schema.

**Required extension types (at least one of):**
- `LabelEvent` (`extensions/labels/label_events.jsonl`)
- `ScopeRecord` (`extensions/scopes/scopes.jsonl`)
- `SetRecord` (`extensions/sets/`)
- `AccessEvent`, `DeniedAccessEvent`, `AuditEvent` (`extensions/access/`, `extensions/audit/`)
- `AnnotationEvent` (`analysis/events/annotation_events.jsonl`)

**Required schemas:**
- `offf-annotation-event-0.1.0.schema.json`

**Required verifier checks (pass/fail):**
- `--profile core+extensions` in `offf-verify` passes.
- Known extension JSONL files are parseable.

**Required negative tests:**
- Invalid JSON in extension file → verifier WARN/FAIL.

**Optional:**
- `DecisionRecord`, `PolicyRef` for governance workflows.

---

## Conformance Report Format

The conformance suite writes `tests/conformance/conformance-report.json` with the following structure:

```json
{
  "generated_at": "2026-05-29T00:00:00Z",
  "profiles": {
    "OFFF Reader Conformant": { "status": "pass", "checks": [...] },
    "OFFF Acquisition Conformant": { "status": "pass", "checks": [...] },
    "OFFF Indexer Conformant": { "status": "partial", "checks": [...], "gaps": [...] }
  },
  "summary": {
    "pass": 3,
    "partial": 3,
    "fail": 0,
    "not_evaluated": 1
  }
}
```

---

## Claiming a Profile

A tool claims a profile by documenting it in `components.toml` under `conformance_profiles`:

```toml
[components.my-tool]
classification = "reference"
conformance_profiles = ["OFFF Reader Conformant", "OFFF Analysis Worker Conformant"]
```

A claim is only valid if:
1. The tool passes all required verifier checks for that profile.
2. The claim is accompanied by a conformance report showing pass status.

---

*Last updated: 2026-05-29*
