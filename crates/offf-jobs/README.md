# offf-jobs

## Purpose
Orchestrates jobs for workers such as keyword and YARA scans.

## Features
- Job dispatch.
- Runtime state tracking.
- Replay metadata.
- Assignment audit trail.
- Worker health logging.

## Usage
```bash
cargo run -p offf-jobs -- --help
```

## Example Commands
```bash
cargo run -p offf-jobs -- create-keyword --case tests/samples/4orensics.case2.offf --keywords password,secret
cargo run -p offf-jobs -- run --case tests/samples/4orensics.case2.offf --job tests/samples/keyword_job.json
```

## Output
Runtime artifacts are written under `jobs/runtime/`, including:
- `*.state.json`
- `assignment_audit.jsonl`
- `worker_health.jsonl`
