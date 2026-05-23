# OFFF – Open Forensic File Format

Formele specificatie (normatief, implementatie-aligned):

- Zie `SPEC_OFFF_Formal_Spec_v0.1.0.md`

Machineleesbare JSON Schema set:

- Zie `docs/schema/offf-schema-catalog-0.1.0.json`

Python SDK:

- Zie `sdk/python/offf_sdk/`

## Open, verifieerbaar en geschikt voor gedistribueerde forensische analyse

OFFF staat voor **Open Forensic File Format**. Het doel van OFFF is het ontwikkelen van een open, forensisch betrouwbaar en chunk-gebaseerd bestandsformaat waarmee digitale bewijskopieën geschikt worden gemaakt voor **gedistribueerde analyse**.

Traditionele forensic images, zoals `raw/dd`, `E01/EWF` of andere monolithische containers, zijn vooral ontworpen als lineaire representatie van een gegevensdrager. Dat werkt goed voor bewaring en klassieke analyse, maar minder goed wanneer grote hoeveelheden data parallel moeten worden verwerkt door meerdere workers, nodes of analyseplatformen.

OFFF behandelt een forensische kopie niet als één groot binair bestand, maar als een verzameling afzonderlijk verifieerbare chunks, indexen, metadata en analyse-uitkomsten.

De kern van OFFF:

```text
bewijsbare chunks → indexen → parallelle analyse → herleidbare resultaten
```

---

## 1. Doel van het project

Ontwikkel het **Open Forensic File Format**, afgekort **OFFF**: een open, verifieerbaar, chunk-gebaseerd forensisch bestandsformaat waarmee disk images en afgeleide analysegegevens geschikt worden gemaakt voor gedistribueerde verwerking.

Het formaat moet een alternatief bieden voor het werken met één groot monolithisch imagebestand. OFFF moet het mogelijk maken om forensische data op te delen in afzonderlijk verifieerbare blokken, deze parallel te verwerken en alle resultaten herleidbaar te houden naar de originele gegevensdrager.

De primaire ontwerpdoelen zijn:

1. Forensische integriteit
2. Reproduceerbaarheid
3. Open specificatie
4. Gedistribueerde verwerking
5. Herleidbaarheid van elk resultaat naar brondata
6. Scheidbaarheid tussen originele evidence en afgeleide analyse
7. Exporteerbaarheid terug naar `raw/dd`
8. Toekomstige uitbreidbaarheid naar artifact-, cloud-, mobile- en AI-analyse

---

## 2. Kernprincipes

### 2.1 Evidence is immutable

De originele bewijsdata mag nooit worden aangepast. Chunks, bronmetadata en hashstructuren zijn na creatie onveranderlijk.

Analysegegevens, annotaties en indexen mogen worden toegevoegd, maar niet stilzwijgend overschreven.

Correcties worden als nieuwe events toegevoegd.

```text
Niet: keyword_hit aanpassen
Wel: keyword_hit_v2 corrigeert keyword_hit_v1 met reden en actor
```

---

### 2.2 Analyse is afgeleid, niet origineel

Maak in de architectuur een harde scheiding tussen:

```text
Evidence Layer      = originele bytes
Structure Layer     = partities, volumes, bestandssystemen
Artifact Layer      = bestanden, logs, registry, browserdata
Analysis Layer      = hits, classificaties, annotaties
Provenance Layer    = chain of custody, toolgebruik, verwerking
```

Geen enkel analysebestand mag worden gepresenteerd alsof het originele brondata is.

---

### 2.3 Elke byte moet herleidbaar zijn

Iedere chunk, indexregel, artifact en analysehit moet uiteindelijk terug te leiden zijn naar:

```text
bronapparaat
→ fysieke offset
→ chunk
→ partitie/volume
→ bestandssysteem
→ bestand/artifact
→ analyse-uitkomst
```

---

### 2.4 Validatie is onderdeel van het formaat

Het formaat is pas bruikbaar als het gevalideerd kan worden. Daarom moet vanaf fase 1 een validatietool worden ontwikkeld.

---

## 3. Gewenste eindarchitectuur

OFFF moet uiteindelijk bestaan uit de volgende lagen:

```text
┌──────────────────────────────────────────────┐
│ 5. Analysis & Annotation Layer                │
│    hits, labels, AI-resultaten, triage        │
├──────────────────────────────────────────────┤
│ 4. Artifact Index Layer                       │
│    files, registry, logs, browserdata         │
├──────────────────────────────────────────────┤
│ 3. Structure Index Layer                      │
│    volumes, partities, filesystems            │
├──────────────────────────────────────────────┤
│ 2. Chunk Store Layer                          │
│    immutable content-addressed chunks         │
├──────────────────────────────────────────────┤
│ 1. Acquisition & Provenance Layer             │
│    chain of custody, source, tooling, hashes  │
└──────────────────────────────────────────────┘
```

De eerste versie hoeft nog niet alle lagen volledig functioneel te hebben, maar het ontwerp moet deze lagen vanaf het begin ondersteunen.

---

## 4. Containerstructuur

Ontwikkel OFFF primair als **directory-based container**.

Een OFFF-container moet er minimaal als volgt uitzien:

```text
case.offf/
  manifest.json
  acquisition.json
  provenance/
    chain_of_custody.jsonl
    tool_log.jsonl
  chunks/
    sha256/
      ab/
        cd/
          abcd1234.chunk
  hashes/
    leaves.parquet
    merkle_tree.bin
  maps/
    physical_to_chunk.parquet
  indexes/
    partition_table.json
    filesystems/
  analysis/
  signatures/
```

### Eisen

De directory-container is leidend.

Een single-file variant mag later worden toegevoegd, maar is niet leidend voor fase 1.

De structuur moet geschikt zijn voor:

```text
lokale filesystemen
S3-compatible object storage
MinIO
Ceph
Kubernetes-gebaseerde verwerking
gedistribueerde workers
```

---

## 5. Technische uitgangspunten

### 5.1 Serialisatieformaten

| Type data | Formaat |
|---|---|
| Manifesten | JSON |
| Kleine metadata | JSON |
| Logs | JSONL |
| Grote tabellen | Parquet |
| Hashlijsten | Parquet |
| Mappingtabellen | Parquet |
| Binaire Merkle Tree | Binair formaat met specificatie |
| Analyse-output | Parquet of JSONL |

Gebruik geen gesloten of vendorspecifieke formaten.

---

### 5.2 Hashing

Gebruik in eerste instantie:

```text
SHA-256
```

Ondersteun later eventueel:

```text
SHA-512
BLAKE3
```

Maar fase 1 moet standaardiseren op SHA-256.

---

### 5.3 Compressie

Gebruik in eerste instantie:

```text
zstd
```

Het manifest moet per chunk kunnen aangeven:

```json
{
  "compression": "none"
}
```

of:

```json
{
  "compression": "zstd"
}
```

---

### 5.4 Chunking

Gebruik in fase 1 fixed-size chunking.

Aanbevolen standaard:

```text
64 MiB per chunk
```

Maak de chunk size configureerbaar, maar leg deze vast in het manifest.

Nog niet implementeren in fase 1:

```text
content-defined chunking
deduplicatie
smart chunking per filesystem
```

---

## 6. Fase 1 – Evidence Container MVP

### Doel

Maak een minimale, betrouwbare OFFF-container die een `raw/dd`-image kan importeren, opdelen in chunks, valideren en reconstrueren naar exact dezelfde `raw/dd`-output.

Deze fase is de kern. Als deze fase niet forensisch klopt, is de rest waardeloos.

### Functionaliteit

Ontwikkel de volgende onderdelen:

```text
offf-convert
offf-verify
offf-export
```

---

### 6.1 `offf-convert`

De tool `offf-convert` moet een `raw/dd`-bestand kunnen omzetten naar een OFFF-directory-container.

Voorbeeld:

```bash
offf-convert \
  --input evidence.dd \
  --output case.offf \
  --chunk-size 64M \
  --compression zstd \
  --hash sha256
```

De tool moet:

1. Inputbestand openen
2. Bronmetadata vastleggen
3. Input opdelen in fixed-size chunks
4. Per chunk een plaintext hash berekenen
5. Chunk optioneel comprimeren
6. Per opgeslagen chunk een stored hash berekenen
7. Chunks content-addressed opslaan
8. `manifest.json` genereren
9. `acquisition.json` genereren
10. `physical_to_chunk.parquet` genereren
11. Merkle leaf hashes genereren
12. Merkle root berekenen
13. Provenance event wegschrijven

---

### 6.2 Chunk metadata

Per chunk moet minimaal worden vastgelegd:

```json
{
  "sequence": 0,
  "chunk_id": "sha256:...",
  "source_offset": 0,
  "source_length": 67108864,
  "stored_length": 12345678,
  "compression": "zstd",
  "plaintext_sha256": "...",
  "stored_sha256": "...",
  "read_errors": []
}
```

---

### 6.3 Manifest

Het manifest moet minimaal bevatten:

```json
{
  "offf_version": "0.1.0",
  "container_id": "urn:offf:case:...",
  "created_at": "2026-05-22T10:15:00Z",
  "created_by_tool": {
    "name": "offf-convert",
    "version": "0.1.0"
  },
  "source": {
    "type": "raw_image",
    "size_bytes": 0,
    "sector_size": 512
  },
  "hashes": {
    "source_sha256": "...",
    "merkle_root_sha256": "..."
  },
  "chunking": {
    "chunk_size": 67108864,
    "chunking_mode": "fixed",
    "compression": "zstd",
    "hash_algorithm": "sha256"
  },
  "indexes": {
    "physical_to_chunk": "maps/physical_to_chunk.parquet"
  }
}
```

---

### 6.4 `offf-verify`

De tool `offf-verify` moet kunnen controleren:

1. Bestaat het manifest?
2. Klopt de OFFF-versie?
3. Bestaan alle chunks?
4. Klopt de stored hash per chunk?
5. Klopt na decompressie de plaintext hash?
6. Klopt de mappingtabel?
7. Klopt de Merkle root?
8. Klopt de gereconstrueerde source hash?
9. Is de container compleet?
10. Zijn provenance logs aanwezig?

Voorbeeld:

```bash
offf-verify case.offf
```

Output moet duidelijk zijn:

```text
Container: case.offf
OFFF version: 0.1.0
Chunks: 15234
Stored hash validation: OK
Plaintext hash validation: OK
Merkle root: OK
Source SHA-256: OK
Result: VALID
```

---

### 6.5 `offf-export`

De tool `offf-export` moet een OFFF-container exact kunnen reconstrueren naar `raw/dd`.

Voorbeeld:

```bash
offf-export case.offf --output reconstructed.dd
```

Acceptatie-eis:

```text
sha256(evidence.dd) == sha256(reconstructed.dd)
```

---

### Acceptance criteria fase 1

```text
[ ] raw/dd kan worden geconverteerd naar OFFF
[ ] OFFF-container heeft vaste directorystructuur
[ ] chunks worden correct opgeslagen
[ ] chunk hashes worden correct berekend
[ ] source hash wordt correct berekend
[ ] Merkle root wordt correct berekend
[ ] manifest is volledig
[ ] provenance log wordt geschreven
[ ] offf-verify valideert de container
[ ] offf-export reconstrueert exact dezelfde raw/dd
[ ] test met minimaal 3 images slaagt:
    - kleine image < 1 GB
    - middelgrote image 10-100 GB
    - image met niet-door-64MiB-deelbare grootte
```

---

## 7. Fase 2 – Partitie- en volumemapping

### Doel

Maak OFFF niet alleen een chunk-container, maar voeg structurele kennis toe over de bron: MBR, GPT, partities en volumes.

### Functionaliteit

Ontwikkel:

```text
offf-index partitions
```

Voorbeeld:

```bash
offf-index partitions case.offf
```

De tool moet:

1. MBR detecteren
2. GPT detecteren
3. Partities uitlezen
4. Offsets en lengtes vastleggen
5. Partitie-ID’s genereren
6. Partities koppelen aan chunks
7. `indexes/partition_table.json` aanmaken
8. Provenance event toevoegen

### Minimale output

```json
{
  "partition_table_type": "gpt",
  "partitions": [
    {
      "partition_id": "gpt-1",
      "name": "EFI System Partition",
      "type_guid": "...",
      "start_offset": 1048576,
      "length": 272629760,
      "first_lba": 2048,
      "last_lba": 534527,
      "chunk_refs": [
        "sha256:..."
      ]
    }
  ]
}
```

### Acceptance criteria fase 2

```text
[ ] MBR wordt correct herkend
[ ] GPT wordt correct herkend
[ ] partitie-offsets kloppen byte-exact
[ ] partities zijn gekoppeld aan chunks
[ ] output is reproduceerbaar
[ ] indexering wijzigt de evidence layer niet
[ ] provenance bevat indexing event
```

---

## 8. Fase 3 – Filesystem Indexing

### Doel

Voeg ondersteuning toe voor bestandssysteemherkenning en basisindexering van bestanden.

Start met één bestandssysteem. Advies:

```text
NTFS eerst
```

Daarna pas:

```text
exFAT
FAT32
ext4
APFS
HFS+
```

### Functionaliteit

Ontwikkel:

```text
offf-index filesystem
```

Voorbeeld:

```bash
offf-index filesystem case.offf --partition gpt-2
```

De tool moet voor NTFS minimaal:

1. Boot sector herkennen
2. MFT lokaliseren
3. Bestandrecords parsen
4. Bestandsnamen uitlezen
5. Bestandsgrootte uitlezen
6. Timestamps uitlezen
7. File extents bepalen
8. File extents koppelen aan fysieke offsets
9. Fysieke offsets koppelen aan chunks
10. `file_index.parquet` genereren

### File index schema

| Kolom | Betekenis |
|---|---|
| file_id | Interne unieke ID |
| filesystem_id | ID van filesystem |
| partition_id | ID van partitie |
| path | Logisch pad |
| filename | Bestandsnaam |
| extension | Extensie |
| size_bytes | Bestandsgrootte |
| created_at | Creatietijd |
| modified_at | Wijzigingstijd |
| accessed_at | Toegangstijd |
| changed_at | Metadatawijziging |
| physical_extents | Fysieke offsets |
| chunk_refs | Betrokken chunks |
| parser | Gebruikte parser |
| parser_version | Versie parser |

Als een bestand niet volledig geparsed kan worden, mag het niet verdwijnen.

Gebruik dan:

```text
parser_status = partial
```

of:

```text
parser_status = error
```

met foutmelding.

### Acceptance criteria fase 3

```text
[ ] NTFS wordt herkend
[ ] MFT wordt gevonden
[ ] bestanden worden geïndexeerd
[ ] paden worden opgebouwd
[ ] timestamps worden vastgelegd
[ ] file extents zijn herleidbaar naar fysieke offsets
[ ] fysieke offsets zijn herleidbaar naar chunks
[ ] parserfouten worden expliciet vastgelegd
[ ] evidence layer blijft immutable
```

---

## 9. Fase 4 – Distributed Processing Framework

### Doel

Maak het mogelijk dat meerdere workers parallel op OFFF-data kunnen werken.

### Architectuur

Ontwikkel een eenvoudig jobmodel:

```text
job manifest
worker
result writer
provenance event
```

### Job manifest

Voorbeeld:

```json
{
  "job_id": "job-2026-000001",
  "case_id": "urn:offf:case:...",
  "task": "keyword_scan",
  "scope": {
    "chunks": [
      "sha256:abc...",
      "sha256:def..."
    ]
  },
  "tool": {
    "name": "offf-keyword-worker",
    "version": "0.1.0"
  },
  "parameters": {
    "keywords": [
      "bitcoin",
      "password",
      "invoice"
    ],
    "encoding": [
      "utf-8",
      "utf-16le"
    ]
  }
}
```

### Workers

Ontwikkel minimaal twee workers:

```text
offf-keyword-worker
offf-yara-worker
```

### Resultaten

Resultaten worden geschreven naar:

```text
analysis/
  keyword_hits.parquet
  yara_hits.parquet
```

### Keyword hit schema

| Kolom | Betekenis |
|---|---|
| hit_id | Unieke hit |
| job_id | Jobreferentie |
| keyword | Gevonden keyword |
| chunk_id | Chunk |
| physical_offset | Fysieke offset |
| file_id | Optioneel bestand |
| context_before | Beperkte context |
| context_after | Beperkte context |
| encoding | Gebruikte encoding |
| worker_id | Worker |
| timestamp | Verwerkingstijd |

### YARA hit schema

| Kolom | Betekenis |
|---|---|
| hit_id | Unieke hit |
| job_id | Jobreferentie |
| rule_name | YARA-rule |
| ruleset_hash | Hash van ruleset |
| chunk_id | Chunk |
| physical_offset | Fysieke offset |
| file_id | Optioneel |
| worker_id | Worker |
| timestamp | Verwerkingstijd |

### Acceptance criteria fase 4

```text
[ ] job manifest kan worden aangemaakt
[ ] workers kunnen chunks onafhankelijk ophalen
[ ] workers valideren chunk vóór verwerking
[ ] resultaten zijn herleidbaar naar chunk en offset
[ ] resultaten worden niet in evidence layer geschreven
[ ] provenance bevat worker-events
[ ] dezelfde job geeft bij gelijke input dezelfde output
[ ] meerdere workers kunnen parallel draaien
```

---

## 10. Fase 5 – Object Storage en schaalbaarheid

### Doel

Maak OFFF geschikt voor gedistribueerde opslag en verwerking op object storage.

Ondersteun minimaal:

```text
local filesystem
S3-compatible storage
MinIO
```

### Eisen

De tools moeten kunnen werken met paden zoals:

```bash
offf-verify s3://forensics/case-001.offf
```

of:

```bash
offf-worker \
  --case s3://forensics/case-001.offf \
  --job job.json
```

### Belangrijke ontwerpkeuzes

1. Chunks moeten onafhankelijk opvraagbaar zijn.
2. Indexen moeten apart leesbaar zijn.
3. Workers mogen niet de hele container hoeven downloaden.
4. Validatie van een subset moet mogelijk zijn.
5. Object keys moeten stabiel en deterministisch zijn.

### Acceptance criteria fase 5

```text
[ ] OFFF-container kan op MinIO staan
[ ] chunks kunnen individueel worden opgehaald
[ ] manifest kan remote worden gelezen
[ ] worker kan remote chunk verwerken
[ ] verify kan subsetvalidatie doen
[ ] performance is acceptabel bij grote aantallen chunks
```

---

## 11. Fase 6 – Analyse- en annotatielaag

### Doel

Voeg een append-only analyse- en annotatielaag toe waarin menselijke en automatische bevindingen kunnen worden vastgelegd.

### Analyse-event

Voorbeeld:

```json
{
  "annotation_id": "ann-000001",
  "timestamp": "2026-05-22T12:30:00Z",
  "actor": "analyst-123",
  "type": "relevance_label",
  "target": {
    "file_id": "file-000123"
  },
  "label": "relevant",
  "comment": "Document lijkt betrekking te hebben op administratie.",
  "confidence": "human_reviewed"
}
```

### AI/ML-resultaat

Voorbeeld:

```json
{
  "annotation_id": "ai-000001",
  "timestamp": "2026-05-22T12:45:00Z",
  "actor": "model:document-classifier",
  "model": {
    "name": "document-classifier",
    "version": "0.3.1",
    "model_hash": "sha256:..."
  },
  "target": {
    "file_id": "file-000123"
  },
  "classification": "financial_document",
  "confidence": 0.87
}
```

### Eisen

AI-resultaten moeten altijd herkenbaar zijn als afgeleid en probabilistisch.

Gebruik velden zoals:

```text
model_name
model_version
model_hash
confidence
input_scope
timestamp
```

AI-output mag nooit de originele evidence overschrijven.

### Acceptance criteria fase 6

```text
[ ] menselijke annotaties kunnen worden toegevoegd
[ ] automatische annotaties kunnen worden toegevoegd
[ ] annotaties zijn append-only
[ ] annotaties verwijzen naar chunk/file/artifact
[ ] correcties worden als nieuwe events vastgelegd
[ ] AI-output is expliciet herkenbaar als afgeleid resultaat
```

---

## 12. Fase 7 – Conversie vanuit bestaande formaten

### Doel

Maak OFFF bruikbaar in bestaande forensische werkprocessen.

Ondersteun conversie vanuit:

```text
raw/dd
E01/EWF
AFF4, indien praktisch haalbaar
```

### Belangrijke eis

Bij conversie vanuit containerformaten moet onderscheid worden gemaakt tussen:

```text
hash van originele container
hash van gedecomprimeerde evidence stream
hashes van OFFF-chunks
Merkle root van OFFF-representatie
```

### Voorbeeld metadata

```json
{
  "source_container": {
    "type": "E01",
    "container_sha256": "...",
    "tool_used": "libewf",
    "conversion_time": "2026-05-22T13:00:00Z"
  },
  "evidence_stream": {
    "stream_sha256": "..."
  }
}
```

### Acceptance criteria fase 7

```text
[ ] E01 kan worden geconverteerd naar OFFF
[ ] raw evidence stream wordt correct gehasht
[ ] E01-containerhash wordt apart vastgelegd
[ ] conversie is reproduceerbaar
[ ] export naar raw blijft mogelijk
[ ] provenance legt conversie volledig vast
```

---

## 13. Fase 8 – Specificatie, testset en conformance

### Doel

Maak van OFFF niet alleen software, maar een echte open standaard.

### Deliverables

Ontwikkel:

```text
OFFF specification v0.1
reference implementation
test corpus
conformance test suite
developer documentation
CLI documentation
schema documentation
```

### Specificatie moet bevatten

1. Terminologie
2. Container layout
3. Manifest schema
4. Acquisition schema
5. Chunk schema
6. Hashingregels
7. Merkle Tree definitie
8. Mappingtabellen
9. Provenance model
10. Indexformaten
11. Analyse-output schema’s
12. Validatieregels
13. Foutafhandeling
14. Versiebeheer
15. Backward compatibility-regels

### Conformance tests

Minimaal:

```text
[ ] valide kleine container
[ ] valide grote container
[ ] ontbrekende chunk
[ ] gewijzigde chunk
[ ] corrupte compressed chunk
[ ] incorrecte Merkle root
[ ] incorrecte source hash
[ ] ontbrekende provenance
[ ] foutieve manifestversie
[ ] export naar raw
```

---

## 14. Niet doen in de eerste release

Het team moet expliciet voorkomen dat OFFF v0.1 te breed wordt.

Niet meenemen in v0.1:

```text
[ ] volledige forensic suite bouwen
[ ] GUI bouwen
[ ] AI-analyse verplicht maken
[ ] live acquisition
[ ] memory forensics
[ ] mobile extraction
[ ] cloud API acquisition
[ ] deduplicatie
[ ] content-defined chunking
[ ] complexe encryptie
[ ] multi-case management
[ ] rechtbankrapportgenerator
```

Deze onderdelen mogen later worden voorbereid in het datamodel, maar niet blokkerend worden voor de eerste werkende release.

---

## 15. Minimale technische stack

Het team mag hiervan afwijken, maar alleen gemotiveerd.

### Aanbevolen

```text
Language: Rust of Go
Metadata: JSON / JSONL
Tabellen: Apache Parquet
Compressie: zstd
Hashing: SHA-256
Storage: local filesystem + later S3-compatible
CLI: cross-platform
Tests: unit, integration, property-based waar zinvol
```

### Waarom Rust of Go?

Voor dit type software zijn belangrijk:

```text
hoge performance
lage memory footprint
goede foutafhandeling
cross-platform distributie
veilige binaire verwerking
goede concurrency
```

Rust heeft voordelen rond memory safety. Go heeft voordelen rond eenvoud en snelheid van ontwikkeling. Kies één primaire taal en voorkom dat fase 1 direct polyglot wordt.

---

## 16. Security- en integriteitseisen

### 16.1 Geen stille fouten

Elke fout bij lezen, schrijven, comprimeren, decomprimeren, hashen of valideren moet expliciet worden gelogd.

### 16.2 Deterministische output

Bij gelijke input en gelijke parameters moet dezelfde OFFF-container ontstaan, met uitzondering van velden zoals timestamp en container-ID.

Voor testdoeleinden moet een deterministic mode beschikbaar zijn:

```bash
offf-convert \
  --input evidence.dd \
  --output case.offf \
  --deterministic
```

### 16.3 Geen impliciete normalisatie

Pas geen inhoudelijke normalisatie toe op evidence bytes.

Dus niet:

```text
line endings aanpassen
encoding corrigeren
lege ruimte overslaan
filesystem automatisch repareren
```

### 16.4 Cryptografische ondertekening

Niet verplicht in fase 1, maar het ontwerp moet ruimte bieden voor:

```text
manifest signature
provenance signature
case officer signature
tool signature
```

---

## 17. Logging en provenance

Elke tool moet provenance-events schrijven.

### Basisschema

```json
{
  "event_id": "evt-000001",
  "timestamp": "2026-05-22T10:21:44Z",
  "actor": "system-or-user",
  "action": "converted_raw_to_offf",
  "tool": {
    "name": "offf-convert",
    "version": "0.1.0"
  },
  "input": {
    "path": "evidence.dd",
    "sha256": "..."
  },
  "output": {
    "container": "case.offf",
    "source_sha256": "...",
    "merkle_root_sha256": "..."
  },
  "parameters": {
    "chunk_size": 67108864,
    "compression": "zstd",
    "hash_algorithm": "sha256"
  }
}
```

### Eisen

```text
[ ] elk commando schrijft een event
[ ] events zijn append-only
[ ] events worden niet overschreven
[ ] event bevat toolnaam en versie
[ ] event bevat input en output
[ ] event bevat relevante parameters
```

---

## 18. Verwerking van leesfouten en bad sectors

Hoewel fase 1 start met `raw/dd`-input, moet het datamodel voorbereid zijn op fysieke acquisitie met leesfouten.

Leg per probleem vast:

```json
{
  "source_offset": 123456789,
  "length": 512,
  "error": "unreadable_sector",
  "fill_policy": "zero_fill",
  "device_reported_error": "UNC"
}
```

Belangrijk:

```text
Een unreadable sector mag nooit verdwijnen.
Een fill policy moet expliciet worden vastgelegd.
Een analysehit mag nooit worden gebaseerd op bytes zonder duidelijke herkomst.
```

---

## 19. Definitie van klaar

Het project is niet klaar als er alleen een container wordt geschreven. Het is pas klaar wanneer het team kan aantonen:

```text
1. We kunnen een raw image converteren naar OFFF.
2. We kunnen OFFF valideren.
3. We kunnen OFFF terug converteren naar raw.
4. De hash van de oorspronkelijke raw is gelijk aan de hash van de gereconstrueerde raw.
5. Elke chunk is afzonderlijk valideerbaar.
6. De Merkle root klopt.
7. De mapping van fysieke offset naar chunk klopt.
8. Een worker kan een subset verwerken zonder de hele image te downloaden.
9. Analyse-output is herleidbaar naar chunk en offset.
10. Evidence en analyse zijn strikt gescheiden.
```

---

## 20. Samenvattende opdracht aan het ontwikkelteam

Ontwikkel OFFF gefaseerd, waarbij de eerste release niet probeert een volledige forensische suite te zijn, maar een **forensisch betrouwbare, open en distributed-ready evidence container**.

De prioriteit is:

```text
Eerst bewijsbaarheid.
Dan indexeerbaarheid.
Dan distributed processing.
Dan analyse.
Dan ecosysteem.
```

De eerste mijlpaal is geslaagd wanneer een `raw/dd`-image naar OFFF kan worden geconverteerd, gevalideerd en byte-exact terug kan worden geëxporteerd naar `raw/dd`.

Alles wat daarna wordt toegevoegd, moet dit bewijsmodel respecteren.

---

## 21. Roadmapoverzicht

| Fase | Naam | Resultaat |
|---|---|---|
| 1 | Evidence Container MVP | Raw/dd naar OFFF, verify en export terug naar raw |
| 2 | Partitie- en volumemapping | MBR/GPT detectie en mapping naar chunks |
| 3 | Filesystem Indexing | Start met NTFS, file index en extents |
| 4 | Distributed Processing Framework | Jobmodel, workers en analyse-output |
| 5 | Object Storage en schaalbaarheid | MinIO/S3-compatible verwerking |
| 6 | Analyse- en annotatielaag | Append-only menselijke en automatische annotaties |
| 7 | Conversie bestaande formaten | E01/EWF en mogelijk AFF4 naar OFFF |
| 8 | Specificatie en conformance | Open standaard, tests en referentie-implementatie |

---

## 22. Kernzin

OFFF is niet alleen een opslagformaat voor forensische kopieën, maar een bewijsbaar verwerkingsformaat voor schaalbare digitale opsporing.
