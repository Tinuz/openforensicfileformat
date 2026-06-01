# Config Directory

## Purpose
This directory contains configuration files used by OFFF services and workers to control runtime behavior without code changes.

## Files
- `tool-registry.example.json`: Example approved tool registry with identity, version, hashes, capabilities, and allowed write layers.

## Quick Start
1. Copy `tool-registry.example.json` to your deployment-specific file, for example `tool-registry.json`.
2. For each tool, provide at least:
   - `name`
   - `vendor`
   - `version`
   - executable hash or image hash
   - allowed OFFF capabilities/profiles
3. Point services to this file using the matching CLI flag or environment variable.

## Example
```bash
cp config/tool-registry.example.json config/tool-registry.json
```

## Guidelines
- Never commit secrets.
- Keep governance changes traceable in provenance/change logs.
- Keep tool hashes aligned with the exact deployed binary or container image.
