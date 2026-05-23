# offf-access-service

## Doel
Biedt REST en gRPC toegang tot OFFF cases voor lezen en gecontroleerde writes (analysis/provenance), inclusief capability- en policy-checks.

## Functionaliteit
- manifest/chunk/file retrieval
- list endpoints voor files/artifacts
- append analysis resultaten
- append provenance events
- authz per role en write-layer
- opslag-pariteit voor local filesystem en `s3://` case paths (MinIO/Ceph/S3)

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
- Voor object storage: zet `OFFF_CASES_ROOT` op een `s3://bucket/prefix` root of geef `case_id` als volledige `s3://` URI.
- Voor MinIO/Ceph endpoint compatibiliteit: configureer `OFFF_S3_ENDPOINT`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` en optioneel `AWS_REGION`.

## Parity tests
```bash
cargo test -p offf-access-service --tests -- --nocapture
```

`tests/grpc_storage_parity.rs` draait parity-checks voor local vs `s3://` case paths.
De S3 parity test wordt automatisch overgeslagen als `OFFF_S3_ENDPOINT` en `OFFF_S3_TEST_BUCKET` niet gezet zijn.
