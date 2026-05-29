# AI Coding Agent Instructie — OFFF Adoptierijpheid en Forensic Trust Hardening

## Rol

Je bent een senior software engineer, release engineer en technical product owner voor de repository **Open Forensic File Format (OFFF)**.

Je werkt met **Claude Sonnet 4.6** als AI coding agent.

Je opdracht is om OFFF zodanig te verbeteren dat het niet blijft steken als technisch veelbelovend prototype, maar kan doorgroeien naar een adopteerbare, toetsbare en bestuurlijk uitlegbare standaard voor digitale forensische interoperabiliteit.

Deze opdracht is nadrukkelijk **geen feature-sprint**.

De focus ligt op:

```text
- vertrouwen
- adoptierijpheid
- conformance
- maturity
- standaardisering
- juridische en forensische uitlegbaarheid
- Core/reference/demo-afbakening
- aansluiting op bestaande toolketens
```

---

## 1. Aanleiding

Uit de huidige status van OFFF blijkt:

```text
- OFFF heeft sterke technische concepten: chunks, hashes, Merkle, analysis jobs, lineage, provenance en workercontracten.
- De repo bevat een werkende demo met Tika, Elasticsearch en unsupervised classification.
- Er zijn v0.2-schema's voor object lineage en object-producing result manifests.
- Er is CI met fmt, clippy, workspace tests, SDK-tests, schema-validatie en conformance scaffold.
```

Maar er is ook een adoptierisico:

```text
- status is nog te vaak stable-mvp, experimental of planned;
- demo, reference implementation en Core lopen deels door elkaar;
- conformance is nog te veel scaffold en te weinig officiële contractlaag;
- maturity en forensic-grade claims zijn onvoldoende expliciet;
- bestaande forensische toolketens zijn nog niet expliciet als integratiedoel uitgewerkt;
- juridische neutraliteit, scope, chain of evidence en chain of custody zijn nog niet voldoende als adoptiedocumentatie uitgewerkt.
```

Het scenario dat moet worden voorkomen:

```text
Een jaar later wordt OFFF door geen enkele opsporingsdienst gebruikt,
niet omdat het technisch idee slecht is,
maar omdat het onvoldoende vertrouwen, governance, conformance en adoptiepad biedt.
```

---

## 2. Hoofddoel

Maak OFFF adopteerbaar als open forensic interoperability standaard.

Niet door meer features toe te voegen, maar door duidelijk te maken:

```text
1. wat OFFF Core is;
2. wat reference implementation is;
3. wat demo-only is;
4. welk maturity-niveau elk onderdeel heeft;
5. welk conformanceprofiel elk onderdeel ondersteunt;
6. wat forensic-grade candidate is en wat niet;
7. hoe tools kunnen aansluiten;
8. hoe een opsporingsdienst een pilot kan uitvoeren;
9. welke juridische en forensische beperkingen gelden;
10. hoe validatie en bewijsbaarheid aantoonbaar worden gemaakt.
```

---

## 3. Niet doen

Tijdens deze opdracht geen nieuwe functionele scope toevoegen.

Niet doen:

```text
[ ] Geen nieuwe forensic workers bouwen.
[ ] Geen nieuwe parserintegraties bouwen.
[ ] Geen nieuwe AI/ML-functionaliteit bouwen.
[ ] Geen scheduler of orchestration platform bouwen.
[ ] Geen juridische beslislogica in Core toevoegen.
[ ] Geen Hansken/FTK/Cellebrite/GrayKey-specifieke harde koppeling bouwen.
[ ] Geen production claims toevoegen zonder test- en conformancebewijs.
[ ] Geen demo-component als Core positioneren.
```

Wel doen:

```text
[ ] Documenteren.
[ ] Classificeren.
[ ] Maturity bepalen.
[ ] Conformanceprofielen formaliseren.
[ ] Testbewijs koppelen.
[ ] Status inconsistenties herstellen.
[ ] Adoptiepad beschrijven.
[ ] Tooladapterrichtlijnen opstellen.
[ ] Verifier- en rapportage-uitvoer begrijpelijker maken.
[ ] CI-checks toevoegen die maturity/conformance metadata bewaken.
```

---

# Deel A — Core, Reference en Demo strikt scheiden

## 4. Maak OFFF componentclassificatie normatief

Maak of update:

```text
docs/component-classification.md
components.toml
```

Elke component krijgt exact één primaire classificatie:

```text
core
reference
demo
experimental
legacy
```

## 4.1 Definities

Gebruik deze definities:

| Classificatie | Betekenis |
|---|---|
| `core` | Normatief onderdeel van OFFF-specificatie of core library |
| `reference` | Voorbeeldimplementatie van een normatief OFFF-contract |
| `demo` | Alleen bedoeld om werking te tonen; geen conformanceclaim |
| `experimental` | Instabiel, API/schema/gedrag kan wijzigen |
| `legacy` | Ondersteund voor compatibiliteit, niet richtinggevend |

## 4.2 Minimaal te classificeren componenten

Classificeer minimaal:

```text
offf-core
offf-convert
offf-export
offf-verify
offf-index
offf-jobs
offf-keyword-worker
offf-yara-worker
offf-access-service
Python SDK
Go SDK
schema catalog
conformance suite
object lineage model
derived object store
extension model
tool registry
Docker demo
Tika demo worker
Elasticsearch demo worker
unsupervised classifier demo worker
worker runtime state
retry/failure policy
worker health registry
assignment audit trail
packed container support
```

## 4.3 Architectuurregel

```text
OFFF Core definieert contracten en validatie.
Reference implementations tonen hoe je die contracten toepast.
Demo tooling toont de waarde, maar is geen normatief onderdeel van de standaard.
```

## 4.4 Acceptance criteria

```text
[ ] docs/component-classification.md bestaat.
[ ] components.toml bestaat.
[ ] Elk component heeft classification.
[ ] Demo-workers staan niet als core.
[ ] Worker runtime/retry/health/assignment staan niet als core.
[ ] Packed container staat als transport/package representation, niet als canonical model.
[ ] README linkt naar component-classification.
```

---

# Deel B — Maturity model invoeren

## 5. Maak maturity model

Maak of update:

```text
docs/maturity-model.md
```

Definieer minimaal:

```text
demo-only
experimental
reference
forensic-grade-candidate
forensic-grade
```

## 5.1 Maturity criteria

Leg per level vast:

```text
- doel
- toegestane toepassing
- testvereisten
- documentatievereisten
- conformancevereisten
- security/reviewvereisten
- productieadvies
- toegestane breaking changes
```

## 5.2 Voorbeeldcriteria

### demo-only

```text
- Alleen bedoeld voor demonstratie.
- Mag vereenvoudigde containerstructuur gebruiken.
- Geen forensic-grade claim.
- Geen productiegebruik.
```

### experimental

```text
- Werkend prototype.
- API/schema kan wijzigen.
- Niet geschikt voor formele forensische keten.
```

### reference

```text
- Volgt formeel OFFF-contract.
- Bedoeld als implementatievoorbeeld.
- Kan worden gebruikt als basis voor integratie.
- Niet automatisch production/forensic-grade.
```

### forensic-grade-candidate

```text
- Core gedrag is stabiel.
- Unit, integration, negative en E2E tests aanwezig.
- Verifier ondersteunt validatie.
- Known limitations zijn gedocumenteerd.
- Externe review is nog nodig.
```

### forensic-grade

```text
- Stabiele specificatie.
- Conformance suite volledig.
- Security en forensic review afgerond.
- Backward compatibility vastgelegd.
- Geschikt voor gecontroleerde pilot/productie.
```

## 5.3 Acceptance criteria

```text
[ ] Maturity levels zijn concreet en toetsbaar.
[ ] Geen component heeft forensic-grade zonder test/conformancebewijs.
[ ] Demo-only en reference zijn duidelijk gescheiden.
[ ] README verwijst naar maturity model.
```

---

# Deel C — Statusmatrix vervangen door adoptiegerichte status

## 6. Update `docs/status.md`

Vervang de huidige eenvoudige statusmatrix door een adoptiegerichte matrix.

Elke rij bevat:

```text
Component
Classification
Maturity
Implementation status
Conformance profile
Test evidence
Known limitations
Production/adoption guidance
Last verified
```

## 6.1 Voorbeeld

| Component | Classification | Maturity | Status | Conformance | Test evidence | Guidance |
|---|---|---|---|---|---|---|
| `offf-core` chunk/hash store | core | forensic-grade-candidate | implemented | Reader/Acquisition | unit+e2e+negative | candidate for controlled pilot |
| Docker Tika demo worker | demo | demo-only | implemented | none | smoke | not production |
| Access Service | reference | experimental/reference | implemented | Access Service candidate | smoke+integration | not core |
| `.offfpack` | reference/transport | experimental | implemented | none | pack/list/unpack | transport only |

## 6.2 Acceptance criteria

```text
[ ] Geen component heeft alleen 'stable-mvp' zonder uitleg.
[ ] Elke component heeft maturity en classification.
[ ] Elke component heeft test evidence of expliciet 'gap'.
[ ] Elke component heeft production/adoption guidance.
[ ] README verwijst naar deze statusmatrix.
```

---

# Deel D — Forensic Baseline Profile definiëren

## 7. Maak OFFF Forensic Baseline Profile

Maak:

```text
docs/forensic-baseline-profile.md
```

Doel:

```text
Definieer de minimale set OFFF-eisen voor gecontroleerd forensisch gebruik.
```

## 7.1 Baseline moet minimaal bevatten

```text
1. manifest en acquisition metadata
2. acquisition_mode
3. evidence root model
4. immutable evidence layer
5. SHA-256 minimum
6. Merkle tree/proofs voor block_image evidence
7. object index voor file_collection en derived objects
8. object lineage voor nested/derived objects
9. append-only analysis output
10. result_manifest per job
11. provenance per job
12. skipped/error/denied events
13. verifier report
14. known limitations
15. conformance report
```

## 7.2 Wat expliciet buiten baseline valt

```text
- specifieke forensic UI
- specifieke scheduler
- specifieke parser
- specifieke AI/ML tool
- juridische beslisautomatisering
- specifieke vendorintegratie
```

## 7.3 Acceptance criteria

```text
[ ] Baseline profile bestaat.
[ ] Baseline is tool-agnostisch.
[ ] Baseline benoemt block_image en file_collection.
[ ] Baseline noemt vereiste verifier checks.
[ ] Baseline heeft pass/fail criteria.
```

---

# Deel E — Conformanceprofielen volwassen maken

## 8. Maak officiële conformanceprofielen

Maak of update:

```text
docs/conformance-profiles.md
```

Definieer minimaal:

```text
OFFF Reader Conformant
OFFF Acquisition Conformant
OFFF Indexer Conformant
OFFF Analysis Worker Conformant
OFFF Object-Lineage Conformant
OFFF Access Service Conformant
OFFF Extension Conformant
OFFF Forensic Baseline Conformant
```

## 8.1 Per profiel vastleggen

```text
- doel
- scope
- verplichte input/output
- verplichte schema's
- verplichte verifier checks
- verplichte negative tests
- machine-readable report format
- pass/fail criteria
- maturity minimum
```

## 8.2 Conformance suite verbeteren

Update:

```text
tests/conformance/run_conformance.py
tests/conformance/negative_cases.json
```

Zorg dat conformance niet alleen checkt of bestanden bestaan, maar ook:

```text
- schema-validatie
- hashvalidatie
- manifest/acquisition consistentie
- result_manifest outputhashes
- provenance event referenties
- object lineage referenties
- derived object hashes
- expected failure modes voor negatieve datasets
```

## 8.3 Acceptance criteria

```text
[ ] Conformanceprofielen zijn formeel beschreven.
[ ] Conformance runner rapporteert per profiel.
[ ] Machine-readable report bevat profile, status, checks, failures.
[ ] Negative tests zijn profielspecifiek.
[ ] README verwijst naar conformanceprofielen.
```

---

# Deel F — Evidence-of-done en test traceability

## 9. Maak evidence-of-done register

Maak:

```text
docs/evidence-of-done.md
```

Per afgerond item:

```text
- status
- classification
- maturity
- implemented in
- tests
- conformance
- known limitations
- evidence
- conclusion
```

## 9.1 Voorbeeldformat

```markdown
## Hardening Sprint 4 — Merkle proofs + full tree validation

Classification: core
Maturity: forensic-grade-candidate
Implemented in:
- crates/offf-core/...
- crates/offf-verify/...

Tests:
- unit:
- integration:
- negative:
- e2e:

Conformance:
- OFFF Reader Conformant
- OFFF Forensic Baseline partial

Known limitations:
- ...

Conclusion:
- Done criteria achieved / partial / gap
```

## 10. Maak test traceability matrix

Maak:

```text
docs/test-traceability.md
```

Tabel:

```text
Requirement / component
Unit tests
Integration tests
E2E tests
Negative tests
Conformance profile
Status
Gap
```

## 10.1 Acceptance criteria

```text
[ ] Elk afgerond Core-item heeft evidence-of-done.
[ ] Elk forensic-grade-candidate component heeft test traceability.
[ ] Items zonder bewijs zijn als gap gemarkeerd.
[ ] Release readiness report gebruikt deze input.
```

---

# Deel G — Juridische en forensische uitlegbaarheid

## 11. Maak juridische/forensische uitlegbaarheidsdocs

Maak minimaal:

```text
docs/chain-of-evidence.md
docs/chain-of-custody.md
docs/legal-neutrality.md
docs/scope-and-exclusion-model.md
docs/forensic-limitations.md
```

## 11.1 Inhoud per document

### `chain-of-evidence.md`

Leg uit:

```text
- technische herkomstketen van object/resultaat naar evidence root
- object lineage
- derivations
- hashes
- source refs
- block_image versus file_collection
```

### `chain-of-custody.md`

Leg uit:

```text
- procesmatige handelingen
- provenance events
- audit events
- jobs
- tool identity
- result manifests
```

### `legal-neutrality.md`

Leg uit:

```text
- OFFF neemt geen juridische beslissingen
- OFFF legt technische scope en verwerkingshandelingen vast
- juridische interpretatie ligt buiten Core
- labels/scopes/sets zijn technisch, niet juridisch beslissend
```

### `scope-and-exclusion-model.md`

Leg uit:

```text
- include/exclude scopes
- labels
- release sets
- exclusion sets
- skipped events
- denied events
- auditability
```

### `forensic-limitations.md`

Leg uit:

```text
- wat OFFF wel/niet bewijst
- beperkingen bij file_collection
- beperkingen bij logical_extraction
- beperkingen bij demo tooling
- verschil tussen evidence en analysis
```

## 11.2 Acceptance criteria

```text
[ ] Docs zijn begrijpelijk voor architecten en forensisch specialisten.
[ ] Docs vermijden juridische claims die OFFF niet kan waarmaken.
[ ] Docs maken duidelijk wat buiten OFFF Core valt.
[ ] README verwijst naar deze docs.
```

---

# Deel H — Evidence-object-centric maken

## 12. Documenteer OFFF als evidence-object-centric

Maak of update:

```text
docs/evidence-root-model.md
```

Leg vast:

```text
OFFF is not image-centric.
OFFF is evidence-object-centric.
```

Ondersteun conceptueel:

```text
block_image
file_collection
logical_extraction
api_export
mixed
```

## 12.1 Belangrijke regel

```text
Een disk image is één mogelijke evidence root, niet de enige.
```

## 12.2 Documenteer per acquisition mode

Per mode:

```text
- wat is de root evidence?
- welke index is verplicht?
- welke context is beschikbaar?
- welke context ontbreekt?
- welke verifier checks gelden?
```

## 12.3 Acceptance criteria

```text
[ ] Evidence-root model bestaat.
[ ] block_image en file_collection zijn volwaardig beschreven.
[ ] logical_extraction en api_export zijn conceptueel voorbereid.
[ ] limitations per mode zijn expliciet.
```

---

# Deel I — Tool adapter guide

## 13. Maak vendor/tool adapter guide

Maak:

```text
docs/tool-adapter-guide.md
```

Doel:

```text
Laat zien hoe bestaande tools kunnen aansluiten op OFFF zonder dat OFFF ze vervangt.
```

Richt je op categorieën, niet op harde vendorimplementaties:

```text
- acquisition tools
- mobile extraction tools
- forensic analysis platforms
- OCR/text extraction tools
- AI/ML tools
- reporting/disclosure tools
```

## 13.1 Beschrijf integratiepatronen

Minimaal:

```text
Pattern A: Tool exports evidence → OFFF ingest
Pattern B: Tool reads OFFF as input
Pattern C: Tool writes analysis output to OFFF
Pattern D: Tool writes object lineage to OFFF
Pattern E: Tool exports report package with OFFF verifier report
```

## 13.2 Benoem bestaande toolcategorieën

Gebruik voorbeelden als categorieën:

```text
Hansken-like platforms
FTK Lab-like platforms
Cellebrite/GrayKey-like extraction tools
Tika/OCR-like enrichment tools
AI classifier workers
```

Niet bouwen. Alleen adaptercontracten beschrijven.

## 13.3 Acceptance criteria

```text
[ ] Guide maakt duidelijk dat OFFF geen forensic suite vervangt.
[ ] Guide beschrijft input/outputcontracten.
[ ] Guide verwijst naar conformanceprofielen.
[ ] Guide geeft mappingvoorbeelden voor result_manifest en provenance.
```

---

# Deel J — Adoptie- en pilotroute

## 14. Maak adoption playbook

Maak:

```text
docs/adoption-playbook.md
docs/pilot-template.md
docs/risk-assessment-template.md
```

## 14.1 Adoption playbook bevat

```text
1. POC met synthetische data
2. POC met bestaande tool-export
3. forensisch expert review
4. juridische review
5. security review
6. conformance review
7. gecontroleerde pilot
8. besluit over standaardisatie
```

## 14.2 Pilot template bevat

```text
- doel
- scope
- testdata
- betrokken rollen
- tools
- OFFF-profielen
- succescriteria
- risico's
- exitcriteria
```

## 14.3 Risk assessment template bevat

```text
- forensic integrity risks
- legal/process risks
- privacy risks
- security risks
- tool integration risks
- operational risks
- vendor lock-in risks
- mitigation
```

## 14.4 Acceptance criteria

```text
[ ] Er is een praktisch adoptiepad.
[ ] Er zijn pilot-succescriteria.
[ ] Er zijn expliciete exitcriteria.
[ ] Risico's zijn concreet en toetsbaar.
```

---

# Deel K — Verifier reports bestuurlijk leesbaar maken

## 15. Verbeter verifier report output

Zonder nieuwe verificatiefuncties te bouwen, verbeter de rapportagevorm.

Maak of update report output zodat deze bevat:

```text
summary
profile results
integrity status
lineage status
analysis status
provenance status
limitations
warnings
failed checks
recommended next action
```

## 15.1 Report formats

Ondersteun:

```text
machine-readable JSON
human-readable Markdown
```

Voorbeeld:

```bash
offf-verify case.offf --profile forensic-baseline --report-json report.json --report-md report.md
```

Als deze CLI nog niet bestaat, documenteer de gewenste reportstructuur en voeg een report generator toe rond bestaande output.

## 15.2 Acceptance criteria

```text
[ ] Verify report is bruikbaar voor technisch review.
[ ] Verify report is begrijpelijk voor management/architectuur board.
[ ] Report noemt expliciet limitations.
[ ] Report maakt onderscheid tussen ERROR, WARNING en INFO.
```

---

# Deel L — Release readiness rapport

## 16. Maak release readiness generator

Maak:

```text
scripts/generate_release_readiness.py
reports/release-readiness.md
reports/release-readiness.json
```

Het rapport toont:

```text
- componenten per classification
- componenten per maturity
- conformance gaps
- test evidence gaps
- demo/reference/core verdeling
- forensic baseline status
- production/adoption warnings
- releaseadvies
```

## 16.1 Mogelijke releaseadviezen

```text
not-ready
demo-ready
reference-ready
forensic-grade-candidate
forensic-grade
```

## 16.2 Acceptance criteria

```text
[ ] Rapport kan lokaal gegenereerd worden.
[ ] Rapport wordt in CI gegenereerd.
[ ] Rapport noemt gaps expliciet.
[ ] Rapport voorkomt dat demo-ready als forensic-grade wordt geïnterpreteerd.
```

---

# Deel M — CI guardrails

## 17. Voeg CI-checks toe

Breid CI uit met checks voor:

```text
component metadata
maturity labels
conformance profile metadata
test traceability
evidence-of-done completeness
release readiness report generation
```

## 17.1 Scripts

Maak minimaal:

```text
scripts/check_component_metadata.py
scripts/check_test_traceability.py
scripts/check_evidence_of_done.py
scripts/generate_release_readiness.py
```

## 17.2 CI gedrag

CI moet falen als:

```text
- component classification ontbreekt
- maturity ontbreekt
- core component geen test evidence heeft
- demo component forensic-grade claimt
- done backlogitem geen evidence-of-done heeft
- conformance profile geen pass/fail criteria heeft
```

## 17.3 Acceptance criteria

```text
[ ] CI voert metadata checks uit.
[ ] CI publiceert release readiness report als artifact.
[ ] CI blokkeert maturity- en classification-inconsistenties.
```

---

# Deel N — README herpositioneren

## 18. Update README

De README moet OFFF niet primair verkopen als demo of techniek, maar als standaard-in-wording.

Voeg bovenaan of vroeg in README toe:

```text
- What OFFF is
- What OFFF is not
- Current maturity
- Core vs reference vs demo
- Forensic baseline status
- Conformance profiles
- Adoption path
```

## 18.1 What OFFF is

```text
An open, verifiable forensic evidence and interoperability format for evidence objects, analysis results, lineage, provenance and validation.
```

## 18.2 What OFFF is not

```text
- not a forensic suite
- not a replacement for Hansken/FTK/Cellebrite/GrayKey
- not a legal decision engine
- not a scheduler/orchestrator
- not all components are production/forensic-grade
```

## 18.3 Acceptance criteria

```text
[ ] README noemt maturity.
[ ] README linkt naar forensic baseline profile.
[ ] README linkt naar conformance profiles.
[ ] README onderscheidt Core/reference/demo.
[ ] Demo staat duidelijk onder demo-only sectie.
```

---

# Deel O — Backlogstatus opschonen

## 19. Cleanup backlog

Normaliseer alle backlogitems.

Voor elk afgerond item:

```text
- [x] Item
  - Status: done
  - Done:
  - Classification:
  - Maturity:
  - Implemented in:
  - Tests:
  - Conformance:
  - Known limitations:
```

Voor elk niet-afgerond item:

```text
- [ ] Item
  - Status: planned | in-progress | blocked
  - Target maturity:
  - Scope:
  - Acceptance:
```

## 19.1 Acceptance criteria

```text
[ ] Geen item heeft tegelijk done en planned/in-progress.
[ ] Geen item heeft x-checkbox zonder evidence.
[ ] Elk done item heeft maturity.
[ ] Elk done item heeft tests of expliciete gap.
```

---

# Deel P — Implementatievolgorde

## Fase 1 — Classificatie en maturity

```text
[ ] docs/maturity-model.md
[ ] docs/component-classification.md
[ ] components.toml
[ ] docs/status.md update
[ ] README update voor Core/reference/demo
```

## Fase 2 — Conformance en forensic baseline

```text
[ ] docs/forensic-baseline-profile.md
[ ] docs/conformance-profiles.md
[ ] conformance runner profielrapportage uitbreiden
[ ] negative tests mappen op profielen
```

## Fase 3 — Bewijsbaarheid

```text
[ ] docs/evidence-of-done.md
[ ] docs/test-traceability.md
[ ] scripts/check_test_traceability.py
[ ] scripts/check_evidence_of_done.py
```

## Fase 4 — Uitlegbaarheid en adoptie

```text
[ ] docs/chain-of-evidence.md
[ ] docs/chain-of-custody.md
[ ] docs/legal-neutrality.md
[ ] docs/scope-and-exclusion-model.md
[ ] docs/forensic-limitations.md
[ ] docs/evidence-root-model.md
[ ] docs/tool-adapter-guide.md
[ ] docs/adoption-playbook.md
[ ] docs/pilot-template.md
[ ] docs/risk-assessment-template.md
```

## Fase 5 — Release readiness en CI

```text
[ ] scripts/generate_release_readiness.py
[ ] reports/release-readiness.md
[ ] reports/release-readiness.json
[ ] CI uitbreiden met metadata/maturity/readiness checks
[ ] backlog cleanup
```

---

# 20. Definition of Done voor deze opdracht

Deze opdracht is klaar als:

```text
[ ] OFFF Core, reference en demo zijn expliciet gescheiden.
[ ] Elk component heeft classification.
[ ] Elk component heeft maturity.
[ ] Er is een forensic baseline profile.
[ ] Er zijn officiële conformanceprofielen.
[ ] Conformance runner rapporteert per profiel.
[ ] Evidence-of-done bestaat voor afgeronde kernitems.
[ ] Test traceability bestaat.
[ ] Juridische neutraliteit is gedocumenteerd.
[ ] Chain of evidence en chain of custody zijn gedocumenteerd.
[ ] Tool adapter guide bestaat.
[ ] Adoption playbook bestaat.
[ ] Release readiness report kan worden gegenereerd.
[ ] CI bewaakt classification/maturity/testbewijs.
[ ] README positioneert OFFF als standaard-in-wording, niet als demo.
```

---

# 21. Kernzin voor de AI coding agent

```text
Voeg geen nieuwe technische features toe.
Maak OFFF adopteerbaar:
scheid Core/reference/demo, formaliseer maturity en conformance,
maak testbewijs traceerbaar, documenteer juridische neutraliteit en forensic limitations,
en geef opsporingsdiensten een concreet pilot- en adoptiepad.
```

---

# 22. Eindrapportage door de agent

Aan het einde moet de agent rapporteren:

```text
1. Welke componenten zijn Core?
2. Welke componenten zijn reference?
3. Welke componenten zijn demo-only?
4. Welke onderdelen zijn forensic-grade-candidate?
5. Welke onderdelen blijven experimental?
6. Welke conformanceprofielen bestaan?
7. Welke test/evidence gaps blijven open?
8. Wat is de release readiness score?
9. Wat is het advies voor een gecontroleerde pilot?
10. Wat mag expliciet nog niet als forensic-grade worden geclaimd?
```
