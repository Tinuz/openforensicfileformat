# offf-core

## Doel
`offf-core` bevat gedeelde domeinlogica voor OFFF:
- chunk read/write/verify
- hash en merkle utilities
- parquet IO helpers
- provenance writer
- storage abstractie voor local en S3
- packed container helpers (`.offfpack` pack/list/unpack)
- centrale types en fouten

## Gebruik
Deze crate wordt gebruikt door vrijwel alle andere crates en wordt normaal niet als los CLI-programma uitgevoerd.

## Belangrijk
- S3/MinIO toegang gebruikt endpoint uit `OFFF_S3_ENDPOINT`.
- Bij custom endpoint wordt path-style addressing gebruikt voor compatibiliteit.
- JSONL appends op S3 gebruiken optimistic concurrency retries.

## Testen
```bash
cargo test -p offf-core -- --nocapture
```
