# OFFF Evidence Root Model

## Purpose

OFFF is **evidence-object-centric**, not image-centric. A disk image is one possible
evidence root, but not the only one. This document defines the evidence root model: what
the root evidence is for each acquisition mode, and what that implies for indexing,
lineage, and verification.

---

## Core Principle

Every OFFF container has exactly one **evidence root**: the primary source object from which
all data in the container derives. The evidence root is recorded in `acquisition.json` and
anchors the technical chain of evidence.

The evidence root is not necessarily a disk image. Depending on the acquisition mode, it may be:
- A physical storage image (block_image).
- A collection of files (file_collection).
- A logical extraction from a device (logical_extraction).
- An export from an API or cloud service (api_export).
- A combination of the above (mixed).

---

## Acquisition Modes

### block_image

| Property | Value |
|---|---|
| Root evidence | Raw disk or storage medium image (sector-level) |
| Root identity | Source hash (SHA-256 of the complete image) in `acquisition.json` |
| Required index | `physical_to_chunk.parquet` (sector → chunk mapping) |
| Optional index | `file_index.parquet` (via NTFS/ext parser) |
| Merkle requirement | **Required** — Merkle root covers all image chunks |
| Context available | Deleted files, unallocated space, partition structure, file system metadata |
| Context missing | Nothing below the sector level (firmware, RAM) |
| Chain strength | Strongest — sector-level SHA-256 binding with Merkle proof |
| Verifier checks | BL-06 (chunk hashes), BL-07 (Merkle root), BL-03 (acquisition_mode) |

**Notes:**
- The source SHA-256 must be verified against the image before chunking begins.
- Sector size must be recorded in `acquisition.parameters.sector_size`.
- File system indexing is optional at baseline; required for `OFFF Indexer Conformant`.
- Deleted file recovery is possible because unallocated blocks are preserved in the chunk store.

---

### file_collection

| Property | Value |
|---|---|
| Root evidence | Set of files collected from a source (device, directory, network share) |
| Root identity | Collection source description + per-file SHA-256 hashes in object index |
| Required index | `file_index.parquet` or `object_index.parquet` |
| Merkle requirement | Not required at acquisition level; per-file hashes replace Merkle |
| Context available | File contents, file metadata (as provided by collection tool) |
| Context missing | Deleted files, unallocated space, file system internals |
| Chain strength | Medium — limited to collected files; completeness unverifiable |
| Verifier checks | BL-08 (object index), BL-06 (per-file hashes), BL-03 |

**Notes:**
- The collection scope (which files were included/excluded) must be recorded in the job manifest
  or exclusion sets.
- Objects that were within scope but not collected must appear as skipped events.
- File metadata fidelity depends on the collection tool.

---

### logical_extraction

| Property | Value |
|---|---|
| Root evidence | Logical data extracted from a device (mobile, IoT, embedded) |
| Root identity | Device identifier + extraction tool identity + per-item hashes |
| Required index | `object_index.parquet` |
| Merkle requirement | Not required |
| Context available | Apps, databases, media files, messages — whatever the extraction tool accessed |
| Context missing | Physical sectors, raw flash memory, deleted items (unless recovered by tool) |
| Chain strength | Moderate — depends on extraction tool completeness and accuracy |
| Verifier checks | BL-08, BL-06 (per-item hashes), BL-03 |

**Notes:**
- Device identity (model, serial, IMEI if applicable) should be recorded in `acquisition.json`.
- The extraction tool version must be recorded; different versions may produce different output.
- Data that the device OS restricted from extraction must appear as skipped or error events.

---

### api_export

| Property | Value |
|---|---|
| Root evidence | Data exported from an API or cloud service |
| Root identity | Service identifier + export scope + per-item hashes |
| Required index | `object_index.parquet` |
| Merkle requirement | Not required |
| Context available | Items returned by the API within the export scope |
| Context missing | Items not returned by the API (deleted, filtered, access-restricted) |
| Chain strength | Weak to moderate — entirely dependent on API completeness and service trustworthiness |
| Verifier checks | BL-08, BL-06, BL-03 |

**Notes:**
- The API service, endpoint, and export parameters must be recorded in `acquisition.json`.
- API exports may be incomplete; the service may have filtered, deleted, or withheld data
  without notification. This limitation must be explicitly noted.
- API-exported data has no physical layer binding — the chain begins at the logical item level.

---

### mixed

| Property | Value |
|---|---|
| Root evidence | Combination of two or more acquisition modes |
| Root identity | One acquisition record per sub-acquisition; combined by object lineage |
| Required index | Object index for each sub-acquisition |
| Merkle requirement | Required for any block_image sub-acquisition |
| Context available | Union of contexts from all sub-acquisitions |
| Context missing | Any gaps in individual sub-acquisitions apply to the mixed container |
| Chain strength | Varies by sub-acquisition; lowest-strength sub-acquisition governs overall |
| Verifier checks | All checks applicable to each sub-acquisition mode |

**Notes:**
- Object lineage must record which sub-acquisition each object derives from.
- The acquisition record in `acquisition.json` must identify all sub-acquisitions and their
  respective sources.
- When combining block_image and file_collection, the file_collection must not be presented
  as sector-level evidence.

---

## Evidence Root and Object Derivation

Regardless of acquisition mode, every object in an OFFF container traces back to the
evidence root through the object lineage graph:

```
Evidence root (acquisition.json)
    └── Evidence objects (object_index)
            └── Derived objects (object_edges, derivations)
                    └── Further derived objects ...
                            └── Analysis results (analysis/jobs/)
```

A derived object is any object produced by processing another object (e.g., a file extracted
from a ZIP archive, an image extracted from a document, text produced by OCR). Each derivation
must be recorded with:
- Source object identifier.
- Derived object identifier.
- Derivation method.
- Tool identity.

This allows any analysis result to be traced back to its root evidence object.

---

## Choosing the Right Acquisition Mode

| Scenario | Recommended mode | Notes |
|---|---|---|
| Physical disk image (suspect's device) | `block_image` | Highest chain strength |
| Forensic image of SSD or USB | `block_image` | Full sector preservation |
| Files collected from live system | `file_collection` | Document scope and exclusions |
| Mobile device extraction | `logical_extraction` | Record device identity |
| Cloud service / email archive export | `api_export` | Note API completeness limitations |
| Multi-source investigation | `mixed` | Separate acquisition records per source |

---

## Related Documents

- `docs/chain-of-evidence.md` — how the evidence root anchors the chain of evidence
- `docs/forensic-baseline-profile.md` — required elements per acquisition mode
- `docs/forensic-limitations.md` — inherent limitations per acquisition mode
- `docs/conformance-profiles.md` — OFFF Acquisition Conformant profile

*Last updated: 2026-05-29*
