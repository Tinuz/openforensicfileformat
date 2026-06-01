# Tests Overview

## Purpose
The `tests` directory contains multiple test levels:
- conformance checks
- integration tests
- sample data and job manifests
- smoke tests for environment-dependent scenarios

## Structure
- `conformance/`: profile-based PASS/FAIL evaluation
- `integration/`: Rust integration tests
- `samples/`: test data, jobs, rules, and containers
- `smoke/`: end-to-end scripts for MinIO and E01 paths

## Run Commands
- Conformance:
  - `python tests/conformance/run_conformance.py`
- Smoke:
  - `python tests/smoke/phase5_minio_smoke.py`
  - `python tests/smoke/phase7_e01_smoke.py`
- Integration:
  - `cargo test -p offf-integration-tests -- --nocapture`

## Recommended Sequence
1. Run integration tests after crate-level changes.
2. Run conformance tests when schema or verification behavior changes.
3. Run smoke tests when storage/auth/E01 behavior changes.
