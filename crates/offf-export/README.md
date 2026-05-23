# offf-export

## Doel
Exporteert of reconstrueert data uit een OFFF container naar een gewenst outputformaat of raw stream.

## Gebruik
```bash
cargo run -p offf-export -- --help
```

### Exploded OFFF -> raw export
```bash
cargo run -p offf-export -- export tests/samples/4orensics.case2.offf --output out.dd
```

### Exploded OFFF -> packed container
```bash
cargo run -p offf-export -- pack tests/samples/4orensics.case2.offf --output case.offfpack
```

### Packed container inspectie
```bash
cargo run -p offf-export -- list case.offfpack
```

### Packed container -> directory
```bash
cargo run -p offf-export -- unpack case.offfpack --output restored.offf
```

## Verwachting
- Leest chunk map en chunk store.
- Reconstructie volgt source-volgorde van chunks.
- Integriteitscontroles horen onderdeel te zijn van exportflow of pre-check.
- Packed container gebruikt een index + footer met checksum voor snelle validatie.
