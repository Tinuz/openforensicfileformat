# OFFF Schema Catalog

## Doel
Deze map bevat JSON Schema definities voor OFFF artefacten, inclusief manifesten, index-rows, provenance en job-objecten.

## Inhoud
Voorbeelden van schema's in deze map:
- `offf-manifest-0.1.0.schema.json`
- `offf-acquisition-0.1.0.schema.json`
- `offf-file-index-row-0.1.0.schema.json`
- `offf-provenance-event-0.1.0.schema.json`
- `offf-schema-catalog-0.1.0.json`

## Gebruik
1. Kies het schema dat hoort bij het objecttype en de versie.
2. Valideer producer-output tijdens tests.
3. Laat consumers falen op schemafouten met duidelijke foutmelding.

## Onderhoudsregels
- Voeg bij een breaking change een nieuwe versiesuffix toe.
- Houd oudere schema's read-only voor reproduceerbaarheid.
- Werk catalog-bestand bij als er schema's bijkomen.
