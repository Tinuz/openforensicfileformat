# Ontwikkelinstructie 2: Aanpassing Analysis Worker Contract voor object-producing workers

## Doel

Pas het bestaande tool-agnostische **Analysis Worker Contract** aan zodat workers niet alleen analyse-resultaten kunnen produceren, maar ook nieuwe afgeleide objecten, objectrelaties en derivation records.

Hiermee kunnen dezelfde generieke workercontracten worden gebruikt voor:

```text
documentanalyse
OCR
hash matching
keyword search
YARA
AI-classificatie
container parsing
mailbox parsing
message parsing
attachment extraction
embedded object extraction
database record extraction
```

De aanpassing moet voorkomen dat OFFF voor elke tool of toepassing apart moet worden uitgebreid.

---

## 1. Nieuwe kernregel

Het Analysis Worker Contract moet twee outputvormen ondersteunen:

```text
1. Result-producing workers
   input object → analysis result rows

2. Object-producing workers
   input object → child objects + object edges + derivations
```

Veel workers doen beide.

Voorbeeld:

```text
Een parser worker kan:
- child objects ontdekken
- object bytes materialiseren
- metadata als analysis result schrijven
- parser errors vastleggen
```

---

## 2. Contract uitbreiden met Worker Output Types

### 2.1 Oude benadering

```text
worker output = results + errors + provenance
```

### 2.2 Nieuwe benadering

```text
worker output =
  analysis_results
  discovered_objects
  object_edges
  derivations
  materialized_objects
  errors
  audit_events
  provenance_events
```

---

## 3. Job Manifest uitbreiden

### 3.1 Nieuwe velden

Breid het generieke job manifest uit met:

```json
{
  "job_id": "job-000001",
  "task": "parse_or_analyze",
  "worker": {
    "tool_id": "generic-worker",
    "name": "Generic Worker",
    "version": "0.1.0"
  },
  "input_scope": {
    "target_types": ["file", "artifact", "object"],
    "selectors": {
      "object_ids": ["obj-file-000123"],
      "file_ids": [],
      "artifact_ids": []
    },
    "exclude": {
      "labels": ["restricted", "excluded"],
      "sets": ["excl-000001"]
    }
  },
  "output_contract": {
    "may_produce_results": true,
    "may_produce_objects": true,
    "may_materialize_objects": true,
    "may_produce_edges": true,
    "may_produce_derivations": true
  },
  "parameters": {
    "tool_specific": "payload"
  }
}
```

### 3.2 Regels

```text
- Core valideert output_contract generiek.
- Core interpreteert parameters niet inhoudelijk.
- Access Service controleert capabilities tegen output_contract.
- Worker mag alleen outputtypes produceren waarvoor hij geautoriseerd is.
```

### Acceptance criteria

```text
[ ] Job manifest ondersteunt output_contract.
[ ] Workers zonder may_produce_objects mogen geen objects/edges/derivations schrijven.
[ ] Access Service controleert output_contract tegen tool registry capabilities.
```

---

## 4. AnalysisInputObject uitbreiden

### 4.1 Ondersteunde input types

Voeg toe of formaliseer:

```text
chunk
chunk_range
filesystem_file
object
derived_object
artifact
analysis_result
```

### 4.2 Generiek inputobject

```json
{
  "input_id": "input-000001",
  "type": "object",
  "id": "obj-file-000123",
  "source_refs": {
    "chunk_refs": ["sha256:abc..."],
    "physical_extents": [
      {
        "offset": 123456,
        "length": 7890
      }
    ],
    "parent_object_id": null,
    "derivation_id": null
  },
  "metadata": {
    "name": "container-or-document",
    "media_type": "application/octet-stream",
    "size_bytes": 123456
  }
}
```

### Acceptance criteria

```text
[ ] Input object kan root evidence of derived object representeren.
[ ] Input object bevat genoeg source refs voor lineage.
[ ] read_input_object_verified werkt voor file/object/materialized derived object.
```

---

## 5. Result Manifest uitbreiden

### 5.1 Nieuwe structuur

Het result manifest moet alle outputcategorieën beschrijven.

```json
{
  "job_id": "job-000001",
  "task": "parse_or_analyze",
  "status": "completed",
  "worker": {
    "tool_id": "generic-worker",
    "name": "Generic Worker",
    "version": "0.1.0",
    "binary_sha256": "sha256:..."
  },
  "input": {
    "container_id": "urn:offf:case:2026-001",
    "source_sha256": "sha256:...",
    "merkle_root_sha256": "sha256:...",
    "input_object_ids": ["obj-file-000123"],
    "scope_ref": "scope-000001"
  },
  "outputs": {
    "analysis_results": [
      {
        "path": "analysis/jobs/job-000001/results.jsonl",
        "sha256": "sha256:...",
        "schema_ref": "schema:offf-analysis-result-envelope-0.2.0"
      }
    ],
    "discovered_objects": {
      "path": "analysis/jobs/job-000001/objects.parquet",
      "sha256": "sha256:...",
      "schema_ref": "schema:offf-object-index-row-0.2.0"
    },
    "object_edges": {
      "path": "analysis/jobs/job-000001/object_edges.parquet",
      "sha256": "sha256:...",
      "schema_ref": "schema:offf-object-edge-row-0.2.0"
    },
    "derivations": {
      "path": "analysis/jobs/job-000001/derivations.parquet",
      "sha256": "sha256:...",
      "schema_ref": "schema:offf-derivation-row-0.2.0"
    },
    "materialized_objects": [
      {
        "object_id": "obj-derived-000001",
        "storage_ref": "derived/objects/sha256/ab/cd/<hash>.bin",
        "sha256": "sha256:..."
      }
    ],
    "errors": {
      "path": "analysis/jobs/job-000001/errors.jsonl",
      "sha256": "sha256:...",
      "schema_ref": "schema:offf-analysis-error-0.2.0"
    }
  },
  "statistics": {
    "inputs_in_scope": 1,
    "inputs_processed": 1,
    "objects_discovered": 5,
    "objects_materialized": 3,
    "results_written": 10,
    "errors": 0
  }
}
```

### Acceptance criteria

```text
[ ] Result manifest ondersteunt alle outputcategorieën.
[ ] Ontbrekende categorieën mogen null/empty zijn.
[ ] Alle outputbestanden hebben sha256.
[ ] result_manifest.json wordt als laatste geschreven.
```

---

## 6. Output directory uitbreiden

### 6.1 Standaardstructuur

Alle workers gebruiken:

```text
analysis/
  jobs/
    {job_id}/
      job_manifest.json
      result_manifest.json
      results.jsonl
      errors.jsonl
      objects.parquet
      object_edges.parquet
      derivations.parquet
      worker_log.jsonl
      artifacts/
```

Materialized objects staan niet onder de jobdirectory, maar content-addressed in:

```text
derived/
  objects/
    sha256/
      ab/
        cd/
          <sha256>.bin
```

De job verwijst daarnaar vanuit `result_manifest.json`.

---

### 6.2 Regels

```text
- Jobdirectory is append-only.
- Worker schrijft eerst naar staging.
- result_manifest.json wordt als laatste geschreven.
- Materialized objects worden content-addressed opgeslagen.
- Bestaande materialized objecten worden geverifieerd voor hergebruik.
```

---

## 7. Nieuwe generieke rowmodellen

### 7.1 DiscoveredObjectRow

```json
{
  "object_id": "obj-derived-000001",
  "object_type": "embedded_object",
  "name": "child-object-name",
  "logical_path": "folder/object-name",
  "media_type": "application/octet-stream",
  "size_bytes": 123456,
  "sha256": "sha256:...",
  "source_layer": "derived_object",
  "storage_ref": "derived/objects/sha256/ab/cd/<hash>.bin",
  "created_by_job_id": "job-000001",
  "parser_status": "success",
  "provenance_ref": "evt-000001",
  "schema_version": "0.2.0"
}
```

### 7.2 ObjectEdgeRow

```json
{
  "edge_id": "edge-000001",
  "parent_object_id": "obj-parent-000001",
  "child_object_id": "obj-derived-000001",
  "relation_type": "contains",
  "method": "container_member_extraction",
  "logical_path": "folder/object-name",
  "sequence": 1,
  "created_by_job_id": "job-000001",
  "provenance_ref": "evt-000001",
  "schema_version": "0.2.0"
}
```

### 7.3 DerivationRow

```json
{
  "derivation_id": "drv-000001",
  "parent_object_id": "obj-parent-000001",
  "child_object_id": "obj-derived-000001",
  "job_id": "job-000001",
  "method": "container_member_extraction",
  "tool_id": "generic-worker",
  "tool_name": "Generic Worker",
  "tool_version": "0.1.0",
  "parameters_hash": "sha256:...",
  "input_sha256": "sha256:...",
  "output_sha256": "sha256:...",
  "storage_mode": "materialized",
  "provenance_ref": "evt-000001",
  "created_at": "2026-05-24T10:00:00Z",
  "schema_version": "0.2.0"
}
```

---

## 8. SDK-aanpassingen

### 8.1 AnalysisWorkerContext uitbreiden

Voeg functies toe:

```rust
context.write_result_row(row)
context.write_error_row(error)

context.write_discovered_object(object_row)
context.write_object_edge(edge_row)
context.write_derivation(derivation_row)
context.materialize_object(object_id, bytes)

context.commit_result_manifest()
```

### 8.2 Verified input reads

Voeg functies toe:

```rust
context.read_input_verified(input_id)
context.read_object_verified(object_id)
context.read_parent_object_verified(object_id)
context.compute_object_sha256(object_id)
```

### 8.3 Acceptance criteria

```text
[ ] SDK voorkomt schrijven buiten jobdirectory.
[ ] SDK materialiseert objecten content-addressed.
[ ] SDK berekent hashes automatisch.
[ ] SDK schrijft object/edge/derivation deltas.
[ ] SDK commit result_manifest als laatste.
```

---

## 9. Access Service-aanpassingen

### 9.1 Nieuwe generieke endpoints

```text
POST /cases/{caseId}/analysis/jobs/{jobId}/objects
POST /cases/{caseId}/analysis/jobs/{jobId}/object-edges
POST /cases/{caseId}/analysis/jobs/{jobId}/derivations
POST /cases/{caseId}/analysis/jobs/{jobId}/materialized-objects

GET /cases/{caseId}/objects/{objectId}
GET /cases/{caseId}/objects/{objectId}/content
GET /cases/{caseId}/objects/{objectId}/lineage
```

### 9.2 Regels

```text
- Alleen workers met juiste capability mogen object outputs schrijven.
- Writes zijn append-only.
- Directe mutatie van centrale object indexes is niet toegestaan.
- Centrale indexes worden gebouwd uit job deltas.
```

### 9.3 Acceptance criteria

```text
[ ] Access Service accepteert object-producing outputs via jobcontext.
[ ] Access Service weigert directe writes naar indexes/objects.
[ ] Access Service valideert schema’s voor objects/edges/derivations.
[ ] Access Service logt denied attempts.
```

---

## 10. Tool registry uitbreiden

### 10.1 Nieuwe capabilities

Voeg capabilities toe:

```text
analysis:produce_results
analysis:produce_objects
analysis:produce_edges
analysis:produce_derivations
analysis:materialize_objects
analysis:read_objects
analysis:read_files
analysis:read_chunks
provenance:append
audit:append
```

### 10.2 Voorbeeld

```json
{
  "tool_id": "generic-container-parser",
  "status": "approved",
  "allowed_roles": ["analysis_worker"],
  "capabilities": [
    "analysis:read_objects",
    "analysis:produce_objects",
    "analysis:produce_edges",
    "analysis:produce_derivations",
    "analysis:materialize_objects",
    "provenance:append",
    "audit:append"
  ],
  "supported_input_types": ["file", "object", "derived_object"],
  "supported_output_types": ["object", "edge", "derivation", "error"],
  "supported_offf_versions": ["0.2.0"],
  "binary_sha256": "sha256:...",
  "container_image_digest": "sha256:..."
}
```

### Acceptance criteria

```text
[ ] Capabilities worden gecontroleerd per write.
[ ] Job output_contract moet passen bij tool capabilities.
[ ] Niet-goedgekeurde object-producing workers worden geblokkeerd.
```

---

## 11. Verifier-aanpassingen

### 11.1 Generic analysis validation

Controleer:

```text
- result_manifest schema
- output artifact hashes
- objects.parquet schema
- object_edges.parquet schema
- derivations.parquet schema
- materialized object hashes
- parent/child references
- provenance refs
```

### 11.2 Lineage validation

Nieuwe optie:

```bash
offf-verify case.offf --object obj-... --lineage
```

Controleer:

```text
- object bestaat
- parent chain bestaat
- derivation chain bestaat
- root evidence bestaat
- chunk refs valideren
- materialized object hash klopt
- provenance refs bestaan
```

### Acceptance criteria

```text
[ ] Verifier kan result-producing jobs valideren.
[ ] Verifier kan object-producing jobs valideren.
[ ] Verifier kan lineage tot root evidence volgen.
[ ] Verifier detecteert ontbrekende parent, edge, derivation of hash mismatch.
```

---

## 12. Migratie bestaande workers

### 12.1 Keyword worker

Keyword worker is vooral result-producing.

Aanpassen naar:

```text
analysis/jobs/{job_id}/keyword_hits.parquet
analysis/jobs/{job_id}/errors.jsonl
analysis/jobs/{job_id}/result_manifest.json
```

### 12.2 YARA worker

YARA worker is vooral result-producing.

Aanpassen naar:

```text
analysis/jobs/{job_id}/yara_hits.parquet
analysis/jobs/{job_id}/errors.jsonl
analysis/jobs/{job_id}/result_manifest.json
```

### 12.3 Parserachtige workers

Nieuwe parserachtige workers zijn object-producing.

Ze moeten minimaal schrijven:

```text
objects.parquet
object_edges.parquet
derivations.parquet
errors.jsonl
result_manifest.json
```

---

## 13. Backward compatibility

### Regels

```text
- Bestaande v0.1 jobs blijven leesbaar.
- v0.2 jobs gebruiken nieuw output_contract.
- v0.1 workers mogen blijven draaien, maar krijgen legacy status.
- Verifier moet legacy analysis output kunnen rapporteren als warning.
```

### Acceptance criteria

```text
[ ] v0.1 keyword/YARA outputs blijven leesbaar.
[ ] Nieuwe v0.2 workers gebruiken jobdirectory + result_manifest.
[ ] Legacy output wordt niet automatisch als forensic-grade v0.2 beschouwd.
```

---

## 14. Teststrategie

### 14.1 Unit tests

```text
AnalysisJobManifest met output_contract
Capability check
Object row validation
Edge row validation
Derivation row validation
Materialized object hashing
Result manifest hashing
```

### 14.2 Integration tests

Maak synthetische nested testobjecten:

```text
root file
→ child object
→ grandchild object
→ analysis result
```

Test:

```text
- worker produceert objects/edges/derivations
- index rebuild werkt
- lineage verifier werkt
- missing parent faalt
- hash mismatch faalt
```

### 14.3 Acceptance criteria

```text
[ ] Tests draaien in CI.
[ ] Negative graph tests zijn aanwezig.
[ ] Object-producing worker mock is aanwezig.
[ ] Verifier faalt voorspelbaar bij corrupte lineage.
```

---

## 15. Implementatievolgorde

### P0

```text
[ ] output_contract in AnalysisJobManifest
[ ] DiscoveredObjectRow
[ ] ObjectEdgeRow
[ ] DerivationRow
[ ] materialized object store
[ ] result_manifest uitbreiden
[ ] SDK schrijffuncties voor objects/edges/derivations
[ ] verifier schema-validatie voor object-producing outputs
```

### P1

```text
[ ] Access Service endpoints
[ ] Tool registry capabilities
[ ] off-verify --object --lineage
[ ] off-index objects rebuild
[ ] migratie keyword/YARA naar result_manifest
```

### P2

```text
[ ] streaming reads voor grote derived objects
[ ] object graph query API
[ ] lineage reports
[ ] graph export
[ ] performance tuning voor miljoenen objecten
```

---

## 16. Definitie van klaar

Het aangepaste Analysis Worker Contract is klaar als:

```text
[ ] Workers tool-agnostisch results kunnen produceren.
[ ] Workers tool-agnostisch child objects kunnen produceren.
[ ] Workers object_edges en derivations kunnen produceren.
[ ] Workers derived object bytes kunnen materialiseren.
[ ] Result manifest alle outputcategorieën beschrijft.
[ ] Access Service en SDK hetzelfde contract ondersteunen.
[ ] Verifier object-producing jobs en lineage kan valideren.
[ ] Nieuwe parser/analyse-tools geen Core-aanpassing nodig hebben.
```

---

## 17. Kernzin

```text
Het Analysis Worker Contract moet niet alleen analyse-uitkomsten ondersteunen,
maar ook het ontdekken en vastleggen van nieuwe afgeleide objecten, hun relaties
en hun derivation chain. Daardoor kan OFFF nested evidence tool-agnostisch
representeren zonder per bestandsformaat aparte Core-logica.
```
