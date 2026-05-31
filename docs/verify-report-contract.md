# Verify Report Contract

OFFF verifier runs now emit a machine-readable report to the stable case-relative path `reports/verify/verify_report.json` for local containers by default. This path is intended to be scanned by downstream dashboard collectors and preserved during case upload or sync.

## Stable Path

- Default JSON report path: `reports/verify/verify_report.json`
- Human-readable Markdown report: only when `--report-md <path>` is explicitly supplied
- Explicit `--report` or `--report-json` still overrides the default JSON path

Verification is considered complete when `reports/verify/verify_report.json` exists and contains a parseable payload with `schema_version`, `completed_at`, `overall_status`, and at least one entry in `checks`.

## Schema Versioning

- Current schema version: `1.0.0`
- Additive optional fields may be introduced in minor versions
- Breaking changes require a major version increment
- The verifier keeps compatibility-oriented fields (`container`, `profile`, `valid`, `summary`, `legacy_checks`) alongside the stable contract fields so existing consumers are not broken unnecessarily

## Required Top-Level Fields

- `schema_version`
- `verifier_name`
- `verifier_version`
- `case_id`
- `started_at`
- `completed_at`
- `overall_status`
- `checks`

## Required Check Fields

- `id`
- `name`
- `status`
- `severity`
- `message`

`evidence_refs` is optional and may be empty.

## Example Payload

```json
{
  "schema_version": "1.0.0",
  "verifier_name": "offf-verify",
  "verifier_version": "0.1.0",
  "case_id": "urn:offf:case:test-case-001",
  "started_at": "2026-05-31T10:00:00Z",
  "completed_at": "2026-05-31T10:00:01Z",
  "overall_status": "pass",
  "checks": [
    {
      "id": "check-000-manifest-present-and-valid",
      "name": "Manifest present and valid",
      "status": "pass",
      "severity": "low",
      "message": "Manifest present and valid",
      "evidence_refs": []
    }
  ]
}
```

## Schema File

The JSON Schema for this contract is published at `docs/schema/offf-verify-report-1.0.0.schema.json` and referenced from `docs/schema/offf-schema-catalog-0.1.0.json`.