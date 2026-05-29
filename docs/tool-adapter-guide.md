# OFFF Tool Adapter Guide

## Purpose

This guide describes how existing forensic tools can interoperate with OFFF without
replacing them. OFFF does not replace Hansken, FTK, Cellebrite, GrayKey, Tika, or any
other forensic platform. It defines open contracts that those tools can implement.

OFFF is an **interoperability format**. A tool integrates with OFFF by implementing one
or more of the adapter patterns described here.

---

## What OFFF Is Not

Before choosing an integration pattern, be clear about what OFFF does not provide:

```
- Not a forensic suite or case management system.
- Not a replacement for acquisition tools.
- Not a replacement for forensic analysis platforms.
- Not a scheduler or orchestration engine.
- Not a reporting or disclosure UI.
- Not a legal decision engine.
```

Tools that already do these things do not need to be replaced. They need to implement the
correct OFFF adapter pattern for the function they perform.

---

## Tool Categories

| Category | Examples | Typical OFFF role |
|---|---|---|
| Acquisition tools | dd, ewfacquire, UFED, Oxygen | Pattern A (evidence producer) |
| Mobile extraction tools | Cellebrite UFED, GrayKey, Oxygen | Pattern A (logical_extraction) |
| Forensic analysis platforms | Hansken, Autopsy, FTK | Pattern B + C (consumer + result producer) |
| OCR / text extraction tools | Apache Tika, Tesseract | Pattern C (analysis result producer) |
| AI / ML classifiers | Custom models, YARA scanners | Pattern C + D (result producer) |
| Reporting / disclosure tools | Custom report generators | Pattern E (package consumer) |
| Indexing / search platforms | Elasticsearch, OpenSearch | Pattern C (derived index producer) |

---

## Integration Patterns

### Pattern A: Tool exports evidence → OFFF ingest

**Use case:** An existing acquisition tool produces a raw image or file collection, and
OFFF is used to ingest it into a verifiable container.

**How it works:**
1. The acquisition tool produces its native output (`.E01`, `.dd`, a directory of files, etc.).
2. `offf-convert` (or an equivalent ingestion component) converts the native output to an
   OFFF container.
3. The OFFF container receives the acquisition metadata from the original tool: source hash,
   sector size, tool identity.

**OFFF contracts to implement:**
- `acquisition.json` with source hash and tool identity from the original tool.
- `provenance_events.jsonl` with an acquisition event crediting the original tool.

**Mapping example:**

| Acquisition tool field | OFFF field |
|---|---|
| Source image SHA-256 / MD5 | `acquisition.source.sha256` |
| Acquisition tool name | `acquisition.tool.name` |
| Acquisition tool version | `acquisition.tool.version` |
| Examiner name | `acquisition.actor` |
| Sector size | `acquisition.parameters.sector_size` |
| Acquisition date/time | `provenance_events[0].timestamp` |

---

### Pattern B: Tool reads OFFF as input

**Use case:** An existing analysis platform reads evidence from an OFFF container instead
of from a proprietary format.

**How it works:**
1. The tool uses the OFFF Reader API (or the Python/Go SDK) to open the container.
2. It reads chunk data, file index, and object index via the standard OFFF access paths.
3. It optionally uses the access service for capability-gated access.

**OFFF contracts to implement:**
- Comply with `OFFF Reader Conformant` profile.
- Use `manifest.json` to discover container structure.
- Use `physical_to_chunk.parquet` for byte-offset to chunk mapping.
- Verify chunk SHA-256 before using chunk data.

**Minimal implementation (Python SDK):**

```python
from offf_sdk import OfffContainer

container = OfffContainer.open("case.offf")
manifest = container.read_manifest()

for chunk_id, chunk_hash in container.iter_chunks():
    data = container.read_chunk(chunk_id, verify=True)
    # process data...
```

---

### Pattern C: Tool writes analysis output to OFFF

**Use case:** An existing analysis tool (keyword scanner, YARA engine, classifier, OCR
processor, indexer) writes its results into an OFFF container's analysis layer.

**How it works:**
1. The tool creates a job manifest declaring its scope and tool identity.
2. It writes output artefacts under `analysis/jobs/{job_id}/`.
3. It writes `result_manifest.json` as the finalization point of the job.
4. It appends a provenance event to `provenance_events.jsonl`.

**OFFF contracts to implement:**
- Comply with `OFFF Analysis Worker Conformant` profile.
- Never write to the evidence layer.
- Finalize with `result_manifest.json` containing SHA-256 hashes of all output artefacts.

**result_manifest mapping example:**

| Analysis tool concept | OFFF field |
|---|---|
| Run identifier | `result_manifest.job_id` |
| Tool name | `result_manifest.tool.name` |
| Tool version | `result_manifest.tool.version` |
| Output files (hits, results, reports) | `result_manifest.artefacts[].path` + `.sha256` |
| Error count | `result_manifest.statistics.error_count` |
| Skipped items | `result_manifest.statistics.skipped_count` |

**provenance event mapping example:**

| Analysis tool concept | OFFF field |
|---|---|
| Run timestamp | `provenance_events[n].timestamp` |
| Tool identity | `provenance_events[n].tool.name` + `.version` |
| Input scope | `provenance_events[n].scope` |
| Outcome | `provenance_events[n].status` (`completed` / `failed` / `partial`) |

---

### Pattern D: Tool writes object lineage to OFFF

**Use case:** A tool that extracts objects from other objects (unarchiver, email attachment
extractor, parser, OCR) records the derivation relationship in OFFF.

**How it works:**
1. For each derived object produced, the tool writes a row to `indexes/derivations.parquet`.
2. Each row records the source object identifier, derived object identifier, derivation method,
   and tool identity.
3. The object index is updated with the new derived objects.

**OFFF contracts to implement:**
- Comply with `OFFF Object-Lineage Conformant` profile.
- Do not use dangling references (source object must be in the object index).
- Record the derivation method clearly (`zip_extraction`, `email_attachment`, `ocr_text`, etc.).

**derivation record mapping example (v0.2 schema):**

| Tool concept | OFFF field |
|---|---|
| Parent file | `derivations.source_object_id` |
| Extracted child file | `derivations.derived_object_id` |
| Extraction type | `derivations.method` |
| Tool that performed extraction | `derivations.tool_name` + `derivations.tool_version` |

---

### Pattern E: Tool exports report package with OFFF verifier report

**Use case:** A reporting or disclosure tool produces a package that includes the OFFF
verification report alongside case findings and metadata.

**How it works:**
1. The reporting tool calls `offf-verify` with `--report-json` and `--report-md` flags.
2. It includes the resulting verification report in the package.
3. The report confirms which conformance profiles the container passed and what the
   verification status is.

**OFFF contracts to implement:**
- Generate the container verification report using `offf-verify`.
- Include the report alongside the disclosed materials.
- Do not modify the OFFF container as part of the reporting step.

**Verification report inclusion example:**

```bash
cargo run -p offf-verify -- case.offf \
  --profile forensic-baseline \
  --report-json case-verification.json \
  --report-md  case-verification.md

# Include case-verification.json and case-verification.md in the disclosure package.
```

---

## Tool Category Integration Notes

### Hansken-like Platforms

A Hansken-like forensic platform (centralized analysis platform with plugin architecture)
can integrate with OFFF by:
- Implementing Pattern B to read evidence from OFFF containers.
- Implementing Pattern C via its plugin workers to write analysis results to OFFF.
- Implementing Pattern D via its extraction plugins for object lineage.

The platform's own case management and UI remain unchanged. OFFF provides the
interoperability layer for evidence and results.

### FTK Lab-like Platforms

An FTK-like platform can:
- Export a case in Pattern A format (raw image or file collection) for OFFF ingestion.
- Export analysis results in Pattern C format by writing to OFFF analysis layer.

Direct OFFF reading by the platform depends on whether the platform supports pluggable
evidence sources.

### Cellebrite / GrayKey-like Extraction Tools

A mobile extraction tool can:
- Produce an OFFF container via Pattern A using its export function.
- Populate `acquisition.json` with device identity, extraction mode, and tool version.
- Use `acquisition_mode = logical_extraction` or `file_collection` as appropriate.

The extraction tool does not need to implement the full OFFF stack — only the acquisition
contract.

### Tika / OCR-like Enrichment Tools

A text extraction or OCR tool can:
- Implement Pattern C to write extracted text as analysis results.
- Implement Pattern D if it also extracts embedded objects (e.g., images from documents).

The tool reads an object from the OFFF container (via Pattern B), processes it, and writes
the result back via Pattern C.

### AI Classifier Workers

An AI classifier (document classifier, image hash matcher, entity extractor) can:
- Implement Pattern C to write classification results as analysis output.
- Use `LabelEvent` records in the extension layer for object-level labels.

Classification results are analysis output, not evidence. They reside in the analysis layer
and are integrity-protected by the result manifest.

---

## Conformance Profile Reference

| Pattern | Minimum conformance profile |
|---|---|
| A (evidence producer) | OFFF Acquisition Conformant |
| B (reader) | OFFF Reader Conformant |
| C (analysis writer) | OFFF Analysis Worker Conformant |
| D (lineage writer) | OFFF Object-Lineage Conformant |
| E (report packager) | No OFFF contract — uses offf-verify output |

See `docs/conformance-profiles.md` for the full profile definitions.

---

## Related Documents

- `docs/conformance-profiles.md` — formal profile definitions and requirements
- `docs/evidence-root-model.md` — acquisition mode details for Pattern A
- `docs/chain-of-evidence.md` — technical integrity chain for Patterns B–D
- `docs/legal-neutrality.md` — OFFF does not replace legal or forensic expert judgment

*Last updated: 2026-05-29*
