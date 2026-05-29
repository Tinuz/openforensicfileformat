# OFFF Adoption Playbook

## Purpose

This playbook describes the recommended path from first contact with OFFF to a controlled
forensic pilot. It is intended for forensic architects, technical programme managers, and
team leads at organisations that are evaluating OFFF for use in a formal forensic context.

The playbook is not prescriptive. Organisations may adapt the sequence and depth of each
step to their own context, risk appetite, and regulatory environment.

---

## Prerequisites

Before starting, confirm:
- Rust toolchain installed (`rustup`, stable channel).
- Python 3.10+ available for SDK and conformance tests.
- Access to at least one of: a raw disk image, a file collection, or a logical extraction
  from a device.
- At least one person who understands the OFFF specification (`SPEC_OFFF_Formal_Spec_v0.1.0.md`).

---

## Step 1: POC with Synthetic Data

**Goal:** Confirm that OFFF core tools work in your environment.

**Activities:**
1. Build the workspace: `cargo build --workspace`.
2. Run all workspace tests: `cargo test --workspace`.
3. Create a synthetic container using the demo script:
   ```bash
   python scripts/create_demo_case.py
   ```
4. Verify the container:
   ```bash
   cargo run -p offf-verify -- demo_case.offf
   ```
5. Run the conformance suite:
   ```bash
   python tests/conformance/run_conformance.py
   ```

**Success criteria:**
- All workspace tests pass.
- `offf-verify` exits 0 on the synthetic container.
- Conformance suite reports at least Reader and Acquisition profiles as `pass`.

**Who:** Infrastructure engineer, Rust/Python developer.

**Output:** Build confirmation; conformance report from synthetic data.

---

## Step 2: POC with Real Tool Export

**Goal:** Demonstrate OFFF ingestion from an existing forensic tool.

**Activities:**
1. Choose an acquisition tool and acquisition mode (see `docs/evidence-root-model.md`).
2. Export or acquire a test image or file collection using the existing tool.
3. Ingest into OFFF:
   ```bash
   # For raw/dd image
   cargo run -p offf-convert -- --input test.dd --output test.offf

   # For E01 image (experimental)
   cargo run -p offf-convert -- --input test.E01 --input-type e01 --output test.offf
   ```
4. Verify the ingested container:
   ```bash
   cargo run -p offf-verify -- test.offf --profile forensic-baseline
   ```
5. Run a sample analysis job:
   ```bash
   cargo run -p offf-jobs -- create-keyword --case test.offf --keywords password,secret
   cargo run -p offf-jobs -- run --case test.offf --job test.offf/jobs/<job_id>.json
   ```

**Success criteria:**
- `offf-convert` exits 0.
- `offf-verify --profile forensic-baseline` exits 0.
- Job output is written under `analysis/jobs/` with a valid `result_manifest.json`.

**Who:** Forensic engineer, existing tool operator.

**Output:** First real-data container with baseline verification report.

---

## Step 3: Forensic Expert Review

**Goal:** Have a forensic practitioner review the evidence model and verify that OFFF
accurately represents what was acquired.

**Activities:**
1. Provide the forensic expert with:
   - The container from Step 2.
   - The verification report (`offf-verify --report-md`).
   - `docs/chain-of-evidence.md` and `docs/evidence-root-model.md`.
2. The expert reviews:
   - Does `acquisition.json` accurately represent the source?
   - Are skipped and error events complete?
   - Does the object index match expectations?
   - Are the known limitations acceptable for the intended use?
3. Document findings and gaps.

**Success criteria:**
- The forensic expert confirms that the chain of evidence model is technically sound.
- Any identified gaps are documented as known limitations or backlog items.

**Who:** Certified forensic examiner or forensic practitioner.

**Output:** Forensic expert review report; list of gaps and limitations.

---

## Step 4: Legal Review

**Goal:** Have a legal expert review OFFF's scope and limitations in the context of the
applicable legal framework.

**Activities:**
1. Provide the legal reviewer with:
   - `docs/legal-neutrality.md`
   - `docs/forensic-limitations.md`
   - The forensic expert report from Step 3.
2. The reviewer assesses:
   - Is OFFF's technical record sufficient to support the intended legal use?
   - Are the limitations acceptable for the specific legal context?
   - Are there compliance or accreditation requirements that OFFF must satisfy?
3. Document legal constraints and requirements.

**Success criteria:**
- The legal reviewer confirms no fundamental incompatibility with the applicable legal framework.
- Legal constraints and requirements are documented for the pilot design.

**Who:** Legal counsel with digital forensics experience.

**Output:** Legal review report; legal requirements for pilot.

---

## Step 5: Security Review

**Goal:** Confirm that OFFF components used in the intended deployment do not introduce
unacceptable security risks.

**Activities:**
1. Identify which OFFF components will be deployed (access service, workers, SDKs).
2. Review the threat model: `docs/threat-model.md`.
3. For the access service: review auth mode configuration (`jwt` vs `dev_headers`).
4. Conduct a security review of the deployment architecture.
5. If using `jwt` auth mode: confirm that `OFFF_JWT_SECRET` is managed securely.

**Success criteria:**
- No critical security issues in the intended deployment configuration.
- Auth mode is appropriate for the deployment context.
- Known security limitations are documented and accepted.

**Who:** Security architect or security reviewer.

**Output:** Security review report; deployment security configuration.

---

## Step 6: Conformance Review

**Goal:** Confirm that all components used in the intended workflow pass their required
conformance profiles.

**Activities:**
1. Run the conformance suite against a representative container:
   ```bash
   python tests/conformance/run_conformance.py
   ```
2. Review which profiles pass and which show gaps.
3. Run the release readiness report:
   ```bash
   python scripts/generate_release_readiness.py
   ```
4. Confirm that the required profiles for the intended use case are satisfied (see
   `docs/forensic-baseline-profile.md` for the minimum).

**Success criteria:**
- Forensic Baseline profile: all required checks pass.
- Any profile gaps are documented with known limitations.

**Who:** Technical architect, forensic engineer.

**Output:** Conformance report; release readiness report.

---

## Step 7: Controlled Pilot

**Goal:** Use OFFF in a real investigation or case, under controlled conditions, with
documented scope and exit criteria.

**Activities:**
1. Complete the `docs/pilot-template.md` before starting.
2. Use a real case with clearly defined scope, data, and success criteria.
3. Run the full OFFF workflow: acquisition → indexing → analysis → verification.
4. Document all deviations, surprises, and gaps encountered.
5. Produce a pilot report: what worked, what did not, what requires further work.

**Success criteria:**
- Defined in the pilot template before the pilot begins.

**Who:** Forensic practitioner team, technical architect, legal advisor.

**Output:** Pilot report with findings, gaps, and recommendations.

---

## Step 8: Decision on Standardisation

**Goal:** Based on the pilot results, decide whether to adopt OFFF as a standard.

**Activities:**
1. Review the pilot report.
2. Assess gaps against requirements for the intended use case.
3. Identify path to close gaps (upstream contributions, extensions, waivers).
4. Make a formal adoption decision with documented rationale.

**Possible outcomes:**
- **Adopt** — OFFF meets requirements; begin rolling deployment.
- **Adopt with conditions** — OFFF meets requirements subject to documented limitations.
- **Continue evaluation** — pilot revealed gaps that must be closed first.
- **Do not adopt** — OFFF does not meet requirements; document why.

**Who:** Programme manager, technical lead, legal and forensic advisors.

**Output:** Formal adoption decision with rationale.

---

## Roles and Responsibilities Summary

| Role | Responsible for |
|---|---|
| Infrastructure engineer | Build, test, and deploy OFFF components |
| Forensic engineer | Container creation, verification, and job execution |
| Forensic practitioner | Evidence review, expert assessment |
| Legal counsel | Legal framework compatibility |
| Security architect | Security review and deployment hardening |
| Programme manager | Pilot coordination, adoption decision |
| Technical architect | Conformance and integration design |

---

## Timeline Guidance

Timelines depend heavily on organisational context. As a rough guide:

| Step | Minimum effort |
|---|---|
| Step 1 — Synthetic POC | 1 day |
| Step 2 — Real data POC | 2–5 days |
| Step 3 — Forensic expert review | 1–2 weeks |
| Step 4 — Legal review | 2–4 weeks |
| Step 5 — Security review | 1–2 weeks |
| Step 6 — Conformance review | 1–3 days |
| Step 7 — Controlled pilot | 4–12 weeks |
| Step 8 — Adoption decision | 1–2 weeks |

---

## Related Documents

- `docs/pilot-template.md` — template for Step 7
- `docs/risk-assessment-template.md` — risk assessment template for Step 4–7
- `docs/forensic-baseline-profile.md` — minimum requirements for forensic use
- `docs/forensic-limitations.md` — known limitations to discuss in Steps 3–4
- `docs/legal-neutrality.md` — scope for legal review in Step 4
- `docs/tool-adapter-guide.md` — integration patterns for Step 2

*Last updated: 2026-05-29*
