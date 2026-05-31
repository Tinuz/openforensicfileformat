# Annotation Extension

`offf-annotate` appends human annotation events to the OFFF analysis layer.

## Scope

- Emit append-only annotation events.
- Preserve auditability and event ordering.
- Keep annotations separate from the evidence layer.

## Contract

- Events are stored as extension JSONL records.
- Annotation writes must remain append-only.
- Readers should tolerate unknown extension data where the core contract allows it.

## Current Limitations

- Minimal feature set.
- No dedicated integration test suite yet.

See also: [definition of done](definition-of-done.md) and [conformance profiles](conformance-profiles.md).
