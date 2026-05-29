# AI Coding Agent Instructie — OFFF ondersteuning voor parallelle verwerking, tool-agnostisch

## Rol

Je bent een senior software engineer en werkt aan de repository voor **Open Forensic File Format (OFFF)**.

Je opdracht is om OFFF uit te breiden zodat parallelle verwerking door externe workers mogelijk wordt, zonder dat OFFF zelf een scheduler, queue, orchestrator of runtime-platform wordt.

Gebruik **Claude Sonnet 4.6** als coding agent en voer deze opdracht gefaseerd uit.

---

## 1. Context

OFFF moet evidence en afgeleide analyse zodanig modelleren dat meerdere workers parallel kunnen werken op dezelfde OFFF-container.

Voorbeelden van parallelle verwerking:

```text
- meerdere text extraction workers verwerken verschillende documenten
- meerdere OCR workers verwerken verschillende afbeeldingen
- meerdere YARA workers scannen verschillende chunks of objecten
- meerdere parser workers verwerken verschillende containers of mailboxen
- meerdere indexing workers indexeren verschillende output shards
- meerdere AI/ML workers verwerken verschillende objectsets
```

Belangrijk:

```text
OFFF moet parallelle verwerking mogelijk maken.
OFFF moet parallelle verwerking niet zelf orkestreren.
```

De scheduler, queue, shard allocator of workload manager draait **bovenop OFFF**.

Voorbeelden van bovenliggende orkestratie:

```text
Docker Compose
Kubernetes Jobs
Argo Workflows
Airflow
Celery
RabbitMQ
Nomad
custom scheduler
forensic lab platform
```

OFFF Core mag daar niet afhankelijk van zijn.

---

## 2. Kernprincipe

De belangrijkste ontwerpregel:

```text
OFFF standaardiseert de werkpakketten en bewijslast.
De scheduler organiseert de uitvoering.
```

Of korter:

```text
OFFF definieert het contract.
De scheduler voert het contract uit.
```

Daarom moet OFFF Core geen scheduler bevatten, maar wel:

```text
- job manifests
- shard manifests
- inputobjecten
- scope-resolutie
- outputcontracten
- result manifests
- coverage reports
- provenance events
- audit events
- validatieprofielen
```

---

## 3. Wat hoort niet in OFFF Core?

Voeg de volgende zaken **niet** toe aan OFFF Core:

```text
[ ] queue engine
[ ] worker autoscaling
[ ] Kubernetes-specifieke logica
[ ] RabbitMQ/Celery-specifieke logica
[ ] retry engine als runtimefunctie
[ ] worker health monitoring
[ ] load balancing
[ ] scheduling policy
[ ] resource placement
[ ] node selection
[ ] Tika-specifieke parallelisatie
[ ] Elasticsearch bulk scheduling
[ ] AI/ML runtime orchestration
```

Deze functies horen in een externe scheduler/orchestrator of in een demo-/platformlaag bovenop OFFF.

OFFF mag wel een **referentieplanner** of **demo shard allocator** bevatten, maar die moet expliciet niet-normatief zijn.

---

## 4. Wat moet OFFF Core wel ondersteunen?

OFFF Core moet de formele contracten en validatiemechanismen bevatten waarmee externe schedulers en workers veilig parallel kunnen werken.

Toevoegen of formaliseren:

```text
AnalysisJobManifest
AnalysisInputObject
AnalysisScope
ScopeResolver
ShardPlan
ShardManifest
ShardResultManifest
ParentResultManifest
AnalysisCoverageReport
WorkerProvenanceEvent
WorkerAuditEvent
VerifiedInputRead
AppendOnlyJobOutput
ParallelJobValidator
```

---

## 5. Nieuwe concepten

## 5.1 Parent Analysis Job

Een parent job beschrijft de volledige analyseopdracht.

Voorbeeld:

```json
{
  "job_id": "job-text-001",
  "task": "extract_text",
  "case_id": "urn:offf:case:demo-001",
  "input_scope": {
    "target_types": ["object"],
    "include": {
      "object_types": ["evidence_file", "derived_object"],
      "extensions": ["docx", "pdf", "txt"]
    },
    "exclude": {
      "labels": ["restricted", "excluded"]
    },
    "limits": {
      "max_object_size_bytes": 104857600
    }
  },
  "parallelization": {
    "enabled": true,
    "mode": "sharded",
    "shard_strategy": "deterministic_object_id_range",
    "shard_count": 4
  },
  "output_contract": {
    "base_path": "analysis/jobs/job-text-001",
    "write_mode": "job_sharded_append_only",
    "requires_result_manifest": true,
    "requires_provenance": true,
    "requires_coverage_report": true
  }
}
```

### Regels

```text
- Parent job beschrijft de totale scope.
- Parent job start geen workers.
- Parent job bevat geen runtime-specifieke schedulinginformatie.
- Parent job mag parallelization metadata bevatten.
- Parent job wordt gebruikt door externe scheduler/planner.
```

---

## 5.2 Analysis Input Object

Een input object is een verwerkbaar object binnen OFFF.

Ondersteun minimaal:

```text
chunk
chunk_range
file
evidence_file
derived_object
artifact
analysis_result
```

Voorbeeld:

```json
{
  "input_id": "input-000001",
  "type": "object",
  "id": "obj-file-000001",
  "source_refs": {
    "root_id": "root-collection-001",
    "sha256": "sha256:...",
    "storage_ref": "evidence/objects/sha256/ab/cd/<hash>.bin"
  },
  "metadata": {
    "name": "contract.docx",
    "extension": "docx",
    "size_bytes": 183422
  }
}
```

### Acceptance criteria

```text
[ ] Elk inputobject heeft stabiele ID.
[ ] Inputobjecten kunnen uit block_image, file_collection en derived_object komen.
[ ] Inputobjecten zijn deterministisch sorteerbaar.
[ ] Inputobjecten zijn geschikt om in shards te worden verdeeld.
```

---

## 5.3 ScopeResolver

### Doel

De ScopeResolver vertaalt een parent job scope naar een deterministische lijst inputobjecten.

Functie:

```rust
resolve_analysis_scope(
    container: &ContainerRef,
    job: &AnalysisJobManifest
) -> Result<Vec<AnalysisInputObject>, OfffError>;
```

### ScopeResolver ondersteunt

```text
object_id selectie
object_type selectie
file_id selectie
chunk selectie
artifact selectie
extension filter
mime type filter
size limits
date filters
include labels
exclude labels
include sets
exclude sets
parser_status
content_state
```

### Regels

```text
- ScopeResolver is tool-agnostisch.
- ScopeResolver voert geen analyse uit.
- ScopeResolver produceert alleen inputobjecten.
- Output is deterministisch gesorteerd.
```

Aanbevolen sortering:

```text
1. root_id
2. source_layer
3. logical_path
4. object_id
```

Of eenvoudiger:

```text
object_id ascending
```

Kies één standaard en documenteer deze.

### Acceptance criteria

```text
[ ] Dezelfde scope levert altijd dezelfde inputlijst op.
[ ] Exclude labels/sets worden toegepast.
[ ] Out-of-scope objecten worden niet in shard manifests opgenomen.
[ ] ScopeResolver werkt voor file_collection en block_image-objecten.
```

---

## 5.4 ShardPlan

### Doel

Een ShardPlan beschrijft hoe een inputlijst wordt verdeeld in meerdere shards.

Dit is géén scheduler. Het is een deterministisch plan.

Voorbeeld:

```json
{
  "parent_job_id": "job-text-001",
  "shard_plan_id": "shardplan-job-text-001",
  "strategy": "deterministic_object_id_range",
  "shard_count": 4,
  "input_count": 10000,
  "input_scope_hash": "sha256:...",
  "created_at": "2026-05-28T10:00:00Z",
  "created_by": "offf-shard-planner"
}
```

### Shard strategies

Ondersteun minimaal:

```text
deterministic_object_id_range
deterministic_round_robin
deterministic_hash_modulo
```

Aanbevolen startstrategie:

```text
deterministic_object_id_range
```

### Acceptance criteria

```text
[ ] ShardPlan is deterministisch.
[ ] ShardPlan bevat input_scope_hash.
[ ] ShardPlan bevat strategy en shard_count.
[ ] ShardPlan bevat geen runtime placement informatie.
```

---

## 5.5 ShardManifest

### Doel

Een ShardManifest beschrijft de subset die één worker mag verwerken.

Voorbeeld:

```json
{
  "shard_id": "job-text-001-shard-02",
  "parent_job_id": "job-text-001",
  "shard_index": 2,
  "shard_count": 4,
  "input_scope_hash": "sha256:...",
  "input_objects": [
    {
      "input_id": "input-000251",
      "type": "object",
      "id": "obj-file-000251"
    },
    {
      "input_id": "input-000252",
      "type": "object",
      "id": "obj-file-000252"
    }
  ],
  "output": {
    "base_path": "analysis/jobs/job-text-001/shards/shard-02"
  },
  "status": "planned"
}
```

### Regels

```text
- ShardManifest bevat alleen inputobjecten binnen parent scope.
- ShardManifest is read-only input voor worker.
- Worker mag alleen inputobjecten uit zijn shard verwerken.
- Worker mag alleen schrijven naar zijn eigen shard output path.
```

### Acceptance criteria

```text
[ ] ShardManifest schema bestaat.
[ ] ShardManifest verwijst naar parent_job_id.
[ ] ShardManifest bevat shard_index en shard_count.
[ ] ShardManifest output base_path is uniek per shard.
[ ] Validator detecteert dubbele inputobjecten binnen één shard.
```

---

## 5.6 ShardResultManifest

### Doel

Elke worker/shard schrijft een result manifest als finalisatiepunt.

Voorbeeld:

```json
{
  "job_id": "job-text-001-shard-02",
  "parent_job_id": "job-text-001",
  "shard_id": "job-text-001-shard-02",
  "status": "completed",
  "worker": {
    "tool_id": "generic-text-worker",
    "name": "Generic Text Worker",
    "version": "0.1.0",
    "binary_sha256": "sha256:..."
  },
  "input": {
    "input_scope_hash": "sha256:...",
    "objects_in_shard": 2500
  },
  "outputs": [
    {
      "path": "analysis/jobs/job-text-001/shards/shard-02/results.jsonl",
      "sha256": "sha256:...",
      "schema_ref": "schema:offf-analysis-result-envelope-0.2.0"
    },
    {
      "path": "analysis/jobs/job-text-001/shards/shard-02/errors.jsonl",
      "sha256": "sha256:...",
      "schema_ref": "schema:offf-analysis-error-0.2.0"
    }
  ],
  "statistics": {
    "objects_in_scope": 2500,
    "objects_processed": 2490,
    "objects_success": 2480,
    "objects_error": 5,
    "objects_skipped": 5
  },
  "created_at": "2026-05-28T10:20:00Z",
  "completed_at": "2026-05-28T10:35:00Z"
}
```

### Regels

```text
- ShardResultManifest wordt als laatste geschreven.
- Alle output artifacts hebben SHA-256.
- ShardResultManifest is het commit point van de shard.
- Geen result manifest betekent: shard niet voltooid.
```

### Acceptance criteria

```text
[ ] ShardResultManifest schema bestaat.
[ ] Output hashes worden gevalideerd.
[ ] Ontbrekende ShardResultManifest betekent incomplete shard.
[ ] Worker schrijft geen output buiten shard directory.
```

---

## 5.7 ParentResultManifest

### Doel

Na afloop verwijst het parent result manifest naar alle shard results.

Dit manifest mag worden geschreven door een externe finalizer. OFFF Core moet alleen schema en validatie ondersteunen.

Voorbeeld:

```json
{
  "job_id": "job-text-001",
  "status": "completed",
  "parallelization": {
    "mode": "sharded",
    "shard_count": 4,
    "shards_completed": 4,
    "shards_failed": 0
  },
  "shard_results": [
    {
      "shard_id": "job-text-001-shard-01",
      "result_manifest_path": "analysis/jobs/job-text-001/shards/shard-01/shard_result_manifest.json",
      "sha256": "sha256:..."
    },
    {
      "shard_id": "job-text-001-shard-02",
      "result_manifest_path": "analysis/jobs/job-text-001/shards/shard-02/shard_result_manifest.json",
      "sha256": "sha256:..."
    }
  ],
  "coverage": {
    "objects_in_scope": 10000,
    "objects_processed": 9980,
    "objects_success": 9900,
    "objects_error": 50,
    "objects_skipped": 30,
    "duplicates_detected": 0,
    "missing_inputs": 0
  },
  "created_at": "2026-05-28T10:40:00Z"
}
```

### Acceptance criteria

```text
[ ] ParentResultManifest schema bestaat.
[ ] ParentResultManifest verwijst naar alle shard manifests.
[ ] ParentResultManifest bevat coverage.
[ ] Validator detecteert ontbrekende shard manifests.
[ ] Validator detecteert hash mismatch in shard manifests.
```

---

## 6. Outputstructuur

Gebruik standaard:

```text
analysis/
  jobs/
    job-text-001/
      job_manifest.json
      shard_plan.json
      parent_result_manifest.json
      shards/
        shard-01/
          shard_manifest.json
          results.jsonl
          errors.jsonl
          skipped.jsonl
          shard_result_manifest.json
        shard-02/
          shard_manifest.json
          results.jsonl
          errors.jsonl
          skipped.jsonl
          shard_result_manifest.json
```

### Regels

```text
- Iedere shard schrijft naar eigen directory.
- Geen twee workers schrijven naar hetzelfde outputbestand.
- Parent manifest wordt pas geschreven na shardvalidatie.
- Centrale indexen worden later gebouwd uit shard outputs, niet direct parallel gemuteerd.
```

---

## 7. Append-only en atomic commit

## 7.1 Staging

Workers schrijven eerst naar staging:

```text
analysis/jobs/{parent_job_id}/shards/{shard_id}.tmp/
```

Na succesvolle verwerking:

```text
1. schrijf outputs
2. bereken hashes
3. schrijf shard_result_manifest.json
4. rename tmp → definitieve sharddirectory
```

Voor object storage:

```text
1. schrijf onder staging prefix
2. schrijf output hashes
3. schrijf shard_result_manifest.json
4. schrijf commit marker _OFFF_SHARD_COMPLETE
```

### Acceptance criteria

```text
[ ] Incomplete shard heeft geen geldig result manifest.
[ ] Shard commit is atomair waar mogelijk.
[ ] Object storage gebruikt commit marker.
[ ] Verifier negeert incomplete staging directories.
```

---

## 8. Coverage validation

## 8.1 Doel

OFFF moet kunnen aantonen welke inputobjecten verwerkt zijn.

Coverage controleert:

```text
- hoeveel objecten zaten in parent scope
- welke objecten zaten in welke shard
- welke objecten zijn succesvol verwerkt
- welke objecten zijn skipped
- welke objecten gaven errors
- zijn er duplicaten
- ontbreken er inputobjecten
```

### CoverageReport

```json
{
  "parent_job_id": "job-text-001",
  "input_scope_hash": "sha256:...",
  "objects_in_scope": 10000,
  "objects_assigned_to_shards": 10000,
  "objects_processed": 9980,
  "objects_success": 9900,
  "objects_error": 50,
  "objects_skipped": 30,
  "duplicates_detected": 0,
  "missing_inputs": 0
}
```

### Acceptance criteria

```text
[ ] Validator kan coverage report berekenen.
[ ] Duplicate input in meerdere shards wordt gedetecteerd.
[ ] Missing inputobjecten worden gedetecteerd.
[ ] Skipped en errors tellen mee in coverage.
[ ] Coverage wordt opgenomen in ParentResultManifest.
```

---

## 9. Error en skipped model

## 9.1 Error rows

Gebruik generiek error model:

```json
{
  "error_id": "error-000001",
  "parent_job_id": "job-text-001",
  "shard_id": "job-text-001-shard-02",
  "target": {
    "type": "object",
    "id": "obj-file-000251"
  },
  "status": "error",
  "error_code": "TOOL_PARSE_FAILED",
  "message": "Worker failed to process object.",
  "recoverable": true,
  "created_at": "2026-05-28T10:30:00Z"
}
```

## 9.2 Skipped rows

Gebruik apart skipped model:

```json
{
  "skipped_id": "skip-000001",
  "parent_job_id": "job-text-001",
  "shard_id": "job-text-001-shard-02",
  "target": {
    "type": "object",
    "id": "obj-file-000252"
  },
  "status": "skipped",
  "reason_code": "EXCLUDED_BY_LABEL",
  "message": "Object excluded by job scope.",
  "created_at": "2026-05-28T10:30:00Z"
}
```

## 9.3 Standaard errorcodes

Ondersteun minimaal:

```text
INPUT_NOT_FOUND
INPUT_OUT_OF_SCOPE
INPUT_TOO_LARGE
INPUT_UNREADABLE
CHUNK_VERIFICATION_FAILED
OBJECT_HASH_MISMATCH
UNSUPPORTED_INPUT_TYPE
TOOL_TIMEOUT
TOOL_PARSE_FAILED
TOOL_INTERNAL_ERROR
OUTPUT_WRITE_FAILED
OUTPUT_HASH_MISMATCH
SCOPE_RESOLUTION_FAILED
AUTHORIZATION_DENIED
```

## 9.4 Standaard skipped reason codes

Ondersteun minimaal:

```text
EXCLUDED_BY_LABEL
EXCLUDED_BY_SET
OUT_OF_SCOPE
TOO_LARGE
UNSUPPORTED_TYPE
DUPLICATE_INPUT
POLICY_DENIED
```

### Acceptance criteria

```text
[ ] Errors worden niet stilzwijgend genegeerd.
[ ] Skipped objecten worden niet stilzwijgend genegeerd.
[ ] Coverage telt errors en skipped mee.
[ ] Verifier valideert errors.jsonl en skipped.jsonl.
```

---

## 10. Provenance events

Voeg generieke provenance-events toe voor parallelle verwerking.

Minimaal:

```text
analysis_job_planned
analysis_shard_planned
analysis_shard_started
analysis_shard_completed
analysis_shard_failed
analysis_parent_job_finalized
analysis_output_committed
```

Voorbeeld:

```json
{
  "event_id": "evt-shard-02-completed",
  "timestamp": "2026-05-28T10:35:00Z",
  "actor": "worker:generic-text-worker-02",
  "action": "analysis_shard_completed",
  "tool": {
    "tool_id": "generic-text-worker",
    "version": "0.1.0"
  },
  "details": {
    "parent_job_id": "job-text-001",
    "shard_id": "job-text-001-shard-02",
    "result_manifest": "analysis/jobs/job-text-001/shards/shard-02/shard_result_manifest.json",
    "status": "completed"
  }
}
```

### Acceptance criteria

```text
[ ] Iedere shard completion heeft provenance event.
[ ] Failed shard heeft failure event.
[ ] Parent finalization heeft provenance event.
[ ] Verifier kan provenance refs controleren.
```

---

## 11. Audit events

Voeg audit-events toe voor:

```text
input_read_allowed
input_read_denied
input_skipped
output_write_attempt
output_write_denied
output_committed
scope_violation_detected
duplicate_input_detected
```

### Acceptance criteria

```text
[ ] Denied reads/writes worden vastgelegd.
[ ] Scope violations worden vastgelegd.
[ ] Audit events zijn append-only.
[ ] Verifier kan audit schema valideren.
```

---

## 12. Merkle proofs en verified input

Voor block-image based evidence moet parallelle verwerking uiteindelijk ook kunnen bewijzen dat een chunk/input hoort bij de source image.

Voeg toe of voorbereid:

```text
MerkleProof
MerkleProofRef
verify_merkle_proof()
```

Voor file_collection is object SHA-256 voldoende als root proof. Voor block_image moet input kunnen verwijzen naar:

```text
chunk_id
chunk_sequence
merkle_proof_ref
physical_offset
physical_length
```

### Acceptance criteria

```text
[ ] Inputobject kan source_refs bevatten voor file_collection en block_image.
[ ] Worker output kan verwijzen naar input hashes en Merkle refs.
[ ] Verifier kan file_collection objecthash controleren.
[ ] Verifier kan block_image chunk/hash controleren.
```

---

## 13. API/SDK-aanpassingen

## 13.1 Core functies

Voeg toe:

```rust
resolve_analysis_scope(...)
plan_shards(...)
validate_shard_manifest(...)
read_input_object_verified(...)
write_shard_artifact(...)
write_shard_result_manifest(...)
write_parent_result_manifest(...)
validate_parallel_job(...)
compute_input_scope_hash(...)
compute_coverage_report(...)
```

## 13.2 SDK helper

Maak een `AnalysisWorkerContext` die shard-aware is.

Voorbeeld API:

```rust
let ctx = AnalysisWorkerContext::open(case_path, shard_manifest_path)?;
let inputs = ctx.inputs();

for input in inputs {
    let bytes = ctx.read_input_verified(input)?;
    let result = run_tool(bytes)?;
    ctx.write_result_row(result)?;
}

ctx.write_errors_and_skipped()?;
ctx.commit_shard_result_manifest()?;
ctx.append_provenance_completed()?;
```

Python-equivalent:

```python
ctx = AnalysisWorkerContext.open(case_path, shard_manifest_path)

for input_obj in ctx.inputs:
    data = ctx.read_input_verified(input_obj)
    result = process(data)
    ctx.write_result(result)

ctx.commit()
```

### Acceptance criteria

```text
[ ] SDK voorkomt writes buiten shard directory.
[ ] SDK schrijft result manifest als laatste.
[ ] SDK berekent artifact hashes automatisch.
[ ] SDK ondersteunt file_collection en block_image inputs.
```

---

## 14. Access Service-aanpassingen

Access Service moet generiek blijven.

### Endpoints

```text
POST /cases/{caseId}/analysis/jobs
GET  /cases/{caseId}/analysis/jobs/{jobId}

POST /cases/{caseId}/analysis/jobs/{jobId}/shards
GET  /cases/{caseId}/analysis/jobs/{jobId}/shards/{shardId}

GET  /cases/{caseId}/analysis/jobs/{jobId}/shards/{shardId}/inputs/{inputId}/content

POST /cases/{caseId}/analysis/jobs/{jobId}/shards/{shardId}/artifacts
POST /cases/{caseId}/analysis/jobs/{jobId}/shards/{shardId}/result-manifest
POST /cases/{caseId}/analysis/jobs/{jobId}/parent-result-manifest
```

### Regels

```text
- Endpoints zijn tool-agnostisch.
- Geen schedulerlogica in Access Service.
- Access Service accepteert alleen geldige shard outputs.
- Access Service weigert writes buiten shard path.
```

### Acceptance criteria

```text
[ ] Access Service kan shard manifests opslaan/lezen.
[ ] Access Service levert verified input content.
[ ] Access Service accepteert shard artifacts append-only.
[ ] Access Service weigert output overwrite.
```

---

## 15. Verifier uitbreiden

## 15.1 Nieuwe CLI-opties

```bash
offf-verify case.offf --analysis-job job-text-001
offf-verify case.offf --parallel-job job-text-001
offf-verify case.offf --shard job-text-001-shard-02
offf-verify case.offf --coverage job-text-001
```

## 15.2 Controles voor parallel job

```text
[ ] parent job manifest bestaat
[ ] shard plan bestaat
[ ] alle shard manifests bestaan
[ ] alle shard result manifests bestaan of status verklaart waarom niet
[ ] alle output hashes kloppen
[ ] input_scope_hash klopt
[ ] alle inputobjecten zijn toegewezen
[ ] geen dubbele inputobjecten tenzij expliciet toegestaan
[ ] errors/skipped zijn valide
[ ] coverage klopt
[ ] provenance events bestaan
[ ] evidence layer is niet gewijzigd
```

### Acceptance criteria

```text
[ ] Verifier accepteert geldige parallel job.
[ ] Verifier faalt bij ontbrekende shard.
[ ] Verifier faalt bij output hash mismatch.
[ ] Verifier detecteert duplicate inputobjecten.
[ ] Verifier detecteert missing inputobjecten.
[ ] Verifier rapporteert coverage.
```

---

## 16. Demo-integratie

Breid de Docker-demo uit.

### Demo-flow

```text
1. create_demo_case.py maakt 100 documenten.
2. offf-demo plan-shards maakt 4 shard manifests.
3. Start 4 text extraction workers parallel.
4. Elke worker schrijft eigen shard output.
5. offf-demo finalize-job schrijft parent result manifest.
6. Elasticsearch index worker indexeert shard outputs.
7. Classifier worker leest alle shard outputs.
8. Verifier valideert parallel job en coverage.
```

### Docker Compose voorbeeldconcept

```text
offf-tika-worker-1 → shard-01
offf-tika-worker-2 → shard-02
offf-tika-worker-3 → shard-03
offf-tika-worker-4 → shard-04
```

Let op:

```text
Dit is demo-orkestratie bovenop OFFF, niet OFFF Core.
```

### Demo acceptance criteria

```text
[ ] Meerdere workers kunnen parallel draaien.
[ ] Iedere worker schrijft eigen sharddirectory.
[ ] Parent finalizer valideert alle shards.
[ ] Verifier toont coverage en valid status.
[ ] Geen worker schrijft naar evidence layer.
```

---

## 17. Tests

## 17.1 Unit tests

```text
ScopeResolver determinisme
Shard planning determinisme
ShardManifest schema
ShardResultManifest schema
ParentResultManifest schema
Coverage berekening
Duplicate input detection
Missing input detection
```

## 17.2 Integration tests

```text
parallel job met 4 shards
één ontbrekende shard
één corrupte output hash
duplicaat inputobject in twee shards
inputobject in scope maar niet toegewezen
worker error row
worker skipped row
```

### Acceptance criteria

```text
[ ] Tests draaien in CI.
[ ] Geldige parallel job valideert.
[ ] Ongeldige parallel job faalt met duidelijke foutcodes.
[ ] Bestaande single-job workers blijven werken.
```

---

## 18. Backward compatibility

### Regels

```text
- Single-worker jobs blijven geldig.
- Parallelization is optioneel.
- Bestaande analysis/jobs/{job_id}/result_manifest.json blijft ondersteund.
- Nieuwe sharded jobs gebruiken analysis/jobs/{job_id}/shards/{shard_id}/.
```

### Acceptance criteria

```text
[ ] Legacy single jobs blijven valide.
[ ] Sharded jobs worden apart gevalideerd.
[ ] Verifier herkent beide modellen.
```

---

## 19. Implementatievolgorde

### P0 — Minimale parallelle basis

```text
[ ] AnalysisJobManifest uitbreiden met parallelization
[ ] AnalysisInputObject formaliseren
[ ] ScopeResolver deterministisch maken
[ ] ShardPlan model
[ ] ShardManifest schema
[ ] ShardResultManifest schema
[ ] ParentResultManifest schema
[ ] Outputstructuur analysis/jobs/{job_id}/shards/{shard_id}/
[ ] Coverage validation
[ ] Verifier parallel job checks
```

### P1 — Worker en SDK

```text
[ ] AnalysisWorkerContext shard-aware maken
[ ] read_input_object_verified gebruiken
[ ] write_shard_artifact
[ ] commit_shard_result_manifest
[ ] provenance events voor shards
[ ] audit events voor skipped/denied
[ ] bestaande workers migreren naar shard-compatible output
```

### P2 — Platform/API

```text
[ ] Access Service shard endpoints
[ ] Tool registry capabilities uitbreiden
[ ] object storage commit marker
[ ] Merkle proof refs
[ ] demo met 4 parallelle workers
```

---

## 20. Definitie van klaar

De opdracht is klaar als:

```text
[ ] OFFF kan een parent analysis job beschrijven.
[ ] OFFF kan een deterministische inputscope oplossen.
[ ] OFFF kan een job opdelen in shard manifests.
[ ] Workers kunnen shard manifests verwerken.
[ ] Workers schrijven alleen naar eigen sharddirectory.
[ ] Elke shard heeft een shard result manifest.
[ ] Parent result manifest vat alle shards samen.
[ ] Verifier kan coverage, duplicates, missing inputs en outputhashes controleren.
[ ] Single-worker jobs blijven werken.
[ ] OFFF bevat geen scheduler, queue of runtime-specifieke orkestratie.
```

---

## 21. Kernzin voor ontwerpbeslissingen

```text
OFFF moet parallelle verwerking bewijsbaar en valideerbaar maken,
maar de uitvoering laten aan een externe scheduler of orchestration layer.
```

---

## 22. Niet doen

Vermijd deze fouten:

```text
[ ] Geen queue engine in OFFF Core.
[ ] Geen Kubernetes-specifieke velden in Core schema's.
[ ] Geen workers naar hetzelfde outputbestand laten schrijven.
[ ] Geen gedeelde JSONL append vanuit meerdere workers.
[ ] Geen output zonder result manifest.
[ ] Geen inputobjecten zonder coverage tracking.
[ ] Geen tool-specifieke shardinglogica.
[ ] Geen Tika-, Elasticsearch- of AI-specifieke parallelisatie in Core.
```

---

## 23. Slidewaardige samenvatting

```text
OFFF Core:
- definieert job, scope, shard, output en validatie
- bewaakt append-only en provenance
- maakt coverage controleerbaar

Scheduler bovenop OFFF:
- kiest wanneer workers draaien
- verdeelt resources
- doet retries
- schaalt workers op en af
```

Kernzin:

```text
OFFF standaardiseert het werkpakket en de bewijslast.
De scheduler organiseert de uitvoering.
```
