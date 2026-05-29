# OFFF Reference Worker Runtime

## Overview

The OFFF worker runtime state — health registry, assignment audit log, retry policy,
and deterministic replay records — is a **reference implementation** of scheduling
and orchestration support. It is **not** part of the OFFF Core specification.

Classification: **reference**
Maturity: **experimental**

---

## What is "Core" vs "Reference"

| Category | Core | Reference |
|---|---|---|
| Format | `manifest.json`, `chunks/`, `hashes/`, `maps/` | Scheduling state, health registry |
| Verification | `offf-verify` | Not verified by `offf-verify` |
| Portability | Every OFFF tool must implement | Optional; may be replaced |
| Forensic evidence | Yes | No |

Scheduling and orchestration support is **not** Core because:
- The OFFF evidence layer is independent of how workers are dispatched.
- External orchestrators (Kubernetes Jobs, Temporal, custom queues) manage their own state.
- The OFFF format does not mandate a specific scheduler or runtime.

---

## Components

### offf-jobs

Manages `JobManifest` documents and routes work to workers.

- Stores job state locally (file-backed or in-memory, implementation-defined).
- Emits assignment events to `provenance/provenance_events.jsonl`.
- Does **not** write to the evidence layer.

### Health registry

Workers register heartbeats. The registry tracks:
- Worker ID, last heartbeat, status (`idle`, `running`, `failed`)
- Assignment history (which job was assigned to which worker, and when)

The health registry state is **ephemeral** and is not included in the OFFF container.
It is implementation-specific and may be stored in memory, Redis, a database, etc.

### Assignment audit log

Every job assignment is logged as a provenance event in the container:

```jsonl
{"event_id":"…","event_type":"job_assigned","actor":"offf-jobs","job_id":"…","worker_id":"…","assigned_at":"…"}
```

This is the only assignment-related data that enters the OFFF container.

### Retry policy

The reference implementation retries failed jobs up to a configurable limit
(default: 3). Retries are logged as separate `job_assigned` provenance events.

### Deterministic replay

When a job is run with `--deterministic` mode, the `JobManifest` includes:
- `deterministic: true`
- `seed` — a fixed seed for any pseudo-random operations

Deterministic mode enables re-running a job on the same input and expecting
byte-equivalent output artifacts.

---

## Integration

External schedulers may replace `offf-jobs` entirely. The only contract is:

1. Every assigned job must write its output to `analysis/jobs/{job_id}/`.
2. `result_manifest.json` must be written as the final step.
3. A `job_assigned` provenance event should be appended to the container.

The external scheduler may manage all other state (retries, health, assignment
audit) independently.

---

## Parallel processing support

`offf-jobs` supports parallel shard execution (Phases J–P):
- A job may be split into `N` shards via `ShardManifest`.
- Each shard is a separate `JobManifest` with a `shard_index` and `total_shards`.
- Results are collected via `ShardResultManifest` and merged.
- Scope hash verification detects mismatched or corrupted shard results.

---

## Limitations

- No distributed coordination: all workers must share a file system or object store.
- No worker authentication: worker identity is asserted, not verified cryptographically.
- Health registry state is lost on restart.

See `components.toml` entry for `worker-runtime-state` for the current known limitations.

---

*Last updated: 2026-05-29*
