# OFFF Component Status Matrix

This matrix tracks current implementation maturity and known limitations.

| Component | Status | Notes |
|---|---|---|
| OFFF Core chunk store | stable-mvp | Hashing and chunk IO work; hardening in progress |
| Merkle root | stable-mvp | Root generation exists; inclusion proofs pending |
| raw/dd convert | stable-mvp | Crash-safe finalization hardening pending |
| E01 convert | experimental | Depends on ewfexport/libewf path and environment |
| verify | stable-mvp | Profiles and deeper conformance checks pending |
| export | stable-mvp | Packed support added; broader remote/export scenarios pending |
| MBR/GPT index | experimental | GPT CRC and EBR chain support pending |
| NTFS index | experimental | Not fully forensic complete yet |
| keyword worker | experimental | Job output isolation and boundary matching hardening pending |
| YARA worker | experimental | Job output isolation and boundary matching hardening pending |
| Access Service | experimental | Production auth mode and denied-event persistence pending |
| Python SDK | experimental | Cache limits and append-only API hardening pending |
| Go SDK | experimental | API parity and v0.2 compatibility hardening pending |
| Extensions v0.2 | planned | Generic extension model not fully implemented yet |

## Status Legend
- `stable-mvp`: working baseline with production hardening still in progress.
- `experimental`: usable for development/testing, not yet hardened for strict forensic operations.
- `planned`: not implemented or only partially scaffolded.

## Related
- Root overview: `README.md`
- Formal spec: `SPEC_OFFF_Formal_Spec_v0.1.0.md`
- Schema catalog: `docs/schema/offf-schema-catalog-0.1.0.json`
- Hardening program: `BACKLOG.txt`
