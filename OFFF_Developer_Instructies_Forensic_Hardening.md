# Ontwikkelinstructie: OFFF verbeteren naar forensic-grade platform

## Doel

Dit document bevat concrete instructies voor het ontwikkelteam om de belangrijkste tekortkomingen, kritische risico’s en verbeterpunten in de huidige `openforensicfileformat` repository op te lossen.

De huidige repository bevat een sterke OFFF v0.1-basis met onder andere Rust crates voor core, convert, verify, export, index, jobs, keyword/YARA workers, access service, SDK’s en schema’s. De volgende stap is de overgang van een werkende MVP naar een **forensic-grade, append-only, distributed, scope-aware en conformance-testbaar platform**.

De kernopdracht:

```text
Verhard eerst de forensische kern.
Voeg daarna pas nieuwe analysefunctionaliteit toe.
```

---

## 1. Ontwerpprincipes

### 1.1 Evidence blijft immutable

Na finalisatie van een OFFF-container mag de evidence layer nooit meer gewijzigd worden.

Onder evidence layer vallen minimaal:

```text
manifest.json
acquisition.json
chunks/
hashes/
maps/
source hash
chunk hashes
Merkle tree
```

Wijzigingen mogen alleen nog plaatsvinden in append-only lagen:

```text
provenance/
analysis/
extensions/
audit/
jobs/
```

### 1.2 Analyse is append-only

Analyse-output mag niet stilzwijgend worden overschreven.

Niet toestaan:

```text
analysis/keyword_hits.parquet wordt overschreven door een nieuwe job
analysis/yara_hits.parquet wordt overschreven door een nieuwe job
bestaande result files worden aangepast
```

Wel toestaan:

```text
analysis/jobs/{job_id}/keyword_hits.parquet
analysis/jobs/{job_id}/yara_hits.parquet
analysis/jobs/{job_id}/result_manifest.json
analysis/events/analysis_events.jsonl
```

Correcties worden vastgelegd als nieuwe events, niet als mutatie van bestaande output.

### 1.3 Distributed processing vereist bewijs per subset

Een worker moet kunnen aantonen dat de chunk of subset die hij verwerkt:

```text
onderdeel is van de OFFF-container
past binnen de Merkle root
past binnen de job scope
past binnen include/exclude sets
niet onder uitgesloten labels valt
```

### 1.4 Core blijft juridisch neutraal

OFFF Core mag geen specifieke juridische regimes hardcoderen.

Niet opnemen in Core:

```text
landeck_status
verschoningsrecht
rechter_commissaris_beslissing
geheimhouder
```

Wel opnemen als generieke extensiepunten:

```text
labels
scopes
sets
decisions
policy_refs
access_events
denied_access_events
audit_events
```

De betekenis van deze generieke objecten wordt bepaald door tooling en beleid bovenop OFFF.

---

# Fase 0 — Stabiliseer repository en documentatie

## 0.1 Voeg een standaard root README toe

### Probleem

Er is documentatie aanwezig, maar de standaard `README.md` moet het centrale entrypoint worden voor developers en consumers.

### Taak

Maak een root `README.md`.

### Minimale inhoud

```text
- Wat is OFFF?
- Waarom OFFF?
- Status van het project
- Architectuur-overzicht
- Quickstart
- Build-instructies
- Voorbeeld: raw/dd naar OFFF
- Voorbeeld: verify
- Voorbeeld: export
- Voorbeeld: index partitions
- Voorbeeld: keyword job
- Projectstructuur
- Stabiliteitsmatrix
- Licentie
```

### Voorbeeld quickstart

```bash
cargo build --workspace

cargo run -p offf-convert -- \
  --input sample.dd \
  --output sample.offf \
  --chunk-size 64M \
  --compression zstd

cargo run -p offf-verify -- sample.offf

cargo run -p offf-export -- sample.offf --output reconstructed.dd

cargo run -p offf-index -- partitions sample.offf
```

### Acceptance criteria

```text
[ ] Root README.md bestaat.
[ ] Nieuwe developer kan binnen 15 minuten een sample converteren, verifiëren en exporteren.
[ ] README bevat status: stable / experimental / planned.
[ ] README verwijst naar formele specificatie en schema’s.
```

---

## 0.2 Maak een statusmatrix

### Taak

Voeg `docs/status.md` toe.

### Minimale matrix

| Component | Status | Opmerking |
|---|---:|---|
| OFFF Core chunk store | stable-mvp | Hashing werkt, hardening nodig |
| Merkle root | stable-mvp | Proofs ontbreken |
| raw/dd convert | stable-mvp | Crash-safe finalisatie ontbreekt |
| E01 convert | experimental | Afhankelijk van ewfexport/libewf |
| verify | stable-mvp | Schema/conformance uitbreiden |
| export | stable-mvp | S3 support toevoegen |
| MBR/GPT index | experimental | GPT CRC en EBR ontbreken |
| NTFS index | experimental | Nog niet volledig forensisch dekkend |
| keyword worker | experimental | Output isolation nodig |
| YARA worker | experimental | Output isolation nodig |
| Access Service | experimental | Auth hardening nodig |
| Python SDK | experimental | Local-only, memory cache beperken |
| Go SDK | experimental | API-parity bewaken |
| Extensions v0.2 | planned | Generieke extensiepunten toevoegen |

### Acceptance criteria

```text
[ ] Statusmatrix staat in docs/status.md.
[ ] README linkt naar docs/status.md.
[ ] Iedere experimental component heeft bekende beperkingen.
```

---

# Fase 1 — Evidence container hardening

## 1.1 Maak conversie crash-safe en atomair

### Probleem

De huidige conversie schrijft direct naar de outputcontainer. Bij crash of fout kan een gedeeltelijke container achterblijven.

### Taak

Pas `offf-convert` aan zodat het werkt met een tijdelijke outputdirectory.

### Gewenst patroon

```text
1. Schrijf naar {output}.tmp-{uuid}
2. Maak directorystructuur aan
3. Schrijf chunks
4. Schrijf maps/hash/indexbestanden
5. Schrijf acquisition.json
6. Schrijf provenance
7. Schrijf manifest.json als laatste
8. Voer interne self-check uit
9. Rename tmp naar definitieve output
10. Ruim tmp op bij fout
```

### Belangrijke regel

`manifest.json` is het finalisatiepunt. Een container zonder manifest mag niet als geldige OFFF-container worden gezien.

### Acceptance criteria

```text
[ ] Bij crash vóór finalisatie blijft geen valide container achter.
[ ] manifest.json wordt als laatste geschreven.
[ ] Bij fout wordt tmp-directory opgeruimd of duidelijk als incomplete gemarkeerd.
[ ] Atomic rename wordt gebruikt voor lokale filesystemen.
[ ] Voor S3/object storage wordt een finalization marker gebruikt, bijvoorbeeld _OFFF_COMPLETE.
```

---

## 1.2 Verifieer bestaande chunks vóór deduplicatie-skip

### Probleem

`write_chunk` slaat schrijven over als het chunkbestand al bestaat. Dat is efficiënt, maar forensisch risicovol als het bestaande bestand corrupt is.

### Taak

Pas `write_chunk` aan.

### Gewenst gedrag

```text
Als chunk path niet bestaat:
  - schrijf chunk
  - fsync waar mogelijk

Als chunk path wel bestaat:
  - lees stored bytes
  - controleer stored_sha256
  - decompress indien nodig
  - controleer plaintext_sha256
  - alleen bij volledige match overslaan
  - bij mismatch: error en conversie stoppen
```

### Acceptance criteria

```text
[ ] Corrupt bestaande chunk wordt gedetecteerd.
[ ] Bestaande correcte chunk wordt hergebruikt.
[ ] Unit test aanwezig voor corrupt existing chunk.
[ ] Unit test aanwezig voor valid existing chunk.
```

---

## 1.3 Maak sector size configureerbaar

### Probleem

Sector size staat impliciet/hardcoded op 512 bytes. Dat is niet universeel.

### Taak

Voeg parameter toe aan `offf-convert`:

```bash
--sector-size 512
```

Default blijft `512`.

Bij E01 moet, waar mogelijk, sector size uit bronmetadata worden gehaald. Als dit niet betrouwbaar kan, gebruik expliciete parameter en leg deze vast in `acquisition.json`.

### Acceptance criteria

```text
[ ] --sector-size parameter bestaat.
[ ] manifest.source.sector_size gebruikt deze waarde.
[ ] acquisition.parameters bevat sector_size.
[ ] Tests voor 512 en 4096 bytes.
```

---

## 1.4 Maak deterministic mode echt deterministisch

### Probleem

`--deterministic` gebruikt een deterministic container ID, maar timestamps blijven variabel.

### Taak

Pas deterministic mode aan.

Als `--deterministic` actief is:

```text
- container_id is afgeleid van source_sha256
- created_at is vaste waarde of afgeleid van source hash
- acquired_at is vaste waarde of afgeleid van source hash
- provenance event IDs zijn stabiel
- JSON-output is canoniek waar mogelijk
```

### Acceptance criteria

```text
[ ] Twee runs met dezelfde input en parameters leveren byte-equivalente metadata.
[ ] Deterministic output heeft dezelfde manifest hash.
[ ] Test aanwezig voor repeated deterministic conversion.
```

---

# Fase 2 — Merkle proof en distributed validation

## 2.1 Voeg Merkle inclusion proofs toe

### Probleem

De Merkle root is aanwezig, maar workers kunnen nog niet aantonen dat een individuele chunk onderdeel is van de container zonder volledige herberekening.

### Taak

Implementeer Merkle proof generatie en verificatie.

### Nieuwe functies in `offf-core`

```rust
generate_merkle_proof(leaf_hashes: &[String], sequence: u64) -> Result<MerkleProof, OfffError>
verify_merkle_proof(leaf_hash: &str, sequence: u64, proof: &MerkleProof, expected_root: &str) -> Result<bool, OfffError>
```

### Conceptueel proof-object

```json
{
  "algorithm": "sha256",
  "tree_version": "0x01",
  "leaf_sequence": 12,
  "leaf_hash": "sha256:...",
  "siblings": [
    { "position": "right", "hash": "sha256:..." },
    { "position": "left", "hash": "sha256:..." }
  ],
  "root": "sha256:..."
}
```

### CLI

Voeg toe:

```bash
offf-verify case.offf --chunk sha256:... --proof
offf-proof generate case.offf --chunk sha256:...
offf-proof verify --proof proof.json --root sha256:...
```

Dit mag in `offf-verify` of in een nieuwe crate `offf-proof`.

### Acceptance criteria

```text
[ ] Proof kan worden gegenereerd voor elke chunk sequence.
[ ] Proof valideert tegen manifest Merkle root.
[ ] Proof faalt bij gewijzigde leaf.
[ ] Proof faalt bij gewijzigde sibling.
[ ] Proof werkt voor oneven aantal leaves.
[ ] Proof werkt voor single-leaf container.
```

---

## 2.2 Valideer `merkle_tree.bin` volledig

### Probleem

De huidige root-extractie leest de root uit de laatste 32 bytes. Dat is nuttig, maar onvoldoende als volledige integriteitscontrole van de boomstructuur.

### Taak

Breid Merkle-verificatie uit.

### Te controleren

```text
- magic bytes
- versie
- leaf_count
- verwachte bestandslengte
- alle levels
- root uit levels
- root aan einde bestand
- root in manifest
- leaves in sequence order
```

### Acceptance criteria

```text
[ ] Corrupt internal node wordt gedetecteerd.
[ ] Corrupt leaf in merkle_tree.bin wordt gedetecteerd.
[ ] Onjuiste leaf_count wordt gedetecteerd.
[ ] Mismatch tussen merkle_tree.bin en manifest wordt gedetecteerd.
```

---

# Fase 3 — Verifier en conformance hardening

## 3.1 Breid `offf-verify` uit met validatieprofielen

### Taak

Introduceer validatieprofielen:

```bash
offf-verify case.offf --profile core
offf-verify case.offf --profile core+schemas
offf-verify case.offf --profile core+extensions
offf-verify case.offf --profile conformance
```

### Profiel `core`

Controleert:

```text
manifest aanwezig
acquisition aanwezig
chunk map aanwezig
chunks aanwezig
stored hash
plaintext hash
source hash
Merkle root
required files
```

### Profiel `core+schemas`

Controleert aanvullend:

```text
manifest JSON Schema
acquisition JSON Schema
provenance event schema
job manifest schema
analysis schema’s waar aanwezig
```

### Profiel `core+extensions`

Controleert aanvullend:

```text
extension manifest entries
labels schema
scopes schema
sets schema
decisions schema
policy_refs schema
access/audit events schema
referentiële integriteit
```

### Profiel `conformance`

Controleert alles en schrijft machine-readable rapport:

```bash
offf-verify case.offf --profile conformance --report report.json
```

### Acceptance criteria

```text
[ ] Validatieprofielen geïmplementeerd.
[ ] JSON report output beschikbaar.
[ ] CI gebruikt minimaal core+schemas.
[ ] Conformance suite gebruikt conformance-profiel.
```

---

## 3.2 Valideer `leaves.parquet`

### Probleem

`leaves.parquet` moet consistent zijn met `physical_to_chunk.parquet` en de Merkle tree.

### Taak

Breid `offf-verify` uit met controle op `hashes/leaves.parquet`.

### Controle

```text
for each row in leaves.parquet:
  sequence == physical_to_chunk.sequence
  hash == physical_to_chunk.plaintext_sha256

number of leaves == number of chunks
order == chunk sequence order
```

### Acceptance criteria

```text
[ ] Mismatch tussen leaves.parquet en physical_to_chunk.parquet wordt gedetecteerd.
[ ] Ontbrekende leaf wordt gedetecteerd.
[ ] Verkeerde leaf-order wordt gedetecteerd.
```

---

## 3.3 Valideer provenance schema en append-only integriteit

### Taak

Breid provenance-validatie uit.

### Controle

```text
- JSONL parsebaar
- ieder event heeft event_id
- timestamp
- actor
- action
- tool.name
- tool.version
- details
- event_id uniek
- event_id monotonic of UUID-based
```

Voor betere append-only integriteit: voeg optioneel hash chaining toe.

### Hash-chain voorstel

```json
{
  "event_id": "evt-000001",
  "timestamp": "...",
  "previous_event_hash": "sha256:...",
  "event_hash": "sha256:..."
}
```

Canonicaliseer event zonder `event_hash`, hash het resultaat en sla `event_hash` op.

### Acceptance criteria

```text
[ ] Invalid JSONL wordt gedetecteerd.
[ ] Duplicate event_id wordt gedetecteerd.
[ ] Missing fields worden gedetecteerd.
[ ] Optionele hash chain kan worden gevalideerd.
```

---

# Fase 4 — Append-only analysis model

## 4.1 Verplaats analysis output naar job-specifieke directories

### Probleem

Workers schrijven naar vaste paden zoals:

```text
analysis/keyword_hits.parquet
analysis/yara_hits.parquet
```

Dit veroorzaakt overschrijven, race conditions en onduidelijke herkomst.

### Taak

Pas alle workers aan naar job-geïsoleerde output.

### Nieuwe structuur

```text
analysis/
  jobs/
    {job_id}/
      result_manifest.json
      keyword_hits.parquet
      yara_hits.parquet
      logs.jsonl
      provenance_refs.jsonl
  indexes/
    keyword_hits_index.parquet
    yara_hits_index.parquet
  events/
    analysis_events.jsonl
```

### Result manifest

```json
{
  "job_id": "job-...",
  "task": "keyword_scan",
  "created_at": "...",
  "tool": {
    "name": "offf-keyword-worker",
    "version": "0.1.0"
  },
  "input": {
    "container_id": "urn:offf:case:...",
    "source_sha256": "...",
    "merkle_root_sha256": "...",
    "scope_ref": null,
    "chunk_count": 10
  },
  "outputs": [
    {
      "path": "analysis/jobs/job-.../keyword_hits.parquet",
      "sha256": "...",
      "schema": "offf-keyword-hit-row-0.1.0"
    }
  ]
}
```

### Acceptance criteria

```text
[ ] Elke job schrijft naar eigen directory.
[ ] Bestaande result files worden niet overschreven.
[ ] Result manifest wordt per job geschreven.
[ ] Result manifest bevat hash van outputbestanden.
[ ] Provenance linkt naar result manifest.
```

---

## 4.2 Maak correcties append-only

### Taak

Introduceer correction events.

### Voorbeeld

```json
{
  "event_id": "analysis-correction-000001",
  "timestamp": "...",
  "actor": "user:analyst-123",
  "correction_of": "analysis/jobs/job-123/keyword_hits.parquet#row-55",
  "correction_type": "false_positive",
  "reason": "Hit occurs in unrelated system file.",
  "provenance_ref": "evt-000123"
}
```

### Acceptance criteria

```text
[ ] Analysis rows worden niet aangepast.
[ ] Correcties worden als events vastgelegd.
[ ] SDK kan correcties lezen.
[ ] Access Service kan correcties append-only schrijven.
```

---

# Fase 5 — Generieke extensiepunten in OFFF Core v0.2

## 5.1 Breid manifest uit met `extensions`

### Taak

Maak OFFF versie `0.2.0` met optionele `extensions`.

### Manifest voorbeeld

```json
{
  "offf_version": "0.2.0",
  "container_id": "urn:offf:case:...",
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

### Compatibility

```text
- v0.1 containers blijven geldig.
- v0.2 consumers kunnen v0.1 lezen.
- v0.1 consumers mogen v0.2 alleen lezen als zij onbekende velden veilig negeren.
- Core evidence validatie mag niet afhankelijk zijn van extensions.
```

### Acceptance criteria

```text
[ ] ManifestJson ondersteunt optionele extensions.
[ ] JSON Schema v0.2.0 toegevoegd.
[ ] v0.1 testcontainers blijven valide.
[ ] v0.2 containers met lege extensions zijn valide.
```

---

## 5.2 Implementeer generieke extension types

### Nieuwe types in `offf-core`

```rust
LabelEvent
ScopeDefinition
SetDefinition
DecisionEvent
PolicyRef
AccessEvent
DeniedAccessEvent
AuditEvent
```

### Vereiste eigenschappen

```text
- serde Serialize/Deserialize
- JSON Schema
- append-only writer
- reader/list API
- validator checks
```

### Generiek targetmodel

Gebruik één generiek targetmodel:

```json
{
  "type": "file",
  "id": "file-000123"
}
```

Ondersteunde target types:

```text
container
chunk
chunk_range
partition
filesystem
file
artifact
analysis_result
job
set
scope
export_package
```

### Acceptance criteria

```text
[ ] Extension structs bestaan in offf-core.
[ ] JSON Schema’s bestaan in docs/schema.
[ ] SDK kan extensions lezen.
[ ] Access Service kan extensions append-only schrijven.
[ ] Validator controleert schema en referenties.
```

---

## 5.3 Breid job manifest uit met scope refs en sets

### Probleem

De huidige job scope is alleen chunk-gebaseerd.

### Nieuwe job scope

```json
{
  "scope_ref": "scope-000001",
  "include_sets": ["ws-000001"],
  "exclude_sets": ["excl-000001"],
  "exclude_labels": ["restricted", "excluded"],
  "chunks": ["sha256:..."],
  "files": ["file-000123"],
  "artifacts": []
}
```

### Worker-regel

Een worker mag alleen verwerken wat expliciet binnen de effectieve scope valt.

Effectieve scope:

```text
include scope
+ include sets
+ explicit chunks/files/artifacts
- exclude sets
- exclude labels
```

### Acceptance criteria

```text
[ ] JobManifest v0.2 ondersteunt scope_ref, include_sets, exclude_sets, exclude_labels.
[ ] Workers respecteren exclude_labels en exclusion_sets.
[ ] Skipped items worden als audit event vastgelegd.
[ ] Validator kan scope refs en set refs controleren.
```

---

# Fase 6 — Access Service hardening

## 6.1 Vervang header-only authenticatie

### Probleem

De huidige Access Service gebruikt headers/metadata zoals:

```text
x-offf-role
x-offf-tool-id
```

Dit is onvoldoende voor productie.

### Taak

Maak authenticatie pluggable.

### Minimale opties

```text
development mode:
  headers toegestaan

production mode:
  JWT/OIDC verplicht
  of mTLS verplicht
```

### Configuratie

```text
OFFF_AUTH_MODE=dev_headers | jwt | mtls
OFFF_JWKS_URL=...
OFFF_EXPECTED_ISSUER=...
OFFF_EXPECTED_AUDIENCE=...
```

### Claims

Token moet minimaal bevatten:

```json
{
  "sub": "tool-or-user-id",
  "offf_role": "analysis_worker",
  "offf_tool_id": "offf-keyword-worker",
  "scope": ["offf:read", "offf:analysis:write"]
}
```

### Acceptance criteria

```text
[ ] Dev mode blijft bruikbaar voor lokale smoke tests.
[ ] Production mode weigert requests zonder geldig token/certificaat.
[ ] Role en tool_id komen uit trusted claims, niet uit onbeveiligde headers.
[ ] Tests voor unauthorized, forbidden en allowed.
```

---

## 6.2 Log denied access persistent

### Probleem

Denied writes worden gelogd via tracing, maar moeten ook persistent auditable zijn.

### Taak

Schrijf denied access events naar:

```text
extensions/access/denied_access_events.jsonl
```

Of, zolang extensions nog niet bestaan:

```text
audit/denied_access_events.jsonl
```

### Event voorbeeld

```json
{
  "denied_event_id": "denied-000001",
  "timestamp": "...",
  "actor": "user-or-tool",
  "tool_id": "offf-keyword-worker",
  "role": "viewer",
  "action": "write_analysis_results",
  "target": {
    "type": "analysis_path",
    "id": "analysis/jobs/job-123/result.jsonl"
  },
  "result": "denied",
  "reason_code": "role_not_allowed",
  "policy_refs": []
}
```

### Acceptance criteria

```text
[ ] Forbidden writes worden persistent vastgelegd.
[ ] Unauthorized requests worden waar mogelijk vastgelegd zonder gevoelige tokeninhoud.
[ ] Denied events zijn append-only.
[ ] Validator kan denied events controleren.
```

---

## 6.3 Versterk write path validation

### Taak

Alle write endpoints moeten afdwingen:

```text
- geen writes naar chunks/
- geen writes naar hashes/
- geen writes naar maps/
- geen writes naar manifest.json
- geen writes naar acquisition.json
- geen path traversal
- analysis writes alleen naar analysis/jobs/{job_id}/...
- provenance append-only
- extensions append-only
```

### Acceptance criteria

```text
[ ] Poging tot schrijven naar evidence layer faalt.
[ ] Poging met ../ faalt.
[ ] Poging tot overwrite van analysis result faalt.
[ ] Alle write attempts worden geaudit.
```

---

# Fase 7 — Storage en concurrency hardening

## 7.1 Vervang JSONL read-modify-write voor object storage

### Probleem

S3 ondersteunt geen echte append. Read-modify-write op JSONL schaalt slecht en is concurrency-gevoelig.

### Taak

Gebruik object-per-event storage voor append-only logs.

### Nieuwe structuur

```text
provenance/events/{event_id}.json
audit/events/{event_id}.json
extensions/access/events/{event_id}.json
extensions/access/denied/{event_id}.json
```

Optioneel kunnen compacte JSONL-indexen worden gegenereerd als afgeleide index.

### Regels

```text
- event object is immutable
- event ID is UUIDv7 of ULID
- write gebruikt create-if-not-exists
- bestaande event objecten worden nooit overschreven
```

### Acceptance criteria

```text
[ ] Provenance append werkt zonder read-modify-write.
[ ] Concurrente appends verliezen geen events.
[ ] S3/MinIO smoke test met parallelle writes slaagt.
[ ] Afgeleide JSONL-index kan opnieuw worden opgebouwd uit event objects.
```

---

## 7.2 Voeg content hashes toe aan analysis artifacts

### Taak

Elke analysis artifact krijgt een hash.

Voorbeeld:

```json
{
  "path": "analysis/jobs/job-123/keyword_hits.parquet",
  "sha256": "...",
  "schema": "offf-keyword-hit-row-0.1.0",
  "created_by": "offf-keyword-worker",
  "created_at": "..."
}
```

### Acceptance criteria

```text
[ ] Result manifest bevat hashes van alle outputs.
[ ] Verifier kan analysis artifact hashes controleren.
[ ] Access Service weigert result manifests zonder artifact hash.
```

---

# Fase 8 — Indexing hardening

## 8.1 GPT-validatie uitbreiden

### Taak

Breid GPT parser uit met:

```text
- primary header CRC
- partition entry array CRC
- backup GPT header parsing
- vergelijking primary vs backup GPT
- duidelijke warnings bij inconsistentie
```

### Acceptance criteria

```text
[ ] Geldige GPT wordt correct gevalideerd.
[ ] Corrupt GPT header CRC wordt gedetecteerd.
[ ] Corrupt partition array CRC wordt gedetecteerd.
[ ] Ontbrekende backup GPT geeft warning.
```

---

## 8.2 Extended MBR / EBR ondersteunen

### Taak

Breid MBR parsing uit met extended partitions.

Ondersteun minimaal:

```text
0x05 Extended CHS
0x0F Extended LBA
0x85 Linux Extended
```

### Outputmodel

```json
{
  "partition_id": "mbr-logical-1",
  "parent_partition_id": "mbr-2",
  "partition_role": "logical",
  "start_offset": 123456,
  "length": 987654
}
```

### Acceptance criteria

```text
[ ] Primary partitions blijven werken.
[ ] Extended partition wordt herkend.
[ ] Logical partitions worden opgenomen.
[ ] Tests met synthetische EBR-chain.
```

---

## 8.3 NTFS-indexer markeren als experimental en uitbreiden

### Taak

Maak NTFS-output explicieter over parserstatus en beperkingen.

### FileIndexRow uitbreiden

```text
mft_entry
mft_sequence
stream_name
stream_type
data_state
parser_confidence
has_attribute_list
has_ads
is_sparse
is_compressed
is_encrypted
is_reparse_point
```

### Belangrijke verbeteringen

```text
- $ATTRIBUTE_LIST oplossen
- Alternate Data Streams modelleren
- sparse/compressed/encrypted data onderscheiden
- sequence number in file identity opnemen
- memorygebruik beperken door streaming MFT parsing
```

### Acceptance criteria

```text
[ ] NTFS indexer documenteert bekende beperkingen.
[ ] FileIndexRow bevat MFT entry en sequence.
[ ] ADS wordt zichtbaar als aparte stream/artifact.
[ ] Parserstatus maakt partial/error expliciet.
```

---

# Fase 9 — Workers verbeteren

## 9.1 Maak worker-output idempotent en job-specifiek

### Taak

Pas keyword en YARA worker aan.

### Regels

```text
- output path bevat job_id
- worker schrijft naar tijdelijke output
- output wordt pas na succesvolle scan gecommit
- result manifest wordt geschreven
- provenance verwijst naar result manifest
- herhaalde run met zelfde replay_id detecteert bestaand resultaat
```

### Acceptance criteria

```text
[ ] Twee jobs overschrijven elkaar niet.
[ ] Gefaalde job laat geen half result manifest achter.
[ ] Replay met zelfde input is idempotent.
```

---

## 9.2 Voeg file_id-resolutie toe aan hits

### Probleem

Keyword en YARA hits hebben vaak lege `file_id`.

### Taak

Gebruik file index en physical extents om `physical_offset` te mappen naar file/artifact.

### Output

```json
{
  "hit_id": "...",
  "physical_offset": 123456,
  "chunk_id": "sha256:...",
  "file_id": "file-000123",
  "path": "/Users/...",
  "artifact_ref": "artifact-..."
}
```

### Acceptance criteria

```text
[ ] Hits binnen bekende file extents krijgen file_id.
[ ] Hits buiten bekende file extents blijven fysiek herleidbaar.
[ ] Deleted files worden correct gemarkeerd.
```

---

## 9.3 Ondersteun chunk-boundary matching

### Probleem

Keyword/YARA matches kunnen over chunkgrenzen heen vallen.

### Taak

Voeg overlap-window toe.

Voor keyword scan:

```text
overlap = max_pattern_length - 1
```

Voor YARA scan:

```text
configureerbare overlap, bijvoorbeeld --overlap-bytes 4096
```

### Acceptance criteria

```text
[ ] Keyword die over chunkgrens valt wordt gevonden.
[ ] Dubbele hits door overlap worden gededupliceerd.
[ ] Physical offset blijft correct.
```

---

## 9.4 Maak context capture configureerbaar

### Probleem

Keyword worker schrijft standaard context before/after. Dat kan gevoelige of uitgesloten data lekken.

### Taak

Voeg parameter toe:

```json
{
  "context_bytes": 0
}
```

Default voor forensic-safe mode:

```text
0
```

### Acceptance criteria

```text
[ ] context_bytes is configureerbaar.
[ ] Default is 0 of expliciet verantwoord.
[ ] Context wordt niet buiten scope gelezen.
```

---

# Fase 10 — SDK hardening

## 10.1 Python SDK: memory cache beperken

### Probleem

De Python SDK cachet chunks in memory zonder zichtbare limiet.

### Taak

Voeg LRU cache toe.

### API

```python
OfffContainer(path, cache_max_bytes=256 * 1024 * 1024)
OfffContainer(path, cache_enabled=False)
```

### Acceptance criteria

```text
[ ] Cache heeft maximale omvang.
[ ] Cache kan worden uitgezet.
[ ] Grote containers veroorzaken geen onbeperkte memory growth.
```

---

## 10.2 Python SDK: append-only writers

### Probleem

`write_analysis_result` kan bestaande output overschrijven.

### Taak

Pas API aan.

Niet toestaan:

```python
write_analysis_result("analysis/keyword_hits.parquet", rows)
```

Wel:

```python
append_analysis_result(job_id, artifact_name, rows)
write_job_result_manifest(job_id, manifest)
append_provenance_event(...)
append_extension_event(...)
```

### Acceptance criteria

```text
[ ] Schrijven naar bestaand analysis artifact faalt.
[ ] Job-specifieke output wordt ondersteund.
[ ] Result manifest wordt automatisch gehasht.
```

---

## 10.3 SDK forward compatibility

### Probleem

De SDK accepteert exact `0.1.0`. Dat breekt bij `0.2.0`.

### Taak

Ondersteun compatibele minor versions.

### Regels

```text
0.1.x lezen als 0.1 profiel
0.2.x lezen als 0.2 profiel
onbekende extensies veilig negeren met warning
major version mismatch weigeren
```

### Acceptance criteria

```text
[ ] v0.1 containers blijven werken.
[ ] v0.2 containers met extensions kunnen worden geopend.
[ ] Onbekende extensions geven warning, geen crash.
```

---

# Fase 11 — Access API uitbreiden voor extensions

## 11.1 REST endpoints

Voeg endpoints toe:

```text
GET  /cases/{caseId}/extensions/labels
POST /cases/{caseId}/extensions/labels

GET  /cases/{caseId}/extensions/scopes
POST /cases/{caseId}/extensions/scopes

GET  /cases/{caseId}/extensions/sets
POST /cases/{caseId}/extensions/sets

GET  /cases/{caseId}/extensions/decisions
POST /cases/{caseId}/extensions/decisions

GET  /cases/{caseId}/extensions/access-events
POST /cases/{caseId}/extensions/access-events

GET  /cases/{caseId}/extensions/audit-events
POST /cases/{caseId}/extensions/audit-events
```

### Acceptance criteria

```text
[ ] Alle POST endpoints zijn append-only.
[ ] Schema validatie vóór schrijven.
[ ] Capability check per extension type.
[ ] Denied attempts worden persistent gelogd.
```

---

## 11.2 gRPC uitbreiden

Voeg messages en RPC’s toe voor extensions.

Minimaal:

```protobuf
rpc ListLabels(ListLabelsRequest) returns (ListLabelsResponse);
rpc AppendLabel(AppendLabelRequest) returns (AppendLabelResponse);

rpc ListScopes(ListScopesRequest) returns (ListScopesResponse);
rpc AppendScope(AppendScopeRequest) returns (AppendScopeResponse);

rpc ListSets(ListSetsRequest) returns (ListSetsResponse);
rpc AppendSet(AppendSetRequest) returns (AppendSetResponse);

rpc ListDecisions(ListDecisionsRequest) returns (ListDecisionsResponse);
rpc AppendDecision(AppendDecisionRequest) returns (AppendDecisionResponse);
```

### Acceptance criteria

```text
[ ] Proto v0.2 toegevoegd.
[ ] Backward compatibility voor bestaande gRPC methods.
[ ] Smoke tests voor extension RPC’s.
```

---

# Fase 12 — CI en conformance

## 12.1 CI baseline verplicht maken

### Minimale CI jobs

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
python sdk tests
go sdk tests
schema validation
conformance tests
```

### Acceptance criteria

```text
[ ] CI draait op elke PR.
[ ] Build faalt bij clippy warnings.
[ ] Schema-validatie faalt bij incompatible schema changes.
[ ] Conformance tests publiceren report artifact.
```

---

## 12.2 CLI end-to-end tests toevoegen

### Probleem

Integratietests gebruiken vooral core helperfuncties. De echte CLI workflow moet apart getest worden.

### Testflow

```text
1. generate synthetic raw image
2. run offf-convert
3. run offf-verify
4. run offf-export
5. compare sha256 original vs reconstructed
6. run offf-index partitions
7. run offf-jobs create-keyword
8. run offf-jobs run
9. verify analysis/jobs/{job_id}/result_manifest.json
```

### Acceptance criteria

```text
[ ] E2E CLI test voor raw image.
[ ] E2E CLI test voor non-aligned image.
[ ] E2E CLI test voor corrupt chunk.
[ ] E2E CLI test voor missing manifest.
[ ] E2E CLI test voor keyword worker output isolation.
```

---

## 12.3 Negative conformance datasets

Maak datasets voor:

```text
- missing chunk
- corrupt chunk stored bytes
- corrupt decompressed plaintext
- invalid manifest schema
- invalid acquisition schema
- invalid provenance JSONL
- duplicate provenance event_id
- wrong Merkle root
- wrong leaf order
- missing leaves.parquet
- path traversal attempt in analysis write
- overwrite attempt in analysis write
```

### Acceptance criteria

```text
[ ] Iedere negatieve dataset faalt met duidelijke foutcode.
[ ] Foutcodes zijn machine-readable.
[ ] Conformance report bevat expected vs actual.
```

---

# Fase 13 — Security en threat model

## 13.1 Voeg threat model toe

Maak `docs/threat-model.md`.

### Behandel minimaal

```text
- corrupte chunk store
- gemanipuleerd manifest
- gemanipuleerde Merkle tree
- race condition in provenance append
- unauthorized analysis writes
- malicious worker
- path traversal
- object storage consistency
- replay van oude job results
- tool identity spoofing
- supply-chain risico’s van workers
```

### Acceptance criteria

```text
[ ] Threat model bestaat.
[ ] Elke dreiging heeft mitigerende maatregel.
[ ] Threat model linkt naar concrete tests of backlog items.
```

---

## 13.2 Tool registry versterken

### Taak

Tool registry moet niet alleen toolnaam bevatten, maar ook uitvoerbare identiteit.

Voorbeeld:

```json
{
  "tool_id": "offf-keyword-worker",
  "status": "approved",
  "allowed_roles": ["analysis_worker"],
  "write_layers": ["analysis", "provenance"],
  "supported_offf_versions": ["0.1.0", "0.2.0"],
  "binary_sha256": "...",
  "container_image_digest": "sha256:...",
  "vendor": "OFFF Project",
  "approved_at": "...",
  "approved_by": "..."
}
```

### Acceptance criteria

```text
[ ] Tool registry schema bestaat.
[ ] Access Service valideert tool status, role en write layer.
[ ] Productiemodus kan binary/image identity afdwingen.
```

---

# Fase 14 — Release naar OFFF v0.2.0

## 14.1 Versiebeleid

Maak expliciet versiebeleid.

### Voorstel

```text
0.1.x = Evidence Container MVP
0.2.x = Generic Extensions + Append-only Analysis
0.3.x = Distributed Proofs + Scope-aware Workers
1.0.0 = Stable forensic interoperability baseline
```

### Semantic versioning

```text
MAJOR: incompatible format changes
MINOR: backward-compatible format extensions
PATCH: bugfixes, schema clarifications, tooling fixes
```

### Acceptance criteria

```text
[ ] docs/versioning.md bestaat.
[ ] Manifest schema verwijst naar versiebeleid.
[ ] SDK’s volgen versiebeleid.
```

---

## 14.2 Migratiepad v0.1 naar v0.2

### Taak

Maak migratietool:

```bash
offf-migrate case-v0.1.offf --to 0.2.0
```

### Gedrag

```text
- evidence layer blijft intact
- manifest wordt v0.2-compatible gemaakt
- extensions directories worden toegevoegd
- provenance event wordt toegevoegd
- source/Merkle hashes blijven gelijk
```

### Acceptance criteria

```text
[ ] v0.1 naar v0.2 migratie werkt.
[ ] Evidence hashes blijven gelijk.
[ ] Verifier accepteert gemigreerde container.
```

---

# Prioriteitenoverzicht

## P0 — Direct oppakken

```text
[ ] Crash-safe convert
[ ] Existing chunk verification before skip
[ ] True deterministic mode
[ ] Merkle proofs
[ ] Full merkle_tree.bin validation
[ ] offf-verify profiles
[ ] leaves.parquet validation
[ ] job-specific analysis output
[ ] Access Service production auth mode
[ ] persistent denied access events
[ ] CI E2E CLI tests
```

## P1 — Daarna

```text
[ ] manifest.extensions v0.2
[ ] extension schemas/types
[ ] scope-aware job manifest
[ ] append-only extension APIs
[ ] object-per-event provenance/audit storage
[ ] GPT CRC validation
[ ] Extended MBR support
[ ] file_id resolution in worker hits
[ ] chunk-boundary matching
[ ] Python SDK LRU cache
```

## P2 — Later

```text
[ ] NTFS streaming MFT parser
[ ] ADS/compressed/sparse/encrypted data modeling
[ ] YARA ruleset references
[ ] Go SDK parity hardening
[ ] full threat model
[ ] migration tool v0.1 → v0.2
```

---

# Definitie van klaar voor forensic-grade v0.2

OFFF v0.2 is klaar als:

```text
[ ] Evidence layer is immutable.
[ ] Analysis layer is append-only.
[ ] Provenance/audit is append-only en concurrency-safe.
[ ] Verifier heeft core, schema, extension en conformance profielen.
[ ] Merkle inclusion proofs werken.
[ ] Workers schrijven job-geïsoleerde resultaten.
[ ] Access Service gebruikt echte authenticatie in production mode.
[ ] Generic extensions zijn technisch geïmplementeerd.
[ ] Scope-aware jobs respecteren labels, sets en exclusions.
[ ] SDK’s kunnen v0.1 en v0.2 lezen.
[ ] CI draait unit, integration, CLI E2E en conformance tests.
[ ] README/status/threat-model/versioning zijn aanwezig.
```

---

# Samenvattende opdracht aan het ontwikkelteam

Verplaats de focus van “meer functionaliteit” naar “forensische betrouwbaarheid en ecosysteemharding”.

De eerstvolgende ontwikkelsprint moet draaien om:

```text
1. Immutable evidence finalization
2. Append-only analysis output
3. Merkle proofs
4. Sterkere verifier
5. Production-grade access control
6. Generic extensions v0.2
```

De kernzin voor alle ontwerpbeslissingen:

```text
Elke byte evidence moet herleidbaar, verifieerbaar en onveranderbaar zijn.
Elke afgeleide analyse moet append-only, scope-aware en reproduceerbaar zijn.
Elke toegang of weigering moet aantoonbaar worden vastgelegd.
```
