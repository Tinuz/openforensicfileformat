# offf-jobs

## Doel
Orkestreert jobs voor workers zoals keyword- en yara-scans.

## Functionaliteit
- job dispatch
- runtime state tracking
- replay metadata
- assignment audit
- worker health logging

## Gebruik
```bash
cargo run -p offf-jobs -- --help
```

## Output
Runtime artefacten worden geschreven onder `jobs/runtime/` zoals:
- `*.state.json`
- `assignment_audit.jsonl`
- `worker_health.jsonl`
