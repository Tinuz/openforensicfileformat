# OFFF Ecosysteem

## Wat is nodig om een volwaardig Open Forensic File Format-ecosysteem te bouwen?

Dit document beschrijft wat nodig is om van OFFF niet alleen een bestandsformaat te maken, maar een volwaardig **gestandaardiseerd forensic interoperability platform**. Het richt zich op de vraag hoe applicaties, diensten van derden en intern ontwikkelde tooling gecontroleerd, forensisch betrouwbaar en schaalbaar gebruik kunnen maken van OFFF.

---

Applicaties en diensten van derden kunnen OFFF gebruiken op drie niveaus:

lezen van OFFF-containers
analyseren van OFFF-data zonder evidence te wijzigen
toevoegen van afgeleide resultaten, indexen of annotaties aan de OFFF-container

Daarvoor is meer nodig dan alleen een bestandsformaat. Je hebt een OFFF-ecosysteem nodig: specificatie, API’s, SDK’s, rechtenmodel, validatietools en conformance-regels.

1. Basisgedachte

OFFF moet niet bedoeld zijn als een gesloten forensic suite, maar als een open evidence- en analysecontainer waar verschillende applicaties op kunnen aansluiten.

Denk aan:

acquisitietool
→ maakt OFFF-container

indexeringstool
→ leest evidence chunks
→ schrijft structurele indexen

analyseplatform
→ leest chunks en indexen
→ schrijft analysehits

AI-dienst
→ leest geselecteerde artifacts
→ schrijft classificaties als afgeleide annotaties

rapportagetool
→ leest evidence metadata, provenance en analyse-output
→ maakt rapportage zonder evidence te wijzigen

Belangrijk: applicaties mogen niet zomaar “in de container rommelen”. Ze moeten via afgesproken interfaces werken.

2. Welke vormen van integratie zijn nodig?

Ik zou vier integratieniveaus definiëren.

Niveau 1 — Read-only OFFF consumer

Een applicatie kan OFFF openen, valideren en data lezen.

Voorbeelden:

viewer
triage-tool
forensic browser
rapportagetool
data lake querytool
chain-of-custody viewer

De applicatie mag:

manifest lezen
provenance lezen
chunks lezen
indexen lezen
analyse-output lezen
validatie uitvoeren

De applicatie mag niet:

evidence chunks aanpassen
hashes wijzigen
manifest herschrijven
provenance overschrijven

Benodigd:

OFFF specificatie
read-only SDK
validatie-API
schema-documentatie
testcontainers
Niveau 2 — Analysis worker

Een applicatie of dienst kan OFFF-data analyseren en resultaten terugschrijven in de analysis layer.

Voorbeelden:

keyword search
YARA scan
hash matching
media classifier
document classifier
timeline generator
OCR-service
malware scanner
AI-analyse
entity extraction

De worker leest bijvoorbeeld:

chunks/
indexes/file_index.parquet
maps/physical_to_chunk.parquet

En schrijft naar:

analysis/
provenance/

De evidence layer blijft onveranderd.

Benodigd:

worker SDK
job manifest schema
result schema
provenance writer
chunk validator
append-only result writer
Niveau 3 — Index producer

Een applicatie mag nieuwe indexen toevoegen.

Voorbeelden:

NTFS parser
ext4 parser
registry parser
browser artifact parser
email parser
container parser
timeline indexer

Deze schrijft naar bijvoorbeeld:

indexes/filesystems/
indexes/files/
indexes/artifacts/

Belangrijk: indexen zijn interpretaties van evidence. Ze moeten dus herleidbaar zijn naar chunks, offsets en parser-versies.

Benodigd:

index schema’s
parser provenance
error model
mapping API
schema registry
Niveau 4 — Acquisition producer

Een applicatie mag een nieuwe OFFF-container maken vanuit brondata.

Voorbeelden:

disk imager
E01 converter
cloud export tool
mobile extraction adapter
live acquisition tool

Dit is het zwaarste integratieniveau, want deze applicatie produceert de evidence layer.

Benodigd:

strikte conformance tests
write SDK
chunking library
hashing library
Merkle tree library
acquisition schema
provenance schema
validator
certificering of goedkeuringsproces
3. Wat moet OFFF aanbieden aan derden?
3.1 Een formele specificatie

Er moet een officiële OFFF-specificatie komen.

Minimaal:

container layout
manifest schema
chunk schema
hashingregels
Merkle tree definitie
mappingtabellen
provenance model
indexformaten
analysis-output schema’s
validatieregels
versiebeheer
compatibiliteitsregels

Zonder specificatie kunnen derden wel “iets bouwen”, maar niet betrouwbaar interoperabel.

3.2 SDK’s

Derden moeten niet zelf hoeven uitvinden hoe chunks, hashes, offsets en provenance werken. OFFF moet SDK’s leveren.

Minimaal:

Rust SDK
Go SDK
Python SDK
eventueel Java/Kotlin SDK

Waarom deze verdeling:

SDK	Doel
Rust	high-performance tooling, acquisition, hashing, low-level parsing
Go	cloud-native services, workers, CLI’s
Python	analyse, data science, notebooks, AI/ML, forensic scripting
Java/Kotlin	enterprise-integratie, grotere backendplatformen

Een SDK moet minimaal bieden:

open_container()
read_manifest()
verify_container()
read_chunk()
verify_chunk()
map_offset_to_chunk()
read_file_index()
write_analysis_result()
append_provenance_event()

Voorbeeld in pseudocode:

from offf import OpenContainer

case = OpenContainer("s3://forensics/case-001.offf")

case.verify_manifest()

for chunk in case.chunks(scope="partition:gpt-2"):
    data = chunk.read_verified()
    hits = scan_keywords(data, ["bitcoin", "invoice"])

    case.analysis.write_keyword_hits(
        job_id="job-001",
        chunk_id=chunk.id,
        hits=hits
    )

case.provenance.append_event({
    "action": "keyword_scan_completed",
    "tool": "third-party-keyword-scanner",
    "version": "1.2.0"
})
3.3 REST/gRPC API

Niet iedere dienst moet direct op de filesystemstructuur of object storage hoeven werken. Daarom is een OFFF Access Service nodig.

Deze service biedt gecontroleerde toegang tot OFFF-containers.

Voorbeeldarchitectuur:

Third-party app
      ↓
OFFF Access API
      ↓
OFFF container op filesystem/S3/MinIO

Mogelijke API’s:

GET /cases/{caseId}/manifest
GET /cases/{caseId}/chunks/{chunkId}
GET /cases/{caseId}/chunks/{chunkId}/verify
GET /cases/{caseId}/files
GET /cases/{caseId}/files/{fileId}
GET /cases/{caseId}/artifacts
POST /cases/{caseId}/analysis-results
POST /cases/{caseId}/provenance-events

Voor high-performance verwerking is gRPC waarschijnlijk beter dan alleen REST.

4. Schrijfrechten: niet iedereen mag alles

Een belangrijk ontwerpprincipe: OFFF moet een capability model krijgen.

Niet iedere applicatie mag dezelfde laag wijzigen.

Voorbeeld:

Rol/type applicatie	Mag lezen	Mag schrijven
Viewer	Ja	Nee
Rapportagetool	Ja	Nee
Keyword worker	Ja	Alleen analysis layer
YARA worker	Ja	Alleen analysis layer
Filesystem parser	Ja	Alleen indexes layer
Acquisitietool	Ja	Evidence + manifest bij creatie
Validator	Ja	Alleen validatierapport
AI-service	Beperkt	Alleen AI-annotation layer

Concreet:

Evidence Layer      = alleen bij creatie schrijfbaar
Structure Layer     = alleen door goedgekeurde indexers
Artifact Layer      = alleen door goedgekeurde parsers
Analysis Layer      = door workers, append-only
Provenance Layer    = append-only
5. Wat moet een derde applicatie minimaal aanleveren?

Elke applicatie die iets toevoegt aan OFFF moet zichzelf kunnen verantwoorden.

Minimaal verplicht:

{
  "tool": {
    "name": "example-third-party-scanner",
    "version": "1.4.2",
    "vendor": "Example BV",
    "tool_hash": "sha256:..."
  },
  "input_scope": {
    "case_id": "urn:offf:case:...",
    "chunks": [
      "sha256:..."
    ]
  },
  "parameters": {
    "scan_mode": "keyword",
    "encoding": ["utf-8", "utf-16le"]
  },
  "output": {
    "result_file": "analysis/example_results.parquet",
    "result_sha256": "..."
  }
}

Voor AI-diensten aanvullend:

{
  "model": {
    "name": "document-classifier",
    "version": "0.3.1",
    "model_hash": "sha256:..."
  },
  "confidence": 0.87,
  "classification": "financial_document",
  "input_artifact": "file-000123"
}
6. Wat is technisch nodig?
6.1 OFFF Core Library

Een centrale library met de kernlogica.

Moet bevatten:

container openen
manifest lezen/schrijven
chunk lezen/schrijven
chunk validatie
compressie/decompressie
hashing
Merkle tree berekening
offset mapping
provenance events
schema-validatie

Deze library moet door alle officiële tools worden gebruikt, zodat er geen verschillende interpretaties ontstaan.

6.2 OFFF Schema Registry

Je hebt formele schema’s nodig voor:

manifest.json
acquisition.json
chunk metadata
physical_to_chunk.parquet
partition_table.json
file_index.parquet
keyword_hits.parquet
yara_hits.parquet
annotation events
provenance events

Bij voorkeur:

JSON Schema voor JSON/JSONL
Parquet schema-definities voor tabellen
OpenAPI/gRPC contracts voor services
6.3 OFFF Access Service

Voor organisatiediensten is een centrale service verstandig.

Die service regelt:

authenticatie
autorisatie
logging
rate limiting
case access
chunk access
subset-validatie
resultaatregistratie
provenance

Zonder zo’n service gaan applicaties rechtstreeks op de container schrijven, en dan verlies je governance.

6.4 OFFF Worker Framework

Voor distributed processing is een worker framework nodig.

Componenten:

job scheduler
worker registry
job manifest
chunk allocator
result writer
provenance writer
retry/failure handling
deterministic job replay

Voorbeeld:

Case: case-001.offf
Task: YARA scan
Scope: partition gpt-2
Workers: 20
Result: analysis/yara_hits.parquet
Provenance: 20 worker-events + job summary
6.5 OFFF Validator

Iedere externe tool moet kunnen aantonen dat output correct is toegevoegd.

Validator moet controleren:

schema-validiteit
hash-validiteit
chunk-herleidbaarheid
offset-herleidbaarheid
provenance-aanwezigheid
append-only gedrag
geen wijziging aan evidence layer
6.6 Conformance Test Suite

Derden moeten hun tooling kunnen testen tegen officiële testcases.

Voorbeelden:

valid OFFF container
container met corrupte chunk
container met ontbrekende chunk
container met gewijzigde manifesthash
analyse-output zonder provenance
analyse-output met foute offset
index zonder chunk_refs
AI-resultaat zonder modelversie

Conformance output:

PASS / FAIL
OFFF profile supported
read support
write support
analysis support
index support
acquisition support
7. Integratieprofielen

Ik zou officiële OFFF-integratieprofielen definiëren.

7.1 OFFF-Reader Profile

Voor applicaties die alleen lezen.

Moet ondersteunen:

manifest lezen
container valideren
chunks lezen
indexen lezen
provenance lezen
7.2 OFFF-Analysis Profile

Voor analyseworkers.

Moet ondersteunen:

read verified chunks
read mappings
write analysis results
append provenance
schema validation
7.3 OFFF-Indexer Profile

Voor parsers/indexers.

Moet ondersteunen:

read verified chunks
read partition maps
write index tables
write parser status
append provenance
7.4 OFFF-Acquisition Profile

Voor tools die OFFF-containers maken.

Moet ondersteunen:

create manifest
write chunks
calculate hashes
calculate Merkle root
write acquisition metadata
write provenance
export/verify roundtrip
8. Voorbeeld: derde dienst voert keyword search uit
1. Dienst vraagt job op bij OFFF Access Service.
2. Dienst krijgt lijst met chunk IDs.
3. Dienst downloadt alleen die chunks.
4. Dienst valideert per chunk de hash.
5. Dienst voert keyword search uit.
6. Dienst schrijft hits naar analysis/keyword_hits.parquet.
7. Dienst schrijft provenance event.
8. OFFF Validator controleert resultaat.

Resultaat:

hit → keyword → chunk_id → physical_offset → optional file_id → job_id → tool_version

Zo blijft iedere hit bewijsbaar herleidbaar.

9. Voorbeeld: organisatie bouwt eigen NTFS-indexer
1. Indexer opent manifest.
2. Indexer leest partition_table.
3. Indexer leest chunks die bij NTFS-volume horen.
4. Indexer parseert MFT.
5. Indexer maakt file_index.parquet.
6. Ieder bestand krijgt physical_extents en chunk_refs.
7. Parserfouten worden vastgelegd.
8. Provenance event wordt toegevoegd.
9. Validator controleert schema en herleidbaarheid.

Belangrijk: de indexer mag geen chunks wijzigen. Als de parser iets niet begrijpt, moet dat als partial of error worden vastgelegd.

10. Voorbeeld: AI-dienst classificeert documenten

Een AI-dienst mag nooit direct claimen dat iets “waar” is. AI-output is een afgeleide classificatie.

Proces:

1. AI-dienst krijgt alleen geselecteerde bestanden of artifacts.
2. Dienst leest file content via OFFF Access API.
3. Dienst voert classificatie uit.
4. Output wordt als annotation toegevoegd.
5. Modelnaam, modelversie, modelhash en confidence worden vastgelegd.
6. Inputscope wordt vastgelegd.
7. Provenance event wordt toegevoegd.

Voorbeeldresultaat:

{
  "annotation_id": "ai-000001",
  "target": {
    "file_id": "file-000123"
  },
  "classification": "financial_document",
  "confidence": 0.87,
  "model": {
    "name": "document-classifier",
    "version": "0.3.1",
    "model_hash": "sha256:..."
  },
  "input_scope": {
    "chunks": [
      "sha256:abc..."
    ]
  }
}
11. Governance: wat moet organisatorisch geregeld worden?

Techniek alleen is niet genoeg. Voor gebruik door derden moet de organisatie bepalen wie wat mag doen.

Nodig beleid
welke tools mogen OFFF lezen?
welke tools mogen analyse-output toevoegen?
welke tools mogen indexen toevoegen?
welke tools mogen evidence-containers creëren?
wie keurt nieuwe tools goed?
welke conformance tests zijn verplicht?
welke logging is verplicht?
hoe wordt toolversie vastgelegd?
hoe worden afwijkingen behandeld?
Toolregistratie

Maak een registry van toegestane tools.

Per tool:

toolnaam
leverancier/team
versie
toegestaan profiel
hash van executable/container image
goedgekeurd door
datum goedkeuring
ondersteunde OFFF-versie

Voorbeeld:

Tool	Profiel	Mag schrijven naar	Status
Internal NTFS Indexer	OFFF-Indexer	indexes/	Goedgekeurd
Keyword Worker	OFFF-Analysis	analysis/	Goedgekeurd
AI Classifier	OFFF-Analysis	analysis/annotations	Beperkt
External Viewer	OFFF-Reader	Niets	Goedgekeurd
Unknown Tool	Geen	Niets	Geblokkeerd
12. Minimale technische voorzieningen

Om applicaties en diensten goed te laten aansluiten, heb je minimaal nodig:

1. OFFF-specificatie
2. OFFF Core Library
3. SDK’s voor minimaal Python, Go en Rust
4. OFFF Access API
5. Schema Registry
6. Validator
7. Conformance Test Suite
8. Worker Framework
9. Tool Registry
10. Provenance- en auditmodel
11. Object-storage ondersteuning
12. Documentatie en voorbeeldcontainers
13. Aanbevolen referentiearchitectuur
┌──────────────────────────────────────────────┐
│ Derde applicaties / interne diensten          │
│ - viewers                                     │
│ - indexers                                    │
│ - AI services                                 │
│ - forensic tools                              │
│ - rapportagetools                             │
└───────────────────────┬──────────────────────┘
                        │
                        ▼
┌──────────────────────────────────────────────┐
│ OFFF Access Service                           │
│ - authenticatie                               │
│ - autorisatie                                 │
│ - chunk access                                │
│ - validation                                  │
│ - provenance                                  │
│ - result intake                               │
└───────────────────────┬──────────────────────┘
                        │
                        ▼
┌──────────────────────────────────────────────┐
│ OFFF Core Library / SDK                       │
│ - manifest                                    │
│ - chunks                                      │
│ - hashes                                      │
│ - Merkle                                      │
│ - mappings                                    │
│ - schemas                                     │
└───────────────────────┬──────────────────────┘
                        │
                        ▼
┌──────────────────────────────────────────────┐
│ OFFF Container Storage                        │
│ - filesystem                                  │
│ - S3 / MinIO / Ceph                           │
│ - immutable evidence layer                    │
│ - append-only analysis layer                  │
└──────────────────────────────────────────────┘
14. Belangrijk ontwerpbesluit

Ik zou derden niet rechtstreeks laten schrijven in de OFFF-container, behalve via goedgekeurde SDK’s of de OFFF Access Service.

Anders krijg je risico’s zoals:

incomplete provenance
schema-afwijkingen
niet-reproduceerbare output
gewijzigde evidence
onduidelijke toolversies
niet-valide analysis results

Daarom:

Lezen mag breed.
Schrijven moet gecontroleerd.
Evidence schrijven mag alleen bij creatie.
Analyse schrijven mag alleen append-only.
Indexen schrijven mag alleen door goedgekeurde indexers.
15. Kort antwoord

Applicaties en diensten kunnen gebruik maken van OFFF via een combinatie van:

OFFF-specificatie
SDK’s
Access API
Worker Framework
Validator
Conformance tests
Tool registry
Provenance model

Daarmee kunnen ze:

containers openen
chunks gevalideerd lezen
indexen gebruiken
analysejobs uitvoeren
resultaten append-only terugschrijven
herleidbaarheid naar brondata behouden

De essentie is dat OFFF niet alleen een bestand is, maar een gestandaardiseerd forensic interoperability platform. Het formaat levert de bewijskundige structuur; de SDK’s en API’s maken gecontroleerd gebruik door interne en externe applicaties mogelijk.

---

## Kernsamenvatting

Een volwaardig OFFF-ecosysteem vraagt om meer dan een bestandsspecificatie. Minimaal nodig zijn:

1. Een open en formele OFFF-specificatie.
2. SDK’s voor meerdere programmeertalen.
3. Een gecontroleerde OFFF Access API.
4. Een worker framework voor gedistribueerde analyse.
5. Een validator en conformance test suite.
6. Een tool registry met toegestane rechten per applicatie.
7. Een provenance- en auditmodel.
8. Object-storage ondersteuning.
9. Documentatie, voorbeeldcontainers en integratieprofielen.
10. Governance-afspraken voor lezen, schrijven, indexeren en evidence-creatie.

De kernregel blijft:

```text
Lezen mag breed.
Schrijven moet gecontroleerd.
Evidence blijft immutable.
Analyse en annotaties zijn append-only.
Elke uitkomst moet herleidbaar blijven naar brondata.
```
