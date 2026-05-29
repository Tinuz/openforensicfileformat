# OFFF Scope and Exclusion Model

## Purpose

OFFF provides a technical mechanism for controlling and recording which objects are included
or excluded from processing. This document explains how scopes, labels, sets, and exclusion
events work, and how their use is recorded for auditability.

All scope and exclusion constructs in OFFF are **technical metadata**. They do not constitute
legal decisions. See `docs/legal-neutrality.md`.

---

## Core Concept: Scope

A **scope** in OFFF defines a set of objects that a job or operation is permitted to process.
Scopes are defined by the operator or tool configuration and are recorded in the job manifest.

A scope may be:

- **Inclusive** — process only objects matching the scope definition.
- **Exclusive** — process all objects except those matching the exclusion definition.
- **Layered** — an inclusive scope further restricted by an exclusion list.

Scopes are recorded in `JobManifest.scope` and optionally in `extensions/scopes/scopes.jsonl`.

---

## Labels

Labels are arbitrary tags attached to objects in an OFFF container. They are stored as
`LabelEvent` records in `extensions/labels/label_events.jsonl`.

| Field | Meaning |
|---|---|
| `object_id` | The object being labelled |
| `label` | The label string (e.g., `relevant`, `excluded`, `sensitive`) |
| `actor` | Who or what applied the label |
| `timestamp` | When the label was applied |
| `reason` | Optional free-text reason |

Labels are:
- **Purely descriptive** — they do not change the object's bytes or hash.
- **Append-only** — once recorded, a label event cannot be overwritten.
- **Non-exclusive** — multiple labels may be applied to the same object.

A label applied by an analysis tool (e.g., `relevant`) reflects that tool's output within
its defined scope. It is not a legal determination.

---

## Release Sets

A **release set** groups objects for a specific disclosure or sharing purpose. It is stored
in `extensions/sets/` as `SetRecord` JSONL files.

A release set records:
- Which objects are included.
- The purpose of the set (e.g., `disclosure_to_defence`, `internal_review`).
- Who created the set.
- When the set was created.

Sets are technical groupings. The decision of what to include in a release set is the
responsibility of the operator, guided by applicable procedural rules. OFFF records the
set and its contents; it does not enforce release decisions.

---

## Exclusion Sets

An **exclusion set** records objects that have been explicitly excluded from a processing
scope. It functions as the inverse of a release set.

Exclusion sets are used when:
- An operator has determined that certain objects fall outside the processing scope.
- A policy prevents certain objects from being processed by a specific tool.
- Objects are excluded for performance reasons (e.g., too large, unsupported format).

Exclusions are stored in `extensions/sets/` and referenced in job manifests. An object
that is in an exclusion set must not appear in the job results, but its exclusion must be
explicitly recorded.

---

## Skipped Events

A **skipped event** records that a tool encountered an object but chose not to process it.
This is distinct from an exclusion: a skip is a tool-level decision during job execution,
not an operator-level scope definition.

Skipped events are written by the analysis tool to
`analysis/jobs/{job_id}/skipped_events.jsonl`.

Each skipped event records:
- The object identifier.
- The reason for skipping (e.g., `unsupported_format`, `encrypted`, `size_exceeds_limit`).
- The tool that made the skip decision.

Skipped events must not be silently discarded. Every object that was within the job scope
but not processed must be accounted for via a result, an error event, or a skipped event.

---

## Error Events

An **error event** records that a tool attempted to process an object but failed. Error
events are written to `analysis/jobs/{job_id}/errors.jsonl`.

Each error event records:
- The object identifier.
- The error type and message.
- Whether the job continued after the error (`partial` completion).

An error event is not an exclusion. It records a processing failure, not an operator
decision to exclude.

---

## Denied Access Events

A **denied access event** records that the access service rejected a write request. These
are written by the access service to `extensions/access/denied_access_events.jsonl`.

Denied access events are produced when:
- A tool attempts to write to the evidence layer (immutability violation).
- A tool attempts to overwrite an existing result manifest (append-only violation).
- An unauthenticated or unauthorized write request is received.

Denied access events are part of the audit trail. They ensure that attempted policy
violations are visible even when no actual modification occurred.

---

## Auditability Requirements

For a container to be auditable with respect to scope and exclusion:

1. Every object within the acquisition scope that was not processed must appear in either:
   - A job result (output artefact).
   - A skipped event.
   - An error event.
   - An exclusion set that covers this job.

2. Every exclusion set referenced by a job must be present and parseable.

3. All label events, scope records, and set records must be append-only. Once recorded,
   they must not be deleted or modified.

4. The access audit trail must record all write operations to extension and analysis layers.

These requirements are checked partially by `offf-verify` under the Extension Conformant
profile. Full scope-completeness verification requires tool-specific configuration.

---

## Summary Table

| Construct | Written by | Location | Append-only | Legal meaning |
|---|---|---|---|---|
| `ScopeRecord` | Operator / orchestrator | `extensions/scopes/scopes.jsonl` | Yes | None |
| `LabelEvent` | Analysis tool or operator | `extensions/labels/label_events.jsonl` | Yes | None |
| `SetRecord` (release) | Operator | `extensions/sets/` | Yes | None |
| `SetRecord` (exclusion) | Operator | `extensions/sets/` | Yes | None |
| `skipped_event` | Analysis tool | `analysis/jobs/{job_id}/skipped_events.jsonl` | Yes | None |
| `error_event` | Analysis tool | `analysis/jobs/{job_id}/errors.jsonl` | Yes | None |
| `denied_access_event` | Access service | `extensions/access/denied_access_events.jsonl` | Yes | None |

---

## Related Documents

- `docs/legal-neutrality.md` — technical constructs are not legal decisions
- `docs/chain-of-custody.md` — how scope records contribute to the custody audit trail
- `docs/forensic-limitations.md` — limitations of scope completeness verification
- `docs/conformance-profiles.md` — OFFF Extension Conformant profile requirements

*Last updated: 2026-05-29*
