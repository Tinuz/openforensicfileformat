# OFFF Legal Neutrality

## Purpose

OFFF is a technical format and reference implementation. It records what happened to data —
how it was acquired, processed, and what results were produced. It does not make legal decisions,
legal findings, or legal recommendations.

This document makes that boundary explicit. It is intended for forensic architects, legal
reviewers, programme managers, and tool vendors who need to understand what OFFF can and cannot
contribute to a forensic or legal process.

---

## What OFFF Is

OFFF is an **open, verifiable forensic evidence and interoperability format** for:

- Recording evidence objects and their acquisition metadata.
- Storing raw evidence bytes with cryptographic integrity protection.
- Tracking analysis results, provenance, and object lineage.
- Enabling interoperability between forensic tools via open schemas.
- Providing machine-verifiable integrity checks.

Everything OFFF records is a technical fact: what bytes were present, what tool processed them,
what hash was computed, what result was written. OFFF records are factual statements about
data operations, not interpretive statements about their legal significance.

---

## What OFFF Is Not

| Claim | Status |
|---|---|
| OFFF makes legal decisions | **False** |
| OFFF certifies legal admissibility of evidence | **False** |
| OFFF determines whether evidence was lawfully obtained | **False** |
| OFFF asserts that a finding is forensically conclusive | **False** |
| OFFF is a replacement for certified forensic tools | **False** |
| OFFF is a replacement for forensic expert judgment | **False** |
| OFFF enforces compliance with legal or regulatory requirements | **False** |

---

## Technical Records vs Legal Interpretation

OFFF produces technical records. The legal interpretation of those records is a separate
activity that OFFF neither performs nor constrains.

**Example: object labelling**

An OFFF container may contain label events that classify objects as `relevant`, `privileged`,
`sensitive`, or any other label defined by the operator. These labels are:

- Technical metadata set by an analysis tool or operator.
- Stored as JSONL events in the extension layer.
- Searchable and filterable by tools reading the container.

These labels are **not** legal determinations. A label `relevant` means "the tool or operator
marked this object as relevant for a defined scope" — it does not mean "this object is
legally admissible" or "this object constitutes evidence in a legal proceeding."

**Example: scope and exclusion**

An OFFF container may define scopes that include or exclude objects from processing. An
object that is excluded from a processing scope is absent from that job's results. It is not
"legally excluded" or "privileged" by virtue of its exclusion.

**Example: derived objects**

OFFF tracks derivation chains (which objects were derived from which source objects). A
derivation record establishes a technical relationship. It does not establish legal authorship,
legal ownership, or legal liability.

---

## Labels, Scopes, and Sets Are Technical

The following OFFF constructs are purely technical:

| Construct | Technical meaning | Not a legal statement about |
|---|---|---|
| `LabelEvent` | A tool or operator attached a label to an object | Admissibility, relevance, or privilege |
| `ScopeRecord` | An operator defined a processing scope | Legal boundaries of investigation |
| `SetRecord` | An operator grouped objects | Legal classification or disclosure status |
| `inclusion_scope` / `exclusion_scope` | A job included or excluded objects | Legal permission to access |
| `skipped_event` | A tool did not process an object | Legal treatment of unprocessed data |
| `denied_access_event` | The access service rejected a write | Legal prohibition on processing |

---

## Who Is Responsible for Legal Interpretation

OFFF provides technical records. The following parties are responsible for legal interpretation:

| Party | Responsibility |
|---|---|
| Forensic analyst | Interpret technical findings in forensic context |
| Legal counsel | Determine legal admissibility and relevance |
| Investigating authority | Apply applicable procedural law |
| Court or tribunal | Make binding legal determinations |

OFFF Core does not perform or replace any of these functions.

---

## Boundaries of OFFF Core

The following decisions are explicitly outside OFFF Core:

```
- Whether a specific acquisition was lawful.
- Whether a specific analysis result is admissible.
- Whether a specific object is legally privileged.
- Whether a specific finding constitutes proof of a legal claim.
- Whether a specific container meets requirements of a specific legal system.
- Whether the tools used in an investigation were certified or accredited.
```

---

## Vendor and Tool Independence

OFFF is designed to be tool-agnostic and vendor-neutral. It does not embed preferences for
any specific forensic platform, vendor, or accreditation body. References to
Hansken, FTK, Cellebrite, GrayKey, or other tools in OFFF documentation are illustrative
of integration categories, not endorsements or certifications.

---

## Use of OFFF in a Legal Context

OFFF technical records may be used as part of a legal process when:

1. The forensic analyst can attest to the integrity of the acquisition process.
2. The container verification report confirms cryptographic integrity.
3. The provenance record documents who performed which actions.
4. Known limitations are disclosed to the relevant parties.

In such use, OFFF provides the technical substrate. The forensic analyst provides the
interpretive expertise. The legal authority makes the legal determination.

OFFF does not certify any of these steps, and a passing `offf-verify` report is not
equivalent to a certificate of legal admissibility.

---

## Related Documents

- `docs/chain-of-evidence.md` — technical data chain
- `docs/chain-of-custody.md` — process audit trail
- `docs/forensic-limitations.md` — technical limitations
- `docs/scope-and-exclusion-model.md` — technical scope model

*Last updated: 2026-05-29*
