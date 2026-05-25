# Ontwikkelinstructie 1: Tool-agnostische ondersteuning voor nested evidence en object lineage in OFFF

## Doel

Breid OFFF uit met een **tool-agnostisch object-lineage model** waarmee nested evidence bewijsbaar kan worden vastgelegd.

OFFF moet kunnen ondersteunen dat een object zich op meerdere niveaus in andere objecten bevindt, bijvoorbeeld:

```text
source image
→ filesystem file
→ container object
→ embedded object
→ message object
→ attachment object
→ archive entry
→ document object
→ extracted analysis result
```

De instructie is bewust **abstract en tool-agnostisch**. OFFF Core mag geen kennis bevatten van specifieke formaten zoals ZIP, RAR, PST, EML, DOCX, PDF of databases. Die kennis hoort in parser/analysis workers. OFFF Core moet alleen het generieke model leveren om objecten, relaties, afleidingen, opslagverwijzingen, hashes en provenance vast te leggen.

---

## 1. Ontwerpprincipes

### 1.1 OFFF Core kent geen specifieke bestandsformaten

Niet opnemen in OFFF Core:

```text
zip_entry
rar_entry
pst_message
docx_text
pdf_page
msg_attachment
```

Wel opnemen in OFFF Core:

```text
object
object_type
object_edge
derivation
storage_ref
hash
source_ref
provenance_ref
parser_status
```

Specifieke workers mogen labels of object types gebruiken, maar Core mag daar geen functionele afhankelijkheid van hebben.

---

### 1.2 Object lineage is de chain of evidence

Voor nested evidence moet OFFF kunnen aantonen:

```text
dit object
→ is afgeleid uit dit parent object
→ via deze methode
→ door deze worker/toolversie
→ met deze inputhash
→ met deze outputhash
→ onder deze job
→ met deze provenance
→ uiteindelijk terugleidbaar naar originele chunks/source image
```

Dat is de technische **chain of evidence**.

---

### 1.3 Chain of custody blijft provenance/audit

Chain of custody gaat niet over de inhoudelijke parent-child-relatie, maar over handelingen:

```text
wie heeft wat gedaan?
wanneer?
met welke tool?
onder welke job?
met welke parameters?
met welk resultaat?
```

Daarom:

```text
object lineage = technische herkomst van objecten
provenance/audit = procesmatige handelingen en verantwoordelijkheid
```

Beide moeten gekoppeld zijn via `provenance_ref`, `job_id` en `tool`.

---

### 1.4 Originele evidence blijft immutable

Afgeleide objecten mogen nooit worden teruggeschreven naar de evidence layer.

Niet toegestaan:

```text
chunks/ aanpassen
hashes/ aanpassen
maps/ aanpassen
manifest.json aanpassen
acquisition.json aanpassen
```

Wel toegestaan:

```text
indexes/objects/
derived/objects/
analysis/jobs/{job_id}/
provenance/
audit/
extensions/
```

---

## 2. Nieuwe OFFF-concepten

Voeg de volgende generieke concepten toe aan de specificatie en implementatie:

```text
ObjectIndex
ObjectEdgeIndex
DerivationIndex
DerivedObjectStore
ObjectStorageRef
ObjectSourceRef
ObjectLineageValidator
```

---

## 3. Object Index

### 3.1 Doel

De Object Index registreert alle logische en afgeleide objecten binnen een OFFF-container.

Objecten kunnen zijn:

```text
source image
filesystem file
container member
embedded object
message
attachment
database record
document
media object
analysis input
analysis output
```

Let op: deze types zijn voorbeelden. Het model moet uitbreidbaar zijn.

---

### 3.2 Locatie

Voeg toe:

```text
indexes/
  objects/
    object_index.parquet
```

---

### 3.3 Minimale kolommen

| Kolom | Type | Betekenis |
|---|---|---|
| object_id | string | Unieke object-ID |
| object_type | string | Generiek type, bijvoorbeeld filesystem_file, embedded_object, message, attachment |
| name | string/null | Naam, bestandsnaam, subject of logische naam |
| logical_path | string/null | Pad binnen parent of container |
| media_type | string/null | MIME/content type indien bekend |
| size_bytes | uint64/null | Grootte van objectbytes indien bekend |
| sha256 | string/null | Hash van objectbytes indien beschikbaar |
| source_layer | string | evidence, derived_object, analysis |
| storage_ref | string/null | Locatie van materialized object bytes |
| root_source_ref | string/null | Verwijzing naar source image of root evidence |
| created_by_job_id | string/null | Job die object ontdekte of materialiseerde |
| parser_status | string | success, partial, error, unknown |
| provenance_ref | string/null | Provenance event dat objectcreatie beschrijft |
| schema_version | string | Schema-versie van deze row |

---

### 3.4 Object ID

Object IDs moeten stabiel en uniek zijn.

Aanbevolen patroon:

```text
obj-{type}-{hash-prefix-or-uuid}
```

Voor deterministic workflows kan object_id worden afgeleid uit:

```text
parent_object_id
relation_type
logical_path
object sha256
```

Voorbeeld:

```text
obj-file-000123
obj-derived-a1b2c3d4
obj-message-8f90abcd
```

---

### 3.5 Acceptance criteria

```text
[ ] object_index.parquet bestaat zodra objecten buiten file_index worden geregistreerd.
[ ] Elk object heeft object_id, object_type, source_layer en parser_status.
[ ] Elk materialized object heeft sha256 en storage_ref.
[ ] Elk object dat door een job is gemaakt heeft created_by_job_id.
[ ] Validator kan object_index.parquet schema-valideren.
```

---

## 4. Object Edges

### 4.1 Doel

Object Edges leggen parent-child-relaties vast tussen objecten.

Voorbeelden:

```text
filesystem file contains embedded object
container object contains child object
message has attachment
database contains record
document contains embedded media
analysis result derived from artifact
```

---

### 4.2 Locatie

Voeg toe:

```text
indexes/
  objects/
    object_edges.parquet
```

---

### 4.3 Minimale kolommen

| Kolom | Type | Betekenis |
|---|---|---|
| edge_id | string | Unieke edge-ID |
| parent_object_id | string | Ouderobject |
| child_object_id | string | Child object |
| relation_type | string | contains, extracted_from, attached_to, embedded_in, derived_from, parsed_from |
| method | string/null | Generieke methode, bijvoorbeeld archive_member_extraction |
| logical_path | string/null | Pad of naam binnen parent |
| sequence | uint64/null | Volgorde binnen parent indien relevant |
| created_by_job_id | string/null | Job die edge maakte |
| provenance_ref | string/null | Provenance event |
| schema_version | string | Schema-versie |

---

### 4.4 Generieke relation types

Gebruik minimaal:

```text
contains
extracted_from
attached_to
embedded_in
parsed_from
derived_from
references
```

Tooling mag uitbreiden, maar Core moet deze minimaal ondersteunen.

---

### 4.5 Acceptance criteria

```text
[ ] Iedere child-parent relatie wordt als edge vastgelegd.
[ ] parent_object_id en child_object_id verwijzen naar bestaande objecten.
[ ] Edges zijn append-only of worden via nieuwe correction events gecorrigeerd.
[ ] Validator detecteert ontbrekende parent/child objecten.
[ ] Validator kan cycles detecteren of als warning rapporteren.
```

---

## 5. Derivation Index

### 5.1 Doel

Derivations beschrijven hoe een child object uit een parent object is ontstaan.

Object edges zeggen:

```text
A is parent van B
```

Derivations zeggen:

```text
B is gemaakt uit A door methode X, tool Y, met parameters Z, inputhash H1 en outputhash H2
```

---

### 5.2 Locatie

Voeg toe:

```text
indexes/
  objects/
    derivations.parquet
```

of:

```text
indexes/
  objects/
    derivations.jsonl
```

Kies Parquet voor schaal, JSONL voor auditvriendelijkheid. Beide mogen, maar de specificatie moet één primaire vorm aanwijzen.

---

### 5.3 Minimale kolommen

| Kolom | Type | Betekenis |
|---|---|---|
| derivation_id | string | Unieke ID |
| parent_object_id | string | Inputobject |
| child_object_id | string | Outputobject |
| job_id | string | Worker job |
| method | string | Generieke afleidingsmethode |
| tool_id | string | Tool/worker |
| tool_name | string | Toolnaam |
| tool_version | string | Toolversie |
| parameters_hash | string/null | Hash van relevante parameters |
| input_sha256 | string/null | Hash van inputobject |
| output_sha256 | string/null | Hash van outputobject |
| storage_mode | string | referenced_only, materialized |
| provenance_ref | string/null | Provenance event |
| created_at | timestamp | Tijdstip |
| schema_version | string | Schema-versie |

---

### 5.4 Generieke method values

Gebruik generieke methodewaarden zoals:

```text
container_member_extraction
message_extraction
attachment_extraction
embedded_object_extraction
record_extraction
metadata_extraction
content_extraction
text_extraction
classification
hash_matching
manual_annotation
```

Specifieke workers mogen in aanvullende metadata vastleggen welk formaat/engine is gebruikt.

---

### 5.5 Acceptance criteria

```text
[ ] Elke objectcreatie door een worker heeft een derivation record.
[ ] input_sha256 en output_sha256 worden vastgelegd indien objectbytes beschikbaar zijn.
[ ] derivation verwijst naar job_id en provenance_ref.
[ ] Validator kan derivation chain volgen tot root evidence.
```

---

## 6. Derived Object Store

### 6.1 Doel

OFFF moet kunnen omgaan met afgeleide objectbytes die uit parent objecten zijn geëxtraheerd.

Voorbeelden:

```text
bestand binnen container
attachment uit message
embedded object uit document
record export uit database
```

Deze objectbytes zijn niet originele evidence, maar wel byte-exact afgeleid uit originele evidence.

---

### 6.2 Locatie

Voeg toe:

```text
derived/
  objects/
    sha256/
      ab/
        cd/
          <sha256>.bin
```

---

### 6.3 Storage modes

Ondersteun twee modi.

#### Mode 1: referenced_only

Objectbytes worden niet opgeslagen. De derivation beschrijft hoe het object opnieuw kan worden gereconstrueerd.

```text
storage_mode = referenced_only
storage_ref = null
```

#### Mode 2: materialized

Objectbytes worden opgeslagen in `derived/objects/`.

```text
storage_mode = materialized
storage_ref = derived/objects/sha256/ab/cd/<hash>.bin
```

---

### 6.4 Regels

```text
- Materialized derived objects zijn immutable.
- Bestandsnaam is gebaseerd op SHA-256.
- Bij bestaande objecthash moet bestaande bytes worden geverifieerd.
- Derived objects mogen niet als originele evidence worden gemarkeerd.
- source_layer moet derived_object zijn.
```

---

### 6.5 Acceptance criteria

```text
[ ] Derived object bytes kunnen content-addressed worden opgeslagen.
[ ] Derived object hash wordt gecontroleerd bij lezen.
[ ] Derived objects worden niet in chunks/ opgeslagen.
[ ] Validator detecteert ontbrekende storage_ref.
[ ] Validator detecteert hash mismatch in derived object store.
```

---

## 7. Source References

### 7.1 Doel

Elk object moet uiteindelijk herleidbaar zijn naar root evidence.

Daarom moet een object source references kunnen bevatten.

Voor filesystem files:

```json
{
  "source_refs": {
    "physical_extents": [
      { "offset": 1293844480, "length": 52428800 }
    ],
    "chunk_refs": ["sha256:abc...", "sha256:def..."]
  }
}
```

Voor afgeleide objecten:

```json
{
  "source_refs": {
    "parent_object_id": "obj-parent-001",
    "derivation_id": "drv-000001",
    "ancestor_object_ids": ["obj-file-001", "obj-source-image-001"],
    "root_chunk_refs": ["sha256:abc...", "sha256:def..."]
  }
}
```

---

### 7.2 Acceptance criteria

```text
[ ] Elk object heeft directe parent of root source refs.
[ ] Validator kan lineage volgen tot source image of filesystem file.
[ ] Materialized objects bevatten parent_object_id en derivation_id.
[ ] Root chunk refs kunnen worden afgeleid of vastgelegd.
```

---

## 8. Object-producing Worker Contract

### 8.1 Doel

Bestaande en toekomstige workers moeten naast analysis results ook objecten en relaties kunnen produceren.

Een worker-output kan bestaan uit:

```text
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

### 8.2 Generieke workerflow

```text
1. Worker leest job manifest.
2. Worker resolveert input scope.
3. Worker leest inputobject via verified read.
4. Worker voert tool-specifieke parsing/analyse uit.
5. Worker registreert child objects.
6. Worker registreert object edges.
7. Worker registreert derivations.
8. Worker materialiseert objectbytes indien nodig.
9. Worker schrijft errors voor mislukte objecten.
10. Worker schrijft result manifest.
11. Worker schrijft provenance en audit.
```

---

### 8.3 Acceptance criteria

```text
[ ] Workers kunnen child objects produceren zonder Core-aanpassing.
[ ] Workers kunnen object_edges produceren.
[ ] Workers kunnen derivations produceren.
[ ] Workers kunnen derived object bytes materialiseren.
[ ] Workers kunnen errors vastleggen per inputobject.
[ ] Verifier kan output generiek valideren.
```

---

## 9. Result Manifest uitbreiden

### 9.1 Doel

Het analysis result manifest moet niet alleen result artifacts beschrijven, maar ook object-producing outputs.

### Voorbeeld

```json
{
  "job_id": "job-parse-000001",
  "task": "parse_container",
  "status": "completed",
  "worker": {
    "tool_id": "generic-container-parser",
    "name": "Generic Container Parser",
    "version": "0.1.0"
  },
  "input": {
    "input_object_ids": ["obj-file-000123"],
    "container_id": "urn:offf:case:2026-001",
    "source_sha256": "sha256:...",
    "merkle_root_sha256": "sha256:..."
  },
  "outputs": {
    "analysis_artifacts": [
      {
        "path": "analysis/jobs/job-parse-000001/results.jsonl",
        "sha256": "sha256:...",
        "schema_ref": "schema:offf-analysis-result-envelope-0.2.0"
      }
    ],
    "object_index_delta": {
      "path": "analysis/jobs/job-parse-000001/objects.parquet",
      "sha256": "sha256:...",
      "schema_ref": "schema:offf-object-index-row-0.2.0"
    },
    "object_edges_delta": {
      "path": "analysis/jobs/job-parse-000001/object_edges.parquet",
      "sha256": "sha256:...",
      "schema_ref": "schema:offf-object-edge-row-0.2.0"
    },
    "derivations_delta": {
      "path": "analysis/jobs/job-parse-000001/derivations.parquet",
      "sha256": "sha256:...",
      "schema_ref": "schema:offf-derivation-row-0.2.0"
    },
    "materialized_objects": [
      {
        "object_id": "obj-derived-001",
        "storage_ref": "derived/objects/sha256/ab/cd/<hash>.bin",
        "sha256": "sha256:..."
      }
    ],
    "errors": {
      "path": "analysis/jobs/job-parse-000001/errors.jsonl",
      "sha256": "sha256:..."
    }
  }
}
```

---

### 9.2 Acceptance criteria

```text
[ ] Result manifest ondersteunt object_index_delta.
[ ] Result manifest ondersteunt object_edges_delta.
[ ] Result manifest ondersteunt derivations_delta.
[ ] Result manifest ondersteunt materialized_objects.
[ ] Alle outputbestanden hebben hashes.
[ ] result_manifest wordt als laatste geschreven.
```

---

## 10. Index merge / materialisatie

### 10.1 Vraagstuk

Workers kunnen delta-bestanden produceren:

```text
analysis/jobs/{job_id}/objects.parquet
analysis/jobs/{job_id}/object_edges.parquet
analysis/jobs/{job_id}/derivations.parquet
```

Daarna moeten deze beschikbaar worden in centrale indexen:

```text
indexes/objects/object_index.parquet
indexes/objects/object_edges.parquet
indexes/objects/derivations.parquet
```

### 10.2 Aanbevolen model

Gebruik append-only job deltas als bron van waarheid. Centrale indexen zijn afgeleide, herbouwbare views.

```text
analysis/jobs/*/objects.parquet
→ build/rebuild
→ indexes/objects/object_index.parquet
```

### 10.3 Tool

Voeg toe:

```bash
offf-index objects case.offf
```

Deze tool:

```text
- leest alle job object deltas
- valideert schema’s
- dedupliceert objecten op object_id/hash
- bouwt centrale object_index.parquet
- bouwt object_edges.parquet
- bouwt derivations.parquet
- schrijft provenance
```

---

### 10.4 Acceptance criteria

```text
[ ] Centrale object indexes kunnen opnieuw worden opgebouwd.
[ ] Job deltas blijven append-only.
[ ] Conflicten worden gedetecteerd.
[ ] Rebuild is deterministisch.
[ ] Validator kan centrale index vergelijken met job deltas.
```

---

## 11. Lineage verification

### 11.1 Nieuwe verifierfunctie

Voeg toe:

```bash
offf-verify case.offf --object obj-... --lineage
```

### 11.2 Controle

De verifier moet controleren:

```text
- object bestaat
- parent chain bestaat
- edges zijn geldig
- derivations zijn geldig
- materialized object hashes kloppen
- root filesystem file bestaat
- root physical extents bestaan
- root chunk refs bestaan
- root chunks valideren
- source image hash/Merkle root klopt of chunk proof klopt
- provenance refs bestaan
```

### 11.3 Output

Voorbeeld:

```text
Object: obj-derived-001
Lineage: VALID

obj-derived-001
← obj-parent-003
← obj-parent-002
← obj-file-000123
← source image chunks

Object hash: OK
Derived object storage: OK
Parent chain: OK
Chunk refs: OK
Provenance refs: OK
```

---

### 11.4 Acceptance criteria

```text
[ ] Verifier kan lineage van derived object tot source image volgen.
[ ] Missing parent wordt gedetecteerd.
[ ] Missing derivation wordt gedetecteerd.
[ ] Hash mismatch wordt gedetecteerd.
[ ] Missing provenance_ref geeft warning of error volgens profiel.
```

---

## 12. API- en SDK-aanpassingen

### 12.1 Core/SDK functies

Voeg toe:

```rust
list_objects()
get_object(object_id)
list_children(parent_object_id)
list_parents(child_object_id)
get_lineage(object_id)
read_object_verified(object_id)
write_object_delta(job_id, objects)
write_edge_delta(job_id, edges)
write_derivation_delta(job_id, derivations)
materialize_derived_object(job_id, object_id, bytes)
verify_object_lineage(object_id)
```

### 12.2 Access API endpoints

Voeg tool-agnostische endpoints toe:

```text
GET /cases/{caseId}/objects
GET /cases/{caseId}/objects/{objectId}
GET /cases/{caseId}/objects/{objectId}/children
GET /cases/{caseId}/objects/{objectId}/parents
GET /cases/{caseId}/objects/{objectId}/lineage
GET /cases/{caseId}/objects/{objectId}/content

POST /cases/{caseId}/analysis/jobs/{jobId}/objects
POST /cases/{caseId}/analysis/jobs/{jobId}/edges
POST /cases/{caseId}/analysis/jobs/{jobId}/derivations
POST /cases/{caseId}/analysis/jobs/{jobId}/materialized-objects
```

### 12.3 Acceptance criteria

```text
[ ] SDK kan object lineage ophalen.
[ ] SDK kan derived object content verified lezen.
[ ] Access API kan object graph lezen.
[ ] Access API kan object-producing outputs append-only accepteren.
[ ] Schrijven naar object indexes gebeurt via job deltas, niet directe mutatie.
```

---

## 13. Schema’s toevoegen

Maak minimaal deze schema’s:

```text
offf-object-index-row-0.2.0.schema.json
offf-object-edge-row-0.2.0.schema.json
offf-derivation-row-0.2.0.schema.json
offf-derived-object-store-0.2.0.schema.json
offf-lineage-report-0.2.0.schema.json
offf-object-producing-result-manifest-0.2.0.schema.json
```

### Acceptance criteria

```text
[ ] Schema’s staan in docs/schema.
[ ] Verifier gebruikt schema’s.
[ ] Conformance tests bevatten geldige en ongeldige object graphs.
```

---

## 14. Conformance tests

Maak testcases voor:

```text
single root file
file containing child object
multi-level nested object
materialized child object
referenced-only child object
missing parent
missing child
cycle in graph
hash mismatch
missing derivation
missing provenance
duplicate object_id
conflicting object hash
```

### Acceptance criteria

```text
[ ] Geldige nested object chain valideert.
[ ] Missing parent faalt.
[ ] Hash mismatch faalt.
[ ] Cycle geeft error of warning volgens profiel.
[ ] Rebuild van object index is deterministisch.
```

---

## 15. Definitie van klaar

Deze functionaliteit is klaar als OFFF tool-agnostisch kan aantonen:

```text
[ ] Een object kan meerdere parent-child-niveaus diep worden vastgelegd.
[ ] Elk child object heeft een parent edge.
[ ] Elk afgeleid object heeft een derivation record.
[ ] Materialized derived objects hebben hashes en storage refs.
[ ] Object lineage kan tot originele chunks/source image worden gevolgd.
[ ] Object-producing workers kunnen worden toegevoegd zonder Core-aanpassing.
[ ] Verifier kan lineage generiek valideren.
[ ] Chain of evidence en chain of custody zijn gekoppeld maar gescheiden.
```

---

## 16. Kernzin

```text
OFFF moet nested evidence ondersteunen via een generieke object-lineage graph:
objecten, relaties, derivations, hashes, storage refs en provenance.
De specifieke parsinglogica hoort in workers; OFFF Core bewaart de bewijsbare
herkomstketen tool-agnostisch.
```
