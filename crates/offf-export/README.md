# offf-export

## Doel
Exporteert of reconstrueert data uit een OFFF container naar een gewenst outputformaat of raw stream.

## Gebruik
```bash
cargo run -p offf-export -- --help
```

## Verwachting
- Leest chunk map en chunk store.
- Reconstructie volgt source-volgorde van chunks.
- Integriteitscontroles horen onderdeel te zijn van exportflow of pre-check.
