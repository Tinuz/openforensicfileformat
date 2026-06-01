# offf-index

## Purpose
Builds indexes on OFFF data, including partition, filesystem, and object-graph related lookup artifacts.

## Output
Index artifacts are written under `indexes/` in the container, including parquet index tables.

## Example Commands
```bash
cargo run -p offf-index -- --help

# Build object graph from filesystem indexes
cargo run -p offf-index -- objects case.offf --from-filesystem

# Full pipeline: partitions -> filesystem -> objects
cargo run -p offf-index -- full case.offf --hash-content deferred
```

## Guidelines
- Keep index output deterministic where possible.
- Emit provenance events for index build actions.
- Validate outputs with conformance and verify flows.
