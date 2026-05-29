# OFFF Pilot Template

## Instructions

Complete this template before beginning a controlled OFFF pilot (Step 7 of the adoption
playbook). The completed template serves as the formal scope definition for the pilot and
as reference for the post-pilot review.

Replace all `[FILL IN]` placeholders with actual values before the pilot begins.

---

## 1. Pilot Identification

| Field | Value |
|---|---|
| Pilot name | `[FILL IN]` |
| Organisation | `[FILL IN]` |
| Pilot lead | `[FILL IN]` |
| Start date | `[FILL IN]` |
| End date (planned) | `[FILL IN]` |
| OFFF version | `[FILL IN]` (e.g., `0.1.0`) |
| Template version | 1.0 (2026-05-29) |

---

## 2. Purpose

**Objective:**
`[FILL IN]` — Describe in one or two sentences what this pilot is intended to demonstrate
or validate.

**Motivation:**
`[FILL IN]` — Why is OFFF being evaluated for this use case? What problem does it address?

---

## 3. Scope

### 3.1 In Scope

`[FILL IN]` — Describe what evidence, data, tools, and workflows are included in this pilot.

Examples:
- Evidence type: `[block_image / file_collection / logical_extraction / api_export]`
- Source: `[type of device or system — do not include case-identifiable details]`
- OFFF workflow steps: `[acquisition / indexing / keyword analysis / YARA analysis / verification]`
- OFFF components: `[offf-convert / offf-index / offf-keyword-worker / offf-yara-worker / offf-verify]`

### 3.2 Out of Scope

`[FILL IN]` — Explicitly list what is not being evaluated in this pilot.

Examples:
- Production deployment.
- Legal admissibility determination.
- Comparison with existing tools.
- Components with `experimental` maturity (unless explicitly in scope).

---

## 4. Test Data

| Property | Value |
|---|---|
| Data type | `[block_image / file_collection / logical_extraction / synthetic]` |
| Data size (approx.) | `[FILL IN]` |
| Data origin | `[FILL IN — real case data / anonymised / synthetic]` |
| Legal basis for use | `[FILL IN]` |
| Data retention policy | `[FILL IN]` |

**Note:** If using real case data, confirm that applicable data protection and chain of
custody requirements are met before the pilot begins.

---

## 5. Roles and Responsibilities

| Role | Person/Team | Responsibilities |
|---|---|---|
| Pilot lead | `[FILL IN]` | Overall coordination, reporting |
| Forensic engineer | `[FILL IN]` | Container creation, verification, job execution |
| Forensic practitioner | `[FILL IN]` | Evidence review, findings assessment |
| Legal advisor | `[FILL IN]` | Legal scope and limitations review |
| Security reviewer | `[FILL IN]` | Deployment security confirmation |
| Infrastructure | `[FILL IN]` | Environment setup and support |

---

## 6. Tools

### Existing tools in use alongside OFFF

| Tool | Version | Purpose | Integration pattern |
|---|---|---|---|
| `[FILL IN]` | `[FILL IN]` | `[FILL IN]` | `[A / B / C / D / E]` |

See `docs/tool-adapter-guide.md` for integration pattern definitions.

### OFFF components used

| Component | Version | Maturity | Notes |
|---|---|---|---|
| `offf-convert` | `[FILL IN]` | `forensic-grade-candidate` | |
| `offf-verify` | `[FILL IN]` | `forensic-grade-candidate` | |
| `offf-index` | `[FILL IN]` | `reference` | |
| `offf-jobs` | `[FILL IN]` | `reference` | |
| `[additional]` | `[FILL IN]` | `[FILL IN]` | |

---

## 7. OFFF Conformance Profiles

Identify which conformance profiles are required for this pilot:

| Profile | Required | Notes |
|---|---|---|
| OFFF Reader Conformant | `[Yes / No]` | |
| OFFF Acquisition Conformant | `[Yes / No]` | |
| OFFF Indexer Conformant | `[Yes / No]` | |
| OFFF Analysis Worker Conformant | `[Yes / No]` | |
| OFFF Object-Lineage Conformant | `[Yes / No]` | |
| OFFF Access Service Conformant | `[Yes / No]` | |
| OFFF Extension Conformant | `[Yes / No]` | |
| OFFF Forensic Baseline Conformant | `[Yes / No]` | Strongly recommended |

See `docs/forensic-baseline-profile.md` for minimum baseline requirements.

---

## 8. Success Criteria

The pilot is considered successful if **all** of the following criteria are met:

| # | Criterion | How measured |
|---|---|---|
| S1 | `offf-convert` produces a valid container from test data | `offf-verify --profile forensic-baseline` exits 0 |
| S2 | All required conformance profiles pass | Conformance report shows `pass` for required profiles |
| S3 | Analysis jobs complete without errors | `result_manifest.json` present; no error events |
| S4 | Verification report is readable by the forensic practitioner | Manual review confirms report usefulness |
| S5 | `[FILL IN additional criteria]` | `[FILL IN how measured]` |

**Partial success:** If S1–S3 pass but S4 has gaps, the pilot is `partial`. Document the
gaps and assess whether they are blockers for adoption.

---

## 9. Known Risks

List risks identified before the pilot begins. See `docs/risk-assessment-template.md`
for a full risk framework.

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | `[FILL IN]` | `[Low / Medium / High]` | `[Low / Medium / High]` | `[FILL IN]` |

---

## 10. Exit Criteria

### Normal exit

The pilot concludes on the planned end date with a pilot report documenting results
against success criteria.

### Early exit (halt conditions)

The pilot halts early if:

- A critical security issue is discovered in the OFFF components being used.
- The evidence integrity chain is found to be broken for reasons attributable to OFFF.
- Legal or regulatory constraints prevent continuation.
- `[FILL IN additional halt conditions]`

In all early exit scenarios, the pilot lead must document the reason and preserve all
artefacts for post-mortem review.

---

## 11. Post-Pilot Deliverables

| Deliverable | Owner | Due |
|---|---|---|
| Pilot report (findings against success criteria) | Pilot lead | `[FILL IN]` |
| Conformance report | Forensic engineer | `[FILL IN]` |
| Verification reports for all containers | Forensic engineer | `[FILL IN]` |
| Gap list (open issues for adoption) | Technical architect | `[FILL IN]` |
| Adoption recommendation | Programme manager | `[FILL IN]` |

---

## 12. Approval

| Role | Name | Date | Signature |
|---|---|---|---|
| Pilot lead | `[FILL IN]` | `[FILL IN]` | |
| Legal advisor | `[FILL IN]` | `[FILL IN]` | |
| Security reviewer | `[FILL IN]` | `[FILL IN]` | |

---

*Template version: 1.0 — Last updated: 2026-05-29*
