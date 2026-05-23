# Config Directory

## Doel
Deze map bevat configuratiebestanden die door OFFF services en workers worden gebruikt om runtime-gedrag te sturen zonder codewijzigingen.

## Bestanden
- `tool-registry.example.json`: voorbeeld van een goedgekeurde tool-registry met identity, versie, hashes en toegestane write-layers.

## Gebruik
1. Kopieer `tool-registry.example.json` naar een eigen bestand, bijvoorbeeld `tool-registry.json`.
2. Vul per tool minimaal in:
   - naam
   - vendor
   - versie
   - executable of image hash
   - toegestane OFFF profielen
3. Verwijs services naar dit bestand via de relevante CLI-flag of environment variable.

## Richtlijnen
- Commit geen secrets.
- Leg wijzigingen in governance-velden vast in provenance of change logs.
- Houd tool-hashes gelijk aan de exact gedeployde binary/image.
