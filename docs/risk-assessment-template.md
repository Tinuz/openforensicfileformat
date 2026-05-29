# OFFF Risk Assessment Template

## Instructions

Complete this template as part of the adoption process (Steps 4–7 of the adoption playbook).
Each risk category should be reviewed by the appropriate role. Accepted residual risks must
be signed off before proceeding to a controlled pilot.

Replace all `[FILL IN]` placeholders. Remove risk rows that are not applicable. Add rows
for organisation-specific risks.

---

## 1. Assessment Identification

| Field | Value |
|---|---|
| Organisation | `[FILL IN]` |
| Assessment lead | `[FILL IN]` |
| Date | `[FILL IN]` |
| OFFF version assessed | `[FILL IN]` |
| Intended use case | `[FILL IN]` |
| Template version | 1.0 (2026-05-29) |

---

## Risk Rating Scale

| Rating | Likelihood | Impact | Combined |
|---|---|---|---|
| `Low` | Unlikely | Minor consequence | Accept |
| `Medium` | Possible | Significant but manageable | Mitigate or accept with controls |
| `High` | Likely | Serious consequence | Mitigate before proceeding |
| `Critical` | Near certain / Very likely | Severe or irreversible | Block until resolved |

---

## 2. Forensic Integrity Risks

*Review by: forensic practitioner, technical architect*

| ID | Risk | Likelihood | Impact | Rating | Mitigation | Residual |
|---|---|---|---|---|---|---|
| FI-01 | Evidence layer corrupted between acquisition and verification | Low | Critical | Medium | Run `offf-verify` immediately after acquisition and before any analysis; store container on write-protected media | Low |
| FI-02 | Chunk SHA-256 collision causes a corrupted chunk to pass verification | Very Low | Critical | Low | SHA-256 collision attacks are computationally infeasible; plan for SHA-3/BLAKE3 migration if SHA-256 is deprecated | Very Low |
| FI-03 | Incomplete acquisition: missing files or objects not recorded in skipped events | Medium | High | High | Require acquisition tool to record skipped events; validate completeness before proceeding to analysis | Medium |
| FI-04 | Merkle root mismatch detected by verifier but ignored by analyst | Low | High | Medium | Enforce non-zero exit code from `offf-verify`; integrate into workflow as a blocking gate | Low |
| FI-05 | Object lineage dangling reference: derived object references non-existent source | Medium | Medium | Medium | Enable lineage verification in `offf-verify`; validate before disclosure | Low |
| FI-06 | Analysis result references a job scope that was not fully processed | Medium | Medium | Medium | Confirm that error and skipped events are complete before citing analysis results | Low |
| FI-07 | `[FILL IN organisation-specific forensic risk]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |

---

## 3. Legal and Process Risks

*Review by: legal counsel, programme manager*

| ID | Risk | Likelihood | Impact | Rating | Mitigation | Residual |
|---|---|---|---|---|---|---|
| LP-01 | OFFF record mistakenly cited as legal determination | Medium | High | High | Train all users on `docs/legal-neutrality.md`; include neutrality disclaimer in verification reports | Low |
| LP-02 | OFFF labels or scope records interpreted as legally binding classifications | Medium | High | High | Document that labels are technical metadata; see `docs/scope-and-exclusion-model.md` | Low |
| LP-03 | Applicable law requires specific accredited tools; OFFF tools not yet accredited | `[FILL IN]` | High | `[FILL IN]` | Identify accreditation requirements; determine whether OFFF can supplement (not replace) accredited tools | `[FILL IN]` |
| LP-04 | Retention and data handling requirements not met by OFFF container storage | `[FILL IN]` | High | `[FILL IN]` | Review data handling policy against OFFF container lifecycle; implement container encryption at rest if required | `[FILL IN]` |
| LP-05 | Pilot uses real case data without appropriate legal basis | `[FILL IN]` | Critical | `[FILL IN]` | Obtain explicit legal authorisation before using real case data in the pilot | `[FILL IN]` |
| LP-06 | `[FILL IN organisation-specific legal risk]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |

---

## 4. Privacy Risks

*Review by: privacy officer, legal counsel*

| ID | Risk | Likelihood | Impact | Rating | Mitigation | Residual |
|---|---|---|---|---|---|---|
| PR-01 | Personal data in OFFF container not protected at rest | `[FILL IN]` | High | `[FILL IN]` | Enable encryption at rest for container storage; restrict access via tool registry and auth mode | `[FILL IN]` |
| PR-02 | Access service logs expose personal data in audit trail | Low | Medium | Low | Review audit log fields; anonymise actor fields if required by data protection law | Low |
| PR-03 | OFFF container transferred to jurisdiction with different data protection requirements | `[FILL IN]` | High | `[FILL IN]` | Restrict container transfer by policy; encrypt containers in transit | `[FILL IN]` |
| PR-04 | Object index exposes sensitive personal information to tools without need-to-know | Medium | Medium | Medium | Use capability-gated access service; restrict analysis scope via job manifests | Low |
| PR-05 | `[FILL IN organisation-specific privacy risk]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |

---

## 5. Security Risks

*Review by: security architect, security reviewer*

| ID | Risk | Likelihood | Impact | Rating | Mitigation | Residual |
|---|---|---|---|---|---|---|
| SE-01 | Access service JWT secret leaked, allowing forged tokens | Low | High | Medium | Store `OFFF_JWT_SECRET` in secrets manager; rotate regularly; use `jwt` mode not `dev_headers` in production | Low |
| SE-02 | Path traversal attack via malformed container path | Low | High | Medium | `normalize_rel_path()` guards are in place; explicit unit tests cover traversal patterns | Very Low |
| SE-03 | Oversized upload causes access service resource exhaustion | Low | Medium | Low | `DefaultBodyLimit` (10 MB) and `MAX_ROWS_PER_REQUEST` (50 000) enforced at API level | Very Low |
| SE-04 | Access service exposes chunk data to unauthenticated clients | `[FILL IN]` | High | `[FILL IN]` | Enable `jwt` auth mode; audit tool registry for least-privilege capabilities | `[FILL IN]` |
| SE-05 | Malicious YARA rule causes resource exhaustion or OOM in worker | Medium | Medium | Medium | Run workers in isolated containers; set resource limits; use `yara-x` (pure Rust, memory-safe) | Low |
| SE-06 | Evidence layer modification by compromised analysis worker | Low | Critical | High | Access service denies evidence layer writes; denied access events logged; `offf-verify` detects post-finalization mutations | Low |
| SE-07 | Access service S3 backend credentials exposed | `[FILL IN]` | High | `[FILL IN]` | Use IAM roles or secrets manager; do not hardcode credentials | `[FILL IN]` |
| SE-08 | `[FILL IN organisation-specific security risk]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |

---

## 6. Tool Integration Risks

*Review by: technical architect, forensic engineer*

| ID | Risk | Likelihood | Impact | Rating | Mitigation | Residual |
|---|---|---|---|---|---|---|
| TI-01 | Existing acquisition tool does not produce output compatible with `offf-convert` | `[FILL IN]` | High | `[FILL IN]` | Verify compatibility in Step 2 of adoption playbook; consider custom Pattern A adapter | `[FILL IN]` |
| TI-02 | E01 conversion requires `ewfexport`/`libewf` not available in deployment environment | Medium | Medium | Medium | Install `ewftools` package; or pre-convert E01 to raw/dd before OFFF ingestion | Low |
| TI-03 | Analysis worker produces output not conformant with OFFF Analysis Worker profile | Medium | Medium | Medium | Test against conformance suite before integration; enforce result manifest in worker contract | Low |
| TI-04 | SDK version incompatibility between Python SDK and OFFF container format version | Low | Medium | Low | Pin SDK version; validate against conformance suite after upgrades | Very Low |
| TI-05 | Tool registry not configured for pilot tools; workers denied access | Medium | Medium | Medium | Configure `config/tool-registry.json` before pilot start; test in Step 2 | Low |
| TI-06 | `[FILL IN organisation-specific integration risk]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |

---

## 7. Operational Risks

*Review by: infrastructure engineer, programme manager*

| ID | Risk | Likelihood | Impact | Rating | Mitigation | Residual |
|---|---|---|---|---|---|---|
| OP-01 | Container stored on non-write-protected media; accidental modification | Medium | High | High | Use read-only media or OS-level write protection after finalization; run `offf-verify` before any analysis | Low |
| OP-02 | `offf-convert` fails mid-run leaving partial container; partial container mistaken for complete | Low | High | Medium | Crash-safe finalization is implemented (atomic rename); incomplete containers have no `manifest.json`; verifier detects | Very Low |
| OP-03 | Insufficient storage for chunk store during acquisition | `[FILL IN]` | Medium | `[FILL IN]` | Estimate container size before acquisition (image size × 1.05 overhead); provision accordingly | `[FILL IN]` |
| OP-04 | Worker job left in `in-progress` state after crash; no recovery path | Low | Medium | Low | Restart job from `job_manifest.json`; OFFF append-only contract prevents partial result from contaminating prior output | Very Low |
| OP-05 | Verification takes too long for large containers in time-sensitive investigations | `[FILL IN]` | Medium | `[FILL IN]` | Profile verification time on representative data before pilot; consider partial verification scope for time-sensitive steps | `[FILL IN]` |
| OP-06 | `[FILL IN organisation-specific operational risk]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |

---

## 8. Vendor Lock-in Risks

*Review by: technical architect, programme manager*

| ID | Risk | Likelihood | Impact | Rating | Mitigation | Residual |
|---|---|---|---|---|---|---|
| VL-01 | Dependency on OFFF reference implementation Rust binaries prevents tool substitution | Low | Medium | Low | OFFF is an open specification; third parties can implement conformant tools; schema catalog is public | Very Low |
| VL-02 | Container format changes in future OFFF versions break existing containers | Low | High | Medium | Versioning policy (`docs/versioning.md`) requires major version bump for breaking changes; migration tools planned | Low |
| VL-03 | OFFF project discontinued; containers become unreadable | Low | High | Medium | OFFF uses open JSON/Parquet schemas; containers are self-describing; community fork is possible | Low |
| VL-04 | Organisation becomes dependent on OFFF for evidence integrity, then must migrate | Low | Medium | Low | Document OFFF dependency explicitly; maintain export capability to raw/dd | Very Low |
| VL-05 | `[FILL IN organisation-specific vendor lock-in risk]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |

---

## 9. Risk Summary

| Category | Open Critical | Open High | Open Medium | Status |
|---|---|---|---|---|
| Forensic Integrity | 0 | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |
| Legal and Process | 0 | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |
| Privacy | 0 | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |
| Security | 0 | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |
| Tool Integration | 0 | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |
| Operational | 0 | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |
| Vendor Lock-in | 0 | `[FILL IN]` | `[FILL IN]` | `[FILL IN]` |
| **Total** | **0** | **`[FILL IN]`** | **`[FILL IN]`** | **`[FILL IN]`** |

**Pilot authorisation:**
- No `Critical` risks open.
- All `High` risks mitigated or accepted with documented residual.
- `Medium` risks acknowledged.

---

## 10. Sign-off

| Role | Name | Date | Decision |
|---|---|---|---|
| Pilot lead | `[FILL IN]` | `[FILL IN]` | Accept / Reject |
| Legal advisor | `[FILL IN]` | `[FILL IN]` | Accept / Reject |
| Security reviewer | `[FILL IN]` | `[FILL IN]` | Accept / Reject |
| Programme manager | `[FILL IN]` | `[FILL IN]` | Accept / Reject |

---

*Template version: 1.0 — Last updated: 2026-05-29*
