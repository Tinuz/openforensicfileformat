# Ontwikkelinstructie: OFFF Core uitbreiden met generieke extensiepunten

## Doel

Breid **OFFF Core** uit met generieke extensiepunten waarmee juridische, organisatorische, analyse- en toegangscontroletooling bovenop OFFF kan worden gebouwd, zonder dat OFFF zelf juridische of procesmatige logica gaat bevatten.

De kernkeuze is:

```text
OFFF Core blijft juridisch neutraal.
Tooling bovenop OFFF bepaalt juridische of organisatorische betekenis.
```

OFFF moet dus niet zelf bepalen of data onder bijvoorbeeld verschoningsrecht, Landeck/Post-Landeck, interne autorisaties of andere regimes valt. OFFF moet alleen generieke mechanismen bieden om:

```text
data te markeren
scope te verwijzen
subsets te definiëren
beslissingen vast te leggen
toegang en verwerking te auditen
provenance te koppelen
validatie mogelijk te maken
```

---

## 1. Architectuurprincipe

### 1.1 Scheiding tussen Core en Tooling

Houd de verantwoordelijkheden strikt gescheiden.

```text
OFFF Core
= bewijsbare opslag, chunks, hashes, mappings, provenance, indexen en generieke extensiepunten

OFFF Access & Processing Platform
= SDK’s, API’s, worker framework, validator, tool registry

OFFF Legal / Policy Tooling
= Landeck-scope, verschoningsrecht, autorisaties, review workflows, release/exclusion workflows
```

OFFF Core mag geen nationale juridische concepten hardcoderen.

Dus niet:

```json
{
  "landeck_status": "allowed",
  "verschoningsrecht": true,
  "rechter_commissaris_beslissing": "vrijgegeven"
}
```

Maar wel generiek:

```json
{
  "label": "restricted",
  "policy_ref": "policy:external:123",
  "decision_ref": "decision:2026-001",
  "scope_ref": "scope:case-001:release-001"
}
```

---

## 2. Ontwerpdoelen

De uitbreiding van OFFF Core moet voldoen aan de volgende doelen:

1. **Technologie-agnostisch**  
   De extensiepunten moeten bruikbaar zijn voor verschillende soorten tooling, talen, platforms en juridische regimes.

2. **Juridisch neutraal**  
   OFFF Core legt geen juridische interpretatie vast. Het biedt alleen technische haakjes.

3. **Append-only waar nodig**  
   Beslissingen, labels, toegangsevents en provenance mogen niet stilzwijgend worden overschreven.

4. **Herleidbaar**  
   Elk label, besluit, resultaat of scope-object moet kunnen verwijzen naar chunks, bestanden, artifacts, jobs, tools en provenance-events.

5. **Valideerbaar**  
   Een validator moet kunnen controleren of schema’s kloppen, verwijzingen bestaan en evidence niet is gewijzigd.

6. **Uitbreidbaar zonder breaking changes**  
   Nieuwe tooling moet nieuwe labels, policies of decision-types kunnen toevoegen zonder het kernformaat te breken.

---

## 3. Nieuwe generieke extensiepunten in OFFF Core

Voeg de volgende generieke extensiepunten toe aan de OFFF Core-specificatie.

```text
labels/
scopes/
sets/
decisions/
access/
policy_refs/
audit/
extensions/
```

Aanbevolen directorystructuur:

```text
case.offf/
  manifest.json
  acquisition.json

  chunks/
  hashes/
  maps/
  indexes/
  analysis/
  provenance/

  extensions/
    labels/
      labels.jsonl
    scopes/
      scopes.jsonl
    sets/
      release_sets.jsonl
      exclusion_sets.jsonl
      working_sets.jsonl
    decisions/
      decisions.jsonl
    access/
      access_events.jsonl
      denied_access_events.jsonl
    policies/
      policy_refs.jsonl
    audit/
      audit_events.jsonl
```

Deze directories zijn generiek. Specifieke tooling mag hierop voortbouwen, maar mag geen juridische betekenis afdwingen in OFFF Core.

---

## 4. Extension Point 1 — Labels

### Doel

Labels maken het mogelijk om objecten binnen OFFF generiek te markeren.

Een label kan worden toegepast op:

```text
container
chunk
chunk range
partition
filesystem
file
artifact
analysis result
job
export package
```

### Niet doen

Labels mogen geen harde juridische betekenis in OFFF Core krijgen.

Dus niet:

```text
privileged=true
landeck_allowed=true
```

Wel:

```text
restricted
candidate
released
excluded
out_of_scope
requires_review
sensitive
```

De betekenis van deze labels wordt bepaald door externe tooling, policies of werkprocessen.

### Schema: `labels.jsonl`

```json
{
  "label_event_id": "label-000001",
  "timestamp": "2026-05-22T10:00:00Z",
  "actor": "tool:scope-manager",
  "tool": {
    "name": "offf-scope-manager",
    "version": "0.1.0"
  },
  "target": {
    "type": "file",
    "id": "file-000123"
  },
  "label": "restricted",
  "reason": "matched_external_policy",
  "policy_ref": "policy:external:scope-001",
  "provenance_ref": "prov-000991"
}
```

### Eisen

```text
[ ] Labels zijn append-only.
[ ] Labels verwijzen altijd naar een target.
[ ] Labels bevatten actor, timestamp en tool.
[ ] Labels mogen niet de evidence layer wijzigen.
[ ] Labels moeten door de validator gecontroleerd kunnen worden.
```

---

## 5. Extension Point 2 — Scopes

### Doel

Scopes definiëren een generieke toegestane of bedoelde reikwijdte van verwerking.

Een scope kan gebaseerd zijn op:

```text
chunk IDs
chunk ranges
file IDs
artifact IDs
partitions
filesystemen
datumbereik
artifact types
job IDs
labels
external policy references
```

### Schema: `scopes.jsonl`

```json
{
  "scope_id": "scope-000001",
  "created_at": "2026-05-22T10:15:00Z",
  "created_by": "tool:scope-manager",
  "description": "Generic processing scope for selected files and artifacts.",
  "include": {
    "file_ids": ["file-000123", "file-000456"],
    "artifact_types": ["email", "document"],
    "date_range": {
      "from": "2025-01-01T00:00:00Z",
      "to": "2025-12-31T23:59:59Z"
    }
  },
  "exclude": {
    "labels": ["restricted", "excluded"]
  },
  "policy_refs": [
    "policy:external:scope-001"
  ],
  "provenance_ref": "prov-001002"
}
```

### Eisen

```text
[ ] Scopes zijn generiek.
[ ] Scopes mogen verwijzen naar externe policies.
[ ] Scopes mogen juridische betekenis niet zelf interpreteren.
[ ] Workers moeten scopes kunnen gebruiken als input voor job manifests.
[ ] Validator moet controleren of scope-referenties bestaan.
```

---

## 6. Extension Point 3 — Sets

### Doel

Sets definiëren reproduceerbare subsets van data.

Er zijn drie generieke settypes:

```text
working_set
release_set
exclusion_set
```

### 6.1 Working Set

Een tijdelijke of operationele selectie voor analyse.

```json
{
  "set_id": "ws-000001",
  "set_type": "working_set",
  "created_at": "2026-05-22T10:30:00Z",
  "created_by": "tool:analysis-planner",
  "scope_ref": "scope-000001",
  "members": {
    "file_ids": ["file-000123"],
    "chunk_ids": ["sha256:abc..."]
  },
  "provenance_ref": "prov-001100"
}
```

### 6.2 Release Set

Een subset die door tooling of proces is vrijgegeven voor gebruik, export of verdere analyse.

```json
{
  "set_id": "rel-000001",
  "set_type": "release_set",
  "created_at": "2026-05-22T11:00:00Z",
  "created_by": "tool:release-manager",
  "members": {
    "file_ids": ["file-000123"],
    "artifact_ids": ["artifact-000991"]
  },
  "decision_ref": "decision-000001",
  "policy_refs": ["policy:external:release-001"],
  "provenance_ref": "prov-001200"
}
```

### 6.3 Exclusion Set

Een subset die moet worden uitgesloten van verwerking, toegang, export of rapportage.

```json
{
  "set_id": "excl-000001",
  "set_type": "exclusion_set",
  "created_at": "2026-05-22T11:15:00Z",
  "created_by": "tool:exclusion-manager",
  "members": {
    "file_ids": ["file-000456"],
    "chunk_ranges": [
      {
        "chunk_id": "sha256:def...",
        "offset_start": 0,
        "offset_end": 8192
      }
    ]
  },
  "decision_ref": "decision-000002",
  "policy_refs": ["policy:external:exclusion-001"],
  "provenance_ref": "prov-001250"
}
```

### Eisen

```text
[ ] Sets wijzigen evidence niet.
[ ] Sets zijn reproduceerbaar.
[ ] Sets verwijzen naar bestaande OFFF-objecten.
[ ] Sets kunnen door export-, worker- en access-tooling worden gebruikt.
[ ] Validator controleert of members bestaan en geldig zijn.
```

---

## 7. Extension Point 4 — Decisions

### Doel

Decisions leggen generieke beslissingen vast die door tooling, mensen of processen zijn genomen.

OFFF Core kent geen juridische betekenis toe aan decisions. Het legt alleen vast:

```text
wie
wat
wanneer
waarop
waarom
met welke policy/verwijzing
met welk gevolg voor OFFF-objecten
```

### Schema: `decisions.jsonl`

```json
{
  "decision_id": "decision-000001",
  "timestamp": "2026-05-22T11:00:00Z",
  "actor": {
    "type": "user",
    "id": "reviewer-123",
    "role": "external_role_ref:reviewer"
  },
  "decision_type": "release",
  "target": {
    "type": "set",
    "id": "ws-000001"
  },
  "outcome": "approved",
  "reason": "Approved by external review process.",
  "policy_refs": [
    "policy:external:release-policy-001"
  ],
  "provenance_ref": "prov-001300"
}
```

### Generieke decision types

Ondersteun minimaal:

```text
release
exclude
restrict
unrestrict
review_required
review_completed
export_approved
export_denied
processing_allowed
processing_denied
```

### Eisen

```text
[ ] Decisions zijn append-only.
[ ] Decisions bevatten actor, timestamp, target en outcome.
[ ] Decisions verwijzen naar policies/provenance waar relevant.
[ ] OFFF Core interpreteert decisions niet juridisch.
[ ] Access tooling mag decisions wel gebruiken voor beleidstoepassing.
```

---

## 8. Extension Point 5 — Policy References

### Doel

Policy references maken het mogelijk om te verwijzen naar externe regels, autorisaties, besluiten, werkprocessen of juridische grondslagen zonder deze in OFFF Core te modelleren.

### Schema: `policy_refs.jsonl`

```json
{
  "policy_ref": "policy:external:scope-001",
  "policy_type": "external",
  "title": "External access scope policy",
  "issuer": "external-system-or-authority",
  "issued_at": "2026-05-22T09:00:00Z",
  "uri": "urn:policy:scope-001",
  "hash": "sha256:...",
  "description": "Reference to externally managed policy or authorization.",
  "provenance_ref": "prov-000900"
}
```

### Eisen

```text
[ ] OFFF mag policy references opslaan.
[ ] OFFF hoeft externe policy-inhoud niet volledig te bevatten.
[ ] Als policy-documenten worden toegevoegd, moeten ze gehasht worden.
[ ] Validator controleert syntaxis en hash indien policy-document aanwezig is.
```

---

## 9. Extension Point 6 — Access Events

### Doel

Access events leggen vast wanneer een actor of tool toegang kreeg tot OFFF-objecten.

Dit geldt voor:

```text
manifest lezen
chunk lezen
file content lezen
artifact lezen
analysis result lezen
export uitvoeren
worker job uitvoeren
```

### Schema: `access_events.jsonl`

```json
{
  "access_event_id": "access-000001",
  "timestamp": "2026-05-22T12:00:00Z",
  "actor": "user:investigator-123",
  "tool": {
    "name": "offf-viewer",
    "version": "0.2.0"
  },
  "action": "read_file_content",
  "target": {
    "type": "file",
    "id": "file-000123"
  },
  "scope_ref": "scope-000001",
  "policy_refs": ["policy:external:scope-001"],
  "result": "allowed",
  "provenance_ref": "prov-001500"
}
```

---

## 10. Extension Point 7 — Denied Access Events

### Doel

Denied access events zijn belangrijk om aantoonbaar te maken dat bepaalde data niet toegankelijk was.

Dit is essentieel voor tooling die toegang moet begrenzen, zonder dat OFFF Core de juridische reden hoeft te kennen.

### Schema: `denied_access_events.jsonl`

```json
{
  "denied_event_id": "denied-000001",
  "timestamp": "2026-05-22T12:05:00Z",
  "actor": "user:investigator-123",
  "tool": {
    "name": "offf-viewer",
    "version": "0.2.0"
  },
  "action": "read_file_content",
  "target": {
    "type": "file",
    "id": "file-000456"
  },
  "result": "denied",
  "reason_code": "restricted_by_policy",
  "scope_ref": "scope-000001",
  "policy_refs": ["policy:external:restriction-001"],
  "provenance_ref": "prov-001501"
}
```

### Eisen

```text
[ ] Access-denials zijn apart logbaar.
[ ] Denials zijn append-only.
[ ] Reden wordt generiek vastgelegd.
[ ] Geen juridische hardcoding in Core.
[ ] Validator controleert target- en policy-referenties.
```

---

## 11. Extension Point 8 — Generic Audit Events

### Doel

Audit events leggen generieke controles en systeemgebeurtenissen vast.

Voorbeelden:

```text
validation executed
export package created
scope evaluated
policy evaluated
worker skipped item
set created
decision recorded
label applied
```

### Schema: `audit_events.jsonl`

```json
{
  "audit_event_id": "audit-000001",
  "timestamp": "2026-05-22T12:30:00Z",
  "actor": "system:offf-validator",
  "event_type": "scope_validation_completed",
  "target": {
    "type": "scope",
    "id": "scope-000001"
  },
  "result": "passed",
  "details": {
    "items_checked": 1523,
    "invalid_references": 0
  },
  "provenance_ref": "prov-001600"
}
```

---

## 12. Manifest-uitbreiding

Breid `manifest.json` uit met een optionele sectie `extensions`.

Voorbeeld:

```json
{
  "offf_version": "0.2.0",
  "container_id": "urn:offf:case:2026-001",
  "extensions": {
    "labels": "extensions/labels/labels.jsonl",
    "scopes": "extensions/scopes/scopes.jsonl",
    "sets": {
      "working_sets": "extensions/sets/working_sets.jsonl",
      "release_sets": "extensions/sets/release_sets.jsonl",
      "exclusion_sets": "extensions/sets/exclusion_sets.jsonl"
    },
    "decisions": "extensions/decisions/decisions.jsonl",
    "policy_refs": "extensions/policies/policy_refs.jsonl",
    "access_events": "extensions/access/access_events.jsonl",
    "denied_access_events": "extensions/access/denied_access_events.jsonl",
    "audit_events": "extensions/audit/audit_events.jsonl"
  }
}
```

### Eisen

```text
[ ] Extensies zijn optioneel.
[ ] Oudere OFFF-consumers moeten container kunnen lezen zonder extensies te interpreteren.
[ ] Validator moet onbekende extensies veilig kunnen negeren of waarschuwen.
[ ] Breaking changes in Core moeten worden vermeden.
```

---

## 13. SDK-impact

Breid de OFFF SDK’s uit met generieke functies.

### Read API

```text
list_labels()
get_labels_for_target(target)
list_scopes()
get_scope(scope_id)
list_sets(set_type)
get_set(set_id)
list_decisions()
get_decisions_for_target(target)
list_policy_refs()
list_access_events()
list_denied_access_events()
```

### Write API

Schrijven mag alleen via expliciete append-functies:

```text
append_label_event()
append_scope()
append_set()
append_decision()
append_policy_ref()
append_access_event()
append_denied_access_event()
append_audit_event()
```

### Niet toestaan

```text
overwrite_label()
delete_decision()
rewrite_access_log()
modify_exclusion_set_in_place()
```

Correcties moeten via nieuwe events worden vastgelegd.

---

## 14. Validator-impact

Breid `offf-verify` of `offf-validator` uit met controles op de generieke extensiepunten.

### Te controleren

```text
[ ] extension files bestaan als ze in manifest staan
[ ] JSONL records zijn valide
[ ] verplichte velden zijn aanwezig
[ ] target references bestaan
[ ] scope references bestaan
[ ] policy references zijn syntactisch geldig
[ ] decision references bestaan
[ ] set members verwijzen naar bestaande objecten
[ ] append-only integriteit is aantoonbaar
[ ] evidence layer is niet gewijzigd
```

### Validatieniveaus

Introduceer validatieprofielen:

```text
core
core+extensions
core+extensions+strict
```

Voorbeeld:

```bash
offf-verify case.offf --profile core+extensions
```

---

## 15. Worker-impact

Workers moeten scopes, sets en labels kunnen gebruiken zonder juridische betekenis te kennen.

### Worker-regel

Een worker mag alleen verwerken wat expliciet in zijn job manifest staat.

Job manifest voorbeeld:

```json
{
  "job_id": "job-000001",
  "task": "keyword_scan",
  "scope_ref": "scope-000001",
  "include_sets": ["ws-000001"],
  "exclude_sets": ["excl-000001"],
  "exclude_labels": ["restricted", "excluded"],
  "policy_refs": ["policy:external:scope-001"]
}
```

### Worker-eisen

```text
[ ] Worker leest scope/set/label-informatie via SDK.
[ ] Worker verwerkt alleen toegestane targets.
[ ] Worker logt skipped items als audit event.
[ ] Worker schrijft resultaten met scope_ref en job_id.
[ ] Worker schrijft provenance event.
```

---

## 16. Access API-impact

De OFFF Access API moet de generieke extensies kunnen gebruiken.

### Mogelijke endpoints

```text
GET  /cases/{caseId}/extensions/labels
GET  /cases/{caseId}/extensions/scopes
GET  /cases/{caseId}/extensions/sets/{setId}
GET  /cases/{caseId}/extensions/decisions
POST /cases/{caseId}/extensions/labels
POST /cases/{caseId}/extensions/scopes
POST /cases/{caseId}/extensions/sets
POST /cases/{caseId}/extensions/decisions
POST /cases/{caseId}/extensions/access-events
POST /cases/{caseId}/extensions/denied-access-events
```

### Eisen

```text
[ ] API gebruikt capability model.
[ ] API schrijft append-only.
[ ] API valideert schema’s vóór schrijven.
[ ] API schrijft provenance/audit events.
[ ] API voorkomt directe wijziging van evidence.
```

---

## 17. Backward compatibility

### Eisen

```text
[ ] OFFF v0.1-containers zonder extensies blijven valide.
[ ] OFFF v0.2-consumers kunnen v0.1 lezen.
[ ] OFFF v0.1-consumers moeten v0.2 kunnen lezen zolang zij extensies negeren.
[ ] Extensies mogen geen verplichte afhankelijkheid worden voor core evidence validatie.
```

---

## 18. Securityprincipes

### 18.1 Geen security by UI

Extensies mogen niet alleen in viewers worden toegepast. Access tooling, SDK’s, workers en exporttooling moeten dezelfde scope-, set- en labelinformatie kunnen gebruiken.

### 18.2 Geen juridische interpretatie door Core

OFFF Core mag labels en decisions opslaan, maar mag niet bepalen wat juridisch waar of toegestaan is.

### 18.3 Append-only

Alle relevante wijzigingshistorie moet bewaard blijven.

### 18.4 Least privilege

Schrijven naar extensies moet via capability checks verlopen.

### 18.5 Validatie vóór gebruik

Tooling die werkt met scopes, sets of decisions moet deze eerst valideren.

---

## 19. Niet doen

Neem de volgende zaken niet op in OFFF Core:

```text
[ ] specifieke Landeck-logica
[ ] specifieke verschoningsrechtelijke beslisregels
[ ] nationale procesrollen als vaste Core-entiteiten
[ ] hardcoded juridische statussen
[ ] automatische juridische conclusies
[ ] UI-only blokkades
[ ] overschrijfbare decisions
[ ] verwijderbare auditlogs
```

Deze zaken horen in aparte tooling, policies en organisatorische processen.

---

## 20. Acceptance criteria

De uitbreiding is geslaagd als:

```text
[ ] OFFF Core bevat generieke extensiepunten voor labels, scopes, sets, decisions, policy refs, access events en audit events.
[ ] Extensies zijn optioneel en breken bestaande OFFF-containers niet.
[ ] Alle extensies zijn append-only of corrigeren via nieuwe events.
[ ] Extensies kunnen verwijzen naar chunks, files, artifacts, jobs en analysis results.
[ ] Extensies bevatten geen hardcoded juridische regimes.
[ ] SDK’s kunnen extensies lezen en append-only schrijven.
[ ] Validator kan extensies schema-technisch en referentieel controleren.
[ ] Workers kunnen scopes, sets en labels gebruiken zonder juridische betekenis te kennen.
[ ] Access API kan extensies gecontroleerd aanbieden en registreren.
[ ] Evidence layer blijft immutable.
```

---

## 21. Samenvattende opdracht aan het ontwikkelteam

Breid OFFF Core uit met generieke extensiepunten die tooling bovenop OFFF in staat stellen om scope, toegang, uitsluiting, vrijgave, beslissingen en audit vast te leggen.

Doe dit zonder juridische of nationale proceslogica in OFFF Core op te nemen.

De ontwerpregel is:

```text
OFFF Core registreert generieke technische feiten en verwijzingen.
Tooling bovenop OFFF interpreteert deze binnen juridische, organisatorische of operationele processen.
```

De kernzin:

```text
OFFF moet juridisch neutraal blijven, maar voldoende generieke haakjes bieden om juridische en organisatorische controle aantoonbaar bovenop OFFF te implementeren.
```
