# OFFF Chain of Custody

## Purpose

This document describes how OFFF records the process chain of custody: who handled the evidence,
what operations were performed, when, and with which tools.

Where `docs/chain-of-evidence.md` addresses the technical integrity of the data, this document
addresses the *process* record — the audit trail that makes the handling of evidence accountable
and reviewable.

A chain of custody in OFFF is a technical record of process events. Its legal significance is
determined by applicable law and forensic practice, not by OFFF itself. See
`docs/legal-neutrality.md`.

---

## Custody Layers in an OFFF Container

OFFF records process custody in four complementary layers:

| Layer | Location | What it records |
|---|---|---|
| Acquisition provenance | `provenance/provenance_events.jsonl` | Source acquisition event |
| Job provenance | `provenance/provenance_events.jsonl` | Analysis job executions |
| Access audit | `extensions/audit/audit_events.jsonl` | Read/write access events |
| Denied access log | `extensions/access/denied_access_events.jsonl` | Rejected write attempts |

Together these layers provide an append-only, chronological record of every significant
action performed on the container.

---

## Provenance Events

Provenance events are the primary custody record. They are written to
`provenance/provenance_events.jsonl` in append-only JSONL format.

### Acquisition Event

The first provenance event must be the acquisition event. It must record:

| Field | Meaning |
|---|---|
| `event_id` | Stable UUID for this event |
| `event_type` | `acquisition` |
| `timestamp` | ISO-8601 UTC time of acquisition |
| `actor` | Identity of the person or system performing the acquisition |
| `tool.name` | Name of the acquisition tool |
| `tool.version` | Version of the acquisition tool |
| `source_sha256` | SHA-256 of the source evidence object |
| `container_id` | OFFF container UUID |

The `source_sha256` in this event must match the value in `acquisition.json`.
This cross-reference is checked by `offf-verify`.

### Job Execution Events

For each completed analysis job, a provenance event must be written recording:

| Field | Meaning |
|---|---|
| `event_id` | Stable UUID for this event |
| `event_type` | `job_execution` |
| `job_id` | Matches the job directory under `analysis/jobs/` |
| `timestamp` | ISO-8601 UTC time of job completion |
| `actor` | Tool identity (from tool registry) |
| `tool.name` | Name of the analysis tool |
| `tool.version` | Version of the analysis tool |
| `scope` | Which evidence objects were processed |
| `status` | `completed`, `failed`, or `partial` |
| `result_manifest_sha256` | SHA-256 of the `result_manifest.json` for this job |

The `result_manifest_sha256` binds the provenance event to the specific output produced,
preventing substitution of results after the event was recorded.

---

## Audit Events

Audit events record read and write access to the container via the access service.
They are written to `extensions/audit/audit_events.jsonl`.

Each audit event records:

| Field | Meaning |
|---|---|
| `event_id` | Stable UUID |
| `timestamp` | Time of the access |
| `actor` | Authenticated tool or user identity |
| `operation` | What was accessed or written |
| `target` | Which container, object, or path |
| `outcome` | `allowed` or `denied` |

Audit events are written by the access service. If the access service is not used, audit events
may be absent. Their absence must be noted in the container's known limitations.

---

## Denied Access Log

When the access service rejects a write request (e.g., attempt to overwrite the evidence layer,
attempt to modify an existing result manifest), it records a denied access event in
`extensions/access/denied_access_events.jsonl`.

This log ensures that attempted violations of the append-only and evidence-immutability
contracts are visible in the audit trail.

---

## Job Records

The job record is the structural output of a completed analysis job. It consists of:

1. `analysis/jobs/{job_id}/job_manifest.json` — defines the job: scope, tool identity,
   parameters.
2. `analysis/jobs/{job_id}/result_manifest.json` — finalizes the job: output artefacts
   and their hashes.
3. A corresponding entry in `provenance/provenance_events.jsonl`.

The job manifest is written before the job runs. The result manifest is written as the
finalization point of the job. Neither may be overwritten after creation.

---

## Tool Identity

Tool identity is the mechanism by which OFFF records which software performed an action.
It is recorded in three places:

1. **Tool registry** (`config/tool-registry.json`) — lists approved tools, their capabilities,
   and allowed operation types.
2. **JobManifest / ProvisioningEvent** — embeds tool name and version.
3. **Access service tokens** — in JWT mode, cryptographically binds requests to a specific
   tool identity.

Tool identity in OFFF is a technical record. It does not constitute legal authentication of
the tool operator or certify that the named tool was unmodified.

---

## Result Manifests

A result manifest (`result_manifest.json`) is the integrity anchor for a completed job.
It records:

- The job identifier.
- All output artefacts with their SHA-256 hashes.
- The tool identity and version that produced the output.
- A timestamp of completion.

The result manifest is written as the last file in the job directory. Once written, it
must not be overwritten. The access service enforces this at the API level.

The chain of custody is complete for an analysis result only when:
- The result manifest exists.
- The artefact hashes match the actual files.
- The corresponding provenance event exists and references the result manifest hash.

---

## Completeness of the Custody Record

A custody record is considered complete for a given container when:

1. An acquisition provenance event is present with a valid source hash.
2. Every completed job has a provenance event with a result manifest reference.
3. Every provenance event has a valid `event_id`, `timestamp`, `actor`, and `tool`.
4. No evidence layer mutations are detected after finalization.

The `offf-verify` tool checks these conditions under the OFFF Forensic Baseline profile.

---

## Gaps in the Custody Record

OFFF explicitly records when the custody record is incomplete:

- **Skipped events** — a tool that skipped processing an object must record why.
- **Error events** — a tool that failed to process an object must record the failure.
- **Absent audit trail** — when no access service is used, the absence of an audit trail
  must be noted in known limitations.

A gap in the custody record does not automatically invalidate the container, but it must
be visible and documented. A forensic analyst or legal reviewer can then assess the
significance of the gap in context.

---

## What the Custody Record Does Not Prove

The custody record established by OFFF does not:

- Prove that the person recorded as `actor` had legal authority to perform the acquisition.
- Prove that the acquisition environment was free from contamination.
- Prove that the tools used were free from vulnerabilities or errors.
- Prevent falsification of the custody record if an adversary had full write access to the
  container before it was read-only protected.

These are process and environmental controls that exist outside OFFF. OFFF provides the
technical substrate for a custody record; it does not enforce or certify the process.

---

## Related Documents

- `docs/chain-of-evidence.md` — technical data integrity chain
- `docs/evidence-root-model.md` — acquisition modes
- `docs/scope-and-exclusion-model.md` — how exclusions and scope limits are recorded
- `docs/legal-neutrality.md` — what OFFF asserts and does not assert
- `docs/forensic-limitations.md` — inherent limitations of the custody model

*Last updated: 2026-05-29*
