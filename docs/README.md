# Documentation

## Purpose
This directory contains formal and operational documentation for the OFFF ecosystem.

## Structure
- `schema/`: JSON Schema definitions for OFFF objects and events.
- Additional governance, maturity, profile, and operational documents used by CI and release readiness tooling.

## Workflow
1. Update schema/doc content with explicit version context.
2. Validate schema files locally and in CI.
3. Link documentation changes to concrete crate/SDK behavior.
4. Add or update tests when behavior changes.
5. Confirm backward compatibility expectations.

## Useful Commands
```bash
python scripts/check_component_metadata.py
python scripts/check_test_traceability.py
python scripts/generate_release_readiness.py
```
