# OFFF Chain of Evidence

## Purpose

This document describes how OFFF establishes a verifiable technical chain of evidence:
the unbroken sequence of recorded facts that connects a forensic finding back to the original
evidence source.

A chain of evidence in OFFF is a technical record. Its legal admissibility and weight are
determined by the jurisdiction, the forensic process applied, and the expertise of the
analyst — not by OFFF itself. See `docs/legal-neutrality.md`.

---

## What Chain of Evidence Means in OFFF

In a traditional forensic workflow, chain of evidence documents:

1. What the original evidence is (the source).
2. That no unrecorded change occurred to the source.
3. How each derived artefact relates to the source.

OFFF models this as a three-layer technical chain:

```
Evidence root (acquisition)
    └── Evidence layer (chunks, hashes, Merkle root, manifest)
            └── Derived objects (object index, object lineage, derivations)
                    └── Analysis results (jobs, result manifests, provenance events)
```

Every node in this chain is:
- Content-addressed (SHA-256 hash).
- Recorded in a machine-readable artefact.
- Verifiable by `offf-verify` without modification to the container.

---

## Evidence Root

The evidence root is the origin of all data in an OFFF container. It is established during
acquisition and recorded in `acquisition.json`.

| Field | Meaning |
|---|---|
| `source.sha256` | SHA-256 hash of the complete source object (image, collection, etc.) |
| `acquisition_mode` | How the evidence was acquired (block_image, file_collection, etc.) |
| `source.sector_size` | Physical sector size, for block images |
| `tool.name` / `tool.version` | Identity of the acquisition tool |
| `actor` | Identity of the person or system performing the acquisition |
| `timestamp` | When the acquisition was performed |

The source SHA-256 in `acquisition.json` must match the SHA-256 recorded in the first
provenance event. This cross-reference is verified by `offf-verify` under the
OFFF Acquisition Conformant profile.

---

## Evidence Layer Integrity

The evidence layer consists of:

- `chunks/sha256/` — raw evidence bytes, split into content-addressed chunks.
- `hashes/leaves.parquet` — per-chunk SHA-256 hashes.
- `hashes/merkle_tree.bin` — binary Merkle tree over all chunk hashes.
- `maps/physical_to_chunk.parquet` — mapping from byte offsets to chunks.
- `manifest.json` — container root metadata including the Merkle root.

**Immutability:** The evidence layer must not be modified after `manifest.json` is written.
`manifest.json` is written last and its presence signals finalization.

**Cryptographic binding:**
- Each chunk is bound to its hash in `leaves.parquet`.
- Each leaf is bound to the Merkle root in `manifest.json`.
- The Merkle root binds the entire evidence layer to a single verifiable value.

To verify the chain: the verifier re-computes every chunk hash, re-builds the Merkle tree, and
compares the root with the value stored in `manifest.json`. Any discrepancy breaks the chain
at the point of the mismatched hash.

---

## Object Index and Object Lineage

### Object Index

When evidence contains discrete objects (files, records, logical items), they are recorded in:

- `indexes/file_index.parquet` — for filesystem objects (file_collection, block_image).
- `indexes/object_index.parquet` — for generic objects (all modes, v0.2).

Each row in the object index contains at minimum:
- A stable, unique object identifier.
- A SHA-256 hash of the object content.
- A reference to the evidence source (chunk offset or source container).

The object index creates the first link between raw evidence bytes and named objects.

### Object Lineage

Derived objects (files extracted from archives, images extracted from documents, structures
parsed from a binary) are recorded in:

- `indexes/object_edges.parquet` — parent/child relationships.
- `indexes/derivations.parquet` — derivation method and tool.

Each derivation record contains:
- The source object identifier.
- The derived object identifier.
- The tool identity that performed the derivation.
- The method used (e.g., `zip_extraction`, `email_attachment`, `ocr_text`).

The lineage graph allows tracing any derived object back to its root evidence object through
a chain of verifiable SHA-256 hashes.

---

## block_image versus file_collection

| Property | block_image | file_collection |
|---|---|---|
| Evidence root | Disk image (sector-level) | Set of files |
| SHA-256 binding | Source image hash in `acquisition.json` | Per-file hashes in object index |
| Merkle root | Covers all chunks of the image | Not required at acquisition level |
| Deleted file recovery | Possible (raw bytes available) | Not possible (logical layer only) |
| Unallocated space | Present in chunk store | Absent |
| File system metadata | Preserved via NTFS/ext indexer | Only what the collection tool extracted |
| Chain strength | Strongest (sector-level binding) | Depends on collection completeness |

For `file_collection` acquisitions, the chain of evidence is limited to the files included
in the collection. Files that were excluded (by scope, filter, or tool limitation) are not
in the chain. This must be documented in `acquisition.json` and any exclusion events must
be recorded as skipped events.

---

## Source References in Analysis Results

Each analysis job output must be traceable to its input evidence. This is recorded via:

1. **`JobManifest.scope`** — defines which evidence objects the job was applied to.
2. **`result_manifest.json`** — lists all output artefacts with their SHA-256 hashes.
3. **`provenance/provenance_events.jsonl`** — records the job execution event with tool
   identity, input scope, and output reference.

An analysis result is only part of the chain if:
- The job that produced it has a provenance event.
- The result manifest hashes match the actual output files.
- The job scope references identifiable evidence objects.

---

## Verifying the Chain

```bash
# Verify full chain (evidence + lineage + analysis)
cargo run -p offf-verify -- <container.offf>

# Verify with forensic baseline profile
cargo run -p offf-verify -- <container.offf> --profile forensic-baseline

# Trace lineage of a specific object
cargo run -p offf-verify -- <container.offf> --object <object_id> --lineage
```

A verification report produced by `offf-verify` shows exactly which links in the chain
passed and which failed. A failed link means the chain is broken at that point.

---

## What the Chain Does Not Prove

The technical chain of evidence established by OFFF does not, by itself:

- Prove that the acquisition was performed with proper legal authority.
- Prove that the evidence was obtained lawfully.
- Prove that the analysis findings are legally relevant or admissible.
- Prevent an adversary who had physical access from replacing the entire container.

These limitations are inherent to any hash-based integrity system. OFFF records and verifies
what happened inside the format; it cannot independently validate what happened outside it.

See `docs/forensic-limitations.md` for a full treatment.

---

## Related Documents

- `docs/chain-of-custody.md` — process chain: provenance events, tool identity, job records
- `docs/evidence-root-model.md` — acquisition modes and root evidence per mode
- `docs/forensic-baseline-profile.md` — minimum required elements for baseline conformance
- `docs/forensic-limitations.md` — limitations of the technical chain model
- `docs/legal-neutrality.md` — what OFFF asserts and does not assert

*Last updated: 2026-05-29*
