# OFFF Python SDK

## Doel
De Python SDK biedt een eenvoudige API voor veelgebruikte OFFF-operaties:
- container openen
- manifest lezen
- container en chunks verifieren
- offsets mappen naar chunks
- analysis resultaten schrijven
- provenance events appenden

## Installatie
1. Activeer een Python environment.
2. Installeer afhankelijkheden via `pyproject.toml`.
3. Installeer de package in editable mode:

```bash
pip install -e sdk/python
```

## Snelle start
```python
from offf_sdk.api import open_container, verify_container

c = open_container("tests/samples/4orensics.case2.offf")
result = verify_container(c)
print(result)
```

## Tests
- Contract-test: `sdk/python/tests/test_api_contract.py`
- Draai lokaal:

```bash
cd sdk/python
python -m unittest tests/test_api_contract.py -v
```

## Belangrijke modules
- `offf_sdk/api.py`: publieke API-functies
- `offf_sdk/container.py`: kernlogica voor lezen/verify/schrijven
- `offf_sdk/schema_validation.py`: schema validatie
