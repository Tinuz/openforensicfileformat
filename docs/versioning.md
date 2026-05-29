# OFFF Versioning Policy

**Version:** 0.1.0
**Date:** 2026-05-29

---

## Overview

OFFF uses **semantic versioning** (MAJOR.MINOR.PATCH) for all schemas, manifests,
and the format specification itself.

Current version: **v0.1.0** (all schemas), **v0.2.0** (manifest extensions, object index)

---

## Version numbers

| Artifact | Current version |
|---|---|
| Format specification | v0.1.0 |
| `manifest.json` schema | v0.1.0 / v0.2.0 |
| `acquisition.json` schema | v0.1.0 |
| `offf-object-index-row` schema | v0.2.0 |
| `offf-job-manifest` schema | v0.1.0 |
| `offf-provenance-event` schema | v0.1.0 |
| `offf-annotation-event` schema | v0.1.0 |

---

## Backward compatibility rules

### Schema changes

| Change type | Allowed in MINOR? | Allowed in PATCH? | Requires MAJOR? |
|---|---|---|---|
| Add optional field | Yes | No | No |
| Add required field | No | No | Yes (breaking) |
| Remove field | No | No | Yes (breaking) |
| Change field type | No | No | Yes (breaking) |
| Add new enum value | Yes | No | No |
| Remove enum value | No | No | Yes (breaking) |
| Rename field | No | No | Yes (breaking) |

### Forward compatibility

Readers **must** ignore unknown fields gracefully. This ensures that a v0.1
reader can still open a v0.2 container, albeit without understanding the new fields.

```rust
// Example: serde with deny_unknown_fields is NOT used for manifest types.
// Unknown fields are silently dropped.
#[derive(serde::Deserialize)]
pub struct Manifest {
    pub version: String,
    // ... known fields ...
    // unknown fields tolerated by default (no deny_unknown_fields)
}
```

---

## v0.1.0 → v0.2.0 migration

### What changed

`offf-manifest-0.2.0.schema.json` adds:
- `extensions` — optional map for extension metadata
- `object_lineage` — optional object index reference

`offf-object-index-row-0.2.0.schema.json` (new schema):
- `object_id`, `parent_id`, `origin_chunk_offset`, `parser_status` fields

### Migration steps

A v0.1.0 container can be opened by a v0.2.0 reader without modification.
To upgrade a v0.1.0 container to v0.2.0 (add object index):

```bash
offf-index build evidence.offf/ --output-version 0.2.0
```

This writes `indexes/objects/object_index.parquet` without modifying the evidence layer.

### Verifier behavior

`offf-verify` reads the `version` field from `manifest.json`:
- `"0.1.0"` — validates against `offf-manifest-0.1.0.schema.json`
- `"0.2.0"` — validates against `offf-manifest-0.2.0.schema.json`

Unknown version strings cause `offf-verify` to warn and fall back to the latest
known schema.

---

## Deprecation policy

1. A field or behavior is **deprecated** when a minor version is released that
   introduces a replacement.
2. The deprecated feature is documented in `BACKLOG.txt` with a `deprecated-in:`
   note and a `removal-in:` target version.
3. The feature is removed only in a MAJOR version bump.

**Currently deprecated:** None.

---

## Breaking changes

Breaking changes require:
1. A MAJOR version bump in all affected schemas.
2. A migration guide in this document.
3. An `offf-migrate` CLI command (or script) that automates migration.
4. Update to `conformance-report.json` to add a `compatibility` check.

### offf-migrate CLI (gap)

The `offf-migrate` CLI tool is not yet implemented. Until it exists, migration
must be performed manually using the steps described in this document.

Track as: `offf-migrate` tool — `experimental` maturity, not yet started.

---

## Patch releases

Patch releases (x.y.Z) are for:
- Bug fixes in reference implementations.
- Documentation corrections.
- Test additions that do not change behavior.

Patch releases do **not** change schemas.

---

## Revision history

| Version | Date | Change |
|---|---|---|
| 0.1.0 | 2026-05-29 | Initial versioning policy |

---

*Last updated: 2026-05-29*
