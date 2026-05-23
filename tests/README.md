# Tests Overview

## Doel
De `tests` map bevat meerdere testniveaus:
- conformance checks
- integratietests
- sample data en jobbestanden
- smoke tests voor omgevingsafhankelijke scenario's

## Structuur
- `conformance/`: profielgebaseerde PASS/FAIL evaluatie
- `integration/`: Rust integratietests
- `samples/`: testdata, jobs, rules en containers
- `smoke/`: end-to-end scripts voor MinIO en E01

## Uitvoering
- Conformance:
  - `python tests/conformance/run_conformance.py`
- Smoke:
  - `python tests/smoke/phase5_minio_smoke.py`
  - `python tests/smoke/phase7_e01_smoke.py`
- Integratie:
  - `cargo test -p offf-integration-tests -- --nocapture`
