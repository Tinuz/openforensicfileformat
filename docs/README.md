# Documentation

## Doel
Deze map bevat formele en operationele documentatie voor het OFFF ecosysteem.

## Structuur
- `schema/`: JSON Schema definities voor OFFF objecten en events.

## Werkwijze
1. Werk schema of documentatie altijd versiegebonden bij.
2. Valideer JSON schema bestanden lokaal en in CI.
3. Koppel wijzigingen aan concrete implementaties in crates of SDK's.

## Aanbevolen workflow
1. Pas schema aan.
2. Werk producer/consumer code bij.
3. Voeg of update tests.
4. Verifieer backward compatibility.
