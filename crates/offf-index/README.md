# offf-index

## Doel
Bouwt indexen op OFFF data, zoals filesystem index en gerelateerde lookup artefacten.

## Resultaat
Index output wordt geschreven onder `indexes/` in de container, inclusief file index parquet bestanden.

## Voorbeeld
```bash
cargo run -p offf-index -- --help
```

## Richtlijnen
- Houd index output deterministisch waar mogelijk.
- Schrijf provenance events voor index build acties.
- Controleer output met conformance en verify flows.
