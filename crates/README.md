# OFFF Rust Crates

## Doel
Deze map bevat alle Rust crates voor OFFF conversie, verificatie, indexing, services en workers.

## Overzicht
- `offf-core`: gedeelde core logica en types
- `offf-convert`: raw/E01 naar OFFF converter
- `offf-verify`: integriteitsverificatie
- `offf-index`: index-opbouw
- `offf-export`: reconstructie/export
- `offf-jobs`: job orchestration
- `offf-access-service`: REST/gRPC access service
- `offf-keyword-worker`: keyword scan worker
- `offf-yara-worker`: YARA scan worker
- `offf-annotate`: annotation tooling

## Build en test
```bash
cargo check --workspace
cargo test --workspace -- --nocapture
```

## Ontwikkelrichtlijnen
- Houd publieke types in `offf-core` stabiel.
- Gebruik end-to-end smoke tests voor omgeving-afhankelijke wijzigingen.
- Voeg provenance events toe bij write-operaties.
