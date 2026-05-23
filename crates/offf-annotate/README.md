# offf-annotate

## Doel
Biedt annotatiefunctionaliteit op OFFF containers, inclusief append-operaties op analysis/provenance lagen.

## Gebruik
```bash
cargo run -p offf-annotate -- --help
```

## Typische acties
- annotatie toevoegen
- annotatie updaten
- annotatie queryen (afhankelijk van subcommands)

## Richtlijnen
- Houd evidence layer immutable.
- Schrijf mutaties alleen naar toegestane append layers.
- Voeg provenance records toe voor auditbaarheid.
