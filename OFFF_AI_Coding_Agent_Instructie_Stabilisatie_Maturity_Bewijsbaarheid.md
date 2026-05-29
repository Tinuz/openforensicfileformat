# AI Coding Agent Instructie — OFFF Stabilisatie, Maturity en Bewijsbaarheid

## Rol

Je bent een senior software engineer / release engineer en werkt aan de repository voor **Open Forensic File Format (OFFF)**.

Je opdracht is **niet** om nieuwe features toe te voegen.

Je opdracht is om de bestaande OFFF-implementatie scherper bewijsbaar, toetsbaar en bestuurbaar te maken.

Gebruik **Claude Sonnet 4.6** als AI coding agent.

---

## 1. Aanleiding

De OFFF-backlog laat zien dat er veel onderdelen zijn gerealiseerd:

```text
- evidence integrity hardening
- deterministic conversion
- Merkle proofs en tree validation
- verifier profiles
- append-only analysis
- Access Service auth en denied audit
- CI en conformance
- object lineage
- derived object store
- object-producing worker contract
- manifest extensions
- demo tooling
- packed container
- SDKs
- Access Service capabilities
```

De belangrijkste verbetering is nu niet: **meer features bouwen**.

De belangrijkste verbetering is:

```text
Maak scherper bewijsbaar:
1. wat af is;
2. welk maturity-niveau het heeft;
3. welke tests dat aantonen;
4. welke onderdelen OFFF Core zijn;
5. welke onderdelen reference implementation zijn;
6. welke onderdelen demo-only zijn;
7. welke onderdelen experimental zijn;
8. welke beperkingen nog gelden.
```

---

## 2. Hoofdopdracht

Voer een stabilisatie- en bewijsbaarheidsronde uit op de repository.

De deliverables zijn:

```text
1. maturity matrix
2. Core vs Reference vs Demo classificatie
3. evidence-of-done per afgerond backlogitem
4. test traceability matrix
5. conformance profile mapping
6. component status badges/labels
7. release readiness report
8. cleanup van tegenstrijdige backlogstatussen
9. documentatie-update
10. CI-checks die voorkomen dat maturity/testbewijs ontbreekt
```

---

## 3. Belangrijke ontwerpregel

```text
OFFF Core moet een open, tool-agnostische evidence-, lineage-, provenance- en contractlaag blijven.

Reference implementations mogen helpen bij adoptie.

Demo tooling mag de werking tonen.

Maar Core, reference en demo mogen niet door elkaar lopen.
```

---

## 4. Niet doen

Voeg tijdens deze opdracht geen nieuwe functionele features toe.

Niet doen:

```text
[ ] geen nieuwe workers bouwen
[ ] geen nieuwe parserfunctionaliteit toevoegen
[ ] geen nieuwe scheduler bouwen
[ ] geen nieuwe juridische logica toevoegen
[ ] geen nieuwe objecttypen toevoegen tenzij nodig voor documentatieconsistentie
[ ] geen nieuwe storage backend toevoegen
[ ] geen nieuwe AI/ML-feature toevoegen
[ ] geen nieuwe API endpoints toevoegen tenzij nodig voor status/maturity introspectie
```

Wel doen:

```text
[ ] documenteren
[ ] classificeren
[ ] testbewijs koppelen
[ ] inconsistenties oplossen
[ ] maturity labels toevoegen
[ ] conformanceprofielen expliciteren
[ ] CI guardrails toevoegen
[ ] statusrapporten genereren
```

---

# Deel A — Maturity model toevoegen

## 5. Maak een formeel maturity model

Voeg een document toe:

```text
docs/maturity-model.md
```

Definieer minimaal deze maturity levels:

| Level | Naam | Betekenis |
|---|---|---|
| `demo-only` | Demo-only | Alleen bedoeld om concept te tonen; niet geschikt als normatief gedrag |
| `experimental` | Experimental | Werkend prototype, API/schema kan wijzigen |
| `reference` | Reference implementation | Voorbeeldimplementatie van het OFFF-contract, bruikbaar voor adoptie |
| `forensic-grade-candidate` | Forensic-grade candidate | Voldoet grotendeels aan test/conformance-eisen, externe review nodig |
| `forensic-grade` | Forensic-grade | Stabiel, getest, gedocumenteerd, conformance-gedekt en releasewaardig |

## 5.1 Criteria per level

Leg per level vast:

```text
- vereiste testdekking
- vereiste documentatie
- backward compatibility eisen
- conformance eisen
- security review eisen
- toegestane breaking changes
- productieadvies
```

Voorbeeld:

```text
demo-only:
  - mag synthetische data gebruiken
  - mag simplified OFFF container gebruiken
  - mag niet als formele OFFF-conformance worden gezien

reference:
  - volgt formele schema's
  - heeft integration tests
  - toont aanbevolen implementatiepatroon
  - is niet per definitie production-grade

forensic-grade:
  - CI, unit, integration, negative conformance en E2E tests aanwezig
  - schema's stabiel
  - verifier ondersteunt validatie
  - threat/security review gedaan
  - limitations gedocumenteerd
```

## 5.2 Acceptance criteria

```text
[ ] docs/maturity-model.md bestaat.
[ ] Elk maturity level is concreet gedefinieerd.
[ ] Er staat expliciet dat demo-only geen forensic-grade is.
[ ] Er staat expliciet dat reference implementation niet automatisch Core is.
```

---

# Deel B — Component classification matrix

## 6. Maak component-classificatie

Voeg toe:

```text
docs/component-classification.md
```

Dit document moet elke belangrijke component classificeren als:

```text
core
reference
demo
experimental
legacy
```

## 6.1 Definities

Gebruik deze definities:

| Classificatie | Betekenis |
|---|---|
| `core` | Normatief onderdeel van OFFF-specificatie of core library |
| `reference` | Voorbeeldimplementatie van een normatief contract |
| `demo` | Alleen bedoeld voor demonstratie |
| `experimental` | Nog instabiel; niet als contract beschouwen |
| `legacy` | Ondersteund voor compatibiliteit maar niet richtinggevend |

## 6.2 Minimale componenten

Classificeer minimaal:

```text
offf-core
offf-convert
offf-verify
offf-export
offf-index
offf-jobs
offf-keyword-worker
offf-yara-worker
offf-access-service
offf-demo
packed container support
Python SDK
Go SDK
Access Service gRPC/REST
object lineage model
derived object store
extension model
tool registry
worker runtime state
worker health registry
assignment audit trail
conformance suite
Docker demo
```

## 6.3 Aandachtspunten

Classificeer expliciet:

```text
worker runtime state
retry/failure policy
worker health registry
assignment audit trail
```

als:

```text
reference
```

of:

```text
demo/reference
```

Niet als `core`, tenzij er een zeer duidelijke reden is.

## 6.4 Acceptance criteria

```text
[ ] Alle belangrijke crates/tools/docs zijn geclassificeerd.
[ ] Geen demo-component staat als core gelabeld.
[ ] Worker orchestration/health/retry staat niet als OFFF Core gelabeld.
[ ] Packed container is duidelijk als transport/package representation gemarkeerd.
[ ] Exploded directory model blijft canonical representation.
```

---

# Deel C — Statusmatrix verbeteren

## 7. Vervang of verbeter `docs/status.md`

Werk `docs/status.md` bij met een uniforme tabel.

Elke rij bevat:

```text
Component
Classification
Maturity
Status
Implemented in
Test evidence
Conformance profile
Known limitations
Last verified date
```

Voorbeeld:

| Component | Classification | Maturity | Status | Test evidence | Limitations |
|---|---|---|---|---|---|
| `offf-core chunk store` | core | forensic-grade-candidate | done | unit + e2e + conformance | object storage finalization separate |
| `offf-demo` | demo | demo-only | done | smoke script | not production-grade |
| `worker health registry` | reference | experimental/reference | done | runtime tests | not OFFF Core |

## 7.1 Acceptance criteria

```text
[ ] docs/status.md heeft uniforme kolommen.
[ ] Elke rij heeft classification en maturity.
[ ] Elke rij heeft test evidence of expliciet 'missing'.
[ ] Elke rij heeft known limitations of 'none known'.
[ ] Tegenstrijdige statussen zijn opgelost.
```

---

# Deel D — Backlogconsistentie herstellen

## 8. Maak backlogstatus eenduidig

De backlog bevat items die tegelijk als afgerond en in-progress/planned lijken te staan.

Voer een cleanup uit:

```text
- één status per item
- één done date per done item
- geen conflicting statusvelden
- remaining items alleen als limitation of follow-up, niet als statusconflict
```

## 8.1 Standaardformat per backlogitem

Gebruik dit format voor afgeronde items:

```markdown
- [x] Sprintnaam
  - Added:
  - Status: done
  - Done:
  - Classification:
  - Maturity:
  - Implemented in:
  - Tests:
  - Conformance:
  - Known limitations:
  - Evidence:
```

Gebruik dit format voor niet-afgeronde items:

```markdown
- [ ] Sprintnaam
  - Added:
  - Status: planned | in-progress | blocked
  - Classification:
  - Target maturity:
  - Scope:
  - Acceptance:
```

## 8.2 Acceptance criteria

```text
[ ] Geen item heeft tegelijk done en planned/in-progress.
[ ] Elk afgerond item heeft maturity.
[ ] Elk afgerond item heeft test evidence.
[ ] Openstaande punten blijven open, maar staan niet als gedeeltelijk done zonder bewijs.
```

---

# Deel E — Evidence-of-done per afgerond item

## 9. Maak evidence-of-done overzicht

Voeg toe:

```text
docs/evidence-of-done.md
```

Doel:

```text
Per afgerond backlogitem aantonen waarom het als done geldt.
```

## 9.1 Format

Per item:

```markdown
## Hardening Sprint 4 — Merkle proofs + full tree validation

Classification: core
Maturity: forensic-grade-candidate
Implemented in:
- crates/offf-core/src/hash.rs
- crates/offf-verify/src/main.rs

Test evidence:
- cargo test ...
- tests/conformance/...
- tests/e2e/...

Verifier coverage:
- merkle_tree.bin magic/version/length/root
- leaves order
- proof generation/verification

Known limitations:
- ...

Conclusion:
- Done criteria met bewijs behaald / deels behaald
```

## 9.2 Acceptance criteria

```text
[ ] Elk afgerond P0-item heeft evidence-of-done sectie.
[ ] Elk afgerond lineage item S9-S12 heeft evidence-of-done sectie.
[ ] Elk afgerond P1/P2-item met impact op Core heeft evidence-of-done sectie.
[ ] Missing evidence wordt expliciet gelabeld als gap.
```

---

# Deel F — Test traceability matrix

## 10. Maak test traceability matrix

Voeg toe:

```text
docs/test-traceability.md
```

Doel:

```text
Traceer requirements/backlogitems naar tests.
```

## 10.1 Format

| Requirement / Backlog item | Unit tests | Integration tests | E2E tests | Negative tests | Conformance | Status |
|---|---|---|---|---|---|---|

Voorbeelden:

```text
Crash-safe convert
Existing chunk verification
Deterministic mode
Merkle proof
Verifier profiles
Append-only analysis
Access Service denied audit
Object lineage
Derived object store
Object-producing result manifest
Packed container
MinIO smoke
E01 smoke
```

## 10.2 Acceptance criteria

```text
[ ] Matrix bevat alle afgeronde P0 items.
[ ] Matrix bevat alle afgeronde lineage items.
[ ] Matrix bevat SDK en Access Service checks.
[ ] Items zonder testbewijs staan als gap.
[ ] CI verwijst naar deze matrix.
```

---

# Deel G — Conformanceprofielen formaliseren

## 11. Maak officiële conformanceprofielen

Voeg toe:

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
```

## 11.1 Per profiel vastleggen

```text
- scope
- verplichte functies
- verplichte schema's
- verplichte verifier checks
- verplichte negative tests
- optionele onderdelen
- report format
```

## 11.2 Acceptance criteria

```text
[ ] Elk profiel heeft duidelijke pass/fail criteria.
[ ] Conformance suite kan profielen apart rapporteren.
[ ] Profielen verwijzen naar relevante schema's en tests.
[ ] Tools kunnen claimen welk profiel zij ondersteunen.
```

---

# Deel H — Core versus Reference versus Demo afdwingen

## 12. Voeg repository labels of metadata toe

Maak een bestand:

```text
components.toml
```

of:

```text
components.json
```

Voorbeeld:

```toml
[components.offf-core]
classification = "core"
maturity = "forensic-grade-candidate"
owner = "offf-core"
docs = ["docs/spec.md", "docs/status.md"]
tests = ["cargo test -p offf-core"]

[components.offf-demo]
classification = "demo"
maturity = "demo-only"
owner = "demo"
docs = ["docs/demo.md"]
tests = ["tests/smoke/demo"]

[components.worker_runtime]
classification = "reference"
maturity = "experimental"
docs = ["docs/worker-runtime.md"]
```

## 12.1 Voeg validator toe

Maak script:

```text
scripts/check_component_metadata.py
```

Dit script controleert:

```text
- elke crate/tool heeft component metadata
- classification is geldig
- maturity is geldig
- core componenten hebben tests en docs
- demo componenten staan niet als forensic-grade
- reference componenten staan niet als core
```

## 12.2 Acceptance criteria

```text
[ ] components.toml/json bestaat.
[ ] Checkscript draait lokaal.
[ ] CI faalt als nieuwe component zonder classification wordt toegevoegd.
[ ] CI faalt als demo-only component als forensic-grade staat.
```

---

# Deel I — Release readiness report

## 13. Voeg release readiness rapport toe

Maak script:

```text
scripts/generate_release_readiness.py
```

Output:

```text
reports/release-readiness.json
reports/release-readiness.md
```

## 13.1 Rapportinhoud

Het rapport toont:

```text
- componenten per maturity
- open evidence gaps
- open test gaps
- open conformance gaps
- core/reference/demo verdeling
- CI status
- laatste verificatiedatum
- releaseadvies
```

Voorbeeld releaseadvies:

```text
Release readiness: forensic-grade-candidate
Blockers:
- 3 components missing test evidence
- 2 done backlog items missing conformance mapping

Warnings:
- demo components present
- reference worker runtime not Core
```

## 13.2 Acceptance criteria

```text
[ ] Rapport kan lokaal gegenereerd worden.
[ ] Rapport wordt als CI artifact gepubliceerd.
[ ] Rapport noemt expliciet wat niet production/forensic-grade is.
```

---

# Deel J — CI guardrails

## 14. Voeg CI-checks toe

Breid CI uit met:

```text
check component metadata
check maturity labels
check status matrix completeness
check test traceability completeness
generate release readiness report
```

## 14.1 Minimale CI job

```yaml
- name: Check OFFF maturity metadata
  run: python scripts/check_component_metadata.py

- name: Generate release readiness report
  run: python scripts/generate_release_readiness.py

- name: Check test traceability
  run: python scripts/check_test_traceability.py
```

## 14.2 Acceptance criteria

```text
[ ] CI faalt bij ontbrekende maturity/classification.
[ ] CI faalt bij done item zonder evidence-of-done.
[ ] CI faalt bij core component zonder tests.
[ ] Release readiness report wordt gepubliceerd.
```

---

# Deel K — Documentatie-aanpassingen

## 15. README aanpassen

De root `README.md` moet expliciet uitleggen:

```text
- wat OFFF Core is
- wat reference implementation is
- wat demo tooling is
- welke maturity het project heeft
- waar status/maturity/conformance te vinden zijn
```

Voeg bijvoorbeeld toe:

```markdown
## Project maturity and component classification

OFFF separates:
- Core specification and libraries
- Reference implementations
- Demo tooling
- Experimental components

See:
- docs/status.md
- docs/maturity-model.md
- docs/component-classification.md
- docs/conformance-profiles.md
```

## 15.1 Acceptance criteria

```text
[ ] README linkt naar maturity model.
[ ] README linkt naar component classification.
[ ] README waarschuwt dat demo tooling niet production-grade is.
[ ] README benoemt canonical exploded directory model versus packed transport format.
```

---

# Deel L — Canonical versus transport format

## 16. Packed container positionering aanscherpen

OFFF heeft een packed container `.offfpack`. Documenteer expliciet:

```text
Exploded OFFF directory = canonical representation
.offfpack = transport/package representation
```

Voeg toe:

```text
docs/packed-container.md
```

## 16.1 Vereisten

```text
- pack/unpack moet metadata-equivalent zijn
- verify moet consistent zijn voor unpacked en packed
- packed is niet de primaire bronrepresentatie
- packed is bedoeld voor transport, distributie of archivering
```

## 16.2 Acceptance criteria

```text
[ ] Documentatie maakt canonical vs transport expliciet.
[ ] Statusmatrix classificeert packed container correct.
[ ] Release readiness report toont packed als transport feature.
```

---

# Deel M — Worker framework positionering

## 17. Worker runtime buiten Core houden

De backlog noemt:

```text
- deterministic job replay
- retry/failure policy
- worker health registry
- assignment audit trail
```

Deze onderdelen zijn nuttig, maar mogen niet als OFFF Core worden gepositioneerd.

Voeg toe:

```text
docs/reference-worker-runtime.md
```

## 17.1 Documenteer

```text
- dit is reference implementation
- niet verplicht voor OFFF-conformance
- externe schedulers mogen eigen runtime gebruiken
- OFFF Core definieert alleen job/shard/result/provenance contracten
```

## 17.2 Acceptance criteria

```text
[ ] Worker runtime staat niet als core in component-classification.
[ ] Docs leggen uit dat scheduling/orchestration bovenop OFFF draait.
[ ] Conformanceprofielen vereisen geen specifieke scheduler/runtime.
```

---

# Deel N — Definition of Done aanscherpen

## 18. Nieuwe Definition of Done

Voeg toe:

```text
docs/definition-of-done.md
```

Voor elk item dat als `done` wordt gemarkeerd, moet aanwezig zijn:

```text
[ ] status = done
[ ] classification ingevuld
[ ] maturity ingevuld
[ ] implemented-in ingevuld
[ ] tests ingevuld
[ ] conformance mapping of expliciet n.v.t.
[ ] known limitations ingevuld
[ ] docs bijgewerkt indien extern zichtbaar
[ ] CI groen
```

## 18.1 Acceptance criteria

```text
[ ] Definition of Done bestaat.
[ ] Backlog verwijst naar deze DoD.
[ ] CI of checklist ondersteunt deze DoD.
```

---

# Deel O — Uitvoering in fasen

## Fase 1 — Inventarisatie en classificatie

```text
[ ] Maak maturity-model.md
[ ] Maak component-classification.md
[ ] Maak components.toml/json
[ ] Classificeer alle bestaande crates/tools/docs
[ ] Werk status.md bij
```

## Fase 2 — Bewijsvoering

```text
[ ] Maak evidence-of-done.md
[ ] Maak test-traceability.md
[ ] Koppel afgeronde backlogitems aan tests
[ ] Markeer missing evidence als gap
```

## Fase 3 — Conformance en release readiness

```text
[ ] Maak conformance-profiles.md
[ ] Maak generate_release_readiness.py
[ ] Maak check_component_metadata.py
[ ] Maak check_test_traceability.py
```

## Fase 4 — Documentatie en CI

```text
[ ] README bijwerken
[ ] packed-container.md toevoegen
[ ] reference-worker-runtime.md toevoegen
[ ] definition-of-done.md toevoegen
[ ] CI uitbreiden met maturity/check scripts
```

## Fase 5 — Backlog cleanup

```text
[ ] Backlogstatussen normaliseren
[ ] Conflicting statuses oplossen
[ ] Done items voorzien van maturity/test evidence
[ ] Demo/reference/Core labels toevoegen
```

---

# 19. Acceptance criteria voor de hele opdracht

De opdracht is klaar als:

```text
[ ] Elk component heeft classification.
[ ] Elk component heeft maturity.
[ ] Elk afgerond backlogitem heeft evidence-of-done.
[ ] Elk afgerond backlogitem is gekoppeld aan testbewijs of expliciete gap.
[ ] Core/reference/demo zijn aantoonbaar gescheiden.
[ ] Worker runtime is als reference/demo gepositioneerd, niet als Core.
[ ] Packed container is als transport representation gepositioneerd.
[ ] Conformanceprofielen zijn formeel beschreven.
[ ] Release readiness report kan gegenereerd worden.
[ ] CI controleert maturity en traceability metadata.
[ ] README wijst naar alle relevante status- en maturitydocumentatie.
```

---

# 20. Kernzin voor de coding agent

```text
Voeg geen nieuwe OFFF-features toe.
Maak de bestaande implementatie bewijsbaar, toetsbaar en bestuurbaar:
wat is Core, wat is reference, wat is demo, welk maturity-niveau heeft het,
en welk testbewijs ondersteunt die claim?
```

---

# 21. Verwachte outputbestanden

Na afronding moeten minimaal deze bestanden bestaan of bijgewerkt zijn:

```text
docs/maturity-model.md
docs/component-classification.md
docs/status.md
docs/evidence-of-done.md
docs/test-traceability.md
docs/conformance-profiles.md
docs/packed-container.md
docs/reference-worker-runtime.md
docs/definition-of-done.md
components.toml or components.json
scripts/check_component_metadata.py
scripts/check_test_traceability.py
scripts/generate_release_readiness.py
reports/release-readiness.md
reports/release-readiness.json
README.md
.github/workflows/offf-ci.yml
```

---

# 22. Finale controle

Voer aan het einde uit:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python scripts/check_component_metadata.py
python scripts/check_test_traceability.py
python scripts/generate_release_readiness.py
```

Rapporteer daarna:

```text
- welke componenten forensic-grade-candidate zijn
- welke componenten reference zijn
- welke componenten demo-only zijn
- welke bewijs-gaps nog bestaan
- welke items niet als production/forensic-grade mogen worden beschouwd
```
