OFFF Smoke Tests
================

Doel
----

Deze map bevat end-to-end smoke tests voor omgevingsafhankelijke OFFF flows.
De scripts zijn bedoeld als snelle regressie-check na wijzigingen in storage, worker of E01 gedrag.

Beschikbare scripts
-------------------

- `phase5_minio_smoke.py`
	- Bouwt een kleine OFFF testcontainer.
	- Uploadt die naar MinIO (`s3://...`).
	- Draait `offf-verify` tegen remote container.
	- Draait `offf-keyword-worker` en `offf-yara-worker` tegen remote container.
	- Controleert of analysis artefacten zijn geschreven.
	- Test concurrente provenance appends met meerdere workerprocessen.

- `phase7_e01_smoke.py`
	- Genereert lokaal een kleine raw sample.
	- Maakt daar een `.E01` van met `ewfacquire` (dockerized).
	- Converteert naar OFFF met `offf-convert --input-type e01`.
	- Controleert vereiste `acquisition.json` velden.
	- Verifieert outputcontainer met `offf-verify`.

Vereisten
---------

- Docker Desktop actief.
- MinIO bereikbaar op `http://localhost:9000` (voor Phase 5).
- Rust toolchain en werkende `cargo` build.

Uitvoeren
---------

Vanaf repository root:

```bash
python tests/smoke/phase5_minio_smoke.py
python tests/smoke/phase7_e01_smoke.py
```

Foutanalyse
-----------

- MinIO lookup fout:
	- Controleer `OFFF_S3_ENDPOINT`, access key en secret key.
- E01 export fout:
	- Controleer of Docker draait en `offf/ewf-tools:latest` buildbaar is.
- Worker write/provenance fout:
	- Controleer bucket permissies en object key prefix.
