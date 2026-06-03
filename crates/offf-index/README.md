# offf-index

## Purpose
Builds indexes on OFFF data, including partition, filesystem, object-graph related lookup artifacts, and case-level cross-root relation inspection.

## Output
Index artifacts are written under `indexes/` in the container, including parquet index tables.

## Example Commands
```bash
cargo run -p offf-index -- --help

# Build object graph from filesystem indexes
cargo run -p offf-index -- objects case.offf --from-filesystem

# Full pipeline: partitions -> filesystem -> objects
cargo run -p offf-index -- full case.offf --hash-content deferred

# Inspect case-level cross-root relations (table output)
cargo run -p offf-index -- case-cross-root case.offf

# Rebuild case indexes first and print relations as JSON
cargo run -p offf-index -- case-cross-root case.offf --rebuild --format json
```

## Case Cross-Root Inspection
Use `case-cross-root` when working with a multi-root case and you need to inspect links across evidence roots.

- Supports `--rebuild` to refresh global case indexes before reading relations.
- Supports `--format table|json` output modes.
- Uses the typed core relation model exported by `offf-core`.

## Guidelines
- Keep index output deterministic where possible.
- Emit provenance events for index build actions.
- Validate outputs with conformance and verify flows.
