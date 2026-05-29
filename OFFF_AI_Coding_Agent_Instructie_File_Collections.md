# AI Coding Agent Instructie — OFFF ondersteuning voor losse bestanden en file collections

## Rol

Je bent een senior software engineer en werkt aan de repository voor **Open Forensic File Format (OFFF)**.

Je opdracht is om OFFF uit te breiden zodat het naast volledige forensic images ook **losse bestanden, mappen en file collections** kan ondersteunen als geldige evidence-bron.

Gebruik **Claude Sonnet 4.6** als coding agent en voer deze opdracht gefaseerd uit.

---

## 1. Context

OFFF is tot nu toe vooral ontworpen rond het scenario:

```text
raw/dd/E01 image
→ OFFF container
→ chunks
→ physical offsets
→ partitions
→ filesystems
→ files/artifacts
→ analysis jobs
```

In de praktijk wordt echter niet altijd een volledige laptop, computer of harde schijf veiliggesteld. Soms worden alleen specifieke bestanden, mappen of exports in beslag genomen.

Voorbeelden:

```text
- losse Word-, PDF-, Excel- of tekstbestanden
- een map met geselecteerde bestanden
- een mailboxbestand
- een ZIP/RAR/7z-archief
- een export uit een cloudomgeving
- een mobile logical extraction
- een USB-selectie
- een subset van een netwerkshare
- door een opsporingsambtenaar geselecteerde bestanden
```

Daarom moet OFFF niet alleen **image-centric** zijn, maar vooral **evidence-object-centric**.

Kernprincipe:

```text
Een volledige disk image is één mogelijke evidence root.
Een file collection, logical extraction of API export kan ook een evidence root zijn.
```

---

## 2. Doel van deze opdracht

Breid OFFF uit met ondersteuning voor:

```text
1. acquisition_mode = file_collection
2. evidence roots anders dan block images
3. losse bestanden als root evidence objects
4. content-addressed opslag van evidence files
5. object index voor file collections
6. acquisition metadata en limitations voor selectieve veiligstelling
7. verified reads voor file evidence objects
8. analyseworkers die transparant op image-based en file-based objects werken
9. verifier checks voor file collections
10. CLI-tooling om een map/losse bestanden naar OFFF te converteren
```

De uitbreiding moet zo worden ontworpen dat bestaande image-based OFFF-functionaliteit blijft werken.

---

## 3. Belangrijkste ontwerpkeuze

OFFF moet meerdere bronvormen ondersteunen.

Introduceer of formaliseer:

```text
Evidence Root
Source Object
Evidence Object
Acquisition Mode
Object Index
Evidence Object Store
```

Vermijd dat OFFF Core veronderstelt dat er altijd fysieke offsets, partities of filesystems zijn.

---

## 4. Nieuwe acquisition modes

Voeg aan de OFFF-specificatie en implementatie een generiek veld toe:

```json
{
  "acquisition_mode": "file_collection"
}
```

Ondersteun minimaal deze waarden:

```text
block_image
file_collection
logical_extraction
api_export
mixed
```

Voor deze opdracht moet minimaal `file_collection` volledig werken.

| acquisition_mode | Betekenis |
|---|---|
| `block_image` | Volledige byte-stream van gegevensdrager, zoals raw/dd/E01 |
| `file_collection` | Verzameling losse bestanden/mappen als evidence |
| `logical_extraction` | Logische extractie uit device/app/cloud/mailbox |
| `api_export` | Export via API uit extern systeem |
| `mixed` | Combinatie van meerdere evidence roots |

---

## 5. Manifest aanpassen

### 5.1 Huidige aanname

De huidige manifeststructuur lijkt sterk uit te gaan van één source image met:

```text
source_sha256
chunking
physical_to_chunk
source_type
```

Dat blijft geldig voor `block_image`, maar mag niet verplicht zijn voor `file_collection`.

### 5.2 Nieuwe generieke manifeststructuur

Breid `manifest.json` uit met:

```json
{
  "offf_version": "0.2.0",
  "container_id": "urn:offf:case:demo-001",
  "acquisition_mode": "file_collection",
  "evidence_roots": [
    {
      "root_id": "root-collection-001",
      "root_type": "file_collection",
      "description": "Selected files seized during search",
      "object_count": 15,
      "root_hash": "sha256:..."
    }
  ],
  "indexes": {
    "object_index": "indexes/objects/object_index.parquet",
    "object_edges": "indexes/objects/object_edges.parquet",
    "derivations": "indexes/objects/derivations.parquet"
  },
  "limitations": [
    "No full disk image available",
    "No physical sector offsets available",
    "No unallocated space captured",
    "Filesystem context may be incomplete"
  ]
}
```

### 5.3 Regels

```text
- Bij block_image blijven chunks, hashes en maps verplicht.
- Bij file_collection is object_index verplicht.
- Bij file_collection zijn physical_to_chunk, partition_table en filesystem_index optioneel/niet van toepassing.
- Manifest moet expliciet de limitations vastleggen.
```

### 5.4 Acceptance criteria

```text
[ ] Manifest ondersteunt acquisition_mode.
[ ] Manifest ondersteunt evidence_roots.
[ ] block_image containers blijven valide.
[ ] file_collection containers zijn valide zonder physical_to_chunk.parquet.
[ ] Verifier herkent per acquisition_mode welke onderdelen verplicht zijn.
```

---

## 6. Acquisition metadata aanpassen

### 6.1 Doel

`acquisition.json` moet kunnen beschrijven hoe losse bestanden zijn veiliggesteld.

Voorbeeld:

```json
{
  "acquisition_id": "acq-000001",
  "acquisition_mode": "file_collection",
  "acquired_at": "2026-05-28T10:00:00Z",
  "acquired_by": "user-or-system",
  "method": "selected_file_collection",
  "tool": {
    "name": "offf-collect",
    "version": "0.1.0"
  },
  "source_context": {
    "description": "Selected files from seized laptop export",
    "original_root_path": "/Users/example/Documents",
    "collection_reason": "Files selected during forensic triage"
  },
  "limitations": [
    "No complete disk image",
    "No deleted files",
    "No unallocated space",
    "Original filesystem context partially preserved"
  ],
  "hash_algorithm": "sha256"
}
```

### 6.2 Acceptance criteria

```text
[ ] acquisition.json ondersteunt file_collection metadata.
[ ] limitations worden verplicht bij file_collection.
[ ] acquired_by, acquired_at, method en tool worden vastgelegd.
[ ] Verifier controleert verplichte velden.
```

---

## 7. Evidence object store voor losse bestanden

### 7.1 Doel

Losse evidence files moeten content-addressed worden opgeslagen op basis van SHA-256.

Aanbevolen doelstructuur:

```text
evidence/
  objects/
    sha256/
      ab/
        cd/
          <sha256>.bin
```

### 7.2 Regels

```text
- Elk los bestand wordt gehasht vóór opslag.
- Bestandsnaam in store is afgeleid van SHA-256.
- Als object al bestaat, verifieer bestaande bytes vóór hergebruik.
- Evidence object store is immutable.
- Originele bestandsnaam wordt alleen in object metadata opgeslagen.
```

### 7.3 Acceptance criteria

```text
[ ] Losse bestanden worden content-addressed opgeslagen.
[ ] Duplicate bestanden worden gededupliceerd op hash.
[ ] Bestaande objecten worden geverifieerd voordat ze worden hergebruikt.
[ ] Hash mismatch faalt veilig.
[ ] Workers schrijven nooit naar evidence/objects.
```

---

## 8. Object index als primaire index

### 8.1 Doel

Bij file collections is er geen fysieke disk-index. Daarom is `object_index` de primaire bronindex.

Locatie:

```text
indexes/
  objects/
    object_index.parquet
```

Voor demo/ontwikkelgemak mag JSONL tijdelijk ondersteund worden, maar Parquet moet het doelmodel zijn.

### 8.2 ObjectIndexRow voor evidence files

Voeg of formaliseer dit rowmodel:

```json
{
  "object_id": "obj-file-000001",
  "object_type": "evidence_file",
  "name": "contract.docx",
  "logical_path": "/selected-files/contract.docx",
  "media_type": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  "size_bytes": 183422,
  "sha256": "sha256:...",
  "source_layer": "evidence",
  "storage_ref": "evidence/objects/sha256/ab/cd/<sha256>.bin",
  "root_id": "root-collection-001",
  "parser_status": "not_parsed",
  "provenance_ref": "evt-acquisition-000001",
  "metadata": {
    "original_created_at": null,
    "original_modified_at": "2026-05-20T12:00:00Z",
    "original_accessed_at": null,
    "original_path_available": true
  }
}
```

### 8.3 Vereiste velden

Minimaal verplicht:

```text
object_id
object_type
name
size_bytes
sha256
source_layer
storage_ref
root_id
parser_status
provenance_ref
```

Aanbevolen:

```text
logical_path
media_type
metadata.original_modified_at
metadata.original_created_at
collection_relative_path
```

### 8.4 Acceptance criteria

```text
[ ] Elke evidence file krijgt een object_index row.
[ ] Elk object verwijst naar root_id.
[ ] Elk object heeft sha256 en storage_ref.
[ ] Object index kan door analysis workers worden gebruikt.
[ ] Verifier controleert dat storage_ref bestaat en sha256 klopt.
```

---

## 9. Object edges voor file collections

### 9.1 Doel

Bij file collections kan een root collection object parent zijn van de losse files.

```text
root-collection-001
├── obj-file-000001
├── obj-file-000002
└── obj-file-000003
```

Leg dit vast in:

```text
indexes/objects/object_edges.parquet
```

### 9.2 Edge model

Voorbeeld:

```json
{
  "edge_id": "edge-000001",
  "parent_object_id": "root-collection-001",
  "child_object_id": "obj-file-000001",
  "relation_type": "contains",
  "method": "file_collection_ingest",
  "logical_path": "/selected-files/contract.docx",
  "sequence": 1,
  "created_by_job_id": null,
  "provenance_ref": "evt-acquisition-000001",
  "schema_version": "0.2.0"
}
```

### 9.3 Acceptance criteria

```text
[ ] Root collection heeft edges naar alle evidence files.
[ ] Edges verwijzen naar bestaande objecten.
[ ] Verifier kan collection → file lineage valideren.
```

---

## 10. Collection root object

### 10.1 Doel

Maak van de file collection zelf ook een object.

Voorbeeld:

```json
{
  "object_id": "root-collection-001",
  "object_type": "evidence_collection",
  "name": "Selected files collection",
  "logical_path": "/",
  "media_type": null,
  "size_bytes": null,
  "sha256": null,
  "source_layer": "evidence",
  "storage_ref": null,
  "root_id": "root-collection-001",
  "parser_status": "not_applicable",
  "provenance_ref": "evt-acquisition-000001"
}
```

### 10.2 Collection root hash

Voor een collection is er geen enkele byte-stream. Introduceer daarom optioneel een deterministic `collection_manifest_hash`.

Bereken deze over een canonieke lijst van:

```text
object_id
logical_path
size_bytes
sha256
storage_ref
```

Gesorteerd op `logical_path` of `object_id`.

### 10.3 Acceptance criteria

```text
[ ] File collection heeft root object.
[ ] Manifest evidence_roots verwijst naar root object.
[ ] Collection root hash wordt deterministisch berekend.
[ ] Verifier kan collection root hash opnieuw berekenen.
```

---

## 11. CLI toevoegen: `offf-collect`

### 11.1 Doel

Maak een CLI-tool waarmee losse bestanden of een map naar OFFF kunnen worden omgezet.

Nieuwe crate of binary:

```text
offf-collect
```

Of subcommand binnen bestaande convert-tool:

```bash
offf-convert collect --input ./selected-files --output case.offf
```

Kies de stijl die het best past bij de bestaande repo.

### 11.2 CLI voorbeelden

Map converteren:

```bash
offf-collect   --input ./selected-files   --output case.offf   --case-id demo-file-collection-001
```

Meerdere losse bestanden:

```bash
offf-collect   --input ./contract.docx   --input ./mailbox.pst   --input ./archive.zip   --output case.offf
```

Opties:

```bash
--preserve-paths true
--hash sha256
--detect-mime true
--follow-symlinks false
--include-hidden false
--deterministic
```

### 11.3 Gedrag

```text
1. Valideer input paths.
2. Maak temporary output directory.
3. Maak manifest/acquisition.
4. Maak root collection object.
5. Loop door alle files.
6. Hash elk bestand.
7. Kopieer bytes naar evidence/objects/sha256/...
8. Maak object_index row.
9. Maak object_edge row root → file.
10. Schrijf provenance event.
11. Bereken collection root hash.
12. Schrijf manifest als laatste.
13. Atomic rename tmp → final output.
```

### 11.4 Acceptance criteria

```text
[ ] offf-collect kan een directory omzetten naar OFFF.
[ ] offf-collect kan meerdere losse inputbestanden verwerken.
[ ] Output is file_collection OFFF-container.
[ ] Duplicate files worden gededupliceerd op hash.
[ ] Manifest wordt als laatste geschreven.
[ ] Incomplete output wordt niet als geldig gezien.
[ ] Deterministic mode produceert reproduceerbare metadata/hashes.
```

---

## 12. Verified read API uitbreiden

### 12.1 Doel

Analysis workers moeten transparant kunnen werken op:

```text
- file uit block image
- evidence file uit file collection
- derived object
```

Zij moeten niet hoeven weten welk acquisition_mode is gebruikt.

### 12.2 Nieuwe of aangepaste functies

Voeg toe aan core/SDK:

```rust
read_object_verified(container: &ContainerRef, object_id: &str) -> Result<Vec<u8>, OfffError>;

read_evidence_file_verified(container: &ContainerRef, object_id: &str) -> Result<Vec<u8>, OfffError>;

compute_object_sha256(container: &ContainerRef, object_id: &str) -> Result<String, OfffError>;

get_object_lineage(container: &ContainerRef, object_id: &str) -> Result<LineageReport, OfffError>;
```

### 12.3 Gedrag bij file_collection

```text
1. Zoek object_id in object_index.
2. Lees storage_ref.
3. Lees file bytes uit evidence/objects.
4. Bereken SHA-256.
5. Vergelijk met object_index.sha256.
6. Return bytes.
```

### 12.4 Gedrag bij block_image

Bestaand of later:

```text
1. Zoek object_id/file_id.
2. Gebruik physical_extents.
3. Lees chunks verified.
4. Reconstrueer file bytes.
5. Bereken SHA-256.
6. Return bytes.
```

### 12.5 Acceptance criteria

```text
[ ] read_object_verified werkt voor file_collection evidence files.
[ ] read_object_verified blijft uitbreidbaar voor block_image files.
[ ] Hash mismatch leidt tot error.
[ ] Workers kunnen dezelfde API gebruiken voor beide modes.
```

---

## 13. Verifier aanpassen

### 13.1 Doel

`offf-verify` moet acquisition-mode aware worden.

Voor `block_image` controleert de verifier:

```text
manifest
acquisition
chunks
hashes
physical_to_chunk
Merkle root
source hash
```

Voor `file_collection` controleert de verifier:

```text
manifest
acquisition
evidence_roots
object_index
object_edges
evidence object store
file hashes
collection root hash
provenance
limitations
```

### 13.2 CLI

```bash
offf-verify case.offf
offf-verify case.offf --profile core
offf-verify case.offf --profile core+objects
offf-verify case.offf --object obj-file-000001
offf-verify case.offf --lineage obj-file-000001
```

### 13.3 Checks voor file_collection

```text
[ ] manifest.acquisition_mode == file_collection
[ ] evidence_roots bestaat
[ ] acquisition limitations bestaan
[ ] root collection object bestaat
[ ] object_index bestaat
[ ] object_edges bestaat
[ ] elk evidence_file object heeft storage_ref
[ ] elk storage_ref bestaat
[ ] elk storage_ref SHA-256 klopt
[ ] root → file edges bestaan
[ ] collection root hash klopt
[ ] provenance_ref bestaat
```

### 13.4 Acceptance criteria

```text
[ ] Verifier accepteert geldige file_collection container.
[ ] Verifier faalt bij ontbrekend evidence object.
[ ] Verifier faalt bij hash mismatch.
[ ] Verifier faalt bij ontbrekende object_index.
[ ] Verifier geeft duidelijke melding dat physical_to_chunk niet vereist is bij file_collection.
```

---

## 14. Analysis workers aanpassen

### 14.1 Doel

Workers mogen niet afhankelijk zijn van block-image aannames.

Aanpassen:

```text
input object lezen via read_object_verified()
niet via direct filesystem pad of chunk-only API
target altijd object_id
source_refs uit object_index gebruiken
```

### 14.2 Voorbeeld result row

```json
{
  "result_id": "result-000001",
  "job_id": "job-text-001",
  "result_type": "extracted_text",
  "target": {
    "type": "object",
    "id": "obj-file-000001"
  },
  "source_refs": {
    "root_id": "root-collection-001",
    "source_layer": "evidence",
    "sha256": "sha256:...",
    "storage_ref": "evidence/objects/sha256/ab/cd/<hash>.bin"
  },
  "status": "success",
  "data": {
    "text": "..."
  }
}
```

### 14.3 Acceptance criteria

```text
[ ] Tika/text worker werkt op file_collection.
[ ] Elasticsearch indexing worker behoudt object_id en source_refs.
[ ] Classification worker behoudt object_id en source_refs.
[ ] Workers schrijven geen absolute host paths in output.
[ ] Workers gebruiken storage_ref alleen als OFFF-interne referentie.
```

---

## 15. Limitations en transparantie

Bij file collections moet OFFF expliciet vastleggen dat bepaalde context ontbreekt.

Voeg aan manifest/acquisition toe:

```json
{
  "limitations": [
    "No full disk image available",
    "No physical sector offsets available",
    "No unallocated space captured",
    "Filesystem metadata may be incomplete",
    "Directory context may be partial"
  ]
}
```

### Acceptance criteria

```text
[ ] file_collection zonder limitations geeft verifier warning of error.
[ ] Report/summary toont limitations.
[ ] Documentation legt verschil uit tussen block_image en file_collection.
```

---

## 16. Tests

### 16.1 Unit tests

Voeg tests toe voor:

```text
content-addressed evidence object path
hashing losse bestanden
duplicate file dedup
object index row generation
collection root hash
read_object_verified
manifest validation per acquisition_mode
```

### 16.2 Integration tests

Maak testcases:

#### Test 1 — Single file

```text
input: contract.docx
output: file_collection OFFF
verify: valid
```

#### Test 2 — Directory with multiple files

```text
input: selected-files/
output: file_collection OFFF
verify: valid
```

#### Test 3 — Duplicate files

```text
input: two identical files
expected: one stored object, two object_index rows or two logical objects referencing same storage_ref
```

#### Test 4 — Tamper evidence object

```text
modify evidence/objects/...bin
expected: verifier fails hash check
```

#### Test 5 — Analysis on file_collection

```text
run text extraction worker
expected: analysis/jobs/... result_manifest valid
```

### 16.3 Acceptance criteria

```text
[ ] Tests draaien in CI.
[ ] block_image tests blijven slagen.
[ ] file_collection tests zijn toegevoegd.
[ ] Tamper tests tonen hash mismatch.
[ ] Analysis worker test werkt op file_collection.
```

---

## 17. Documentatie

### 17.1 README aanpassen

Voeg sectie toe:

```text
OFFF acquisition modes
```

Beschrijf:

```text
block_image
file_collection
logical_extraction
api_export
mixed
```

### 17.2 Nieuwe docs

Maak:

```text
docs/file-collection-mode.md
```

Inhoud:

```text
- wat is file_collection mode
- wanneer gebruik je dit
- wat wordt wel/niet vastgelegd
- directorystructuur
- manifest voorbeeld
- acquisition voorbeeld
- object_index voorbeeld
- verifier gedrag
- beperkingen t.o.v. full disk image
```

### 17.3 Acceptance criteria

```text
[ ] Documentatie noemt dat OFFF niet alleen image-based is.
[ ] Documentatie legt beperkingen van losse bestanden eerlijk uit.
[ ] Voorbeelden zijn reproduceerbaar.
```

---

## 18. Demo-integratie

Breid de bestaande Docker-demo uit zodat `create_demo_case.py` twee opties ondersteunt:

```bash
python scripts/create_demo_case.py --mode file_collection
python scripts/create_demo_case.py --mode block_image_mock
```

Voor file_collection:

```text
- maak demo documenten
- run offf-collect
- run Tika worker
- run Elasticsearch index worker
- run classifier worker
- run verifier
```

### 18.1 Demo acceptance criteria

```text
[ ] Demo kan starten met losse bestanden.
[ ] OFFF-container is acquisition_mode=file_collection.
[ ] Tika worker verwerkt evidence file objects.
[ ] Elasticsearch en classifier blijven werken.
[ ] Verifier valideert file_collection en analysis jobs.
```

---

## 19. Backward compatibility

### 19.1 Regels

```text
- Bestaande block_image containers blijven geldig.
- source_sha256 en chunking blijven bestaan voor block_image.
- Nieuwe file_collection velden mogen block_image niet breken.
- Verifier kiest verplichte checks op basis van acquisition_mode.
```

### 19.2 Acceptance criteria

```text
[ ] Oude convert/verify flow blijft werken.
[ ] Nieuwe file_collection flow werkt naast bestaande flow.
[ ] Manifest schema ondersteunt beide zonder ambiguïteit.
```

---

## 20. Implementatievolgorde

### P0 — Minimale ondersteuning

```text
[ ] acquisition_mode toevoegen
[ ] evidence_roots toevoegen aan manifest
[ ] file_collection acquisition metadata
[ ] evidence/objects content-addressed store
[ ] object_index rows voor evidence files
[ ] root collection object
[ ] root → file edges
[ ] offf-collect CLI
[ ] read_object_verified voor file_collection
[ ] verifier checks voor file_collection
```

### P1 — Analysis integratie

```text
[ ] Tika/text worker gebruikt read_object_verified
[ ] Elasticsearch worker behoudt source_refs
[ ] classifier worker behoudt source_refs
[ ] result rows verwijzen naar object_id
[ ] verifier valideert analysis jobs op file_collection
```

### P2 — Volwassenheid

```text
[ ] collection root hash canoniek maken
[ ] Parquet object_index volledig ondersteunen
[ ] mixed mode ondersteunen
[ ] logical_extraction voorbereiden
[ ] API endpoints voor file_collection objects
[ ] lineage reports voor file_collection
```

---

## 21. Definitie van klaar

De opdracht is klaar als:

```text
[ ] OFFF kan een map met losse bestanden converteren naar een valide file_collection container.
[ ] Elk bestand wordt gehasht en content-addressed opgeslagen.
[ ] Elk bestand krijgt een object_id en object_index row.
[ ] Root collection object en edges zijn aanwezig.
[ ] Manifest en acquisition leggen acquisition_mode en limitations vast.
[ ] Verifier valideert file_collection containers.
[ ] read_object_verified werkt op losse evidence files.
[ ] Analysis workers kunnen file_collection objects verwerken zonder speciale toollogica.
[ ] Bestaande block_image functionaliteit blijft werken.
```

---

## 22. Kernzin voor ontwerpbeslissingen

```text
OFFF moet evidence-object-centric zijn, niet image-centric.
Een disk image, een los bestand, een mapselectie, een logical extraction en een API export
zijn allemaal mogelijke evidence roots binnen dezelfde bewijsbare verwerkingsketen.
```

---

## 23. Niet doen

Vermijd deze fouten:

```text
[ ] Niet doen alsof losse bestanden fysieke diskcontext hebben.
[ ] Geen fake physical offsets genereren.
[ ] Geen verplicht physical_to_chunk.parquet bij file_collection.
[ ] Geen absolute host paths opslaan als primaire storage_ref.
[ ] Geen evidence files opslaan onder analysis/.
[ ] Geen analysis workers direct host paths laten gebruiken.
[ ] Geen file_collection beperkingen verbergen.
```

---

## 24. Verwachte voorbeeldstructuur

Na succesvolle `offf-collect`:

```text
case.offf/
  manifest.json
  acquisition.json

  evidence/
    objects/
      sha256/
        ab/
          cd/
            <sha256>.bin

  indexes/
    objects/
      object_index.parquet
      object_edges.parquet
      derivations.parquet

  analysis/
    jobs/

  provenance/
    chain_of_custody.jsonl

  audit/
```

Voor tijdelijke demo mag dit zijn:

```text
case.offf/
  manifest.json
  acquisition.json
  evidence_files/
  indexes/
    objects/
      object_index.jsonl
      object_edges.jsonl
      derivations.jsonl
  analysis/
    jobs/
  provenance/
    chain_of_custody.jsonl
```

Maar het doelmodel blijft content-addressed evidence object storage.
