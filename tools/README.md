# Tools

## Doel
Deze map bevat tooling-assets die lokaal worden gebruikt voor ontwikkeling en smoke-validatie.

## Huidige inhoud
- `ewfexport-docker/`: Docker build context voor EWF tooling image.

## Gebruik
1. Bouw tooling image:

```bash
docker build -t offf/ewf-tools:latest tools/ewfexport-docker
```

2. Gebruik image voor E01 exportflows in lokale smoke tests.

## Richtlijnen
- Houd tool-images klein en reproduceerbaar.
- Pin versies waar mogelijk.
- Documenteer command line gedrag in de betreffende submap.
