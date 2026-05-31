# Analysis Worker Surfaces

This document covers the stable reference surface for worker-style analysis outputs, including keyword and YARA workers.

## Scope

- Read OFFF evidence chunks.
- Emit append-only analysis results under `analysis/jobs/{job_id}/...`.
- Write `result_manifest.json` last so downstream consumers can trust the artifact set.

## Shared Constraints

- Workers must not mutate the evidence layer.
- Artifact hashes must be recorded in the job result manifest.
- Job-scoped outputs must remain isolated across concurrent runs.

## Current Limitations

- Cross-chunk boundary matching and job output isolation still have explicit hardening notes in the backlog.
- Negative datasets are not exhaustive.

See also: [conformance profiles](conformance-profiles.md), [evidence of done](evidence-of-done.md), and [test traceability](test-traceability.md).
