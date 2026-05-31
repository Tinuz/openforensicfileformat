# Access Service Reference Surface

The OFFF access service exposes read and append-only write paths for containers, analysis outputs, and audit events.

## Scope

- Capability-gated access to verified reads.
- Append-only write paths for supported analysis and extension artifacts.
- Denied writes are auditable and must not mutate the evidence layer.

## Operational Notes

- `OFFF_AUTH_MODE` controls authentication mode selection.
- Local, MinIO, and S3-backed cases should resolve through the same container abstraction.
- Read paths must remain stable even when write paths are restricted.

## Current Limitations

- JWT and mTLS modes are implemented but still need independent review.
- S3 parity is smoke-tested rather than exhaustively verified.

See also: [tool adapter guide](tool-adapter-guide.md) and [conformance profiles](conformance-profiles.md).
