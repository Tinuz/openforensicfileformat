# OFFF Schema Catalog

## Purpose
This directory contains JSON Schema definitions for OFFF artifacts, including manifests, index rows, provenance events, and job-related objects.

## Example Files
- `offf-manifest-0.1.0.schema.json`
- `offf-acquisition-0.1.0.schema.json`
- `offf-file-index-row-0.1.0.schema.json`
- `offf-provenance-event-0.1.0.schema.json`
- `offf-schema-catalog-0.1.0.json`

## How to Use
1. Select the schema that matches object type and version.
2. Validate producer output during tests.
3. Fail consumers on schema violations with explicit errors.

## Maintenance Rules
- Add a new version suffix for breaking changes.
- Keep older schemas read-only for reproducibility.
- Update schema catalog files whenever new schemas are added.

## Quick Validation Example
```bash
python - << 'PY'
import json
from pathlib import Path
for p in sorted(Path('docs/schema').glob('*.json')):
	json.loads(p.read_text(encoding='utf-8'))
	print('OK', p)
PY
```
