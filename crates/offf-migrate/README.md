# offf-migrate

CLI tool that migrates OFFF containers between format versions while preserving all evidence hash invariants.

## Usage

```
# Preview changes (no files written)
offf-migrate --dry-run /path/to/container

# Migrate to a new output directory
offf-migrate /path/to/container --out /path/to/migrated

# Migrate in-place (rewrites manifest.json only)
offf-migrate /path/to/container --in-place

# Write migration report to a file
offf-migrate /path/to/container --out /path/to/migrated --report migration-report.json
```

## Evidence hash invariants

The following values are verified before migration and after writing output:

| Field | Location | Guarantee |
|-------|----------|-----------|
| `container_id` | `manifest.json` | Copied verbatim; never altered |
| `hashes.source_sha256` | `manifest.json` | Never recomputed; must match |
| `hashes.merkle_root_sha256` | `manifest.json` | Never recomputed; must match |
| Chunk files | `chunks/` | Content-addressed; name = SHA-256 |
| Physical map | `maps/physical_to_chunk.parquet` | Must be present |

If any invariant check fails the tool exits with code 1 and reports the failure in the JSON migration report without writing any output.

## Supported migrations

| From | To | Notes |
|------|----|-------|
| `0.1.0` | `0.2.0` | Bumps `offf_version`; adds `extensions["offf:migration"]` |

## Migration report

The tool writes a JSON report to stdout (or `--report <path>`):

```json
{
  "source_container": "/evidence/case-001",
  "container_id": "urn:offf:case:case-001",
  "source_version": "0.1.0",
  "target_version": "0.2.0",
  "dry_run": false,
  "migrated_at": "2026-01-01T00:00:00Z",
  "migrated_by_tool": { "name": "offf-migrate", "version": "0.1.0" },
  "invariant_checks": {
    "container_id_preserved": true,
    "source_sha256_preserved": true,
    "merkle_root_preserved": true,
    "chunk_files_unmodified": true,
    "physical_map_unmodified": true
  },
  "changes": [
    "manifest.json: offf_version '0.1.0' → '0.2.0'",
    "manifest.json: extensions[\"offf:migration\"] added"
  ],
  "result": "success"
}
```

## Recommended workflow

1. `offf-verify /path/to/container` — verify integrity first
2. `offf-migrate --dry-run /path/to/container` — preview changes
3. `offf-migrate /path/to/container --out /path/to/migrated` — apply migration
4. `offf-verify /path/to/migrated` — verify the result
