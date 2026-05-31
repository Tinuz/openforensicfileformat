# Tool Registry

The tool registry is the governance input for approved tool identity and capability tracking.

## Scope

- Record tool identity, versioning, and allowed capabilities.
- Support access-control decisions and auditability.
- Provide a stable example format for integration work.

## Current State

- The repository currently ships an example JSON file at `config/tool-registry.example.json`.
- Enforcement is integrated in the access service write paths, but the registry itself remains an evolving governance artifact.

## Current Limitations

- Example file only; no dedicated enforcement tooling.
- Full governance workflow still needs formalization.

See also: [access service reference surface](access-service-reference.md) and [definition of done](definition-of-done.md).