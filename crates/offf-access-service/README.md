# offf-access-service

## Doel
Biedt REST en gRPC toegang tot OFFF cases voor lezen en gecontroleerde writes (analysis/provenance), inclusief capability- en policy-checks.

## Functionaliteit
- manifest/chunk/file retrieval
- list endpoints voor files/artifacts
- append analysis resultaten
- append provenance events
- authz per role en write-layer

## Start (voorbeeld)
```bash
cargo run -p offf-access-service -- --help
```

## Testen
```bash
cargo test -p offf-access-service --test grpc_smoke -- --nocapture
```

## Configuratie
- Koppel tool registry configuratie uit `config/` voor governance enforcement.
- Gebruik sample OFFF containers uit `tests/samples` voor snelle smoke runs.
