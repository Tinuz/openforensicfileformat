# offf-export

## Purpose
Exports or reconstructs data from an OFFF container into a desired output format or raw stream.

## Usage
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

## Behavior Summary
- Reads chunk maps and chunk store data.
- Reconstructs output in source chunk order.
- Integrity verification should be part of the export workflow or a pre-check.
- Packed containers use an index + checksum footer for fast validation.
