# SDK Overview

## Purpose
The `sdk` directory contains language SDKs for common OFFF workflows such as reading containers, verifying integrity, and writing analysis/provenance outputs.

## Available SDKs
- `python/`: Python SDK with minimal contract surface and tests.
- `go/`: Go SDK with minimal OFFF API surface and smoke tests.

## Typical Use Cases
- Open a container and read manifest metadata.
- Verify container and chunk integrity.
- Map offsets to chunks.
- Append analysis/provenance outputs via SDK APIs.

## Guidelines
- Keep parity with the agreed minimal OFFF SDK profile.
- Add/update contract tests when extending public APIs.
- Document breaking changes explicitly per language SDK.
