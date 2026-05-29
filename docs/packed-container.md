# OFFF Packed Container Format

## Overview

The OFFF format has two representations:

| Representation | Extension | Role |
|---|---|---|
| Exploded directory | `*.offf/` | **Canonical** — used for verification, analysis, and archival |
| Packed archive | `*.offfpack` | **Transport** — used for transfer, backup, and tooling pipelines |

The **exploded directory is the canonical form**. `offf-verify` and all analysis
workers operate on exploded containers only. `.offfpack` is a packaging layer, not
a replacement for the canonical structure.

---

## Exploded directory structure (canonical)

```
evidence.offf/
  manifest.json           ← finalization point; last written
  acquisition.json
  chunks/sha256/          ← content-addressed chunk storage
  hashes/
    leaves.parquet
    merkle_tree.bin
  maps/
    physical_to_chunk.parquet
    chunk_to_logical.parquet
  indexes/
    objects/
    partitions/
  analysis/
    jobs/{job_id}/
  provenance/
    provenance_events.jsonl
  extensions/
```

---

## Packing and unpacking

### Pack

```
offf-convert pack evidence.offf/ -o evidence.offfpack
```

Writes a tar-like or zip-like archive containing all files from the exploded
directory, preserving relative paths. The archive includes a `OFFFPACK_META.json`
at the root with:
- `source_path` — original exploded directory name
- `packed_at` — ISO 8601 timestamp
- `tool` — packer identity
- `manifest_hash` — SHA-256 of `manifest.json` inside the archive

### Unpack

```
offf-convert unpack evidence.offfpack -o evidence.offf/
```

Extracts the archive to the target directory. Unpacking does **not** re-verify
chunk hashes; run `offf-verify` on the unpacked container afterward.

---

## Metadata equivalence guarantee

A packed and then unpacked container must be byte-equivalent to the original:

```
sha256(original/manifest.json) == sha256(unpacked/manifest.json)
sha256(original/chunks/sha256/<hash>) == sha256(unpacked/chunks/sha256/<hash>)
```

The packer and unpacker must not modify any file contents, permissions, or
relative paths.

---

## Verification workflow

Always unpack before verifying:

```bash
offf-convert unpack evidence.offfpack -o /tmp/evidence.offf/
offf-verify /tmp/evidence.offf/
```

Running `offf-verify` directly on a `.offfpack` file is not supported and will
return an error.

---

## Conformance

`.offfpack` is a transport format; there is no "Packed Container Conformant" profile.
A tool that packs or unpacks must:
1. Preserve byte-identical file contents.
2. Preserve all directory structure.
3. Write `OFFFPACK_META.json` with at minimum `manifest_hash`.
4. Support unpack → exploded directory without data loss.

---

*Last updated: 2026-05-29*
