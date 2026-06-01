# offf-annotate

## Purpose
Provides annotation workflows for OFFF containers, including append operations on analysis/provenance layers.

## Usage
```bash
cargo run -p offf-annotate -- --help
```

## Typical Actions
- Add annotations.
- Update annotations.
- Query annotations (depending on available subcommands).

## Example
```bash
cargo run -p offf-annotate -- annotate --case tests/samples/4orensics.case2.offf --help
```

## Guidelines
- Keep the evidence layer immutable.
- Write mutations only to allowed append layers.
- Emit provenance records for auditability.
