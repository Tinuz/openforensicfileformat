OFFF Smoke Tests
================

Purpose
-------

This directory contains end-to-end smoke tests for environment-dependent OFFF flows.
These scripts are intended as fast regression checks after storage, worker, or E01 pipeline changes.

Available Scripts
-----------------

- `../e2e/run_cli_e2e.py`
	- Runs a local CLI chain without external infrastructure:
	- `offf-convert` -> `offf-verify` -> `offf-keyword-worker` -> `offf-yara-worker`.
	- Validates job-scoped analysis outputs and `result_manifest.json`.
	- Writes a machine-readable report to `tests/e2e/cli-e2e-report.json`.

- `phase5_minio_smoke.py`
	- Builds a small OFFF test container.
	- Uploads it to MinIO (`s3://...`).
	- Runs `offf-verify` against the remote container.
	- Runs `offf-keyword-worker` and `offf-yara-worker` on the remote container.
	- Validates that analysis artifacts were written.
	- Tests concurrent provenance appends with multiple worker processes.

- `phase7_e01_smoke.py`
	- Generates a small local raw sample.
	- Creates a `.E01` sample with `ewfacquire` (dockerized).
	- Converts to OFFF using `offf-convert --input-type e01`.
	- Validates required `acquisition.json` fields.
	- Verifies the output container using `offf-verify`.

Requirements
------------

- Docker Desktop running.
- MinIO reachable at `http://localhost:9000` (for Phase 5).
- Rust toolchain and working `cargo` build.

Run
---

From repository root:

```bash
python tests/smoke/phase5_minio_smoke.py
python tests/smoke/phase7_e01_smoke.py
python tests/e2e/run_cli_e2e.py
```

Troubleshooting
---------------

- MinIO lookup failure:
	- Check `OFFF_S3_ENDPOINT`, access key, and secret key.
- E01 export failure:
	- Check Docker availability and that `offf/ewf-tools:latest` can be built.
- Worker write/provenance failure:
	- Check bucket permissions and object key prefix.
